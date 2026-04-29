use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::str;

use redox_scheme::scheme::SchemeSync;
use redox_scheme::{CallerCtx, OpenResult};
use syscall::data::Stat;
use syscall::error::{Error, Result, EACCES, EBADF, EINVAL, ENOENT};
use syscall::flag::{EventFlags, MODE_CHR};
use syscall::schemev2::NewFdFlags;

use crate::controlterm::PtyControlTerm;
use crate::pgrp::PtyPgrp;
use crate::pty::Pty;
use crate::resource::Resource;
use crate::subterm::PtySubTerm;
use crate::termios::PtyTermios;
use crate::winsize::PtyWinsize;

pub enum Handle {
    Resource(Box<dyn Resource>),
    SchemeRoot,
}

pub struct PtyScheme {
    next_id: usize,
    pub handles: BTreeMap<usize, Handle>,
}

impl PtyScheme {
    pub fn new() -> Self {
        PtyScheme {
            next_id: 0,
            handles: BTreeMap::new(),
        }
    }

    fn get_resource_mut(&mut self, id: usize) -> Result<&mut Box<dyn Resource>> {
        match self.handles.get_mut(&id).ok_or(Error::new(EBADF))? {
            Handle::Resource(res) => Ok(res),
            Handle::SchemeRoot => Err(Error::new(EBADF)),
        }
    }
}

impl SchemeSync for PtyScheme {
    fn scheme_root(&mut self) -> Result<usize> {
        let id = self.next_id;
        self.next_id += 1;
        self.handles.insert(id, Handle::SchemeRoot);
        Ok(id)
    }

    fn openat(
        &mut self,
        dirfd: usize,
        path: &str,
        flags: usize,
        fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<OpenResult> {
        if !matches!(
            self.handles.get(&dirfd).ok_or(Error::new(EBADF))?,
            Handle::SchemeRoot
        ) {
            return Err(Error::new(EACCES));
        }

        let path = path.trim_matches('/');

        let id = self.next_id;
        self.next_id += 1;

        if path.is_empty() {
            let pty = Rc::new(RefCell::new(Pty::new(id)));
            self.handles.insert(
                id,
                Handle::Resource(Box::new(PtyControlTerm::new(pty, flags))),
            );
        } else {
            let control_term_id = path.parse::<usize>().or(Err(Error::new(EINVAL)))?;
            let pty = {
                let handle = self
                    .handles
                    .get(&control_term_id)
                    .ok_or(Error::new(ENOENT))?;

                match handle {
                    Handle::Resource(res) => res.pty(),
                    Handle::SchemeRoot => return Err(Error::new(ENOENT)),
                }
            };

            self.handles.insert(
                id,
                Handle::Resource(Box::new(PtySubTerm::new(pty, flags | fcntl_flags as usize))),
            );
        }

        Ok(OpenResult::ThisScheme {
            number: id,
            flags: NewFdFlags::empty(),
        })
    }

    fn dup(&mut self, old_id: usize, buf: &[u8], _ctx: &CallerCtx) -> Result<OpenResult> {
        let handle: Box<dyn Resource> = {
            let old_handle = self.handles.get(&old_id).ok_or(Error::new(EBADF))?;

            let old_resource = match old_handle {
                Handle::Resource(res) => res,
                Handle::SchemeRoot => return Err(Error::new(EBADF)),
            };

            if buf == b"pgrp" {
                Box::new(PtyPgrp::new(old_resource.pty(), old_resource.flags()))
            } else if buf == b"termios" {
                Box::new(PtyTermios::new(old_resource.pty(), old_resource.flags()))
            } else if buf == b"winsize" {
                Box::new(PtyWinsize::new(old_resource.pty(), old_resource.flags()))
            } else {
                return Err(Error::new(EINVAL));
            }
        };

        let id = self.next_id;
        self.next_id += 1;
        self.handles.insert(id, Handle::Resource(handle));

        Ok(OpenResult::ThisScheme {
            number: id,
            flags: NewFdFlags::empty(),
        })
    }

    fn read(
        &mut self,
        id: usize,
        buf: &mut [u8],
        _offset: u64,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let handle = self.get_resource_mut(id)?;
        handle.read(buf)
    }

    fn write(
        &mut self,
        id: usize,
        buf: &[u8],
        _offset: u64,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let handle = self.get_resource_mut(id)?;
        handle.write(buf)
    }

    fn fcntl(&mut self, id: usize, cmd: usize, arg: usize, _ctx: &CallerCtx) -> Result<usize> {
        let handle = self.get_resource_mut(id)?;
        handle.fcntl(cmd, arg)
    }

    fn fevent(&mut self, id: usize, _flags: EventFlags, _ctx: &CallerCtx) -> Result<EventFlags> {
        let handle = self.get_resource_mut(id)?;
        handle.fevent()
    }

    fn fpath(&mut self, id: usize, buf: &mut [u8], _ctx: &CallerCtx) -> Result<usize> {
        let handle = self.get_resource_mut(id)?;
        handle.path(buf)
    }

    fn fstat(&mut self, id: usize, stat: &mut Stat, _ctx: &CallerCtx) -> Result<()> {
        let handle = self.handles.get(&id).ok_or(Error::new(EBADF))?;

        match handle {
            Handle::SchemeRoot => return Err(Error::new(EBADF)),
            Handle::Resource(_res) => {
                *stat = Stat {
                    st_mode: MODE_CHR | 0o666,
                    ..Default::default()
                };
            }
        }

        Ok(())
    }

    fn fsync(&mut self, id: usize, _ctx: &CallerCtx) -> Result<()> {
        let handle = self.get_resource_mut(id)?;
        handle.sync()
    }

    fn on_close(&mut self, id: usize) {
        let _ = self.handles.remove(&id);
    }
}
