use std::collections::BTreeMap;
use std::os::fd::AsRawFd;

use event::{EventQueue, UserData};
use redox_scheme::scheme::SchemeSync;
use redox_scheme::{CallerCtx, OpenResult};
use scheme_utils::{FpathWriter, HandleMap};
use syscall::schemev2::NewFdFlags;
use syscall::{Error, EventFlags, Result, EACCES, EAGAIN, EBADF, ENOENT, O_NONBLOCK};

use crate::display::Display;
use crate::text::TextScreen;

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Debug)]
pub struct VtIndex(usize);

impl VtIndex {
    pub const SCHEMA_SENTINEL: VtIndex = VtIndex(usize::MAX);
}

impl UserData for VtIndex {
    fn into_user_data(self) -> usize {
        self.0
    }

    fn from_user_data(user_data: usize) -> Self {
        VtIndex(user_data)
    }
}

pub struct FdHandle {
    pub vt_i: VtIndex,
    pub flags: usize,
    pub events: EventFlags,
    pub notified_read: bool,
}

pub enum Handle {
    Vt(FdHandle),
    SchemeRoot,
}

pub struct FbconScheme {
    pub vts: BTreeMap<VtIndex, TextScreen>,
    pub handles: HandleMap<Handle>,
}

impl FbconScheme {
    pub fn new(vt_ids: &[usize], event_queue: &mut EventQueue<VtIndex>) -> FbconScheme {
        let mut vts = BTreeMap::new();

        for &vt_i in vt_ids {
            let display = Display::open_new_vt().expect("Failed to open display for vt");
            event_queue
                .subscribe(
                    display.input_handle.event_handle().as_raw_fd() as usize,
                    VtIndex(vt_i),
                    event::EventFlags::READ,
                )
                .expect("Failed to subscribe to input events for vt");
            vts.insert(VtIndex(vt_i), TextScreen::new(display));
        }

        FbconScheme {
            vts,
            handles: HandleMap::new(),
        }
    }

    fn get_vt_handle_mut(&mut self, id: usize) -> Result<&mut FdHandle> {
        match self.handles.get_mut(id)? {
            Handle::Vt(handle) => Ok(handle),
            Handle::SchemeRoot => Err(Error::new(EBADF)),
        }
    }
}

impl SchemeSync for FbconScheme {
    fn scheme_root(&mut self) -> Result<usize> {
        Ok(self.handles.insert(Handle::SchemeRoot))
    }

    fn openat(
        &mut self,
        dirfd: usize,
        path_str: &str,
        flags: usize,
        fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<OpenResult> {
        if !matches!(self.handles.get(dirfd)?, Handle::SchemeRoot) {
            return Err(Error::new(EACCES));
        }

        let vt_i = VtIndex(path_str.parse::<usize>().map_err(|_| Error::new(ENOENT))?);
        if self.vts.contains_key(&vt_i) {
            let id = self.handles.insert(Handle::Vt(FdHandle {
                vt_i,
                flags: flags | fcntl_flags as usize,
                events: EventFlags::empty(),
                notified_read: false,
            }));

            Ok(OpenResult::ThisScheme {
                number: id,
                flags: NewFdFlags::empty(),
            })
        } else {
            Err(Error::new(ENOENT))
        }
    }

    fn fevent(
        &mut self,
        id: usize,
        flags: syscall::EventFlags,
        _ctx: &CallerCtx,
    ) -> Result<syscall::EventFlags> {
        let handle = self.get_vt_handle_mut(id)?;

        handle.notified_read = false;
        handle.events = flags;

        Ok(syscall::EventFlags::empty())
    }

    fn fpath(&mut self, id: usize, buf: &mut [u8], _ctx: &CallerCtx) -> Result<usize> {
        FpathWriter::with_legacy(buf, "fbcon", |w| {
            let handle = self.get_vt_handle_mut(id)?;
            write!(w, "{}", handle.vt_i.0).unwrap();
            Ok(())
        })
    }

    fn fsync(&mut self, id: usize, _ctx: &CallerCtx) -> Result<()> {
        let _handle = self.get_vt_handle_mut(id)?;
        Ok(())
    }

    fn fcntl(&mut self, id: usize, _cmd: usize, _arg: usize, _ctx: &CallerCtx) -> Result<usize> {
        self.handles.get(id)?;
        Ok(0)
    }

    fn read(
        &mut self,
        id: usize,
        buf: &mut [u8],
        _offset: u64,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let handle = match self.handles.get(id)? {
            Handle::Vt(handle) => Ok(handle),
            Handle::SchemeRoot => Err(Error::new(EBADF)),
        }?;

        if let Some(screen) = self.vts.get_mut(&handle.vt_i) {
            if !screen.can_read() {
                if handle.flags & O_NONBLOCK != 0 {
                    Err(Error::new(EAGAIN))
                } else {
                    Err(Error::new(EAGAIN))
                }
            } else {
                screen.read(buf)
            }
        } else {
            Err(Error::new(EBADF))
        }
    }

    fn write(
        &mut self,
        id: usize,
        buf: &[u8],
        _offset: u64,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let vt_i = self.get_vt_handle_mut(id)?.vt_i;

        if let Some(console) = self.vts.get_mut(&vt_i) {
            console.write(buf)
        } else {
            Err(Error::new(EBADF))
        }
    }

    fn on_close(&mut self, id: usize) {
        self.handles.remove(id);
    }
}
