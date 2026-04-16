use std::collections::BTreeMap;

use log::debug;
use redox_driver_sys::dma::DmaBuffer;

use crate::driver::{DriverError, Result};

pub type GemHandle = u32;

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

    #[allow(dead_code)]
    pub fn object_mut_ptr(&mut self, handle: GemHandle) -> Result<usize> {
        let allocation = self
            .objects
            .get_mut(&handle)
            .ok_or_else(|| DriverError::NotFound(format!("unknown GEM handle {handle}")))?;
        Ok(allocation.dma.as_mut_ptr() as usize)
    }
}
