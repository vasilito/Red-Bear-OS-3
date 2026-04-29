use std::convert::{TryFrom, TryInto};
use std::os::unix::io::AsRawFd;
use std::{mem, str};

use scheme_utils::{FpathWriter, HandleMap};
use syscall::dirent::{DirEntry, DirentBuf, DirentKind};
use syscall::error::{
    EACCES, EBADF, EBADFD, EEXIST, EINVAL, EIO, EISDIR, ENOMEM, ENOSYS, ENOTDIR, ENOTEMPTY,
    EOPNOTSUPP, EOVERFLOW, EPERM,
};
use syscall::flag::{
    StdFsCallKind, O_ACCMODE, O_CREAT, O_DIRECTORY, O_EXCL, O_RDONLY, O_RDWR, O_STAT, O_TRUNC,
    O_WRONLY,
};
use syscall::schemev2::NewFdFlags;
use syscall::{Error, EventFlags, Result, Stat, StatVfs, StdFsCallMeta, TimeSpec, ENOENT};
use syscall::{MODE_DIR, MODE_FILE, MODE_PERM, MODE_TYPE};

use indexmap::IndexMap;

use redox_scheme::scheme::SchemeSync;
use redox_scheme::{CallerCtx, OpenResult};

use crate::filesystem::{self, File, FileData, Filesystem, Inode};

enum Handle {
    Inode(usize),
    SchemeRoot,
}

pub struct Scheme {
    scheme_name: String,
    filesystem: Filesystem,
    handles: HandleMap<Handle>,
    proc_creds_capability: libredox::Fd,
}
impl Scheme {
    /// Create the scheme, with the name being used for `fpath`.
    pub fn new(scheme_name: String) -> Result<Self> {
        Ok(Self {
            scheme_name,
            filesystem: Filesystem::new()?,
            handles: HandleMap::new(),
            proc_creds_capability: {
                libredox::Fd::open(
                    "/scheme/proc/proc-creds-capability",
                    libredox::flag::O_RDONLY,
                    0,
                )?
            },
        })
    }
    /// Remove a directory entry, where the entry can be both a file or a directory. Used by `unlinkat`.
    pub fn remove_dentry(&mut self, path: &str, uid: u32, gid: u32, directory: bool) -> Result<()> {
        let removed_inode = {
            let (parent_dir_inode, name_to_delete) =
                self.filesystem.resolve_except_last(path, uid, gid)?;
            let name_to_delete = name_to_delete.ok_or(Error::new(EINVAL))?; // can't remove root
            let parent = self
                .filesystem
                .files
                .get_mut(&parent_dir_inode)
                .ok_or(Error::new(EIO))?;

            let mode = current_perm(parent, uid, gid);
            if mode & 0o2 == 0 {
                return Err(Error::new(EACCES));
            }

            let FileData::Directory(ref mut dentries) = parent.data else {
                return Err(Error::new(ENOTDIR));
            };

            let Inode(entry_inode) = dentries
                .shift_remove(name_to_delete)
                .ok_or(Error::new(ENOENT))?;

            if let Some(File {
                data: FileData::Directory(ref data),
                ..
            }) = self.filesystem.files.get(&entry_inode)
            {
                if !directory {
                    return Err(Error::new(EISDIR));
                } else if !data.is_empty() {
                    return Err(Error::new(ENOTEMPTY));
                }
                let parent = self
                    .filesystem
                    .files
                    .get_mut(&parent_dir_inode)
                    .ok_or(Error::new(EIO))?;
                parent.nlink -= 1; // '..' of subdirectory
            }

            entry_inode
        };

        let removed_inode_info = self
            .filesystem
            .files
            .get_mut(&removed_inode)
            .ok_or(Error::new(EIO))?;

        if let FileData::File(_) = removed_inode_info.data {
            if directory {
                return Err(Error::new(EISDIR));
            }
            removed_inode_info.nlink -= 1; // only the parent entry
        } else {
            if !directory {
                return Err(Error::new(ENOTDIR));
            }
            removed_inode_info.nlink -= 2; // both the parent entry and '.'
        }

        if removed_inode_info.nlink == 0 && removed_inode_info.open_handles == 0 {
            self.filesystem.files.remove(&removed_inode);
        }

        Ok(())
    }

    fn open_existing(&mut self, path: &str, flags: usize, uid: u32, gid: u32) -> Result<Inode> {
        let inode = self.filesystem.resolve(path, uid, gid)?;
        let file = self
            .filesystem
            .files
            .get_mut(&inode)
            .ok_or(Error::new(EIO))?;

        if flags & O_STAT == 0 && flags & O_DIRECTORY != 0 && file.mode & MODE_TYPE != MODE_DIR {
            return Err(Error::new(ENOTDIR));
        }

        // Unlike on Linux, which allows directories to be opened without O_DIRECTORY, Redox has no
        // getdents(2) syscall, and thus it adds the additional restriction that directories have
        // to be opened with O_DIRECTORY, if they aren't opened with O_STAT to check whether it's a
        // directory.
        if flags & O_STAT == 0 && flags & O_DIRECTORY == 0 && file.mode & MODE_TYPE == MODE_DIR {
            return Err(Error::new(EISDIR));
        }

        let current_perm = current_perm(file, uid, gid);
        check_permissions(flags, current_perm)?;

        let opened_as_write = flags & O_ACCMODE == O_WRONLY || flags & O_ACCMODE == O_RDWR;

        if flags & O_TRUNC == O_TRUNC && opened_as_write {
            match file.data {
                // file.data and file.mode should match
                FileData::Directory(_) => return Err(Error::new(EBADFD)),

                // If we opened an existing file with O_CREAT and O_TRUNC
                FileData::File(ref mut data) => data.clear(),
            }
        }

        file.open_handles += 1;

        Ok(Inode(inode))
    }
}

impl SchemeSync for Scheme {
    fn scheme_root(&mut self) -> Result<usize> {
        Ok(self.handles.insert(Handle::SchemeRoot))
    }
    fn openat(
        &mut self,
        dirfd: usize,
        path: &str,
        flags: usize,
        fcntl_flags: u32,
        ctx: &CallerCtx,
    ) -> Result<OpenResult> {
        if !matches!(self.handles.get(dirfd)?, Handle::SchemeRoot) {
            return Err(Error::new(EACCES));
        }

        let exists = self.filesystem.resolve(path, 0, 0).is_ok();
        if flags & O_CREAT != 0 && flags & O_EXCL != 0 && exists {
            return Err(Error::new(EEXIST));
        }

        let inode = if flags & O_CREAT != 0 && exists {
            self.open_existing(path, flags, ctx.uid, ctx.gid)?.0
        } else if flags & O_CREAT != 0 {
            if flags & O_STAT != 0 {
                return Err(Error::new(EINVAL));
            }

            let (parent_dir_inode, new_name) = self
                .filesystem
                .resolve_except_last(path, ctx.uid, ctx.gid)?;
            let new_name = new_name.ok_or(Error::new(EINVAL))?; // cannot mkdir /

            let current_time = filesystem::current_time();

            let new_inode_number = self.filesystem.next_inode_number()?;

            let mut mode = (flags & 0xFFFF) as u16;

            let new_inode = if flags & O_DIRECTORY != 0 {
                if mode & MODE_TYPE == 0 {
                    mode |= MODE_DIR
                }
                if mode & MODE_TYPE != MODE_DIR {
                    return Err(Error::new(EINVAL));
                }

                File {
                    atime: current_time,
                    ctime: current_time,
                    mtime: current_time,
                    gid: ctx.gid,
                    uid: ctx.uid,
                    mode,
                    nlink: 2, // parent entry, "."
                    data: FileData::Directory(IndexMap::new()),
                    open_handles: 1,
                    parent: Inode(parent_dir_inode),
                }
            } else {
                if mode & MODE_TYPE == 0 {
                    mode |= MODE_FILE
                }
                if mode & MODE_TYPE == MODE_DIR {
                    return Err(Error::new(EINVAL));
                }

                File {
                    atime: current_time,
                    ctime: current_time,
                    mtime: current_time,
                    gid: ctx.gid,
                    uid: ctx.uid,
                    mode,
                    nlink: 1,
                    data: FileData::File(Vec::new()),
                    open_handles: 1,
                    parent: Inode(parent_dir_inode),
                }
            };
            let current_perm = current_perm(&new_inode, ctx.uid, ctx.gid);
            check_permissions(flags, current_perm)?;

            self.filesystem.files.insert(new_inode_number, new_inode);

            let parent_file = self
                .filesystem
                .files
                .get_mut(&parent_dir_inode)
                .ok_or(Error::new(EIO))?;
            match parent_file.data {
                FileData::File(_) => return Err(Error::new(EIO)),
                FileData::Directory(ref mut entries) => {
                    entries.insert(new_name.to_owned(), Inode(new_inode_number));
                }
            }

            new_inode_number
        } else {
            self.open_existing(path, flags | fcntl_flags as usize, ctx.uid, ctx.gid)?
                .0
        };
        let new_id = self.handles.insert(Handle::Inode(inode));
        Ok(OpenResult::ThisScheme {
            number: new_id,
            flags: NewFdFlags::POSITIONED,
        })
    }
    fn unlinkat(&mut self, dirfd: usize, path: &str, flags: usize, ctx: &CallerCtx) -> Result<()> {
        {
            if !matches!(self.handles.get(dirfd)?, Handle::SchemeRoot) {
                return Err(Error::new(EACCES));
            }
        }
        self.remove_dentry(
            path,
            ctx.uid,
            ctx.gid,
            flags & syscall::AT_REMOVEDIR == syscall::AT_REMOVEDIR,
        )
    }
    fn read(
        &mut self,
        fd: usize,
        buf: &mut [u8],
        offset: u64,
        fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let Ok(offset) = usize::try_from(offset) else {
            return Ok(0);
        };
        let inode = match self.handles.get(fd)? {
            Handle::Inode(inode) => inode,
            Handle::SchemeRoot => return Err(Error::new(EISDIR)),
        };
        let file = self
            .filesystem
            .files
            .get_mut(&inode)
            .ok_or(Error::new(EBADFD))?;

        if !matches!((fcntl_flags as usize) & O_ACCMODE, O_RDONLY | O_RDWR) {
            return Err(Error::new(EBADF));
        }

        match file.data {
            FileData::File(ref bytes) => {
                if file.mode & MODE_TYPE == MODE_DIR {
                    return Err(Error::new(EBADFD));
                }
                let src_bytes = bytes.get(offset..).unwrap_or(&[]);
                let bytes_to_read = src_bytes.len().min(buf.len());
                buf[..bytes_to_read].copy_from_slice(&src_bytes[..bytes_to_read]);
                Ok(bytes_to_read)
            }
            FileData::Directory(_) => return Err(Error::new(EISDIR)),
        }
    }
    fn getdents<'buf>(
        &mut self,
        fd: usize,
        mut buf: DirentBuf<&'buf mut [u8]>,
        opaque_offset: u64,
    ) -> Result<DirentBuf<&'buf mut [u8]>> {
        let Ok(offset) = usize::try_from(opaque_offset) else {
            return Ok(buf);
        };
        let inode = match self.handles.get(fd)? {
            Handle::Inode(inode) => inode,
            Handle::SchemeRoot => return Err(Error::new(EISDIR)),
        };
        let file = self
            .filesystem
            .files
            .get_mut(&inode)
            .ok_or(Error::new(EBADFD))?;

        let FileData::Directory(ref dir) = file.data else {
            return Err(Error::new(ENOTDIR));
        };

        for (i, (dent_name, Inode(dent_inode))) in dir.iter().enumerate().skip(offset) {
            buf.entry(DirEntry {
                inode: *dent_inode as u64,
                name: dent_name,
                kind: DirentKind::Unspecified,
                next_opaque_id: i as u64 + 1,
            })?;
        }
        Ok(buf)
    }
    fn write(
        &mut self,
        fd: usize,
        buf: &[u8],
        offset: u64,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let Ok(offset) = usize::try_from(offset) else {
            return Ok(0);
        };
        let inode = match self.handles.get(fd)? {
            Handle::Inode(inode) => inode,
            Handle::SchemeRoot => return Err(Error::new(EISDIR)),
        };
        let file = self
            .filesystem
            .files
            .get_mut(&inode)
            .ok_or(Error::new(EBADFD))?;

        if let &mut FileData::File(ref mut bytes) = &mut file.data {
            if file.mode & MODE_TYPE == MODE_DIR {
                return Err(Error::new(EBADFD));
            }

            // if there's a seek hole, fill it with 0 and continue writing.
            let end_off = offset.checked_add(buf.len()).ok_or(Error::new(EOVERFLOW))?;
            if end_off > bytes.len() {
                let additional = end_off - bytes.len();
                bytes.try_reserve(additional).or(Err(Error::new(ENOMEM)))?;
                bytes.resize(end_off, 0u8);
            }
            bytes[offset..][..buf.len()].copy_from_slice(buf);

            Ok(buf.len())
        } else {
            Err(Error::new(EISDIR))
        }
    }
    fn fchmod(&mut self, fd: usize, mode: u16, _ctx: &CallerCtx) -> Result<()> {
        let inode = match self.handles.get(fd)? {
            Handle::Inode(inode) => inode,
            Handle::SchemeRoot => return Err(Error::new(EISDIR)),
        };
        let file = self
            .filesystem
            .files
            .get_mut(&inode)
            .ok_or(Error::new(EBADFD))?;

        let cur_type = file.mode & MODE_TYPE;

        /*
        if mode & MODE_TYPE != 0 {
            return Err(Error::new(EINVAL));
        }
        */

        file.mode = mode | cur_type;

        Ok(())
    }
    fn fchown(&mut self, inode: usize, uid: u32, gid: u32, _ctx: &CallerCtx) -> Result<()> {
        let file = self
            .filesystem
            .files
            .get_mut(&inode)
            .ok_or(Error::new(EBADFD))?;

        file.uid = uid;
        file.gid = gid;

        Ok(())
    }
    fn fcntl(
        &mut self,
        _inode: usize,
        _cmd: usize,
        _arg: usize,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        Ok(0)
    }
    fn fevent(
        &mut self,
        _inode: usize,
        _flags: EventFlags,
        _ctx: &CallerCtx,
    ) -> Result<EventFlags> {
        // TODO?
        Err(Error::new(ENOSYS))
    }
    fn mmap_prep(
        &mut self,
        _inode: usize,
        _offset: u64,
        _size: usize,
        _flags: syscall::MapFlags,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        // TODO
        Err(Error::new(ENOSYS))
    }
    fn fpath(&mut self, fd: usize, buf: &mut [u8], _ctx: &CallerCtx) -> Result<usize> {
        FpathWriter::with(buf, &self.scheme_name, |w| {
            let mut current_inode = match *self.handles.get(fd)? {
                Handle::Inode(inode) => inode,
                Handle::SchemeRoot => return Err(Error::new(EISDIR)),
            };

            let mut chain = Vec::new();

            let mut current_info = self
                .filesystem
                .files
                .get(&current_inode)
                .ok_or(Error::new(EBADFD))?;

            while current_inode != Filesystem::ROOT_INODE {
                let parent_info = self
                    .filesystem
                    .files
                    .get(&current_info.parent.0)
                    .ok_or(Error::new(EBADFD))?;

                let FileData::Directory(ref dir) = parent_info.data else {
                    return Err(Error::new(EBADFD));
                };
                // TODO: error handling?
                let (name, _) = dir
                    .iter()
                    .find(|(_name, inode)| inode.0 == current_inode)
                    .ok_or(Error::new(ENOENT))?;
                chain.push(&**name);

                current_inode = current_info.parent.0;
                current_info = parent_info;
            }

            for (i, component) in chain.iter().copied().rev().enumerate() {
                if i != 0 {
                    w.push_str("/");
                }
                w.push_str(component);
            }
            Ok(())
        })
    }
    fn frename(&mut self, _inode: usize, _path: &str, _ctx: &CallerCtx) -> Result<usize> {
        // TODO
        Err(Error::new(ENOSYS))
    }
    fn fstat(&mut self, fd: usize, stat: &mut Stat, _ctx: &CallerCtx) -> Result<()> {
        let inode = match *self.handles.get(fd)? {
            Handle::Inode(inode) => inode,
            Handle::SchemeRoot => return Err(Error::new(EISDIR)),
        };

        let block_size = self.filesystem.block_size();
        let file = self
            .filesystem
            .files
            .get_mut(&inode)
            .ok_or(Error::new(EBADFD))?;

        let size = file.data.size().try_into().or(Err(Error::new(EOVERFLOW)))?;

        *stat = Stat {
            st_mode: file.mode,
            st_uid: file.uid,
            st_gid: file.gid,
            st_ino: inode.try_into().map_err(|_| Error::new(EOVERFLOW))?,
            st_nlink: file.nlink.try_into().or(Err(Error::new(EOVERFLOW)))?,
            st_dev: 0,

            st_size: size,
            st_blksize: block_size,
            st_blocks: size.next_multiple_of(u64::from(block_size)),

            st_atime: file
                .atime
                .tv_sec
                .try_into()
                .or(Err(Error::new(EOVERFLOW)))?,
            st_atime_nsec: file
                .atime
                .tv_nsec
                .try_into()
                .or(Err(Error::new(EOVERFLOW)))?,

            st_ctime: file
                .ctime
                .tv_sec
                .try_into()
                .or(Err(Error::new(EOVERFLOW)))?,
            st_ctime_nsec: file
                .ctime
                .tv_nsec
                .try_into()
                .or(Err(Error::new(EOVERFLOW)))?,

            st_mtime: file
                .mtime
                .tv_sec
                .try_into()
                .or(Err(Error::new(EOVERFLOW)))?,
            st_mtime_nsec: file
                .mtime
                .tv_nsec
                .try_into()
                .or(Err(Error::new(EOVERFLOW)))?,
        };

        Ok(())
    }
    fn fstatvfs(&mut self, _inode: usize, stat: &mut StatVfs, _ctx: &CallerCtx) -> Result<()> {
        let abi_stat = libredox::call::fstatvfs(self.filesystem.memory_file.as_raw_fd() as usize)?;
        // TODO: From impl
        *stat = StatVfs {
            f_bavail: abi_stat.f_bavail as u64,
            f_bfree: abi_stat.f_bfree as u64,
            f_blocks: abi_stat.f_blocks as u64,
            f_bsize: abi_stat.f_bsize as u32,
        };

        Ok(())
    }
    fn fsync(&mut self, _inode: usize, _ctx: &CallerCtx) -> Result<()> {
        Ok(())
    }
    fn ftruncate(&mut self, fd: usize, size: u64, _ctx: &CallerCtx) -> Result<()> {
        let inode = match self.handles.get(fd)? {
            Handle::Inode(inode) => inode,
            Handle::SchemeRoot => return Err(Error::new(EISDIR)),
        };
        let file = self
            .filesystem
            .files
            .get_mut(&inode)
            .ok_or(Error::new(EBADFD))?;

        if file.mode & MODE_TYPE == MODE_DIR {
            return Err(Error::new(EISDIR));
        }
        let size = usize::try_from(size).map_err(|_| Error::new(EOVERFLOW))?;
        match &mut file.data {
            &mut FileData::File(ref mut bytes) => {
                if size > bytes.len() {
                    let additional = size - bytes.len();
                    bytes.try_reserve(additional).or(Err(Error::new(ENOMEM)))?;
                    bytes.resize(size, 0u8)
                } else {
                    bytes.resize(size, 0u8)
                }
            }
            &mut FileData::Directory(_) => return Err(Error::new(EBADFD)),
        }
        Ok(())
    }
    fn futimens(&mut self, fd: usize, times: &[TimeSpec], _ctx: &CallerCtx) -> Result<()> {
        let inode = match self.handles.get(fd)? {
            Handle::Inode(inode) => inode,
            Handle::SchemeRoot => return Err(Error::new(EISDIR)),
        };
        let file = self
            .filesystem
            .files
            .get_mut(&inode)
            .ok_or(Error::new(EBADFD))?;

        let new_atime = *times.get(0).ok_or(Error::new(EINVAL))?;
        let new_mtime = *times.get(1).ok_or(Error::new(EINVAL))?;

        file.atime = new_atime;
        file.mtime = new_mtime;

        Ok(())
    }

    fn std_fs_call(
        &mut self,
        id: usize,
        kind: StdFsCallKind,
        payload: &mut [u8],
        metadata: StdFsCallMeta,
        ctx: &CallerCtx,
    ) -> Result<usize> {
        match kind {
            StdFsCallKind::Fchown => {
                let (new_uid, new_gid) = (metadata.arg1 as u32, metadata.arg1 >> 32 as u32);
                let (_pid, uid, gid) = get_uid_gid_from_pid(&self.proc_creds_capability, ctx.pid)?;
                if uid != 0 && (uid != ctx.uid || gid != ctx.gid) {
                    return Err(Error::new(EPERM));
                }
                self.fchown(id, new_uid, new_gid as u32, ctx).map(|_| 0)
            }
            /* TODO: Support Unlinkat using std_fs_call
            Unlinkat => {
                let path = unsafe { str::from_utf8_unchecked(payload) };
                let flags = metadata.arg1;                {
                    if !matches!(
                        self.handles.get(&id).ok_or(Error::new(EBADF))?,
                        Handle::SchemeRoot
                    ) {
                        return Err(Error::new(EACCES));
                    }
                }
                let (_pid, uid, gid) = get_uid_gid_from_pid(&self.proc_creds_capability, ctx.pid)?;
                self.remove_dentry(
                    path,
                    uid,
                    gid,
                    *flags as usize & syscall::AT_REMOVEDIR == syscall::AT_REMOVEDIR,
                )
                .map(|_| 0)
            }
            */
            _ => Err(Error::new(EOPNOTSUPP)),
        }
    }

    fn on_close(&mut self, fd: usize) {
        let Some(Handle::Inode(inode)) = self.handles.remove(fd) else {
            return;
        };
        let Some(inode_info) = self.filesystem.files.get_mut(&inode) else {
            return;
        };

        inode_info.open_handles -= 1;

        if inode_info.nlink == 0 && inode_info.open_handles == 0 {
            self.filesystem.files.remove(&inode);
        }
    }
}
pub fn current_perm(file: &crate::filesystem::File, uid: u32, gid: u32) -> u8 {
    let perm = file.mode & MODE_PERM;

    if uid == 0 {
        // root doesn't have to be checked
        0o7
    } else if uid == file.uid {
        ((perm & 0o700) >> 6) as u8
    } else if gid == file.gid {
        ((perm & 0o70) >> 3) as u8
    } else {
        (perm & 0o7) as u8
    }
}
fn check_permissions(flags: usize, single_mode: u8) -> Result<()> {
    if flags & O_ACCMODE == O_RDONLY && single_mode & 0o4 == 0 {
        return Err(Error::new(EACCES));
    } else if flags & O_ACCMODE == O_WRONLY && single_mode & 0o2 == 0 {
        return Err(Error::new(EACCES));
    } else if flags & O_ACCMODE == O_RDWR && single_mode & 0o6 != 0o6 {
        return Err(Error::new(EACCES));
    }
    Ok(())
}

fn get_uid_gid_from_pid(cap_fd: &libredox::Fd, target_pid: usize) -> Result<(u32, u32, u32)> {
    let mut buffer = [0u8; mem::size_of::<libredox::protocol::ProcMeta>()];
    let _ = libredox::call::get_proc_credentials(cap_fd.raw(), target_pid, &mut buffer).map_err(
        |e| {
            eprintln!(
                "Failed to get process credentials for pid {}: {:?}",
                target_pid, e
            );
            Error::new(EINVAL)
        },
    )?;
    let mut cursor = 0;
    let pid = read_u32(&buffer, cursor)?;
    cursor += mem::size_of::<u32>() * 3;
    let uid = read_u32(&buffer, cursor)?;
    cursor += mem::size_of::<u32>() * 3;
    let gid = read_u32(&buffer, cursor)?;
    Ok((pid, uid, gid))
}

fn read_u32(buffer: &[u8], offset: usize) -> Result<u32> {
    let bytes = buffer
        .get(offset..offset + 4)
        .and_then(|slice| slice.try_into().ok())
        .ok_or_else(|| Error::new(EINVAL))?;

    Ok(u32::from_le_bytes(bytes))
}
