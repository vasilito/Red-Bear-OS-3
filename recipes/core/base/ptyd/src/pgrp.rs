use std::cell::RefCell;
use std::rc::Weak;

use syscall::error::{Error, Result, EBADF, EINVAL, EPIPE};
use syscall::flag::{EventFlags, F_GETFL, F_SETFL, O_ACCMODE};

use crate::pty::Pty;
use crate::resource::Resource;

/// Read side of a pipe
pub struct PtyPgrp {
    pty: Weak<RefCell<Pty>>,
    flags: usize,
}

impl PtyPgrp {
    pub fn new(pty: Weak<RefCell<Pty>>, flags: usize) -> Self {
        PtyPgrp { pty, flags }
    }
}

impl Resource for PtyPgrp {
    fn pty(&self) -> Weak<RefCell<Pty>> {
        self.pty.clone()
    }

    fn flags(&self) -> usize {
        self.flags
    }

    fn path(&mut self, buf: &mut [u8]) -> Result<usize> {
        if let Some(pty_lock) = self.pty.upgrade() {
            pty_lock.borrow_mut().path(buf)
        } else {
            Err(Error::new(EPIPE))
        }
    }

    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if let Some(pty_lock) = self.pty.upgrade() {
            let pty = pty_lock.borrow();

            //println!("READ PGRP {}: {}", pty.id, pty.pgrp);
            let dst_buf = buf
                .get_mut(..4)
                .and_then(|b| <&mut [u8; 4]>::try_from(b).ok())
                .ok_or(Error::new(EBADF))?;
            *dst_buf = (pty.pgrp as u32).to_ne_bytes();

            Ok(4)
        } else {
            Ok(0)
        }
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        if let Some(pty_lock) = self.pty.upgrade() {
            let mut pty = pty_lock.borrow_mut();

            let new_pgrp = u32::from_ne_bytes(
                buf.get(..4)
                    .and_then(|b| <[u8; 4]>::try_from(b).ok())
                    .ok_or(Error::new(EBADF))?,
            );
            pty.pgrp = new_pgrp as usize;
            //println!("WRITE PGRP {}: {} => {}", pty.id, pty.pgrp, new_pgrp);

            Ok(4)
        } else {
            Err(Error::new(EPIPE))
        }
    }

    fn sync(&mut self) -> Result<()> {
        Ok(())
    }

    fn fcntl(&mut self, cmd: usize, arg: usize) -> Result<usize> {
        match cmd {
            F_GETFL => Ok(self.flags),
            F_SETFL => {
                self.flags = (self.flags & O_ACCMODE) | (arg & !O_ACCMODE);
                Ok(0)
            }
            _ => Err(Error::new(EINVAL)),
        }
    }

    fn fevent(&mut self) -> Result<EventFlags> {
        Err(Error::new(EBADF))
    }

    fn events(&mut self) -> EventFlags {
        EventFlags::empty()
    }
}
