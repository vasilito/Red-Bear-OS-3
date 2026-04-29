use std::{ptr, slice};

use crate::device::ggtt::GlobalGtt;
use crate::device::MmioRegion;

#[derive(Debug)]
pub struct GpuBuffer {
    pub virt: *mut u8,
    pub gm_offset: u32,
    pub size: u32,
}

impl GpuBuffer {
    pub unsafe fn new(gm: &MmioRegion, gm_offset: u32, size: u32, clear: bool) -> Self {
        let virt = ptr::with_exposed_provenance_mut::<u8>(gm.virt + gm_offset as usize);

        if clear {
            let onscreen = slice::from_raw_parts_mut(virt, size as usize);
            onscreen.fill(0);
        }

        Self {
            virt,
            gm_offset,
            size,
        }
    }

    pub fn alloc(gm: &MmioRegion, ggtt: &mut GlobalGtt, size: u32) -> syscall::Result<Self> {
        let gm_offset = ggtt.alloc_phys_mem(size)?;

        Ok(unsafe { GpuBuffer::new(gm, gm_offset, size, true) })
    }

    pub fn alloc_dumb(
        gm: &MmioRegion,
        ggtt: &mut GlobalGtt,
        width: u32,
        height: u32,
    ) -> syscall::Result<(Self, u32)> {
        //TODO: documentation on this is not great
        let stride = (width * 4).next_multiple_of(64);

        Ok((GpuBuffer::alloc(gm, ggtt, stride * height)?, stride))
    }
}
