use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicUsize, Ordering};

use fatfs::{Date, DateTime, Dir, FileAttributes, FileSystem, Time};
use fscommon::BufStream;
use redox_scheme::{CallerCtx, OpenResult, SendFdRequest, scheme::SchemeSync};
use syscall::{
    data::{Stat, StatVfs},
    dirent::{DirEntry, DirentBuf, DirentKind},
    error::{
        EACCES, EBADF, EEXIST, EINVAL, EISDIR, ENOENT, ENOTDIR, ENOTEMPTY, EOPNOTSUPP,
        EPERM, Error, Result,
    },
    flag::{
        AT_REMOVEDIR, EventFlags, F_GETFD, F_GETFL, F_SETFD, F_SETFL, MODE_DIR, MODE_FILE,
        O_ACCMODE, O_CREAT, O_DIRECTORY, O_EXCL, O_RDONLY, O_TRUNC,
    },
    schemev2::NewFdFlags,
};

use crate::handle::{DirectoryHandle, FileHandle, Handle};

const KIND_UNSPECIFIED: u8 = 0;
const KIND_DIRECTORY: u8 = 1;
const KIND_REGULAR: u8 = 2;
const PERM_EXEC: u16 = 0o1;
const PERM_WRITE: u16 = 0o2;
const PERM_READ: u16 = 0o4;

#[derive(Clone, Copy)]
struct EntryTimestamps {
    created: DateTime,
    accessed: Date,
    modified: DateTime,
}

#[derive(Clone)]
struct Lookup {
    path: String,
    is_dir: bool,
    attrs: FileAttributes,
    len: u64,
    times: EntryTimestamps,
}

pub struct FatScheme<D: Read + Write + Seek> {
    mounted_path: String,
    fs: FileSystem<BufStream<D>>,
    next_id: AtomicUsize,
    handles: BTreeMap<usize, Handle>,
}

impl<D: Read + Write + Seek> FatScheme<D> {
    pub fn new(_scheme_name: String, mounted_path: String, fs: FileSystem<BufStream<D>>) -> Self {
        Self {
            mounted_path,
            fs,
            next_id: AtomicUsize::new(1),
            handles: BTreeMap::new(),
        }
    }

    pub fn cleanup(self) -> Result<()> {
        let FatScheme { fs, .. } = self;
        fs.unmount().map_err(fat_error)
    }

    fn insert_handle(&mut self, handle: Handle) -> usize {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.handles.insert(id, handle);
        id
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
            Some((parent, child)) if !child.is_empty() => Ok((parent.to_string(), child.to_string())),
            None => Ok((String::new(), normalized)),
            _ => Err(Error::new(EINVAL)),
        }
    }

    fn make_fat_path(path: &str) -> String {
        Self::normalize_path(path).trim_start_matches('/').to_string()
    }

    fn fat_name_eq(lhs: &str, rhs: &str) -> bool {
        lhs.chars().flat_map(|c| c.to_uppercase())
            .eq(rhs.chars().flat_map(|c| c.to_uppercase()))
    }

    fn synthetic_inode(path: &str, is_dir: bool) -> u64 {
        if path.is_empty() {
            return 2;
        }

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        is_dir.hash(&mut hasher);
        path.hash(&mut hasher);

        let inode = hasher.finish();
        if inode == 0 {
            1
        } else {
            inode
        }
    }

    fn inode_usize(path: &str, is_dir: bool) -> usize {
        match usize::try_from(Self::synthetic_inode(path, is_dir)) {
            Ok(value) => value,
            Err(_) => usize::MAX,
        }
    }

    fn dos_epoch() -> DateTime {
        DateTime {
            date: Date {
                year: 1980,
                month: 1,
                day: 1,
            },
            time: Time {
                hour: 0,
                min: 0,
                sec: 0,
                millis: 0,
            },
        }
    }

    fn root_lookup() -> Lookup {
        Lookup {
            path: String::new(),
            is_dir: true,
            attrs: FileAttributes::DIRECTORY,
            len: 0,
            times: EntryTimestamps {
                created: Self::dos_epoch(),
                accessed: Self::dos_epoch().date,
                modified: Self::dos_epoch(),
            },
        }
    }

    fn lookup_from_entry(path: String, entry: &fatfs::DirEntry<'_, BufStream<D>>) -> Lookup {
        Lookup {
            path,
            is_dir: entry.is_dir(),
            attrs: entry.attributes(),
            len: entry.len(),
            times: EntryTimestamps {
                created: entry.created(),
                accessed: entry.accessed(),
                modified: entry.modified(),
            },
        }
    }

    fn check_permission(mode: u16, ctx: &CallerCtx, perm: u16) -> bool {
        if ctx.uid == 0 {
            return true;
        }

        let granted = if ctx.gid == 0 {
            (mode >> 3) & 0o7
        } else {
            mode & 0o7
        };

        granted & perm == perm
    }

    fn require_permission(lookup: &Lookup, ctx: &CallerCtx, perm: u16) -> Result<()> {
        if Self::check_permission(Self::mode_from_lookup(lookup), ctx, perm) {
            Ok(())
        } else {
            Err(Error::new(EACCES))
        }
    }

    fn lookup_path(&self, path: &str, ctx: &CallerCtx) -> Result<Option<Lookup>> {
        let normalized = Self::normalize_path(path);
        if normalized.is_empty() {
            return Ok(Some(Self::root_lookup()));
        }

        let mut current_lookup = Self::root_lookup();
        let mut current_dir = self.fs.root_dir();
        let mut components = normalized.split('/').peekable();

        while let Some(component) = components.next() {
            Self::require_permission(&current_lookup, ctx, PERM_EXEC)?;

            let mut found = None;
            for entry in current_dir.iter() {
                let entry = entry.map_err(fat_error)?;
                if Self::fat_name_eq(&entry.file_name(), component) {
                    found = Some(entry);
                    break;
                }
            }

            let Some(entry) = found else {
                return Ok(None);
            };

            let entry_name = entry.file_name();
            let next_path = if current_lookup.path.is_empty() {
                entry_name
            } else {
                format!("{}/{}", current_lookup.path, entry_name)
            };

            let lookup = Self::lookup_from_entry(next_path, &entry);

            if components.peek().is_some() {
                if !lookup.is_dir {
                    return Err(Error::new(ENOTDIR));
                }
                current_dir = entry.to_dir();
                current_lookup = lookup;
                continue;
            }

            return Ok(Some(lookup));
        }

        Ok(None)
    }

    fn lookup_existing(&self, path: &str, ctx: &CallerCtx) -> Result<Lookup> {
        self.lookup_path(path, ctx)?.ok_or(Error::new(ENOENT))
    }

    fn open_dir_for_path(&self, path: &str) -> Result<Dir<'_, BufStream<D>>> {
        let fat_path = Self::make_fat_path(path);
        if fat_path.is_empty() {
            Ok(self.fs.root_dir())
        } else {
            self.fs.root_dir().open_dir(&fat_path).map_err(fat_error)
        }
    }

    fn open_file_for_path(&self, path: &str) -> Result<fatfs::File<'_, BufStream<D>>> {
        let fat_path = Self::make_fat_path(path);
        if fat_path.is_empty() {
            return Err(Error::new(EISDIR));
        }
        self.fs.root_dir().open_file(&fat_path).map_err(fat_error)
    }

    fn lookup_parent(&self, path: &str, ctx: &CallerCtx) -> Result<(Lookup, String)> {
        let (parent_path, child) = Self::split_parent_child(path)?;
        let parent = self.lookup_existing(&parent_path, ctx)?;
        if !parent.is_dir {
            return Err(Error::new(ENOTDIR));
        }
        Self::require_permission(&parent, ctx, PERM_EXEC | PERM_WRITE)?;
        Ok((parent, child))
    }

    fn directory_entries(&self, path: &str) -> Result<Vec<(u64, String, u8)>> {
        let dir = self.open_dir_for_path(path)?;
        let mut entries = Vec::new();

        for entry in dir.iter() {
            let entry = entry.map_err(fat_error)?;
            let name = entry.file_name();
            let child_path = Self::join_path(path, &name);
            let kind = if entry.is_dir() {
                KIND_DIRECTORY
            } else if entry.is_file() {
                KIND_REGULAR
            } else {
                KIND_UNSPECIFIED
            };

            entries.push((
                Self::synthetic_inode(&child_path, entry.is_dir()),
                name,
                kind,
            ));
        }

        Ok(entries)
    }

    fn create_directory_handle(&mut self, lookup: Lookup, flags: usize) -> Result<OpenResult> {
        let entries = self.directory_entries(&lookup.path)?;
        let id = self.insert_handle(Handle::Directory(DirectoryHandle::new(
            lookup.path,
            entries,
            flags,
        )));

        Ok(OpenResult::ThisScheme {
            number: id,
            flags: NewFdFlags::POSITIONED,
        })
    }

    fn create_file_handle(&mut self, path: String, flags: usize) -> OpenResult {
        let id = self.insert_handle(Handle::File(FileHandle::new(path, flags)));

        OpenResult::ThisScheme {
            number: id,
            flags: NewFdFlags::POSITIONED,
        }
    }

    fn handle_lookup_for_stat(&self, id: usize, ctx: &CallerCtx) -> Result<Lookup> {
        match self.handles.get(&id) {
            Some(Handle::SchemeRoot) => Ok(Self::root_lookup()),
            Some(Handle::Directory(handle)) => self.lookup_existing(handle.path(), ctx),
            Some(Handle::File(handle)) => self.lookup_existing(handle.path(), ctx),
            None => Err(Error::new(EBADF)),
        }
    }

    fn mode_from_lookup(lookup: &Lookup) -> u16 {
        let base = if lookup.is_dir {
            MODE_DIR | 0o755
        } else {
            MODE_FILE | 0o644
        };

        if lookup.attrs.contains(FileAttributes::READ_ONLY) {
            base & !0o222
        } else {
            base
        }
    }

    fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
        let adjusted_year = year - i64::from(month <= 2);
        let era = if adjusted_year >= 0 {
            adjusted_year / 400
        } else {
            (adjusted_year - 399) / 400
        };
        let yoe = adjusted_year - era * 400;
        let adjusted_month = month + if month > 2 { -3 } else { 9 };
        let doy = (153 * adjusted_month + 2) / 5 + day - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;

        era * 146_097 + doe - 719_468
    }

    fn date_time_to_unix(date_time: DateTime) -> (u64, u32) {
        let days = Self::days_from_civil(
            i64::from(date_time.date.year),
            i64::from(date_time.date.month),
            i64::from(date_time.date.day),
        );
        let seconds = days * 86_400
            + i64::from(date_time.time.hour) * 3_600
            + i64::from(date_time.time.min) * 60
            + i64::from(date_time.time.sec);

        if seconds <= 0 {
            (0, 0)
        } else {
            (seconds as u64, u32::from(date_time.time.millis) * 1_000_000)
        }
    }

    fn date_to_unix(date: Date) -> (u64, u32) {
        Self::date_time_to_unix(DateTime {
            date,
            time: Time {
                hour: 0,
                min: 0,
                sec: 0,
                millis: 0,
            },
        })
    }

    fn stat_from_lookup(&self, lookup: &Lookup, stat: &mut Stat) {
        *stat = Stat::default();
        stat.st_dev = 0;
        stat.st_ino = Self::synthetic_inode(&lookup.path, lookup.is_dir);
        stat.st_mode = Self::mode_from_lookup(lookup);
        stat.st_nlink = if lookup.is_dir { 2 } else { 1 };
        stat.st_uid = 0;
        stat.st_gid = 0;
        stat.st_size = lookup.len;
        stat.st_blksize = self.fs.cluster_size();
        stat.st_blocks = lookup.len.div_ceil(512);

        let (atime, atime_nsec) = Self::date_to_unix(lookup.times.accessed);
        let (mtime, mtime_nsec) = Self::date_time_to_unix(lookup.times.modified);
        let (ctime, ctime_nsec) = Self::date_time_to_unix(lookup.times.created);

        stat.st_atime = atime;
        stat.st_atime_nsec = atime_nsec;
        stat.st_mtime = mtime;
        stat.st_mtime_nsec = mtime_nsec;
        stat.st_ctime = ctime;
        stat.st_ctime_nsec = ctime_nsec;
    }

    fn dirent_kind_from_byte(kind: u8) -> DirentKind {
        match kind {
            KIND_DIRECTORY => DirentKind::Directory,
            KIND_REGULAR => DirentKind::Regular,
            _ => DirentKind::Unspecified,
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

    fn set_file_offset(&mut self, id: usize, offset: u64) -> Result<()> {
        match self.handles.get_mut(&id) {
            Some(Handle::File(handle)) => {
                handle.set_offset(offset);
                Ok(())
            }
            _ => Err(Error::new(EBADF)),
        }
    }
}

impl<D: Read + Write + Seek> SchemeSync for FatScheme<D> {
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

                if lookup.is_dir {
                    if flags & O_ACCMODE != O_RDONLY {
                        return Err(Error::new(EISDIR));
                    }
                    Self::require_permission(&lookup, ctx, PERM_READ)?;
                    return self.create_directory_handle(lookup, flags);
                }

                if flags & O_DIRECTORY == O_DIRECTORY {
                    return Err(Error::new(ENOTDIR));
                }

                if lookup.attrs.contains(FileAttributes::READ_ONLY)
                    && (flags & O_ACCMODE != O_RDONLY || flags & O_TRUNC == O_TRUNC)
                {
                    return Err(Error::new(EACCES));
                }

                if flags & O_ACCMODE != syscall::flag::O_WRONLY {
                    Self::require_permission(&lookup, ctx, PERM_READ)?;
                }
                if flags & O_ACCMODE != O_RDONLY {
                    Self::require_permission(&lookup, ctx, PERM_WRITE)?;
                }

                if flags & O_TRUNC == O_TRUNC {
                    let mut file = self.open_file_for_path(&resolved_path)?;
                    file.seek(SeekFrom::Start(0)).map_err(fat_error)?;
                    file.truncate().map_err(fat_error)?;
                    file.flush().map_err(fat_error)?;
                }

                Ok(self.create_file_handle(lookup.path, flags))
            }
            None => {
                if flags & O_CREAT != O_CREAT {
                    return Err(Error::new(ENOENT));
                }

                let (parent, child) = self.lookup_parent(&resolved_path, ctx)?;
                let parent_dir = self.open_dir_for_path(&parent.path)?;

                if flags & O_DIRECTORY == O_DIRECTORY {
                    parent_dir.create_dir(&child).map_err(fat_error)?;
                    drop(parent_dir);
                    let lookup = self.lookup_existing(&resolved_path, ctx)?;
                    self.create_directory_handle(lookup, flags)
                } else {
                    parent_dir.create_file(&child).map_err(fat_error)?;
                    drop(parent_dir);
                    let lookup = self.lookup_existing(&resolved_path, ctx)?;
                    Ok(self.create_file_handle(lookup.path, flags))
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
        let path = match self.handles.get(&id) {
            Some(Handle::File(handle)) => {
                Self::ensure_regular_file_access(handle, false)?;
                handle.path().to_string()
            }
            Some(Handle::Directory(_)) | Some(Handle::SchemeRoot) => return Err(Error::new(EISDIR)),
            None => return Err(Error::new(EBADF)),
        };

        let mut file = self.open_file_for_path(&path)?;
        file.seek(SeekFrom::Start(offset)).map_err(fat_error)?;
        let count = file.read(buf).map_err(fat_error)?;
        drop(file);
        self.set_file_offset(id, offset.saturating_add(count as u64))?;
        Ok(count)
    }

    fn write(
        &mut self,
        id: usize,
        buf: &[u8],
        offset: u64,
        _fcntl_flags: u32,
        ctx: &CallerCtx,
    ) -> Result<usize> {
        let path = match self.handles.get(&id) {
            Some(Handle::File(handle)) => {
                Self::ensure_regular_file_access(handle, true)?;
                let lookup = self.lookup_existing(handle.path(), ctx)?;
                if lookup.attrs.contains(FileAttributes::READ_ONLY) {
                    return Err(Error::new(EACCES));
                }
                handle.path().to_string()
            }
            Some(Handle::Directory(_)) | Some(Handle::SchemeRoot) => return Err(Error::new(EISDIR)),
            None => return Err(Error::new(EBADF)),
        };

        let mut file = self.open_file_for_path(&path)?;
        file.seek(SeekFrom::Start(offset)).map_err(fat_error)?;
        file.write_all(buf).map_err(fat_error)?;
        file.flush().map_err(fat_error)?;
        drop(file);
        self.set_file_offset(id, offset.saturating_add(buf.len() as u64))?;
        Ok(buf.len())
    }

    fn fsize(&mut self, id: usize, ctx: &CallerCtx) -> Result<u64> {
        match self.handles.get(&id) {
            Some(Handle::File(handle)) => {
                let _ = self.lookup_existing(handle.path(), ctx)?;
                let mut file = self.open_file_for_path(handle.path())?;
                file.seek(SeekFrom::End(0)).map_err(fat_error)
            }
            Some(Handle::Directory(handle)) => Ok(self.lookup_existing(handle.path(), ctx)?.len),
            Some(Handle::SchemeRoot) => Ok(0),
            None => Err(Error::new(EBADF)),
        }
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

        let stats = self.fs.stats().map_err(fat_error)?;
        stat.f_bsize = stats.cluster_size();
        stat.f_blocks = u64::from(stats.total_clusters());
        stat.f_bfree = u64::from(stats.free_clusters());
        stat.f_bavail = u64::from(stats.free_clusters());
        Ok(())
    }

    fn getdents<'buf>(
        &mut self,
        id: usize,
        mut buf: DirentBuf<&'buf mut [u8]>,
        opaque_offset: u64,
    ) -> Result<DirentBuf<&'buf mut [u8]>> {
        match self.handles.get_mut(&id) {
            Some(Handle::Directory(handle)) => {
                let start = opaque_offset as usize;
                handle.set_cursor(start);
                let mut cursor = start;

                for (index, (inode, name, kind)) in handle.entries().iter().enumerate().skip(start) {
                    buf.entry(DirEntry {
                        inode: *inode,
                        next_opaque_id: (index + 1) as u64,
                        name,
                        kind: Self::dirent_kind_from_byte(*kind),
                    })?;
                    cursor = index + 1;
                }

                handle.set_cursor(cursor);

                Ok(buf)
            }
            Some(Handle::SchemeRoot) => {
                let entries = self.directory_entries("")?;
                let start = opaque_offset as usize;

                for (index, (inode, name, kind)) in entries.iter().enumerate().skip(start) {
                    buf.entry(DirEntry {
                        inode: *inode,
                        next_opaque_id: (index + 1) as u64,
                        name,
                        kind: Self::dirent_kind_from_byte(*kind),
                    })?;
                }

                Ok(buf)
            }
            Some(Handle::File(_)) => Err(Error::new(ENOTDIR)),
            None => Err(Error::new(EBADF)),
        }
    }

    fn fsync(&mut self, id: usize, _ctx: &CallerCtx) -> Result<()> {
        match self.handles.get(&id) {
            Some(Handle::File(handle)) => {
                let mut file = self.open_file_for_path(handle.path())?;
                file.flush().map_err(fat_error)
            }
            Some(Handle::Directory(_)) | Some(Handle::SchemeRoot) => Ok(()),
            None => Err(Error::new(EBADF)),
        }
    }

    fn ftruncate(&mut self, id: usize, len: u64, ctx: &CallerCtx) -> Result<()> {
        let path = match self.handles.get(&id) {
            Some(Handle::File(handle)) => {
                Self::ensure_regular_file_access(handle, true)?;
                let lookup = self.lookup_existing(handle.path(), ctx)?;
                if lookup.attrs.contains(FileAttributes::READ_ONLY) {
                    return Err(Error::new(EACCES));
                }
                handle.path().to_string()
            }
            Some(Handle::Directory(_)) | Some(Handle::SchemeRoot) => return Err(Error::new(EISDIR)),
            None => return Err(Error::new(EBADF)),
        };

        let mut file = self.open_file_for_path(&path)?;
        let current_len = file.seek(SeekFrom::End(0)).map_err(fat_error)?;

        if len < current_len {
            file.seek(SeekFrom::Start(len)).map_err(fat_error)?;
            file.truncate().map_err(fat_error)?;
        } else if len > current_len {
            file.seek(SeekFrom::Start(current_len)).map_err(fat_error)?;
            let zeros = [0u8; 4096];
            let mut remaining = len - current_len;
            while remaining > 0 {
                let chunk = remaining.min(zeros.len() as u64) as usize;
                file.write_all(&zeros[..chunk]).map_err(fat_error)?;
                remaining -= chunk as u64;
            }
        }

        file.flush().map_err(fat_error)
    }

    fn unlinkat(&mut self, dirfd: usize, path: &str, flags: usize, ctx: &CallerCtx) -> Result<()> {
        let resolved_path = self.dirfd_base_path(dirfd, path)?;
        let lookup = self.lookup_existing(&resolved_path, ctx)?;
        let (parent, child) = self.lookup_parent(&resolved_path, ctx)?;
        let parent_dir = self.open_dir_for_path(&parent.path)?;

        if flags & AT_REMOVEDIR == AT_REMOVEDIR {
            if !lookup.is_dir {
                return Err(Error::new(ENOTDIR));
            }

            let dir = self.open_dir_for_path(&resolved_path)?;
            for entry in dir.iter() {
                let entry = entry.map_err(fat_error)?;
                let name = entry.file_name();
                if name != "." && name != ".." {
                    return Err(Error::new(ENOTEMPTY));
                }
            }
        } else if lookup.is_dir {
            return Err(Error::new(EISDIR));
        }

        parent_dir.remove(&child).map_err(fat_error)
    }

    fn frename(&mut self, id: usize, path: &str, ctx: &CallerCtx) -> Result<usize> {
        let source_path = match self.handles.get(&id) {
            Some(Handle::File(handle)) => handle.path().to_string(),
            Some(Handle::Directory(handle)) => handle.path().to_string(),
            Some(Handle::SchemeRoot) => return Err(Error::new(EOPNOTSUPP)),
            None => return Err(Error::new(EBADF)),
        };

        let _ = self.lookup_existing(&source_path, ctx)?;
        let resolved_path = Self::normalize_path(path);
        let (src_parent, src_child) = self.lookup_parent(&source_path, ctx)?;
        let (dst_parent, dst_child) = self.lookup_parent(&resolved_path, ctx)?;
        let src_parent_dir = self.open_dir_for_path(&src_parent.path)?;
        let dst_parent_dir = self.open_dir_for_path(&dst_parent.path)?;

        src_parent_dir
            .rename(&src_child, &dst_parent_dir, &dst_child)
            .map_err(fat_error)?;

        drop(src_parent_dir);
        drop(dst_parent_dir);

        let new_path = resolved_path;
        match self.handles.get_mut(&id) {
            Some(Handle::File(handle)) => handle.update_path(new_path),
            Some(Handle::Directory(handle)) => handle.update_path(new_path),
            _ => {}
        }

        Ok(0)
    }

    fn on_close(&mut self, id: usize) {
        let _ = self.handles.remove(&id);
    }

    fn on_sendfd(&mut self, _sendfd_request: &SendFdRequest) -> Result<usize> {
        Err(Error::new(EPERM))
    }

    fn inode(&self, id: usize) -> Result<usize> {
        match self.handles.get(&id) {
            Some(Handle::File(handle)) => Ok(Self::inode_usize(handle.path(), false)),
            Some(Handle::Directory(handle)) => Ok(Self::inode_usize(handle.path(), true)),
            Some(Handle::SchemeRoot) => Ok(2),
            None => Err(Error::new(EBADF)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test normalize_path
    #[test]
    fn test_normalize_path_empty() {
        assert_eq!(FatScheme::<std::io::Cursor<Vec<u8>>>::normalize_path(""), "");
    }

    #[test]
    fn test_normalize_path_root() {
        assert_eq!(FatScheme::<std::io::Cursor<Vec<u8>>>::normalize_path("/"), "");
    }

    #[test]
    fn test_normalize_path_simple() {
        assert_eq!(FatScheme::<std::io::Cursor<Vec<u8>>>::normalize_path("/a/b"), "a/b");
    }

    #[test]
    fn test_normalize_path_dot_dot() {
        assert_eq!(
            FatScheme::<std::io::Cursor<Vec<u8>>>::normalize_path("a/./b/../c"),
            "a/c"
        );
    }

    #[test]
    fn test_normalize_path_leading_slashes() {
        assert_eq!(FatScheme::<std::io::Cursor<Vec<u8>>>::normalize_path("///a//b///"), "a/b");
    }

    #[test]
    fn test_normalize_path_dot_only() {
        assert_eq!(FatScheme::<std::io::Cursor<Vec<u8>>>::normalize_path("."), "");
    }

    #[test]
    fn test_normalize_path_dot_dot_only() {
        assert_eq!(FatScheme::<std::io::Cursor<Vec<u8>>>::normalize_path(".."), "");
    }

    #[test]
    fn test_normalize_path_up_from_root() {
        // ../.. from root pops both, leaving empty
        assert_eq!(FatScheme::<std::io::Cursor<Vec<u8>>>::normalize_path("../.."), "");
    }

    // Test split_parent_child
    #[test]
    fn test_split_parent_child_single() {
        let (parent, child) = FatScheme::<std::io::Cursor<Vec<u8>>>::split_parent_child("a").unwrap();
        assert_eq!(parent, "");
        assert_eq!(child, "a");
    }

    #[test]
    fn test_split_parent_child_two_levels() {
        let (parent, child) = FatScheme::<std::io::Cursor<Vec<u8>>>::split_parent_child("a/b").unwrap();
        assert_eq!(parent, "a");
        assert_eq!(child, "b");
    }

    #[test]
    fn test_split_parent_child_three_levels() {
        let (parent, child) = FatScheme::<std::io::Cursor<Vec<u8>>>::split_parent_child("a/b/c").unwrap();
        assert_eq!(parent, "a/b");
        assert_eq!(child, "c");
    }

    #[test]
    fn test_split_parent_child_empty_error() {
        let result = FatScheme::<std::io::Cursor<Vec<u8>>>::split_parent_child("/");
        assert!(result.is_err());
    }

    // Test fat_name_eq
    #[test]
    fn test_fat_name_eq_exact() {
        assert!(FatScheme::<std::io::Cursor<Vec<u8>>>::fat_name_eq("foo.txt", "foo.txt"));
    }

    #[test]
    fn test_fat_name_eq_case_insensitive() {
        assert!(FatScheme::<std::io::Cursor<Vec<u8>>>::fat_name_eq("foo.txt", "FOO.TXT"));
        assert!(FatScheme::<std::io::Cursor<Vec<u8>>>::fat_name_eq("TEST", "test"));
        assert!(FatScheme::<std::io::Cursor<Vec<u8>>>::fat_name_eq("MixedCase", "mixedcase"));
    }

    #[test]
    fn test_fat_name_eq_not_equal() {
        assert!(!FatScheme::<std::io::Cursor<Vec<u8>>>::fat_name_eq("foo.txt", "bar.txt"));
    }

    #[test]
    fn test_fat_name_eq_different_lengths() {
        assert!(!FatScheme::<std::io::Cursor<Vec<u8>>>::fat_name_eq("foo", "foobar"));
    }

    // Test synthetic_inode determinism
    #[test]
    fn test_synthetic_inode_determinism() {
        let inode1 = FatScheme::<std::io::Cursor<Vec<u8>>>::synthetic_inode("test/path", false);
        let inode2 = FatScheme::<std::io::Cursor<Vec<u8>>>::synthetic_inode("test/path", false);
        assert_eq!(inode1, inode2);
    }

    #[test]
    fn test_synthetic_inode_different_paths() {
        let inode1 = FatScheme::<std::io::Cursor<Vec<u8>>>::synthetic_inode("path/a", false);
        let inode2 = FatScheme::<std::io::Cursor<Vec<u8>>>::synthetic_inode("path/b", false);
        assert_ne!(inode1, inode2);
    }

    #[test]
    fn test_synthetic_inode_dir_vs_file() {
        let inode_dir = FatScheme::<std::io::Cursor<Vec<u8>>>::synthetic_inode("test/path", true);
        let inode_file = FatScheme::<std::io::Cursor<Vec<u8>>>::synthetic_inode("test/path", false);
        assert_ne!(inode_dir, inode_file);
    }

    #[test]
    fn test_synthetic_inode_non_zero() {
        // Empty path returns 2, non-empty should never return 0 due to fallback
        let inode = FatScheme::<std::io::Cursor<Vec<u8>>>::synthetic_inode("nonempty", false);
        assert_ne!(inode, 0);
    }

    #[test]
    fn test_synthetic_inode_empty_returns_two() {
        let inode = FatScheme::<std::io::Cursor<Vec<u8>>>::synthetic_inode("", false);
        assert_eq!(inode, 2);
    }

    // Test days_from_civil with known dates
    #[test]
    fn test_days_from_civil_epoch() {
        // Unix epoch: 1970-01-01 should be day 0
        let days = FatScheme::<std::io::Cursor<Vec<u8>>>::days_from_civil(1970, 1, 1);
        assert_eq!(days, 0);
    }

    #[test]
    fn test_days_from_civil_dos_epoch() {
        let days = FatScheme::<std::io::Cursor<Vec<u8>>>::days_from_civil(1980, 1, 1);
        assert_eq!(days, 3652);
    }

    #[test]
    fn test_days_from_civil_negative() {
        // 1969-01-01 should be -365 (before epoch, no leap days in 1969)
        let days = FatScheme::<std::io::Cursor<Vec<u8>>>::days_from_civil(1969, 1, 1);
        assert_eq!(days, -365);
    }

    #[test]
    fn test_days_from_civil_2024_leap_year() {
        // 2024 is a leap year. Jan 1 to Dec 31 is 366 days.
        // Days from 1970 to 2024-01-01: count years, add leap days
        // 1970-2023: 54 years. Leap years: 1972,1976,1980,...,2020 (every 4 years, but 1900 not in range)
        // Count: 1972,1976,1980,1984,1988,1992,1996,2000,2004,2008,2012,2016,2020 = 13 leap years
        // 54*365 + 13 = 19710 + 13 = 19723
        // Jan 1 2024 adds 0 more days = 19723
        let days = FatScheme::<std::io::Cursor<Vec<u8>>>::days_from_civil(2024, 1, 1);
        assert_eq!(days, 19723);
    }
}

fn fat_error(err: io::Error) -> Error {
    match err.kind() {
        io::ErrorKind::NotFound => Error::new(ENOENT),
        io::ErrorKind::AlreadyExists => Error::new(EEXIST),
        io::ErrorKind::InvalidInput => Error::new(EINVAL),
        io::ErrorKind::PermissionDenied => Error::new(EACCES),
        io::ErrorKind::WriteZero => Error::new(syscall::error::ENOSPC),
        io::ErrorKind::UnexpectedEof => Error::new(syscall::error::EIO),
        _ => {
            let message = err.to_string().to_ascii_lowercase();
            if message.contains("is a directory") {
                Error::new(EISDIR)
            } else if message.contains("not a directory") {
                Error::new(ENOTDIR)
            } else if message.contains("directory not empty") {
                Error::new(ENOTEMPTY)
            } else if message.contains("file name") {
                Error::new(EINVAL)
            } else {
                Error::new(syscall::error::EIO)
            }
        }
    }
}
