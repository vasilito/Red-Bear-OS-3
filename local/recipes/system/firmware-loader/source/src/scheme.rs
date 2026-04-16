use std::collections::BTreeMap;
use std::sync::Arc;

use log::warn;
use redox_scheme::scheme::SchemeSync;
use redox_scheme::{CallerCtx, OpenResult};
use syscall::error::*;
use syscall::schemev2::NewFdFlags;
use syscall::{EventFlags, Stat, MODE_FILE};

use crate::blob::FirmwareRegistry;

#[cfg_attr(not(target_os = "redox"), allow(dead_code))]
const SCHEME_ROOT_ID: usize = 1;

#[cfg_attr(not(target_os = "redox"), allow(dead_code))]
struct Handle {
    blob_key: String,
    data: Arc<Vec<u8>>,
    map_count: usize,
    closed: bool,
}

#[cfg_attr(not(target_os = "redox"), allow(dead_code))]
pub struct FirmwareScheme {
    registry: FirmwareRegistry,
    next_id: usize,
    handles: BTreeMap<usize, Handle>,
}

#[cfg_attr(not(target_os = "redox"), allow(dead_code))]
impl FirmwareScheme {
    pub fn new(registry: FirmwareRegistry) -> Self {
        FirmwareScheme {
            registry,
            next_id: SCHEME_ROOT_ID + 1,
            handles: BTreeMap::new(),
        }
    }

    fn handle(&self, id: usize) -> Result<&Handle> {
        self.handles.get(&id).ok_or(Error::new(EBADF))
    }

    fn handle_mut(&mut self, id: usize) -> Result<&mut Handle> {
        self.handles.get_mut(&id).ok_or(Error::new(EBADF))
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
    let key = cleaned.to_string();
    // Final sanity: key must be purely alphanumeric with /, -, _, .
    if !key
        .chars()
        .all(|c| c.is_alphanumeric() || c == '/' || c == '-' || c == '_' || c == '.')
    {
        log::warn!(
            "firmware-loader: rejecting invalid characters in key: {:?}",
            key
        );
        return None;
    }
    Some(key)
}

impl SchemeSync for FirmwareScheme {
    fn scheme_root(&mut self) -> Result<usize> {
        Ok(SCHEME_ROOT_ID)
    }

    fn openat(
        &mut self,
        dirfd: usize,
        path: &str,
        _flags: usize,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<OpenResult> {
        if dirfd != SCHEME_ROOT_ID {
            return Err(Error::new(EACCES));
        }

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
                map_count: 0,
                closed: false,
            },
        );

        Ok(OpenResult::ThisScheme {
            number: id,
            flags: NewFdFlags::empty(),
        })
    }

    fn read(
        &mut self,
        id: usize,
        buf: &mut [u8],
        offset: u64,
        _flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let handle = self.handle(id)?;
        let offset = usize::try_from(offset).map_err(|_| Error::new(EINVAL))?;
        let data = &handle.data;

        if offset >= data.len() {
            return Ok(0);
        }

        let available = data.len() - offset;
        let to_copy = available.min(buf.len());
        buf[..to_copy].copy_from_slice(&data[offset..offset + to_copy]);

        Ok(to_copy)
    }

    fn write(
        &mut self,
        id: usize,
        _buf: &[u8],
        _offset: u64,
        _flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let _ = self.handle(id)?;
        Err(Error::new(EROFS))
    }

    fn fpath(&mut self, id: usize, buf: &mut [u8], _ctx: &CallerCtx) -> Result<usize> {
        let handle = self.handle(id)?;
        let path = format!("firmware:/{}", handle.blob_key);
        let bytes = path.as_bytes();
        let len = bytes.len().min(buf.len());
        buf[..len].copy_from_slice(&bytes[..len]);
        Ok(len)
    }

    fn fstat(&mut self, id: usize, stat: &mut Stat, _ctx: &CallerCtx) -> Result<()> {
        let handle = self.handle(id)?;
        stat.st_mode = MODE_FILE | 0o444;
        stat.st_size = handle.data.len() as u64;
        stat.st_blksize = 4096;
        stat.st_blocks = (handle.data.len() as u64 + 511) / 512;
        stat.st_nlink = 1;

        Ok(())
    }

    fn fsync(&mut self, id: usize, _ctx: &CallerCtx) -> Result<()> {
        let _ = self.handle(id)?;
        Ok(())
    }

    fn fcntl(&mut self, id: usize, _cmd: usize, _arg: usize, _ctx: &CallerCtx) -> Result<usize> {
        let _ = self.handle(id)?;
        Ok(0)
    }

    fn fsize(&mut self, id: usize, _ctx: &CallerCtx) -> Result<u64> {
        let handle = self.handle(id)?;
        Ok(handle.data.len() as u64)
    }

    fn ftruncate(&mut self, id: usize, _len: u64, _ctx: &CallerCtx) -> Result<()> {
        let _ = self.handle(id)?;
        Err(Error::new(EROFS))
    }

    fn fevent(&mut self, id: usize, _flags: EventFlags, _ctx: &CallerCtx) -> Result<EventFlags> {
        let _ = self.handle(id)?;
        Ok(EventFlags::empty())
    }

    fn mmap_prep(
        &mut self,
        id: usize,
        offset: u64,
        size: usize,
        _flags: syscall::MapFlags,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let handle = self.handle_mut(id)?;
        let data_len = handle.data.len() as u64;

        if offset > data_len {
            return Err(Error::new(EINVAL));
        }
        if offset + size as u64 > data_len {
            return Err(Error::new(EINVAL));
        }

        let ptr = &handle.data[offset as usize] as *const u8;
        handle.map_count += 1;
        Ok(ptr as usize)
    }

    fn munmap(
        &mut self,
        id: usize,
        _offset: u64,
        _size: usize,
        _flags: syscall::MunmapFlags,
        _ctx: &CallerCtx,
    ) -> Result<()> {
        let handle = self.handle_mut(id)?;
        if handle.map_count > 0 {
            handle.map_count -= 1;
        }
        let should_cleanup = handle.closed && handle.map_count == 0;
        if should_cleanup {
            self.handles.remove(&id);
        }
        Ok(())
    }

    fn on_close(&mut self, id: usize) {
        if id == SCHEME_ROOT_ID {
            return;
        }

        if let Some(handle) = self.handles.get_mut(&id) {
            handle.closed = true;
            let should_remove = handle.map_count == 0;
            if should_remove {
                self.handles.remove(&id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_key;

    #[test]
    fn accepts_real_firmware_extensions() {
        assert_eq!(
            resolve_key("iwlwifi-bz-b0-gf-a0-92.ucode").as_deref(),
            Some("iwlwifi-bz-b0-gf-a0-92.ucode")
        );
        assert_eq!(
            resolve_key("iwlwifi-bz-b0-gf-a0.pnvm").as_deref(),
            Some("iwlwifi-bz-b0-gf-a0.pnvm")
        );
        assert_eq!(
            resolve_key("amdgpu/psp_13_0_0_sos.bin").as_deref(),
            Some("amdgpu/psp_13_0_0_sos.bin")
        );
    }
}
