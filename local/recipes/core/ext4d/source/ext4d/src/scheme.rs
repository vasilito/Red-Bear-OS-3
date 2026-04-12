use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use redox_scheme::{CallerCtx, OpenResult, SendFdRequest, scheme::SchemeSync};
use rsext4::{
    BlockDevice, Ext4Error, Ext4FileSystem, Jbd2Dev, api, delete_dir, delete_file, dir,
    disknode::Ext4Inode,
    entries::{DirEntryIterator, Ext4DirEntry2},
    loopfile, mkdir, mkfile, truncate, umount,
};
use syscall::{
    data::{Stat, StatVfs},
    dirent::{DirEntry, DirentBuf, DirentKind},
    error::{
        EACCES, EBADF, EEXIST, EINVAL, EISDIR, ENOENT, ENOTDIR, ENOTEMPTY, EPERM, Error, Result,
    },
    flag::{
        AT_REMOVEDIR, EventFlags, F_GETFD, F_GETFL, F_SETFD, F_SETFL, O_ACCMODE, O_CREAT,
        O_DIRECTORY, O_EXCL, O_RDONLY, O_TRUNC, O_WRONLY,
    },
    schemev2::NewFdFlags,
};

use crate::handle::{DirectoryHandle, FileHandle, Handle};

const PERM_EXEC: u16 = 0o1;
const PERM_WRITE: u16 = 0o2;
const PERM_READ: u16 = 0o4;

struct Lookup {
    path: String,
    inode_num: rsext4::bmalloc::InodeNumber,
    inode: Ext4Inode,
}

pub struct Ext4Scheme<D: BlockDevice> {
    mounted_path: String,
    fs: Ext4FileSystem,
    journal: Jbd2Dev<D>,
    next_id: AtomicUsize,
    handles: BTreeMap<usize, Handle>,
}

impl<D: BlockDevice> Ext4Scheme<D> {
    pub fn new(
        _scheme_name: String,
        mounted_path: String,
        fs: Ext4FileSystem,
        journal: Jbd2Dev<D>,
    ) -> Self {
        Self {
            mounted_path,
            fs,
            journal,
            next_id: AtomicUsize::new(1),
            handles: BTreeMap::new(),
        }
    }

    pub fn cleanup(self) -> Result<()> {
        let Ext4Scheme {
            mut fs,
            mut journal,
            ..
        } = self;

        fs.sync_filesystem(&mut journal).map_err(ext4_error)?;
        umount(fs, &mut journal).map_err(ext4_error)
    }

    fn insert_handle(&mut self, handle: Handle) -> usize {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.handles.insert(id, handle);
        id
    }

    fn root_lookup(&mut self) -> Result<Lookup> {
        let (inode_num, inode) = dir::get_inode_with_num(&mut self.fs, &mut self.journal, "/")
            .map_err(ext4_error)?
            .ok_or(Error::new(ENOENT))?;

        Ok(Lookup {
            path: String::new(),
            inode_num,
            inode,
        })
    }

    fn make_ext4_path(path: &str) -> String {
        if path.is_empty() {
            "/".to_string()
        } else {
            format!("/{path}")
        }
    }

    fn normalize_path(path: &str) -> String {
        let mut components = Vec::new();

        for component in path.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    let _ = components.pop();
                }
                part => components.push(part),
            }
        }

        components.join("/")
    }

    fn join_path(base: &str, path: &str) -> String {
        if path.starts_with('/') {
            return Self::normalize_path(path);
        }

        if base.is_empty() {
            Self::normalize_path(path)
        } else if path.is_empty() {
            base.to_string()
        } else {
            Self::normalize_path(&format!("{base}/{path}"))
        }
    }

    fn dirfd_base_path(&self, dirfd: usize, path: &str) -> Result<String> {
        if path.starts_with('/') {
            return Ok(Self::normalize_path(path));
        }

        match self.handles.get(&dirfd) {
            Some(Handle::SchemeRoot) => Ok(Self::normalize_path(path)),
            Some(Handle::Directory(handle)) => Ok(Self::join_path(handle.path(), path)),
            Some(Handle::File(_)) => Err(Error::new(ENOTDIR)),
            None => Err(Error::new(EBADF)),
        }
    }

    fn split_parent_child(path: &str) -> Result<(String, String)> {
        let normalized = Self::normalize_path(path);
        if normalized.is_empty() {
            return Err(Error::new(EPERM));
        }

        match normalized.rsplit_once('/') {
            Some((parent, child)) if !child.is_empty() => {
                Ok((parent.to_string(), child.to_string()))
            }
            None => Ok((String::new(), normalized)),
            _ => Err(Error::new(EINVAL)),
        }
    }

    fn check_permission(inode: &Ext4Inode, ctx: &CallerCtx, perm: u16) -> bool {
        if ctx.uid == 0 {
            return true;
        }

        let mode = inode.permissions();
        let granted = if ctx.uid == inode.uid() {
            (mode >> 6) & 0o7
        } else if ctx.gid == inode.gid() {
            (mode >> 3) & 0o7
        } else {
            mode & 0o7
        };

        granted & perm == perm
    }

    fn require_permission(inode: &Ext4Inode, ctx: &CallerCtx, perm: u16) -> Result<()> {
        if Self::check_permission(inode, ctx, perm) {
            Ok(())
        } else {
            Err(Error::new(EACCES))
        }
    }

    fn lookup_path(&mut self, path: &str, ctx: &CallerCtx) -> Result<Option<Lookup>> {
        let normalized = Self::normalize_path(path);
        if normalized.is_empty() {
            return self.root_lookup().map(Some);
        }

        let mut current = self.root_lookup()?;
        for component in normalized.split('/') {
            if !current.inode.is_dir() {
                return Err(Error::new(ENOTDIR));
            }

            Self::require_permission(&current.inode, ctx, PERM_EXEC)?;

            let next_path = if current.path.is_empty() {
                component.to_string()
            } else {
                format!("{}/{}", current.path, component)
            };

            let Some((inode_num, inode)) = dir::get_inode_with_num(
                &mut self.fs,
                &mut self.journal,
                &Self::make_ext4_path(&next_path),
            )
            .map_err(ext4_error)?
            else {
                return Ok(None);
            };

            current = Lookup {
                path: next_path,
                inode_num,
                inode,
            };
        }

        Ok(Some(current))
    }

    fn lookup_existing(&mut self, path: &str, ctx: &CallerCtx) -> Result<Lookup> {
        self.lookup_path(path, ctx)?.ok_or(Error::new(ENOENT))
    }

    fn lookup_parent(&mut self, path: &str, ctx: &CallerCtx) -> Result<(Lookup, String)> {
        let (parent_path, child) = Self::split_parent_child(path)?;
        let parent = self.lookup_existing(&parent_path, ctx)?;
        if !parent.inode.is_dir() {
            return Err(Error::new(ENOTDIR));
        }
        Self::require_permission(&parent.inode, ctx, PERM_EXEC | PERM_WRITE)?;
        Ok((parent, child))
    }

    fn stat_from_lookup(&self, lookup: &Lookup, stat: &mut Stat) {
        *stat = Stat::default();
        stat.st_dev = 0;
        stat.st_ino = u64::from(lookup.inode_num.raw());
        stat.st_mode = lookup.inode.i_mode;
        stat.st_nlink = u32::from(lookup.inode.i_links_count);
        stat.st_uid = lookup.inode.uid();
        stat.st_gid = lookup.inode.gid();
        stat.st_size = lookup.inode.size();
        stat.st_blksize = self.fs.superblock.block_size() as u32;
        stat.st_blocks = lookup.inode.blocks_count();

        let inode_size = self.fs.superblock.inode_size();
        let atime = lookup.inode.atime_ts(inode_size);
        let mtime = lookup.inode.mtime_ts(inode_size);
        let ctime = lookup.inode.ctime_ts(inode_size);

        stat.st_atime = atime.sec.max(0) as u64;
        stat.st_atime_nsec = atime.nsec;
        stat.st_mtime = mtime.sec.max(0) as u64;
        stat.st_mtime_nsec = mtime.nsec;
        stat.st_ctime = ctime.sec.max(0) as u64;
        stat.st_ctime_nsec = ctime.nsec;
    }

    fn refresh_file_handle(&mut self, id: usize) -> Result<()> {
        let (path, offset) = match self.handles.get(&id) {
            Some(Handle::File(handle)) => (handle.path().to_string(), handle.file.offset),
            _ => return Err(Error::new(EBADF)),
        };

        let file = api::open(
            &mut self.journal,
            &mut self.fs,
            &Self::make_ext4_path(&path),
            false,
        )
        .map_err(ext4_error)?;

        let mut file = file;
        api::lseek(&mut file, offset).map_err(ext4_error)?;

        match self.handles.get_mut(&id) {
            Some(Handle::File(handle)) => {
                handle.file = file;
                handle.set_path(path);
                Ok(())
            }
            _ => Err(Error::new(EBADF)),
        }
    }

    fn dirent_kind_from_file_type(file_type: u8) -> DirentKind {
        match file_type {
            Ext4DirEntry2::EXT4_FT_DIR => DirentKind::Directory,
            Ext4DirEntry2::EXT4_FT_REG_FILE => DirentKind::Regular,
            Ext4DirEntry2::EXT4_FT_CHRDEV => DirentKind::CharDev,
            Ext4DirEntry2::EXT4_FT_BLKDEV => DirentKind::BlockDev,
            Ext4DirEntry2::EXT4_FT_SYMLINK => DirentKind::Symlink,
            Ext4DirEntry2::EXT4_FT_SOCK => DirentKind::Socket,
            _ => DirentKind::Unspecified,
        }
    }

    fn directory_entries(
        &mut self,
        _path: &str,
        inode: &Ext4Inode,
    ) -> Result<Vec<(u64, u64, String, DirentKind)>> {
        let mut inode_copy = *inode;
        let blocks = loopfile::resolve_inode_block_allextend(
            &mut self.fs,
            &mut self.journal,
            &mut inode_copy,
        )
        .map_err(ext4_error)?;

        let block_size = self.fs.superblock.block_size() as usize;
        let mut entries = Vec::new();
        let mut opaque = 1u64;

        for &phys in blocks.values() {
            let cached = self
                .fs
                .datablock_cache
                .get_or_load(&mut self.journal, phys)
                .map_err(ext4_error)?;
            for (entry, _) in DirEntryIterator::new(&cached.data[..block_size]) {
                let Some(name) = entry.name_str() else {
                    continue;
                };

                let kind = match name {
                    "." | ".." => DirentKind::Directory,
                    _ => Self::dirent_kind_from_file_type(entry.file_type),
                };

                entries.push((u64::from(entry.inode), opaque, name.to_string(), kind));
                opaque = opaque.saturating_add(1);
            }
        }

        Ok(entries)
    }

    fn create_directory_handle(&mut self, lookup: Lookup, flags: usize) -> OpenResult {
        let id = self.insert_handle(Handle::Directory(DirectoryHandle::new(
            lookup.path,
            lookup.inode_num,
            lookup.inode,
            flags,
        )));

        OpenResult::ThisScheme {
            number: id,
            flags: NewFdFlags::POSITIONED,
        }
    }

    fn create_file_handle(
        &mut self,
        path: String,
        file: api::OpenFile,
        flags: usize,
    ) -> OpenResult {
        let id = self.insert_handle(Handle::File(FileHandle::new(path, file, flags)));

        OpenResult::ThisScheme {
            number: id,
            flags: NewFdFlags::POSITIONED,
        }
    }

    fn handle_lookup_for_stat(&mut self, id: usize, ctx: &CallerCtx) -> Result<Lookup> {
        let path = match self.handles.get(&id) {
            Some(Handle::SchemeRoot) => None,
            Some(Handle::Directory(handle)) => Some(handle.path().to_string()),
            Some(Handle::File(handle)) => Some(handle.path().to_string()),
            None => return Err(Error::new(EBADF)),
        };

        match path {
            Some(path) => self.lookup_existing(&path, ctx),
            None => self.root_lookup(),
        }
    }

    fn ensure_regular_file_access(handle: &FileHandle, write: bool) -> Result<()> {
        if write && !handle.can_write() {
            return Err(Error::new(EBADF));
        }
        if !write && !handle.can_read() {
            return Err(Error::new(EBADF));
        }
        Ok(())
    }
}

impl<D: BlockDevice> SchemeSync for Ext4Scheme<D> {
    fn scheme_root(&mut self) -> Result<usize> {
        Ok(self.insert_handle(Handle::SchemeRoot))
    }

    fn openat(
        &mut self,
        dirfd: usize,
        path: &str,
        flags: usize,
        _fcntl_flags: u32,
        ctx: &CallerCtx,
    ) -> Result<OpenResult> {
        let resolved_path = self.dirfd_base_path(dirfd, path)?;

        match self.lookup_path(&resolved_path, ctx)? {
            Some(lookup) => {
                if flags & (O_CREAT | O_EXCL) == O_CREAT | O_EXCL {
                    return Err(Error::new(EEXIST));
                }

                if lookup.inode.is_dir() {
                    if flags & O_ACCMODE != O_RDONLY {
                        return Err(Error::new(EISDIR));
                    }
                    Self::require_permission(&lookup.inode, ctx, PERM_READ)?;
                    return Ok(self.create_directory_handle(lookup, flags));
                }

                if flags & O_DIRECTORY == O_DIRECTORY {
                    return Err(Error::new(ENOTDIR));
                }

                if flags & O_ACCMODE != O_WRONLY {
                    Self::require_permission(&lookup.inode, ctx, PERM_READ)?;
                }
                if flags & O_ACCMODE != O_RDONLY {
                    Self::require_permission(&lookup.inode, ctx, PERM_WRITE)?;
                }

                let ext4_path = Self::make_ext4_path(&resolved_path);
                if flags & O_TRUNC == O_TRUNC {
                    truncate(&mut self.journal, &mut self.fs, &ext4_path, 0).map_err(ext4_error)?;
                }

                let file = api::open(&mut self.journal, &mut self.fs, &ext4_path, false)
                    .map_err(ext4_error)?;
                Ok(self.create_file_handle(resolved_path, file, flags))
            }
            None => {
                if flags & O_CREAT != O_CREAT {
                    return Err(Error::new(ENOENT));
                }

                let (_parent, _name) = self.lookup_parent(&resolved_path, ctx)?;
                let ext4_path = Self::make_ext4_path(&resolved_path);

                if flags & O_DIRECTORY == O_DIRECTORY {
                    mkdir(&mut self.journal, &mut self.fs, &ext4_path).map_err(ext4_error)?;
                    let lookup = self.lookup_existing(&resolved_path, ctx)?;
                    Ok(self.create_directory_handle(lookup, flags))
                } else {
                    mkfile(&mut self.journal, &mut self.fs, &ext4_path, None, None)
                        .map_err(ext4_error)?;
                    let file = api::open(&mut self.journal, &mut self.fs, &ext4_path, false)
                        .map_err(ext4_error)?;
                    Ok(self.create_file_handle(resolved_path, file, flags))
                }
            }
        }
    }

    fn read(
        &mut self,
        id: usize,
        buf: &mut [u8],
        offset: u64,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        match self.handles.get_mut(&id) {
            Some(Handle::File(handle)) => {
                Self::ensure_regular_file_access(handle, false)?;
                api::lseek(&mut handle.file, offset).map_err(ext4_error)?;
                let data =
                    api::read_at(&mut self.journal, &mut self.fs, &mut handle.file, buf.len())
                        .map_err(ext4_error)?;
                let count = data.len();
                buf[..count].copy_from_slice(&data);
                Ok(count)
            }
            Some(Handle::Directory(_)) | Some(Handle::SchemeRoot) => Err(Error::new(EISDIR)),
            None => Err(Error::new(EBADF)),
        }
    }

    fn write(
        &mut self,
        id: usize,
        buf: &[u8],
        offset: u64,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        match self.handles.get_mut(&id) {
            Some(Handle::File(handle)) => {
                Self::ensure_regular_file_access(handle, true)?;
                api::lseek(&mut handle.file, offset).map_err(ext4_error)?;
                api::write_at(&mut self.journal, &mut self.fs, &mut handle.file, buf)
                    .map_err(ext4_error)?;
                Ok(buf.len())
            }
            Some(Handle::Directory(_)) | Some(Handle::SchemeRoot) => Err(Error::new(EISDIR)),
            None => Err(Error::new(EBADF)),
        }
    }

    fn fsize(&mut self, id: usize, ctx: &CallerCtx) -> Result<u64> {
        Ok(self.handle_lookup_for_stat(id, ctx)?.inode.size())
    }

    fn fcntl(&mut self, id: usize, cmd: usize, _arg: usize, _ctx: &CallerCtx) -> Result<usize> {
        let handle = self.handles.get(&id).ok_or(Error::new(EBADF))?;
        match cmd {
            F_GETFL => Ok(handle.flags().unwrap_or(O_RDONLY)),
            F_GETFD => Ok(0),
            F_SETFL | F_SETFD => Ok(0),
            _ => Err(Error::new(EINVAL)),
        }
    }

    fn fevent(&mut self, id: usize, _flags: EventFlags, _ctx: &CallerCtx) -> Result<EventFlags> {
        if self.handles.contains_key(&id) {
            Err(Error::new(EPERM))
        } else {
            Err(Error::new(EBADF))
        }
    }

    fn fpath(&mut self, id: usize, buf: &mut [u8], _ctx: &CallerCtx) -> Result<usize> {
        let handle = self.handles.get(&id).ok_or(Error::new(EBADF))?;
        let Some(path) = handle.path() else {
            return Err(Error::new(EBADF));
        };

        let full_path = if path.is_empty() {
            self.mounted_path.clone()
        } else {
            format!("{}/{}", self.mounted_path, path)
        };

        let bytes = full_path.as_bytes();
        let count = bytes.len().min(buf.len());
        buf[..count].copy_from_slice(&bytes[..count]);
        Ok(count)
    }

    fn fstat(&mut self, id: usize, stat: &mut Stat, ctx: &CallerCtx) -> Result<()> {
        let lookup = self.handle_lookup_for_stat(id, ctx)?;
        self.stat_from_lookup(&lookup, stat);
        Ok(())
    }

    fn fstatvfs(&mut self, id: usize, stat: &mut StatVfs, _ctx: &CallerCtx) -> Result<()> {
        if !self.handles.contains_key(&id) {
            return Err(Error::new(EBADF));
        }

        let stats = self.fs.statfs();
        stat.f_bsize = stats.block_size as u32;
        stat.f_blocks = stats.total_blocks;
        stat.f_bfree = stats.free_blocks;
        stat.f_bavail = stats.free_blocks;
        Ok(())
    }

    fn getdents<'buf>(
        &mut self,
        id: usize,
        mut buf: DirentBuf<&'buf mut [u8]>,
        opaque_offset: u64,
    ) -> Result<DirentBuf<&'buf mut [u8]>> {
        let (path, inode) = match self.handles.get(&id) {
            Some(Handle::Directory(handle)) => (handle.path().to_string(), *handle.inode()),
            Some(Handle::SchemeRoot) => {
                let lookup = self.root_lookup()?;
                (lookup.path, lookup.inode)
            }
            Some(Handle::File(_)) => return Err(Error::new(ENOTDIR)),
            None => return Err(Error::new(EBADF)),
        };

        let entries = self.directory_entries(&path, &inode)?;
        for (inode, next_opaque_id, name, kind) in entries {
            if next_opaque_id <= opaque_offset {
                continue;
            }

            buf.entry(DirEntry {
                inode,
                next_opaque_id,
                name: &name,
                kind,
            })?;
        }

        Ok(buf)
    }

    fn fsync(&mut self, id: usize, _ctx: &CallerCtx) -> Result<()> {
        if !self.handles.contains_key(&id) {
            return Err(Error::new(EBADF));
        }

        self.fs
            .sync_filesystem(&mut self.journal)
            .map_err(ext4_error)
    }

    fn ftruncate(&mut self, id: usize, len: u64, _ctx: &CallerCtx) -> Result<()> {
        let path = match self.handles.get(&id) {
            Some(Handle::File(handle)) => handle.path().to_string(),
            Some(Handle::Directory(_)) | Some(Handle::SchemeRoot) => {
                return Err(Error::new(EISDIR));
            }
            None => return Err(Error::new(EBADF)),
        };

        truncate(
            &mut self.journal,
            &mut self.fs,
            &Self::make_ext4_path(&path),
            len,
        )
        .map_err(ext4_error)?;
        self.refresh_file_handle(id)
    }

    fn unlinkat(&mut self, dirfd: usize, path: &str, flags: usize, ctx: &CallerCtx) -> Result<()> {
        let resolved_path = self.dirfd_base_path(dirfd, path)?;
        let lookup = self.lookup_existing(&resolved_path, ctx)?;
        let (_parent, _name) = self.lookup_parent(&resolved_path, ctx)?;
        let ext4_path = Self::make_ext4_path(&resolved_path);

        if flags & AT_REMOVEDIR == AT_REMOVEDIR {
            if !lookup.inode.is_dir() {
                return Err(Error::new(ENOTDIR));
            }

            let entries = self.directory_entries(&lookup.path, &lookup.inode)?;
            if entries
                .into_iter()
                .any(|(_, _, name, _)| name != "." && name != "..")
            {
                return Err(Error::new(ENOTEMPTY));
            }

            delete_dir(&mut self.fs, &mut self.journal, &ext4_path).map_err(ext4_error)
        } else {
            if lookup.inode.is_dir() {
                return Err(Error::new(EISDIR));
            }

            delete_file(&mut self.fs, &mut self.journal, &ext4_path).map_err(ext4_error)
        }
    }

    fn on_close(&mut self, id: usize) {
        let _ = self.handles.remove(&id);
    }

    fn on_sendfd(&mut self, _sendfd_request: &SendFdRequest) -> Result<usize> {
        Err(Error::new(EPERM))
    }

    fn inode(&self, id: usize) -> Result<usize> {
        match self.handles.get(&id) {
            Some(Handle::File(handle)) => Ok(handle.inode_num().raw() as usize),
            Some(Handle::Directory(handle)) => Ok(handle.inode_num().raw() as usize),
            Some(Handle::SchemeRoot) => Ok(2),
            None => Err(Error::new(EBADF)),
        }
    }
}

fn ext4_error(err: Ext4Error) -> Error {
    Error::new(err.code.as_i32())
}
