use std::collections::BTreeMap;

use syscall::data::Stat;
use syscall::error::{Error, Result, EBADF, EINVAL, ENOENT, EROFS};
use syscall::flag::{EventFlags, MODE_DIR, MODE_FILE, SEEK_CUR, SEEK_END, SEEK_SET};

use crate::device_db::{classify_pci_device, format_device_info, DeviceInfo, Subsystem};

struct Handle {
    kind: HandleKind,
    offset: usize,
}

enum HandleKind {
    Root,
    Device(usize),
}

pub struct UdevScheme {
    next_id: usize,
    handles: BTreeMap<usize, Handle>,
    devices: Vec<DeviceInfo>,
}

impl UdevScheme {
    pub fn new() -> Self {
        UdevScheme {
            next_id: 0,
            handles: BTreeMap::new(),
            devices: Vec::new(),
        }
    }

    pub fn scan_pci_devices(&mut self) -> Result<usize> {
        let dir = match std::fs::read_dir("/scheme/pci") {
            Ok(d) => d,
            Err(e) => {
                log::warn!("udev-shim: failed to read /scheme/pci: {e}");
                return Ok(0);
            }
        };

        let mut count = 0;
        for entry in dir {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let name = match entry.file_name().to_str() {
                Some(n) => n.to_string(),
                None => continue,
            };

            let parts: Vec<&str> = name.split('.').collect();
            if parts.len() < 3 {
                continue;
            }

            let bus: u8 = parts[0].parse().unwrap_or(0);
            let dev: u8 = parts[1].parse().unwrap_or(0);
            let func: u8 = parts[2].parse().unwrap_or(0);

            let info = classify_pci_device(bus, dev, func);
            self.devices.push(info);
            count += 1;
        }

        Ok(count)
    }
}

impl redox_scheme::SchemeBlockMut for UdevScheme {
    fn open(&mut self, path: &str, _flags: usize, _uid: u32, _gid: u32) -> Result<Option<usize>> {
        let cleaned = path.trim_matches('/');

        let kind = if cleaned.is_empty() {
            HandleKind::Root
        } else if cleaned == "devices" || cleaned == "devices/" {
            HandleKind::Root
        } else if let Some(rest) = cleaned.strip_prefix("devices/") {
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

        let content = match &handle.kind {
            HandleKind::Root => {
                let mut listing = String::new();
                for (i, dev) in self.devices.iter().enumerate() {
                    listing.push_str(&format!("devices/{}\n", i));
                }
                listing
            }
            HandleKind::Device(idx) => {
                let dev = &self.devices[*idx];
                format_device_info(dev)
            }
        };

        let bytes = content.as_bytes();
        let remaining = &bytes[handle.offset..];
        let to_copy = remaining.len().min(buf.len());
        buf[..to_copy].copy_from_slice(&remaining[..to_copy]);
        handle.offset += to_copy;
        Ok(Some(to_copy))
    }

    fn write(&mut self, id: usize, _buf: &[u8]) -> Result<Option<usize>> {
        let _ = self.handles.get(&id).ok_or(Error::new(EBADF))?;
        Err(Error::new(EROFS))
    }

    fn seek(&mut self, id: usize, pos: isize, whence: usize) -> Result<Option<isize>> {
        let handle = self.handles.get_mut(&id).ok_or(Error::new(EBADF))?;
        let len = match &handle.kind {
            HandleKind::Root => self.devices.len() * 20,
            HandleKind::Device(idx) => format_device_info(&self.devices[*idx]).len(),
        };
        let new_offset = match whence {
            SEEK_SET => pos as isize,
            SEEK_CUR => handle.offset as isize + pos,
            SEEK_END => len as isize + pos,
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

    fn fevent(&mut self, id: usize, _flags: EventFlags) -> Result<Option<EventFlags>> {
        let _ = self.handles.get(&id).ok_or(Error::new(EBADF))?;
        Ok(Some(EventFlags::empty()))
    }

    fn close(&mut self, id: usize) -> Result<Option<usize>> {
        self.handles.remove(&id);
        Ok(Some(0))
    }
}
