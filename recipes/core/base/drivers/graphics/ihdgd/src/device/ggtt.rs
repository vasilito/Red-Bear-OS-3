use std::sync::Arc;
use std::{mem, ptr};

use pcid_interface::PciFunctionHandle;
use range_alloc::RangeAllocator;
use syscall::{Error, EIO};

use crate::device::MmioRegion;

/// Global Graphics Translation Table (global GTT)
///
/// The global GTT is a page table used by all parts of the GPU that don't use
/// the PPGTT (Per-Process GTT). This includes the display engine and the GM
/// aperture that the CPU can access.
///
/// The global GTT is located in the GTTMM BAR at offset 8MiB, is up to 8MiB big
/// and consists of 64bit entries. Each entry has a present bit as LSB and the
/// address of the frame at bits 12 through 38. The rest of the bits are ignored.
///
/// Source: Pages 6 and 75 of intel-gfx-prm-osrc-kbl-vol05-memory_views.pdf
pub struct GlobalGtt {
    gttmm: Arc<MmioRegion>,
    /// Base the GTT
    gtt_base: *mut u64,
    /// Size of the GTT
    gtt_size: usize,

    /// Allocator for GM aperture pages
    gm_alloc: RangeAllocator<u32>,

    // FIXME reuse DSM memory for something useful
    /// Base Data of Stolen Memory (DSM)
    base_dsm: *mut (),
    /// Size of DSM
    size_data_stolen_memory: usize,
}

const GTT_PAGE_SIZE: u32 = 4096;

impl GlobalGtt {
    pub unsafe fn new(
        pcid_handle: &mut PciFunctionHandle,
        gttmm: Arc<MmioRegion>,
        gm_size: u32,
    ) -> Self {
        let gtt_offset = 8 * 1024 * 1024;
        let gtt_base = ptr::with_exposed_provenance_mut(gttmm.virt + gtt_offset);

        let base_dsm = unsafe { pcid_handle.read_config(0x5C) };
        let ggc = unsafe { pcid_handle.read_config(0x50) };

        let dsm_size = match (ggc >> 8) & 0xFF {
            size if size & 0xF0 == 0 => size * 32 * 1024 * 1024,
            size => (size & !0xF0) * 4 * 1024 * 1024,
        } as usize;
        let gtt_size = match (ggc >> 6) & 0x3 {
            0 => 0,
            1 => 2 * 1024 * 1024,
            2 => 4 * 1024 * 1024,
            3 => 8 * 1024 * 1024,
            _ => unreachable!(),
        } as usize;

        log::info!("Base DSM: {:X}", base_dsm);
        log::info!(
            "GGC: {:X} => global GTT size: {}MiB; DSM size: {}MiB",
            ggc,
            gtt_size / 1024 / 1024,
            dsm_size / 1024 / 1024,
        );

        let gm_alloc = RangeAllocator::new(0..gm_size / 4096);

        GlobalGtt {
            gttmm,
            gtt_base,
            gtt_size,
            gm_alloc,
            base_dsm: core::ptr::with_exposed_provenance_mut(base_dsm as usize),
            size_data_stolen_memory: dsm_size,
        }
    }

    /// Reset the global GTT by clearing out all existing mappings.
    pub unsafe fn reset(&mut self) {
        for i in 0..self.gtt_size / 8 {
            unsafe { *self.gtt_base.add(i) = 0 };
        }
    }

    pub fn reserve(&mut self, surf: u32, surf_size: u32) {
        assert!(surf.is_multiple_of(GTT_PAGE_SIZE));
        assert!(surf_size.is_multiple_of(GTT_PAGE_SIZE));

        self.gm_alloc
            .allocate_exact_range(
                surf / GTT_PAGE_SIZE..surf / GTT_PAGE_SIZE + surf_size / GTT_PAGE_SIZE,
            )
            .unwrap_or_else(|err| {
                panic!(
                    "failed to allocate pre-existing surface at 0x{:x} of size {}: {:?}",
                    surf, surf_size, err
                );
            });
    }

    pub fn alloc_phys_mem(&mut self, size: u32) -> syscall::Result<u32> {
        let size = size.next_multiple_of(GTT_PAGE_SIZE);

        let sgl = common::sgl::Sgl::new(size as usize)?;

        let range = self
            .gm_alloc
            .allocate_range(size / GTT_PAGE_SIZE)
            .map_err(|err| {
                log::warn!("failed to allocate buffer of size {}: {:?}", size, err);
                Error::new(EIO)
            })?;

        for chunk in sgl.chunks() {
            for i in 0..chunk.length / GTT_PAGE_SIZE as usize {
                unsafe {
                    *self
                        .gtt_base
                        .add(range.start as usize + chunk.offset / GTT_PAGE_SIZE as usize + i) =
                        chunk.phys as u64 + i as u64 * u64::from(GTT_PAGE_SIZE) + 1;
                }
            }
        }
        mem::forget(sgl);

        Ok(range.start * 4096)
    }
}
