use std::collections::BTreeMap;
use std::sync::Arc;

use log::warn;
use redox_scheme::SchemeBlockMut;
use syscall04::data::Stat;
use syscall04::error::{Error, Result, EBADF, EINVAL, EISDIR, ENOENT, EROFS};
use syscall04::flag::{EventFlags, MapFlags, MunmapFlags, MODE_FILE, SEEK_CUR, SEEK_END, SEEK_SET};

use crate::blob::FirmwareRegistry;

struct Handle {
    blob_key: String,
    data: Arc<Vec<u8>>,
    offset: u64,
    map_count: usize,
    closed: bool,
}

pub struct FirmwareScheme {
    registry: FirmwareRegistry,
    next_id: usize,
    handles: BTreeMap<usize, Handle>,
}

impl FirmwareScheme {
    pub fn new(registry: FirmwareRegistry) -> Self {
        FirmwareScheme {
            registry,
            next_id: 0,
            handles: BTreeMap::new(),
        }
    }
}

fn resolve_key(path: &str) -> Option<String> {
    let cleaned = path.trim_matches('/');
    if cleaned.is_empty() || cleaned.ends_with('/') {
        return None;
    }
    // Reject path traversal attempts — only allow safe characters
    if cleaned.starts_with('.') || cleaned.contains("..") {
        log::warn!(
            "firmware-loader: rejecting path traversal in key: {:?}",
            path
        );
        return None;
    }
    let key = if cleaned.ends_with(".bin") {
        cleaned.trim_end_matches(".bin").to_string()
    } else {
        cleaned.to_string()
    };
    // Final sanity: key must be purely alphanumeric with /, -, _
    if !key
        .chars()
        .all(|c| c.is_alphanumeric() || c == '/' || c == '-' || c == '_')
    {
        log::warn!(
            "firmware-loader: rejecting invalid characters in key: {:?}",
            key
        );
        return None;
    }
    Some(key)
}

impl SchemeBlockMut for FirmwareScheme {
    fn open(&mut self, path: &str, _flags: usize, _uid: u32, _gid: u32) -> Result<Option<usize>> {
        let key = resolve_key(path).ok_or(Error::new(EISDIR))?;

        if !self.registry.contains(&key) {
            warn!("firmware-loader: firmware not found: {}", path);
            return Err(Error::new(ENOENT));
        }

        let data = self.registry.load(&key).map_err(|e| {
            warn!("firmware-loader: failed to load firmware '{}': {}", key, e);
            Error::new(ENOENT)
        })?;

        let id = self.next_id;
        self.next_id += 1;

        self.handles.insert(
            id,
            Handle {
                blob_key: key,
                data,
                offset: 0,
                map_count: 0,
                closed: false,
            },
        );

        Ok(Some(id))
    }

    fn seek(&mut self, id: usize, pos: isize, whence: usize) -> Result<Option<isize>> {
        let handle = self.handles.get_mut(&id).ok_or(Error::new(EBADF))?;
        let len = handle.data.len() as i64;
        let new_offset = match whence {
            SEEK_SET => pos as i64,
            SEEK_CUR => handle.offset as i64 + pos as i64,
            SEEK_END => len + pos as i64,
            _ => return Err(Error::new(EINVAL)),
        };
        if new_offset < 0 {
            return Err(Error::new(EINVAL));
        }
        handle.offset = new_offset as u64;
        let new_offset = isize::try_from(new_offset).map_err(|_| Error::new(EINVAL))?;
        Ok(Some(new_offset))
    }

    fn read(&mut self, id: usize, buf: &mut [u8]) -> Result<Option<usize>> {
        let handle = self.handles.get_mut(&id).ok_or(Error::new(EBADF))?;
        let offset = handle.offset as usize;
        let data = &handle.data;

        if offset >= data.len() {
            return Ok(Some(0));
        }

        let available = data.len() - offset;
        let to_copy = available.min(buf.len());
        buf[..to_copy].copy_from_slice(&data[offset..offset + to_copy]);
        handle.offset += to_copy as u64;

        Ok(Some(to_copy))
    }

    fn write(&mut self, id: usize, _buf: &[u8]) -> Result<Option<usize>> {
        let _ = self.handles.get(&id).ok_or(Error::new(EBADF))?;
        Err(Error::new(EROFS))
    }

    fn fpath(&mut self, id: usize, buf: &mut [u8]) -> Result<Option<usize>> {
        let handle = self.handles.get(&id).ok_or(Error::new(EBADF))?;
        let path = format!("firmware:/{}.bin", handle.blob_key);
        let bytes = path.as_bytes();
        let len = bytes.len().min(buf.len());
        buf[..len].copy_from_slice(&bytes[..len]);
        Ok(Some(len))
    }

    fn fstat(&mut self, id: usize, stat: &mut Stat) -> Result<Option<usize>> {
        let handle = self.handles.get(&id).ok_or(Error::new(EBADF))?;
        stat.st_mode = MODE_FILE | 0o444;
        stat.st_size = handle.data.len() as u64;
        stat.st_blksize = 4096;
        stat.st_blocks = (handle.data.len() as u64 + 511) / 512;
        Ok(Some(0))
    }

    fn fsync(&mut self, id: usize) -> Result<Option<usize>> {
        if !self.handles.contains_key(&id) {
            return Err(Error::new(EBADF));
        }
        Ok(Some(0))
    }

    fn fevent(&mut self, id: usize, _flags: EventFlags) -> Result<Option<EventFlags>> {
        if !self.handles.contains_key(&id) {
            return Err(Error::new(EBADF));
        }
        Ok(Some(EventFlags::empty()))
    }

    fn close(&mut self, id: usize) -> Result<Option<usize>> {
        let handle = self.handles.get_mut(&id).ok_or(Error::new(EBADF))?;
        handle.closed = true;
        let should_remove = handle.map_count == 0;
        if should_remove {
            self.handles.remove(&id);
        }
        Ok(Some(0))
    }

    fn mmap_prep(
        &mut self,
        id: usize,
        offset: u64,
        size: usize,
        _flags: MapFlags,
    ) -> Result<Option<usize>> {
        let handle = self.handles.get_mut(&id).ok_or(Error::new(EBADF))?;
        let data_len = handle.data.len() as u64;

        if offset > data_len {
            return Err(Error::new(EINVAL));
        }
        if offset + size as u64 > data_len {
            return Err(Error::new(EINVAL));
        }

        let ptr = &handle.data[offset as usize] as *const u8;
        handle.map_count += 1;
        Ok(Some(ptr as usize))
    }

    fn munmap(
        &mut self,
        id: usize,
        _offset: u64,
        _size: usize,
        _flags: MunmapFlags,
    ) -> Result<Option<usize>> {
        let handle = self.handles.get_mut(&id).ok_or(Error::new(EBADF))?;
        if handle.map_count > 0 {
            handle.map_count -= 1;
        }
        let should_cleanup = handle.closed && handle.map_count == 0;
        if should_cleanup {
            self.handles.remove(&id);
        }
        Ok(Some(0))
    }
}
