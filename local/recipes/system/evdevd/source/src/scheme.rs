use std::collections::BTreeMap;

use syscall::data::Stat;
use syscall::error::{Error, Result, EBADF, EINVAL, ENOENT, EROFS};
use syscall::flag::{EventFlags, MODE_DIR, MODE_FILE, SEEK_CUR, SEEK_END, SEEK_SET};

use crate::device::{DeviceKind, InputDevice};
use crate::translate;

struct Handle {
    kind: HandleKind,
    offset: usize,
}

enum HandleKind {
    Root,
    Device(usize),
}

pub struct EvdevScheme {
    next_id: usize,
    handles: BTreeMap<usize, Handle>,
    devices: Vec<InputDevice>,
}

impl EvdevScheme {
    pub fn new() -> Self {
        let mut scheme = EvdevScheme {
            next_id: 0,
            handles: BTreeMap::new(),
            devices: Vec::new(),
        };
        scheme.devices.push(InputDevice::new_keyboard(0));
        scheme.devices.push(InputDevice::new_mouse(0));
        scheme
    }

    pub fn feed_keyboard_event(&mut self, key: u8, pressed: bool) {
        let events = translate::translate_keyboard(key, pressed);
        if !events.is_empty() {
            if let Some(dev) = self
                .devices
                .iter_mut()
                .find(|d| d.kind == DeviceKind::Keyboard)
            {
                dev.push_events(&events);
            }
        }
    }

    pub fn feed_mouse_move(&mut self, dx: i32, dy: i32) {
        if let Some(dev) = self
            .devices
            .iter_mut()
            .find(|d| d.kind == DeviceKind::Mouse)
        {
            dev.push_events(&translate::translate_mouse_dx(dx));
            dev.push_events(&translate::translate_mouse_dy(dy));
        }
    }

    pub fn feed_mouse_scroll(&mut self, y: i32) {
        if let Some(dev) = self
            .devices
            .iter_mut()
            .find(|d| d.kind == DeviceKind::Mouse)
        {
            dev.push_events(&translate::translate_mouse_scroll(y));
        }
    }

    pub fn feed_mouse_button(&mut self, button: usize, pressed: bool) {
        if let Some(dev) = self
            .devices
            .iter_mut()
            .find(|d| d.kind == DeviceKind::Mouse)
        {
            dev.push_events(&translate::translate_mouse_button(button, pressed));
        }
    }
}

impl redox_scheme::SchemeBlockMut for EvdevScheme {
    fn open(&mut self, path: &str, _flags: usize, _uid: u32, _gid: u32) -> Result<Option<usize>> {
        let cleaned = path.trim_matches('/');

        let kind = if cleaned.is_empty() {
            HandleKind::Root
        } else if let Some(rest) = cleaned.strip_prefix("event") {
            let idx: usize = rest
                .trim_end_matches('/')
                .parse()
                .map_err(|_| Error::new(ENOENT))?;
            if idx >= self.devices.len() {
                return Err(Error::new(ENOENT));
            }
            HandleKind::Device(idx)
        } else {
            return Err(Error::new(ENOENT));
        };

        let id = self.next_id;
        self.next_id += 1;
        self.handles.insert(id, Handle { kind, offset: 0 });
        Ok(Some(id))
    }

    fn read(&mut self, id: usize, buf: &mut [u8]) -> Result<Option<usize>> {
        let handle = self.handles.get_mut(&id).ok_or(Error::new(EBADF))?;

        match &handle.kind {
            HandleKind::Root => {
                let mut listing = String::new();
                for (i, _dev) in self.devices.iter().enumerate() {
                    listing.push_str(&format!("event{}\n", i));
                }
                let bytes = listing.as_bytes();
                let remaining = &bytes[handle.offset..];
                let to_copy = remaining.len().min(buf.len());
                buf[..to_copy].copy_from_slice(&remaining[..to_copy]);
                handle.offset += to_copy;
                Ok(Some(to_copy))
            }
            HandleKind::Device(idx) => {
                let dev = &mut self.devices[*idx];
                let written = dev.pop_bytes(buf);
                handle.offset += written;
                Ok(if written == 0 { None } else { Some(written) })
            }
        }
    }

    fn write(&mut self, id: usize, _buf: &[u8]) -> Result<Option<usize>> {
        let _ = self.handles.get(&id).ok_or(Error::new(EBADF))?;
        Err(Error::new(EROFS))
    }

    fn seek(&mut self, id: usize, pos: isize, whence: usize) -> Result<Option<isize>> {
        let handle = self.handles.get_mut(&id).ok_or(Error::new(EBADF))?;
        let new_offset = match whence {
            SEEK_SET => pos as isize,
            SEEK_CUR => handle.offset as isize + pos,
            SEEK_END => pos,
            _ => return Err(Error::new(EINVAL)),
        };
        if new_offset < 0 {
            return Err(Error::new(EINVAL));
        }
        handle.offset = new_offset as usize;
        Ok(Some(new_offset))
    }

    fn fstat(&mut self, id: usize, stat: &mut Stat) -> Result<Option<usize>> {
        let handle = self.handles.get(&id).ok_or(Error::new(EBADF))?;
        match &handle.kind {
            HandleKind::Root => {
                stat.st_mode = MODE_DIR | 0o555;
            }
            HandleKind::Device(_) => {
                stat.st_mode = MODE_FILE | 0o444;
            }
        }
        Ok(Some(0))
    }

    fn close(&mut self, id: usize) -> Result<Option<usize>> {
        self.handles.remove(&id);
        Ok(Some(0))
    }

    fn fpath(&mut self, id: usize, buf: &mut [u8]) -> Result<Option<usize>> {
        let handle = self.handles.get(&id).ok_or(Error::new(EBADF))?;
        let path = match &handle.kind {
            HandleKind::Root => "evdev:".to_string(),
            HandleKind::Device(idx) => format!("evdev:event{}", idx),
        };
        let bytes = path.as_bytes();
        let to_copy = bytes.len().min(buf.len());
        buf[..to_copy].copy_from_slice(&bytes[..to_copy]);
        Ok(Some(to_copy))
    }

    fn fcntl(&mut self, id: usize, _cmd: usize, _arg: usize) -> Result<Option<usize>> {
        let _ = self.handles.get(&id).ok_or(Error::new(EBADF))?;
        Ok(Some(0))
    }

    fn fevent(&mut self, id: usize, flags: EventFlags) -> Result<Option<EventFlags>> {
        let _ = self.handles.get(&id).ok_or(Error::new(EBADF))?;
        Ok(Some(flags))
    }
}
