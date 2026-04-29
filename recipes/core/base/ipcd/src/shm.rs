use redox_scheme::{scheme::SchemeSync, CallerCtx, OpenResult};
use scheme_utils::{FpathWriter, HandleMap};
use std::{
    cmp,
    collections::{hash_map::Entry, HashMap},
    rc::Rc,
};
use syscall::{
    data::Stat, error::*, schemev2::NewFdFlags, Error, Map, MapFlags, MremapFlags, Result,
    MAP_PRIVATE, PAGE_SIZE, PROT_READ, PROT_WRITE,
};

enum Handle {
    Shm(Rc<str>),
    SchemeRoot,
}
impl Handle {
    fn as_shm(&self) -> Result<&Rc<str>, Error> {
        match self {
            Self::Shm(path) => Ok(path),
            Self::SchemeRoot => Err(Error::new(EBADF)),
        }
    }
}

// TODO: Move to relibc
const AT_REMOVEDIR: usize = 0x200;

pub struct ShmHandle {
    buffer: MmapGuard,
    refs: usize,
    unlinked: bool,
}
pub struct ShmScheme {
    maps: HashMap<Rc<str>, ShmHandle>,
    handles: HandleMap<Handle>,
}
impl ShmScheme {
    pub fn new() -> Self {
        Self {
            maps: HashMap::new(),
            handles: HandleMap::new(),
        }
    }
}

impl SchemeSync for ShmScheme {
    fn scheme_root(&mut self) -> Result<usize> {
        Ok(self.handles.insert(Handle::SchemeRoot))
    }
    //FIXME: Handle O_RDONLY/O_WRONLY/O_RDWR
    fn openat(
        &mut self,
        dirfd: usize,
        path: &str,
        flags: usize,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<OpenResult> {
        let handle = self.handles.get(dirfd)?;
        if !matches!(handle, Handle::SchemeRoot) {
            return Err(Error::new(EACCES));
        }

        let path = Rc::from(path);
        let entry = match self.maps.entry(Rc::clone(&path)) {
            Entry::Occupied(e) => {
                if flags & syscall::O_EXCL != 0 && flags & syscall::O_CREAT != 0 {
                    return Err(Error::new(EEXIST));
                }
                e.into_mut()
            }
            Entry::Vacant(e) => {
                if flags & syscall::O_CREAT == 0 {
                    return Err(Error::new(ENOENT));
                }
                e.insert(ShmHandle {
                    buffer: MmapGuard::new(),
                    refs: 0,
                    unlinked: false,
                })
            }
        };
        entry.refs += 1;
        let id = self.handles.insert(Handle::Shm(path));

        Ok(OpenResult::ThisScheme {
            number: id,
            flags: NewFdFlags::POSITIONED,
        })
    }
    fn fpath(&mut self, id: usize, buf: &mut [u8], _ctx: &CallerCtx) -> Result<usize> {
        FpathWriter::with(buf, "shm", |w| {
            w.push_str(self.handles.get(id).and_then(Handle::as_shm)?);
            Ok(())
        })
    }
    fn on_close(&mut self, id: usize) {
        let Handle::Shm(path) = self.handles.remove(id).unwrap() else {
            return;
        };
        let mut entry = match self.maps.entry(path) {
            Entry::Occupied(entry) => entry,
            Entry::Vacant(_) => panic!("handle pointing to nothing"),
        };
        entry.get_mut().refs -= 1;
        if entry.get().refs == 0 && entry.get().unlinked {
            // There is no other reference to this entry, drop
            entry.remove_entry();
        }
    }
    fn unlinkat(&mut self, dirfd: usize, path: &str, flags: usize, _ctx: &CallerCtx) -> Result<()> {
        let handle = self.handles.get(dirfd)?;
        if !matches!(handle, Handle::SchemeRoot) {
            return Err(Error::new(EACCES));
        }
        if flags & AT_REMOVEDIR == AT_REMOVEDIR {
            return Err(Error::new(ENOTDIR));
        }
        let path = Rc::from(path);
        let mut entry = match self.maps.entry(Rc::clone(&path)) {
            Entry::Occupied(e) => e,
            Entry::Vacant(_) => return Err(Error::new(ENOENT)),
        };

        entry.get_mut().unlinked = true;
        if entry.get().refs == 0 {
            // There is no other reference to this entry, drop
            entry.remove_entry();
        }
        Ok(())
    }
    fn fstat(&mut self, id: usize, stat: &mut Stat, _ctx: &CallerCtx) -> Result<()> {
        let path = self.handles.get(id).and_then(Handle::as_shm)?;
        let size = self
            .maps
            .get(path)
            .expect("handle pointing to nothing")
            .buffer
            .len();

        //TODO: fill in more items?
        *stat = Stat {
            st_mode: syscall::MODE_FILE,
            st_size: size as _,
            ..Default::default()
        };

        Ok(())
    }
    fn fsize(&mut self, id: usize, _ctx: &CallerCtx) -> Result<u64> {
        let path = self.handles.get(id).and_then(Handle::as_shm)?;
        let size = self
            .maps
            .get(path)
            .expect("handle pointing to nothing")
            .buffer
            .len();

        Ok(size as u64)
    }
    fn ftruncate(&mut self, id: usize, len: u64, _ctx: &CallerCtx) -> Result<()> {
        let path = self.handles.get(id).and_then(Handle::as_shm)?;
        self.maps
            .get_mut(path)
            .expect("handle pointing to nothing")
            .buffer
            .grow_to(len as usize)
    }
    fn mmap_prep(
        &mut self,
        id: usize,
        offset: u64,
        size: usize,
        _flags: MapFlags,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let path = self.handles.get(id).and_then(Handle::as_shm)?;
        self.maps
            .get_mut(path)
            .expect("handle pointing to nothing")
            .buffer
            .mmap(offset as usize, size)
    }
    fn read(
        &mut self,
        id: usize,
        buf: &mut [u8],
        offset: u64,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let path = self.handles.get(id).and_then(Handle::as_shm)?;
        self.maps
            .get_mut(path)
            .expect("handle pointing to nothing")
            .buffer
            .read(offset as usize, buf)
    }
    fn write(
        &mut self,
        id: usize,
        buf: &[u8],
        offset: u64,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let path = self.handles.get(id).and_then(Handle::as_shm)?;
        self.maps
            .get_mut(path)
            .expect("handle pointing to nothing")
            .buffer
            .write(offset as usize, buf)
    }
}

pub struct MmapGuard {
    base: *mut (),
    len: usize,
}

impl MmapGuard {
    pub fn new() -> Self {
        Self {
            base: core::ptr::null_mut(),
            len: 0,
        }
    }

    fn grow_to(&mut self, new_len: usize) -> Result<()> {
        if new_len <= self.total_capacity() {
            // FIXME clear bytes after new_len
            self.len = new_len;
            return Ok(());
        }

        let needed = new_len - self.total_capacity();
        let page_count = needed.div_ceil(PAGE_SIZE);
        let alloc_size = page_count * PAGE_SIZE;

        let new_base = unsafe {
            if self.base.is_null() {
                syscall::fmap(
                    !0,
                    &Map {
                        offset: 0,
                        size: alloc_size,
                        flags: MAP_PRIVATE | PROT_READ | PROT_WRITE,
                        address: 0,
                    },
                )
            } else {
                syscall::syscall5(
                    syscall::SYS_MREMAP,
                    self.base as usize,
                    self.len.next_multiple_of(PAGE_SIZE),
                    0,
                    new_len.next_multiple_of(PAGE_SIZE),
                    MremapFlags::empty().bits() | (PROT_READ | PROT_WRITE).bits(),
                )
            }
        }?;

        self.base = new_base as *mut ();
        self.len = new_len;
        Ok(())
    }

    fn total_capacity(&self) -> usize {
        self.len.next_multiple_of(PAGE_SIZE)
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn mmap(&mut self, offset: usize, size: usize) -> Result<usize> {
        let total_size = offset + size;

        if total_size > self.len.next_multiple_of(PAGE_SIZE) {
            return Err(Error::new(ERANGE));
        }

        if size == 0 {
            return Ok(0);
        }

        Ok(self.base.addr() + offset)
    }

    pub fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize> {
        if offset >= self.len {
            // FIXME read as zeros
            return Ok(0);
        }

        let to_read = cmp::min(self.len - offset, buf.len());
        unsafe {
            let src = (self.base as *const u8).add(offset);
            let dst = buf.as_mut_ptr();
            core::ptr::copy_nonoverlapping(src, dst, to_read);
        }

        Ok(to_read)
    }

    pub fn write(&mut self, offset: usize, buf: &[u8]) -> Result<usize> {
        let end = offset.checked_add(buf.len()).ok_or(Error::new(ERANGE))?;
        self.grow_to(end)?;

        let to_write = cmp::min(self.len - offset, buf.len());
        unsafe {
            let src = buf.as_ptr();
            let dst = (self.base as *mut u8).add(offset);
            core::ptr::copy_nonoverlapping(src, dst, to_write);
        }

        Ok(to_write)
    }
}

impl Drop for MmapGuard {
    fn drop(&mut self) {
        if !self.base.is_null() {
            let _ =
                unsafe { syscall::funmap(self.base.addr(), self.len.next_multiple_of(PAGE_SIZE)) };
        }
    }
}
