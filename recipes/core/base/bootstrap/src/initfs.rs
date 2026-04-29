use core::convert::TryFrom;
#[allow(deprecated)]
use core::hash::{BuildHasherDefault, SipHasher};
use core::str;

use alloc::string::String;

use hashbrown::HashMap;
use redox_initfs::{InitFs, Inode, InodeDir, InodeKind, InodeStruct};

use redox_rt::proc::FdGuard;
use redox_scheme::{
    CallerCtx, OpenResult, RequestKind,
    scheme::{SchemeState, SchemeSync},
};

use redox_scheme::{SignalBehavior, Socket};
use syscall::PAGE_SIZE;
use syscall::data::Stat;
use syscall::dirent::DirEntry;
use syscall::dirent::DirentBuf;
use syscall::dirent::DirentKind;
use syscall::error::*;
use syscall::flag::*;
use syscall::schemev2::NewFdFlags;

enum Handle {
    Node(Node),
    SchemeRoot,
}
impl Handle {
    fn as_node(&self) -> Result<&Node> {
        match self {
            Handle::Node(n) => Ok(n),
            _ => Err(Error::new(EBADF)),
        }
    }
    fn as_node_mut(&mut self) -> Result<&mut Node> {
        match self {
            Handle::Node(n) => Ok(n),
            _ => Err(Error::new(EBADF)),
        }
    }
}

struct Node {
    inode: Inode,
    // TODO: Any better way to implement fpath? Or maybe work around it, e.g. by giving paths such
    // as `initfs:__inodes__/<inode>`?
    filename: String,
}
pub struct InitFsScheme {
    #[allow(deprecated)]
    handles: HashMap<usize, Handle, BuildHasherDefault<SipHasher>>,
    next_id: usize,
    fs: InitFs<'static>,
}
impl InitFsScheme {
    pub fn new(bytes: &'static [u8]) -> Self {
        Self {
            handles: HashMap::default(),
            next_id: 0,
            fs: InitFs::new(bytes, Some(PAGE_SIZE.try_into().unwrap()))
                .expect("failed to parse initfs"),
        }
    }

    fn get_inode(fs: &InitFs<'static>, inode: Inode) -> Result<InodeStruct<'static>> {
        fs.get_inode(inode).ok_or_else(|| Error::new(EIO))
    }
    fn next_id(&mut self) -> usize {
        assert_ne!(self.next_id, usize::MAX, "usize overflow in initfs scheme");
        self.next_id += 1;
        self.next_id
    }
}

struct Iter {
    dir: InodeDir<'static>,
    idx: u32,
}
impl Iterator for Iter {
    type Item = Result<redox_initfs::Entry<'static>>;

    fn next(&mut self) -> Option<Self::Item> {
        let entry = self.dir.get_entry(self.idx).map_err(|_| Error::new(EIO));
        self.idx += 1;
        entry.transpose()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self.dir.entry_count().ok() {
            Some(size) => {
                let size =
                    usize::try_from(size).expect("expected u32 to be convertible into usize");
                (size, Some(size))
            }
            None => (0, None),
        }
    }
}

fn inode_len(inode: InodeStruct<'static>) -> Result<usize> {
    Ok(match inode.kind() {
        InodeKind::File(file) => file.data().map_err(|_| Error::new(EIO))?.len(),
        InodeKind::Dir(dir) => (Iter { dir, idx: 0 }).fold(0, |len, entry| {
            len + entry
                .and_then(|entry| entry.name().map_err(|_| Error::new(EIO)))
                .map_or(0, |name| name.len() + 1)
        }),
        InodeKind::Link(link) => link.data().map_err(|_| Error::new(EIO))?.len(),
        InodeKind::Unknown => return Err(Error::new(EIO)),
    })
}

impl SchemeSync for InitFsScheme {
    fn openat(
        &mut self,
        dirfd: usize,
        path: &str,
        flags: usize,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<OpenResult> {
        if !matches!(
            self.handles.get(&dirfd).ok_or(Error::new(EBADF))?,
            Handle::SchemeRoot
        ) {
            return Err(Error::new(EACCES));
        }
        let mut components = path
            // trim leading and trailing slash
            .trim_matches('/')
            // divide into components
            .split('/')
            // filter out double slashes (e.g. /usr//bin/...)
            .filter(|c| !c.is_empty());

        let mut current_inode = self.fs.root_inode();

        while let Some(component) = components.next() {
            match component {
                "." => continue,
                ".." => {
                    let _ = components.next_back();
                    continue;
                }

                _ => (),
            }

            let current_inode_struct = Self::get_inode(&self.fs, current_inode)?;

            let dir = match current_inode_struct.kind() {
                InodeKind::Dir(dir) => dir,

                // TODO: Support symlinks in other position than xopen target
                InodeKind::Link(_) => {
                    return Err(Error::new(EOPNOTSUPP));
                }

                // If we still have more components in the path, and the file tree for that
                // particular branch is not all directories except the last, then that file cannot
                // exist.
                InodeKind::File(_) | InodeKind::Unknown => return Err(Error::new(ENOENT)),
            };

            let mut entries = Iter { dir, idx: 0 };

            current_inode = loop {
                let entry_res = match entries.next() {
                    Some(e) => e,
                    None => return Err(Error::new(ENOENT)),
                };
                let entry = entry_res?;
                let name = entry.name().map_err(|_| Error::new(EIO))?;
                if name == component.as_bytes() {
                    break entry.inode();
                }
            };
        }

        // xopen target is link -- return EXDEV so that the file is opened as a link.
        // TODO: Maybe follow initfs-local symlinks here? Would be faster
        let is_link = matches!(
            Self::get_inode(&self.fs, current_inode)?.kind(),
            InodeKind::Link(_)
        );
        let o_stat_nofollow = flags & O_STAT != 0 && flags & O_NOFOLLOW != 0;
        let o_symlink = flags & O_SYMLINK != 0;
        if is_link && !o_stat_nofollow && !o_symlink {
            return Err(Error::new(EXDEV));
        }

        let id = self.next_id();
        let old = self.handles.insert(
            id,
            Handle::Node(Node {
                inode: current_inode,
                filename: path.into(),
            }),
        );
        assert!(old.is_none());

        Ok(OpenResult::ThisScheme {
            number: id,
            flags: NewFdFlags::POSITIONED,
        })
    }

    fn read(
        &mut self,
        id: usize,
        buffer: &mut [u8],
        offset: u64,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let Ok(offset) = usize::try_from(offset) else {
            return Ok(0);
        };

        let handle = self
            .handles
            .get_mut(&id)
            .ok_or(Error::new(EBADF))?
            .as_node_mut()?;

        match Self::get_inode(&self.fs, handle.inode)?.kind() {
            InodeKind::File(file) => {
                let data = file.data().map_err(|_| Error::new(EIO))?;
                let src_buf = &data[core::cmp::min(offset, data.len())..];

                let to_copy = core::cmp::min(src_buf.len(), buffer.len());
                buffer[..to_copy].copy_from_slice(&src_buf[..to_copy]);

                Ok(to_copy)
            }
            InodeKind::Dir(_) => Err(Error::new(EISDIR)),
            InodeKind::Link(link) => {
                let link_data = link.data().map_err(|_| Error::new(EIO))?;
                let src_buf = &link_data[core::cmp::min(offset, link_data.len())..];

                let to_copy = core::cmp::min(src_buf.len(), buffer.len());
                buffer[..to_copy].copy_from_slice(&src_buf[..to_copy]);

                Ok(to_copy)
            }
            InodeKind::Unknown => Err(Error::new(EIO)),
        }
    }
    fn getdents<'buf>(
        &mut self,
        id: usize,
        mut buf: DirentBuf<&'buf mut [u8]>,
        opaque_offset: u64,
    ) -> Result<DirentBuf<&'buf mut [u8]>> {
        let Ok(offset) = u32::try_from(opaque_offset) else {
            return Ok(buf);
        };
        let handle = self.handles.get(&id).ok_or(Error::new(EBADF))?.as_node()?;
        let InodeKind::Dir(dir) = Self::get_inode(&self.fs, handle.inode)?.kind() else {
            return Err(Error::new(ENOTDIR));
        };
        let iter = Iter { dir, idx: offset };
        for (index, entry) in iter.enumerate() {
            let entry = entry?;
            buf.entry(DirEntry {
                // TODO: Add getter
                //inode: entry.inode(),
                inode: 0,

                name: entry
                    .name()
                    .ok()
                    .and_then(|utf8| core::str::from_utf8(utf8).ok())
                    .ok_or(Error::new(EIO))?,
                next_opaque_id: index as u64 + 1,
                kind: DirentKind::Unspecified,
            })?;
        }
        Ok(buf)
    }

    fn fsize(&mut self, id: usize, _ctx: &CallerCtx) -> Result<u64> {
        let handle = self
            .handles
            .get_mut(&id)
            .ok_or(Error::new(EBADF))?
            .as_node_mut()?;

        Ok(inode_len(Self::get_inode(&self.fs, handle.inode)?)? as u64)
    }

    fn fcntl(&mut self, id: usize, _cmd: usize, _arg: usize, _ctx: &CallerCtx) -> Result<usize> {
        let _handle = self.handles.get(&id).ok_or(Error::new(EBADF))?.as_node()?;

        Ok(0)
    }

    fn fpath(&mut self, id: usize, buf: &mut [u8], _ctx: &CallerCtx) -> Result<usize> {
        let handle = self.handles.get(&id).ok_or(Error::new(EBADF))?.as_node()?;

        // TODO: Copy scheme part in kernel
        let scheme_path = b"/scheme/initfs";
        let scheme_bytes = core::cmp::min(scheme_path.len(), buf.len());
        buf[..scheme_bytes].copy_from_slice(&scheme_path[..scheme_bytes]);

        let source = handle.filename.as_bytes();
        let path_bytes = core::cmp::min(buf.len() - scheme_bytes, source.len());
        buf[scheme_bytes..scheme_bytes + path_bytes].copy_from_slice(&source[..path_bytes]);

        Ok(scheme_bytes + path_bytes)
    }

    fn fstat(&mut self, id: usize, stat: &mut Stat, _ctx: &CallerCtx) -> Result<()> {
        let handle = self.handles.get(&id).ok_or(Error::new(EBADF))?.as_node()?;

        let inode = Self::get_inode(&self.fs, handle.inode)?;

        stat.st_ino = inode.id();
        stat.st_mode = inode.mode()
            | match inode.kind() {
                InodeKind::Dir(_) => MODE_DIR,
                InodeKind::File(_) => MODE_FILE,
                InodeKind::Link(_) => MODE_SYMLINK,
                _ => 0,
            };
        stat.st_uid = 0;
        stat.st_gid = 0;
        stat.st_size = u64::try_from(inode_len(inode)?).unwrap_or(u64::MAX);

        stat.st_ctime = 0;
        stat.st_ctime_nsec = 0;
        stat.st_mtime = 0;
        stat.st_mtime_nsec = 0;

        Ok(())
    }

    fn fsync(&mut self, id: usize, _ctx: &CallerCtx) -> Result<()> {
        if !self.handles.contains_key(&id) {
            return Err(Error::new(EBADF));
        }

        Ok(())
    }

    fn mmap_prep(
        &mut self,
        id: usize,
        offset: u64,
        size: usize,
        flags: MapFlags,
        _ctx: &CallerCtx,
    ) -> syscall::Result<usize> {
        let handle = self.handles.get(&id).ok_or(Error::new(EBADF))?;
        let Handle::Node(node) = handle else {
            return Err(Error::new(EBADF));
        };
        let data = match Self::get_inode(&self.fs, node.inode)?.kind() {
            InodeKind::File(file) => file.data().map_err(|_| Error::new(EIO))?,
            InodeKind::Dir(_) => return Err(Error::new(EISDIR)),
            InodeKind::Link(_) => return Err(Error::new(ELOOP)),
            InodeKind::Unknown => return Err(Error::new(EIO)),
        };

        if flags.contains(MapFlags::PROT_WRITE) {
            return Err(Error::new(EPERM));
        }

        let Some(last_addr) = offset.checked_add(size as u64) else {
            return Err(Error::new(EINVAL));
        };

        if last_addr > data.len().next_multiple_of(PAGE_SIZE) as u64 {
            return Err(Error::new(EINVAL));
        }

        Ok(data.as_ptr() as usize)
    }
}

pub fn run(bytes: &'static [u8], sync_pipe: FdGuard, socket: Socket) -> ! {
    log::info!("bootstrap: starting initfs scheme");
    let mut state = SchemeState::new();
    let mut scheme = InitFsScheme::new(bytes);

    // send open-capability to bootstrap
    let new_id = scheme.next_id();
    scheme.handles.insert(new_id, Handle::SchemeRoot);
    let cap_fd = socket
        .create_this_scheme_fd(0, new_id, 0, 0)
        .expect("failed to issue initfs root fd");
    let _ = syscall::call_rw(
        sync_pipe.as_raw_fd(),
        &mut cap_fd.to_ne_bytes(),
        CallFlags::FD,
        &[],
    );
    drop(sync_pipe);

    loop {
        let Some(req) = socket
            .next_request(SignalBehavior::Restart)
            .expect("bootstrap: failed to read scheme request from kernel")
        else {
            break;
        };
        match req.kind() {
            RequestKind::Call(req) => {
                let resp = req.handle_sync(&mut scheme, &mut state);

                if !socket
                    .write_response(resp, SignalBehavior::Restart)
                    .expect("bootstrap: failed to write scheme response to kernel")
                {
                    break;
                }
            }
            RequestKind::OnClose { id } => {
                scheme.handles.remove(&id);
            }
            _ => (),
        }
    }

    unreachable!()
}

// TODO: Restructure bootstrap so it calls into relibc, or a split-off derivative without the C
// parts, such as "redox-rt".

#[unsafe(no_mangle)]
pub unsafe extern "C" fn redox_read_v1(fd: usize, ptr: *mut u8, len: usize) -> isize {
    Error::mux(syscall::read(fd, unsafe {
        core::slice::from_raw_parts_mut(ptr, len)
    })) as isize
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn redox_write_v1(fd: usize, ptr: *const u8, len: usize) -> isize {
    Error::mux(syscall::write(fd, unsafe {
        core::slice::from_raw_parts(ptr, len)
    })) as isize
}

#[unsafe(no_mangle)]
pub unsafe fn redox_dup_v1(fd: usize, buf: *const u8, len: usize) -> isize {
    Error::mux(syscall::dup(fd, unsafe {
        core::slice::from_raw_parts(buf, len)
    })) as isize
}

#[unsafe(no_mangle)]
pub extern "C" fn redox_close_v1(fd: usize) -> isize {
    Error::mux(syscall::close(fd)) as isize
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn redox_sys_call_v0(
    fd: usize,
    payload: *mut u8,
    payload_len: usize,
    flags: usize,
    metadata: *const u64,
    metadata_len: usize,
) -> isize {
    let flags = CallFlags::from_bits_retain(flags);

    let metadata = unsafe { core::slice::from_raw_parts(metadata, metadata_len) };

    let result = if flags.contains(CallFlags::READ) {
        let payload = unsafe { core::slice::from_raw_parts_mut(payload, payload_len) };
        if flags.contains(CallFlags::WRITE) {
            syscall::call_rw(fd, payload, flags, metadata)
        } else {
            syscall::call_ro(fd, payload, flags, metadata)
        }
    } else {
        let payload = unsafe { core::slice::from_raw_parts(payload, payload_len) };
        syscall::call_wo(fd, payload, flags, metadata)
    };

    Error::mux(result) as isize
}
