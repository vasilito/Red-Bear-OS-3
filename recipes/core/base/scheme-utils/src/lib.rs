#![feature(never_type)]

use std::collections::{BTreeMap, btree_map};
use std::fmt;
use std::num::Wrapping;

use syscall::{EBADF, Error, Result};

mod blocking;
mod readiness_based;

pub use blocking::Blocking;
pub use readiness_based::ReadinessBased;

pub struct HandleMap<T> {
    handles: BTreeMap<usize, T>,
    next_id: Wrapping<usize>,
}

impl<T> HandleMap<T> {
    pub const fn new() -> Self {
        HandleMap {
            handles: BTreeMap::new(),
            next_id: Wrapping(1),
        }
    }

    pub fn insert(&mut self, handle: T) -> usize {
        let id = self.next_id;

        // If we've looped round there's a small chance that the file descriptor still exists, so loop till we get one that doesn't
        self.next_id += Wrapping(1);
        loop {
            if !self.handles.contains_key(&self.next_id.0) {
                break;
            } else {
                self.next_id += Wrapping(1);
            }
        }

        self.handles.insert(id.0, handle);
        id.0
    }

    pub fn remove(&mut self, id: usize) -> Option<T> {
        self.handles.remove(&id)
    }

    pub fn get(&self, id: usize) -> Result<&T> {
        self.handles.get(&id).ok_or(Error::new(EBADF))
    }

    pub fn get_mut(&mut self, id: usize) -> Result<&mut T> {
        self.handles.get_mut(&id).ok_or(Error::new(EBADF))
    }

    pub fn iter(&self) -> btree_map::Iter<'_, usize, T> {
        self.handles.iter()
    }

    pub fn iter_mut(&mut self) -> btree_map::IterMut<'_, usize, T> {
        self.handles.iter_mut()
    }

    pub fn keys(&self) -> btree_map::Keys<'_, usize, T> {
        self.handles.keys()
    }

    pub fn values(&self) -> btree_map::Values<'_, usize, T> {
        self.handles.values()
    }

    pub fn values_mut(&mut self) -> btree_map::ValuesMut<'_, usize, T> {
        self.handles.values_mut()
    }
}

pub struct FpathWriter<'a> {
    buf: &'a mut [u8],
    written: usize,
}

impl<'a> FpathWriter<'a> {
    pub fn with(
        buf: &'a mut [u8],
        scheme_name: &str,
        f: impl FnOnce(&mut Self) -> Result<()>,
    ) -> Result<usize> {
        let mut w = FpathWriter { buf, written: 0 };
        write!(w, "/scheme/{scheme_name}/").unwrap();
        f(&mut w)?;
        Ok(w.written)
    }

    pub fn with_legacy(
        buf: &'a mut [u8],
        scheme_name: &str,
        f: impl FnOnce(&mut Self) -> Result<()>,
    ) -> Result<usize> {
        let mut w = FpathWriter { buf, written: 0 };
        write!(w, "{scheme_name}:").unwrap();
        f(&mut w)?;
        Ok(w.written)
    }

    pub fn push_str(&mut self, s: &str) {
        let count = core::cmp::min(s.len(), self.buf.len() - self.written);
        self.buf[self.written..self.written + count].copy_from_slice(&s.as_bytes()[..count]);
        self.written += count;
    }

    pub fn write_fmt(&mut self, args: fmt::Arguments<'_>) -> fmt::Result {
        std::fmt::write(self, args)
    }
}

impl fmt::Write for FpathWriter<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.push_str(s);
        Ok(())
    }
}
