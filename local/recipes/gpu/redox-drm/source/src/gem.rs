use std::collections::BTreeMap;

use log::debug;
use redox_driver_sys::dma::DmaBuffer;

use crate::driver::{DriverError, Result};

pub type GemHandle = u32;

const MAX_GEM_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct GemObject {
    #[allow(dead_code)]
    pub handle: GemHandle,
    #[allow(dead_code)]
    pub size: u64,
    pub phys_addr: usize,
    pub virt_addr: usize,
    pub gpu_addr: Option<u64>,
}

struct GemAllocation {
    object: GemObject,
    #[allow(dead_code)]
    dma: DmaBuffer,
}

pub struct GemManager {
    next_handle: GemHandle,
    objects: BTreeMap<GemHandle, GemAllocation>,
}

impl GemManager {
    pub fn new() -> Self {
        Self {
            next_handle: 1,
            objects: BTreeMap::new(),
        }
    }

    pub fn create(&mut self, size: u64) -> Result<GemHandle> {
        if size == 0 {
            return Err(DriverError::InvalidArgument(
                "GEM create size must be non-zero",
            ));
        }
        if size > MAX_GEM_BYTES {
            return Err(DriverError::InvalidArgument(
                "GEM create size exceeds the trusted shared-core limit",
            ));
        }

        let handle = self.next_handle;
        self.next_handle = self.next_handle.saturating_add(1);

        let dma = DmaBuffer::allocate(size as usize, 4096)
            .map_err(|e| DriverError::Buffer(format!("DMA allocation failed: {e}")))?;
        if !dma.is_physically_contiguous() {
            debug!(
                "redox-drm: GEM handle {} allocated without physically contiguous backing",
                handle
            );
        }

        let object = GemObject {
            handle,
            size,
            phys_addr: dma.physical_address(),
            virt_addr: dma.as_ptr() as usize,
            gpu_addr: None,
        };

        debug!(
            "redox-drm: created GEM handle {} size={} phys={:#x} virt={:#x}",
            handle, size, object.phys_addr, object.virt_addr
        );

        self.objects.insert(handle, GemAllocation { object, dma });
        Ok(handle)
    }

    pub fn close(&mut self, handle: GemHandle) -> Result<()> {
        if self.objects.remove(&handle).is_none() {
            return Err(DriverError::NotFound(format!(
                "unknown GEM handle {handle}"
            )));
        }
        Ok(())
    }

    pub fn mmap(&self, handle: GemHandle) -> Result<usize> {
        let allocation = self
            .objects
            .get(&handle)
            .ok_or_else(|| DriverError::NotFound(format!("unknown GEM handle {handle}")))?;
        Ok(allocation.object.virt_addr)
    }

    pub fn object(&self, handle: GemHandle) -> Result<&GemObject> {
        self.objects
            .get(&handle)
            .map(|allocation| &allocation.object)
            .ok_or_else(|| DriverError::NotFound(format!("unknown GEM handle {handle}")))
    }

    pub fn phys_addr(&self, handle: GemHandle) -> Result<usize> {
        Ok(self.object(handle)?.phys_addr)
    }

    pub fn set_gpu_addr(&mut self, handle: GemHandle, gpu_addr: u64) -> Result<()> {
        let allocation = self
            .objects
            .get_mut(&handle)
            .ok_or_else(|| DriverError::NotFound(format!("unknown GEM handle {handle}")))?;
        allocation.object.gpu_addr = Some(gpu_addr);
        Ok(())
    }

    pub fn gpu_addr(&self, handle: GemHandle) -> Result<Option<u64>> {
        Ok(self.object(handle)?.gpu_addr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_object_exists() {
        let mut mgr = GemManager::new();
        let h = mgr.create(4096).expect("create should succeed");
        let obj = mgr.object(h).expect("object should exist after create");
        assert_eq!(obj.handle, h);
        assert_eq!(obj.size, 4096);
    }

    #[test]
    fn close_removes_object() {
        let mut mgr = GemManager::new();
        let h = mgr.create(4096).expect("create should succeed");
        mgr.close(h).expect("close should succeed");
        assert!(mgr.object(h).is_err(), "object should be gone after close");
    }

    #[test]
    fn double_close_returns_error() {
        let mut mgr = GemManager::new();
        let h = mgr.create(4096).expect("create should succeed");
        mgr.close(h).expect("first close should succeed");
        assert!(mgr.close(h).is_err(), "second close should fail");
    }

    #[test]
    fn object_by_invalid_handle_returns_error() {
        let mgr = GemManager::new();
        assert!(
            mgr.object(99999).is_err(),
            "querying a non-existent handle should fail"
        );
    }
}
