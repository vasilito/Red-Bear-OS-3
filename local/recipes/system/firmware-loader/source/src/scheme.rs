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
    use super::*;
    use crate::blob::FirmwareRegistry;
    use redox_scheme::scheme::SchemeSync;
    use redox_scheme::{CallerCtx, OpenResult};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};
    use syscall::{EventFlags, MapFlags, MunmapFlags, Stat, MODE_FILE};

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

    // --- Helpers ---

    fn test_ctx() -> CallerCtx {
        CallerCtx {
            pid: 0,
            uid: 0,
            gid: 0,
            id: unsafe { std::mem::zeroed() },
        }
    }

    fn setup_registry() -> (PathBuf, FirmwareRegistry) {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("rbos-fw-scheme-{stamp}"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("test-blob.bin"), b"Hello, firmware!").unwrap();
        fs::create_dir_all(dir.join("subdir")).unwrap();
        fs::write(dir.join("subdir/nested.bin"), b"nested data content").unwrap();
        let registry = FirmwareRegistry::new(&dir).unwrap();
        (dir, registry)
    }

    fn open_test_blob(scheme: &mut FirmwareScheme) -> usize {
        let ctx = test_ctx();
        match scheme
            .openat(SCHEME_ROOT_ID, "test-blob.bin", 0, 0, &ctx)
            .unwrap()
        {
            OpenResult::ThisScheme { number, .. } => number,
            other => panic!("expected ThisScheme, got {:?}", other),
        }
    }

    #[test]
    fn new_creates_empty_scheme_with_correct_next_id() {
        let (dir, registry) = setup_registry();
        let scheme = FirmwareScheme::new(registry);
        assert!(scheme.handles.is_empty());
        assert_eq!(scheme.next_id, SCHEME_ROOT_ID + 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn openat_valid_key_returns_this_scheme() {
        let (dir, registry) = setup_registry();
        let mut scheme = FirmwareScheme::new(registry);
        let ctx = test_ctx();

        let result = scheme
            .openat(SCHEME_ROOT_ID, "test-blob.bin", 0, 0, &ctx)
            .unwrap();

        match result {
            OpenResult::ThisScheme { number, flags } => {
                assert_eq!(number, SCHEME_ROOT_ID + 1);
                assert_eq!(flags, NewFdFlags::empty());
            }
            other => panic!("expected ThisScheme, got {:?}", other),
        }
        assert_eq!(scheme.handles.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn openat_missing_key_returns_enoent() {
        let (dir, registry) = setup_registry();
        let mut scheme = FirmwareScheme::new(registry);
        let ctx = test_ctx();

        let err = scheme
            .openat(SCHEME_ROOT_ID, "nonexistent.bin", 0, 0, &ctx)
            .unwrap_err();
        assert_eq!(err.errno, ENOENT);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn openat_rejects_path_traversal() {
        let (dir, registry) = setup_registry();
        let mut scheme = FirmwareScheme::new(registry);
        let ctx = test_ctx();

        let err = scheme
            .openat(SCHEME_ROOT_ID, "../etc/passwd", 0, 0, &ctx)
            .unwrap_err();
        assert_eq!(err.errno, EISDIR);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn openat_empty_path_returns_eisdir() {
        let (dir, registry) = setup_registry();
        let mut scheme = FirmwareScheme::new(registry);
        let ctx = test_ctx();

        let err = scheme
            .openat(SCHEME_ROOT_ID, "", 0, 0, &ctx)
            .unwrap_err();
        assert_eq!(err.errno, EISDIR);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn openat_wrong_dirfd_returns_eacces() {
        let (dir, registry) = setup_registry();
        let mut scheme = FirmwareScheme::new(registry);
        let ctx = test_ctx();

        let err = scheme
            .openat(999, "test-blob.bin", 0, 0, &ctx)
            .unwrap_err();
        assert_eq!(err.errno, EACCES);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_at_offset_zero() {
        let (dir, registry) = setup_registry();
        let mut scheme = FirmwareScheme::new(registry);
        let id = open_test_blob(&mut scheme);
        let ctx = test_ctx();

        let mut buf = [0u8; 64];
        let n = scheme.read(id, &mut buf, 0, 0, &ctx).unwrap();
        assert_eq!(n, 16);
        assert_eq!(&buf[..16], b"Hello, firmware!");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_at_nonzero_offset() {
        let (dir, registry) = setup_registry();
        let mut scheme = FirmwareScheme::new(registry);
        let id = open_test_blob(&mut scheme);
        let ctx = test_ctx();

        let mut buf = [0u8; 64];
        let n = scheme.read(id, &mut buf, 7, 0, &ctx).unwrap();
        assert_eq!(n, 9);
        assert_eq!(&buf[..9], b"firmware!");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_past_end_returns_zero() {
        let (dir, registry) = setup_registry();
        let mut scheme = FirmwareScheme::new(registry);
        let id = open_test_blob(&mut scheme);
        let ctx = test_ctx();

        let mut buf = [0u8; 64];
        let n = scheme.read(id, &mut buf, 16, 0, &ctx).unwrap();
        assert_eq!(n, 0);
        let n2 = scheme.read(id, &mut buf, 1000, 0, &ctx).unwrap();
        assert_eq!(n2, 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn fstat_reports_correct_size_and_mode() {
        let (dir, registry) = setup_registry();
        let mut scheme = FirmwareScheme::new(registry);
        let id = open_test_blob(&mut scheme);
        let ctx = test_ctx();

        let mut stat: Stat = unsafe { std::mem::zeroed() };
        scheme.fstat(id, &mut stat, &ctx).unwrap();
        assert_eq!(stat.st_mode, MODE_FILE | 0o444);
        assert_eq!(stat.st_size, 16);
        assert_eq!(stat.st_blksize, 4096);
        assert!(stat.st_blocks > 0);
        assert_eq!(stat.st_nlink, 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn fsize_returns_correct_length() {
        let (dir, registry) = setup_registry();
        let mut scheme = FirmwareScheme::new(registry);
        let id = open_test_blob(&mut scheme);
        let ctx = test_ctx();

        let size = scheme.fsize(id, &ctx).unwrap();
        assert_eq!(size, 16);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_returns_erofs() {
        let (dir, registry) = setup_registry();
        let mut scheme = FirmwareScheme::new(registry);
        let id = open_test_blob(&mut scheme);
        let ctx = test_ctx();

        let err = scheme.write(id, b"test", 0, 0, &ctx).unwrap_err();
        assert_eq!(err.errno, EROFS);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ftruncate_returns_erofs() {
        let (dir, registry) = setup_registry();
        let mut scheme = FirmwareScheme::new(registry);
        let id = open_test_blob(&mut scheme);
        let ctx = test_ctx();

        let err = scheme.ftruncate(id, 0, &ctx).unwrap_err();
        assert_eq!(err.errno, EROFS);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn mmap_prep_returns_pointer_and_increments_count() {
        let (dir, registry) = setup_registry();
        let mut scheme = FirmwareScheme::new(registry);
        let id = open_test_blob(&mut scheme);
        let ctx = test_ctx();

        let ptr = scheme
            .mmap_prep(id, 0, 16, MapFlags::empty(), &ctx)
            .unwrap();
        assert_ne!(ptr, 0);

        let handle = scheme.handles.get(&id).unwrap();
        assert_eq!(handle.map_count, 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn mmap_prep_rejects_offset_beyond_data() {
        let (dir, registry) = setup_registry();
        let mut scheme = FirmwareScheme::new(registry);
        let id = open_test_blob(&mut scheme);
        let ctx = test_ctx();

        let err = scheme
            .mmap_prep(id, 17, 1, MapFlags::empty(), &ctx)
            .unwrap_err();
        assert_eq!(err.errno, EINVAL);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn mmap_prep_rejects_offset_plus_size_beyond_data() {
        let (dir, registry) = setup_registry();
        let mut scheme = FirmwareScheme::new(registry);
        let id = open_test_blob(&mut scheme);
        let ctx = test_ctx();

        let err = scheme
            .mmap_prep(id, 8, 16, MapFlags::empty(), &ctx)
            .unwrap_err();
        assert_eq!(err.errno, EINVAL);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn munmap_decrements_count_without_removing_handle() {
        let (dir, registry) = setup_registry();
        let mut scheme = FirmwareScheme::new(registry);
        let id = open_test_blob(&mut scheme);
        let ctx = test_ctx();

        scheme
            .mmap_prep(id, 0, 16, MapFlags::empty(), &ctx)
            .unwrap();
        assert_eq!(scheme.handles.get(&id).unwrap().map_count, 1);

        scheme
            .munmap(id, 0, 16, MunmapFlags::empty(), &ctx)
            .unwrap();

        assert!(scheme.handles.contains_key(&id));
        let handle = scheme.handles.get(&id).unwrap();
        assert_eq!(handle.map_count, 0);
        assert!(!handle.closed);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn on_close_keeps_handle_when_mapped() {
        let (dir, registry) = setup_registry();
        let mut scheme = FirmwareScheme::new(registry);
        let id = open_test_blob(&mut scheme);
        let ctx = test_ctx();

        scheme
            .mmap_prep(id, 0, 16, MapFlags::empty(), &ctx)
            .unwrap();

        scheme.on_close(id);

        assert!(scheme.handles.contains_key(&id));
        let handle = scheme.handles.get(&id).unwrap();
        assert!(handle.closed);
        assert_eq!(handle.map_count, 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn on_close_then_munmap_removes_handle() {
        let (dir, registry) = setup_registry();
        let mut scheme = FirmwareScheme::new(registry);
        let id = open_test_blob(&mut scheme);
        let ctx = test_ctx();

        scheme
            .mmap_prep(id, 0, 16, MapFlags::empty(), &ctx)
            .unwrap();

        scheme.on_close(id);
        assert!(scheme.handles.contains_key(&id));

        scheme
            .munmap(id, 0, 16, MunmapFlags::empty(), &ctx)
            .unwrap();
        assert!(!scheme.handles.contains_key(&id));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn fsync_returns_ok() {
        let (dir, registry) = setup_registry();
        let mut scheme = FirmwareScheme::new(registry);
        let id = open_test_blob(&mut scheme);
        let ctx = test_ctx();

        scheme.fsync(id, &ctx).unwrap();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn fcntl_returns_ok_zero() {
        let (dir, registry) = setup_registry();
        let mut scheme = FirmwareScheme::new(registry);
        let id = open_test_blob(&mut scheme);
        let ctx = test_ctx();

        let result = scheme.fcntl(id, 0, 0, &ctx).unwrap();
        assert_eq!(result, 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn fevent_returns_empty_flags() {
        let (dir, registry) = setup_registry();
        let mut scheme = FirmwareScheme::new(registry);
        let id = open_test_blob(&mut scheme);
        let ctx = test_ctx();

        let flags = scheme
            .fevent(id, EventFlags::empty(), &ctx)
            .unwrap();
        assert_eq!(flags, EventFlags::empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
