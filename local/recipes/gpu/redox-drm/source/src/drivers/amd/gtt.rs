use std::collections::BTreeMap;

use log::{info, warn};
use redox_driver_sys::dma::DmaBuffer;
use redox_driver_sys::memory::MmioRegion;

use crate::driver::{DriverError, Result};

const GPU_PAGE_SIZE: u64 = 4096;
const PAGE_TABLE_LEVELS: usize = 4;
const PTE_COUNT: usize = 512;
const PT_BYTES: usize = PTE_COUNT * 8;
const PTE_INDEX_MASK: u64 = 0x1ff;
const PAGE_OFFSET_MASK: u64 = GPU_PAGE_SIZE - 1;
const AMD_PTE_VALID: u64 = 1 << 0;
const AMD_PTE_SYSTEM: u64 = 1 << 1;
const AMD_PTE_FLAG_MASK: u64 = 0x0fff;
const AMD_PTE_ADDR_MASK: u64 = 0x000f_ffff_ffff_f000;
const GTT_MIN_VA_SIZE: u64 = 256 * 1024 * 1024;
const TLB_POLL_LIMIT: usize = 10_000;

// GC 11.0 (RDNA2) VM register offsets (DWORD index * 4 = byte offset)
const MM_VM_CONTEXT0_CNTL: usize = 0x1688 * 4;
const MM_VM_CONTEXT0_PT_BASE_LO32: usize = 0x16f3 * 4;
const MM_VM_CONTEXT0_PT_BASE_HI32: usize = 0x16f4 * 4;
const MM_VM_CONTEXT0_PT_START_LO32: usize = 0x1713 * 4;
const MM_VM_CONTEXT0_PT_START_HI32: usize = 0x1714 * 4;
const MM_VM_CONTEXT0_PT_END_LO32: usize = 0x1733 * 4;
const MM_VM_CONTEXT0_PT_END_HI32: usize = 0x1734 * 4;
const MMVM_INVALIDATE_ENG0_REQ: usize = 0x16ab * 4;
const MMVM_INVALIDATE_ENG0_ACK: usize = 0x16bd * 4;

struct PageTable {
    dma: DmaBuffer,
    children: BTreeMap<usize, Box<PageTable>>,
}

impl PageTable {
    fn allocate() -> Result<Self> {
        let dma = DmaBuffer::allocate(PT_BYTES, 4096)
            .map_err(|e| DriverError::Buffer(format!("GTT page table alloc failed: {e}")))?;
        if !dma.is_physically_contiguous() {
            warn!("redox-drm: GTT page table not guaranteed physically contiguous");
        }
        Ok(Self {
            dma,
            children: BTreeMap::new(),
        })
    }

    fn phys(&self) -> u64 {
        self.dma.physical_address() as u64
    }

    fn entries(&self) -> &[u64] {
        unsafe { std::slice::from_raw_parts(self.dma.as_ptr() as *const u64, PTE_COUNT) }
    }

    fn entries_mut(&mut self) -> &mut [u64] {
        unsafe { std::slice::from_raw_parts_mut(self.dma.as_mut_ptr() as *mut u64, PTE_COUNT) }
    }

    fn map_page(&mut self, level: usize, gpu_addr: u64, phys_addr: u64, flags: u64) -> Result<()> {
        let idx = pt_index(gpu_addr, level)?;
        if level == PAGE_TABLE_LEVELS - 1 {
            self.entries_mut()[idx] = encode_pte(phys_addr, flags);
            return Ok(());
        }
        let child = match self.children.get_mut(&idx) {
            Some(c) => c,
            None => {
                let c = Box::new(PageTable::allocate()?);
                let c_phys = c.phys();
                self.entries_mut()[idx] =
                    (c_phys & AMD_PTE_ADDR_MASK) | AMD_PTE_VALID | AMD_PTE_SYSTEM;
                self.children.entry(idx).or_insert(c)
            }
        };
        child.map_page(level + 1, gpu_addr, phys_addr, flags)
    }

    fn unmap_page(&mut self, level: usize, gpu_addr: u64) -> Result<()> {
        let idx = pt_index(gpu_addr, level)?;
        if level == PAGE_TABLE_LEVELS - 1 {
            self.entries_mut()[idx] = 0;
            return Ok(());
        }
        if let Some(child) = self.children.get_mut(&idx) {
            child.unmap_page(level + 1, gpu_addr)?;
        }
        Ok(())
    }

    fn translate(&self, level: usize, gpu_addr: u64) -> Option<u64> {
        let idx = pt_index(gpu_addr, level).ok()?;
        let entry = self.entries()[idx];
        if entry & AMD_PTE_VALID == 0 {
            return None;
        }
        if level == PAGE_TABLE_LEVELS - 1 {
            return Some((entry & AMD_PTE_ADDR_MASK) | (gpu_addr & PAGE_OFFSET_MASK));
        }
        self.children.get(&idx)?.translate(level + 1, gpu_addr)
    }
}

pub struct GttManager {
    initialized: bool,
    root: Option<PageTable>,
    va_start: u64,
    va_end: u64,
    fb_offset: u64,
    next_alloc: u64,
    free_list: Vec<(u64, u64)>,
}

impl Default for GttManager {
    fn default() -> Self {
        Self::new()
    }
}

impl GttManager {
    pub fn new() -> Self {
        Self {
            initialized: false,
            root: None,
            va_start: 0,
            va_end: GTT_MIN_VA_SIZE - 1,
            fb_offset: 0,
            next_alloc: 0,
            free_list: Vec::new(),
        }
    }

    pub fn initialize(&mut self) -> Result<()> {
        if self.root.is_none() {
            self.root = Some(PageTable::allocate()?);
        }
        self.fb_offset = 0;
        self.va_start = self.fb_offset;
        self.va_end = self
            .va_start
            .checked_add(GTT_MIN_VA_SIZE)
            .ok_or_else(|| DriverError::Initialization("GTT VA range overflow".into()))?;
        self.next_alloc = self.va_start;
        self.initialized = true;
        info!(
            "redox-drm: AMD GTT initialized va={:#x}..{:#x} root_pt={:#x}",
            self.va_start,
            self.va_end,
            self.root.as_ref().map(|r| r.phys()).unwrap_or(0)
        );
        Ok(())
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    pub fn alloc_gpu_range(&mut self, size: u64) -> Result<u64> {
        self.ensure_init()?;
        let aligned_size = (size + GPU_PAGE_SIZE - 1) & !(GPU_PAGE_SIZE - 1);
        if let Some(idx) = self.free_list.iter().position(|&(_, s)| s >= aligned_size) {
            let (start, free_size) = self.free_list.remove(idx);
            let remainder = free_size - aligned_size;
            if remainder > 0 {
                self.free_list.push((start + aligned_size, remainder));
            }
            return Ok(start);
        }
        let gpu_addr = self.next_alloc;
        let new_next = gpu_addr
            .checked_add(aligned_size)
            .ok_or_else(|| DriverError::Buffer("GTT VA allocation overflow".into()))?;
        if new_next > self.va_end {
            return Err(DriverError::Buffer(format!(
                "GTT VA space exhausted: need {:#x}..{:#x}, have ..{:#x}",
                gpu_addr, new_next, self.va_end
            )));
        }
        self.next_alloc = new_next;
        Ok(gpu_addr)
    }

    pub fn unmap_range(&mut self, gpu_start: u64, size: u64) -> Result<()> {
        self.ensure_init()?;
        let aligned_size = (size + GPU_PAGE_SIZE - 1) & !(GPU_PAGE_SIZE - 1);
        let num_pages = (aligned_size / GPU_PAGE_SIZE) as usize;
        for i in 0..num_pages {
            let gpu_addr = gpu_start + (i as u64) * GPU_PAGE_SIZE;
            self.root
                .as_mut()
                .ok_or_else(|| DriverError::Initialization("GTT root missing".into()))?
                .unmap_page(0, gpu_addr)?;
        }
        Ok(())
    }

    pub fn release_range(&mut self, gpu_start: u64, size: u64) {
        let aligned_size = (size + GPU_PAGE_SIZE - 1) & !(GPU_PAGE_SIZE - 1);
        self.free_list.push((gpu_start, aligned_size));
    }

    pub fn map_page(&mut self, gpu_addr: u64, phys_addr: u64, flags: u64) -> Result<()> {
        self.ensure_init()?;
        if gpu_addr & PAGE_OFFSET_MASK != 0 {
            return Err(DriverError::InvalidArgument("gpu_addr not page-aligned"));
        }
        if phys_addr & PAGE_OFFSET_MASK != 0 {
            return Err(DriverError::InvalidArgument("phys_addr not page-aligned"));
        }
        if gpu_addr < self.va_start || gpu_addr > self.va_end {
            return Err(DriverError::InvalidArgument(
                "gpu_addr outside GTT aperture",
            ));
        }
        self.root
            .as_mut()
            .ok_or_else(|| DriverError::Initialization("GTT root missing".into()))?
            .map_page(0, gpu_addr, phys_addr, flags)
    }

    pub fn unmap_page(&mut self, gpu_addr: u64) -> Result<()> {
        self.ensure_init()?;
        self.root
            .as_mut()
            .ok_or_else(|| DriverError::Initialization("GTT root missing".into()))?
            .unmap_page(0, gpu_addr)
    }

    pub fn map_range(
        &mut self,
        gpu_start: u64,
        phys_start: u64,
        size: u64,
        flags: u64,
    ) -> Result<()> {
        self.ensure_init()?;
        let aligned_size = (size + GPU_PAGE_SIZE - 1) & !(GPU_PAGE_SIZE - 1);
        let num_pages = (aligned_size / GPU_PAGE_SIZE) as usize;
        for i in 0..num_pages {
            let gpu_addr = gpu_start + (i as u64) * GPU_PAGE_SIZE;
            let phys_addr = phys_start + (i as u64) * GPU_PAGE_SIZE;
            self.map_page(gpu_addr, phys_addr, flags)?;
        }
        Ok(())
    }

    pub fn flush_tlb(&self, mmio: &MmioRegion) -> Result<()> {
        if !self.initialized {
            return Err(DriverError::Initialization("GTT not initialized".into()));
        }
        let req =
            (1u32 << 0) | (1u32 << 19) | (1u32 << 20) | (1u32 << 21) | (1u32 << 22) | (1u32 << 23);
        mmio.write32(MMVM_INVALIDATE_ENG0_REQ, req);
        for _ in 0..TLB_POLL_LIMIT {
            let ack = mmio.read32(MMVM_INVALIDATE_ENG0_ACK);
            if ack & (1u32 << 0) != 0 {
                return Ok(());
            }
        }
        Err(DriverError::Mmio("GTT TLB flush timeout".into()))
    }

    pub fn translate(&self, gpu_addr: u64) -> Option<u64> {
        if !self.initialized || gpu_addr < self.va_start || gpu_addr > self.va_end {
            return None;
        }
        self.root.as_ref()?.translate(0, gpu_addr)
    }

    pub fn program_vm_context(&self, mmio: &MmioRegion) -> Result<()> {
        let root_phys = self
            .root
            .as_ref()
            .map(|r| r.phys())
            .ok_or_else(|| DriverError::Initialization("GTT root missing".into()))?;

        mmio.write32(MM_VM_CONTEXT0_PT_BASE_LO32, root_phys as u32);
        mmio.write32(MM_VM_CONTEXT0_PT_BASE_HI32, (root_phys >> 32) as u32);

        let va_start_pages = self.va_start >> 12;
        let va_end_pages = self.va_end >> 12;
        mmio.write32(MM_VM_CONTEXT0_PT_START_LO32, va_start_pages as u32);
        mmio.write32(MM_VM_CONTEXT0_PT_START_HI32, (va_start_pages >> 32) as u32);
        mmio.write32(MM_VM_CONTEXT0_PT_END_LO32, va_end_pages as u32);
        mmio.write32(MM_VM_CONTEXT0_PT_END_HI32, (va_end_pages >> 32) as u32);

        // Enable VM context 0: depth=0 (4-level), block_size=0 (4KB pages)
        mmio.write32(MM_VM_CONTEXT0_CNTL, 1);

        self.flush_tlb(mmio)
    }

    fn ensure_init(&self) -> Result<()> {
        if !self.initialized {
            return Err(DriverError::Initialization(
                "GTT manager not initialized".into(),
            ));
        }
        Ok(())
    }
}

fn pt_index(gpu_addr: u64, level: usize) -> Result<usize> {
    if level >= PAGE_TABLE_LEVELS {
        return Err(DriverError::Initialization(format!(
            "invalid PT level {level}"
        )));
    }
    let shift = 12 + ((PAGE_TABLE_LEVELS - 1 - level) * 9);
    Ok(((gpu_addr >> shift) & PTE_INDEX_MASK) as usize)
}

fn encode_pte(phys_addr: u64, flags: u64) -> u64 {
    (phys_addr & AMD_PTE_ADDR_MASK) | (flags & AMD_PTE_FLAG_MASK) | AMD_PTE_VALID | AMD_PTE_SYSTEM
}
