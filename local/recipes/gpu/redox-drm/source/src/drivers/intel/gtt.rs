use std::collections::BTreeMap;

use log::{debug, info};
use redox_driver_sys::memory::MmioRegion;

use crate::driver::{DriverError, Result};

const GTT_BASE: usize = 0x0000;
const GFX_FLSH_CNTL_REG: usize = 0x101008;
const GFX_FLSH_CNTL_EN: u32 = 1 << 0;

const GTT_PAGE_SIZE: u64 = 4096;
const GTT_PAGE_MASK: u64 = GTT_PAGE_SIZE - 1;
const GTT_PTE_PRESENT: u64 = 1 << 0;
const GTT_PTE_WRITE: u64 = 1 << 1;
const GTT_PTE_ADDR_MASK: u64 = 0xFFFF_FFFF_FFFF_F000;

pub struct IntelGtt {
    gtt_mmio: MmioRegion,
    control_mmio: MmioRegion,
    page_count: usize,
    aperture_size: u64,
    next_allocation: u64,
    free_list: Vec<(u64, u64)>,
    mappings: BTreeMap<u64, u64>,
}

impl IntelGtt {
    pub fn init(gtt_mmio: MmioRegion, control_mmio: MmioRegion) -> Result<Self> {
        let page_count = gtt_mmio.size() / core::mem::size_of::<u64>();
        if page_count == 0 {
            return Err(DriverError::Initialization(
                "Intel GGTT BAR exposes no page table entries".to_string(),
            ));
        }

        let aperture_size = (page_count as u64)
            .checked_mul(GTT_PAGE_SIZE)
            .ok_or_else(|| DriverError::Initialization("Intel GGTT aperture overflow".into()))?;

        let gtt = Self {
            gtt_mmio,
            control_mmio,
            page_count,
            aperture_size,
            next_allocation: 0,
            free_list: Vec::new(),
            mappings: BTreeMap::new(),
        };

        gtt.flush()?;
        info!(
            "redox-drm: Intel GGTT initialized with {} entries ({:#x} aperture)",
            page_count, aperture_size
        );
        Ok(gtt)
    }

    pub fn alloc_range(&mut self, size: u64) -> Result<u64> {
        let aligned_size = align_up(size, GTT_PAGE_SIZE)?;

        if let Some(index) = self
            .free_list
            .iter()
            .position(|&(_, free_size)| free_size >= aligned_size)
        {
            let (start, free_size) = self.free_list.remove(index);
            let remainder = free_size.saturating_sub(aligned_size);
            if remainder != 0 {
                self.free_list.push((start + aligned_size, remainder));
            }
            return Ok(start);
        }

        let start = self.next_allocation;
        let end = start
            .checked_add(aligned_size)
            .ok_or_else(|| DriverError::Buffer("Intel GGTT allocation overflow".into()))?;
        if end > self.aperture_size {
            return Err(DriverError::Buffer(format!(
                "Intel GGTT aperture exhausted: need {:#x} bytes, remaining {:#x}",
                aligned_size,
                self.aperture_size.saturating_sub(start)
            )));
        }

        self.next_allocation = end;
        Ok(start)
    }

    pub fn release_range(&mut self, gpu_addr: u64, size: u64) -> Result<()> {
        let aligned_size = align_up(size, GTT_PAGE_SIZE)?;
        self.free_list.push((gpu_addr, aligned_size));
        Ok(())
    }

    pub fn map_range(
        &mut self,
        gpu_addr: u64,
        phys_addr: u64,
        size: u64,
        flags: u64,
    ) -> Result<()> {
        let aligned_size = align_up(size, GTT_PAGE_SIZE)?;
        let page_count = (aligned_size / GTT_PAGE_SIZE) as usize;

        for page in 0..page_count {
            let page_offset = (page as u64) * GTT_PAGE_SIZE;
            self.insert_page(gpu_addr + page_offset, phys_addr + page_offset, flags)?;
        }

        self.mappings.insert(gpu_addr, aligned_size);
        self.flush()
    }

    pub fn unmap_range(&mut self, gpu_addr: u64, size: u64) -> Result<()> {
        let aligned_size = align_up(size, GTT_PAGE_SIZE)?;
        let page_count = (aligned_size / GTT_PAGE_SIZE) as usize;

        for page in 0..page_count {
            let page_offset = (page as u64) * GTT_PAGE_SIZE;
            self.remove_page(gpu_addr + page_offset)?;
        }

        self.mappings.remove(&gpu_addr);
        self.flush()
    }

    pub fn insert_page(&self, virtual_addr: u64, physical_addr: u64, flags: u64) -> Result<()> {
        ensure_page_alignment(virtual_addr, "virtual_addr")?;
        ensure_page_alignment(physical_addr, "physical_addr")?;

        let entry_index = self.entry_index(virtual_addr)?;
        let entry_offset = gtt_entry_offset(entry_index)?;
        self.ensure_gtt_access(entry_offset, core::mem::size_of::<u64>(), "GGTT PTE write")?;

        let pte = encode_pte(physical_addr, flags);
        self.gtt_mmio.write64(entry_offset, pte);
        debug!(
            "redox-drm: Intel GGTT map va={:#x} -> pa={:#x} flags={:#x}",
            virtual_addr, physical_addr, flags
        );
        Ok(())
    }

    pub fn remove_page(&self, virtual_addr: u64) -> Result<()> {
        ensure_page_alignment(virtual_addr, "virtual_addr")?;

        let entry_index = self.entry_index(virtual_addr)?;
        let entry_offset = gtt_entry_offset(entry_index)?;
        self.ensure_gtt_access(entry_offset, core::mem::size_of::<u64>(), "GGTT PTE clear")?;

        self.gtt_mmio.write64(entry_offset, 0);
        debug!("redox-drm: Intel GGTT unmap va={:#x}", virtual_addr);
        Ok(())
    }

    pub fn flush(&self) -> Result<()> {
        self.ensure_control_access(GFX_FLSH_CNTL_REG, core::mem::size_of::<u32>(), "GGTT flush")?;
        self.control_mmio
            .write32(GFX_FLSH_CNTL_REG, GFX_FLSH_CNTL_EN);
        let _ = self.control_mmio.read32(GFX_FLSH_CNTL_REG);
        Ok(())
    }

    fn entry_index(&self, virtual_addr: u64) -> Result<usize> {
        let entry_index = (virtual_addr / GTT_PAGE_SIZE) as usize;
        if entry_index >= self.page_count {
            return Err(DriverError::Buffer(format!(
                "Intel GGTT entry {entry_index} outside aperture of {} entries",
                self.page_count
            )));
        }
        Ok(entry_index)
    }

    fn ensure_gtt_access(&self, offset: usize, width: usize, op: &str) -> Result<()> {
        ensure_mmio_access(self.gtt_mmio.size(), offset, width, op)
    }

    fn ensure_control_access(&self, offset: usize, width: usize, op: &str) -> Result<()> {
        ensure_mmio_access(self.control_mmio.size(), offset, width, op)
    }
}

fn align_up(value: u64, alignment: u64) -> Result<u64> {
    value
        .checked_add(alignment - 1)
        .map(|v| v & !(alignment - 1))
        .ok_or_else(|| DriverError::Buffer("Intel GGTT size alignment overflow".into()))
}

fn ensure_page_alignment(value: u64, name: &'static str) -> Result<()> {
    if value & GTT_PAGE_MASK != 0 {
        return Err(DriverError::InvalidArgument(name));
    }
    Ok(())
}

fn gtt_entry_offset(entry_index: usize) -> Result<usize> {
    GTT_BASE
        .checked_add(
            entry_index
                .checked_mul(core::mem::size_of::<u64>())
                .ok_or_else(|| DriverError::Mmio("Intel GGTT entry offset overflow".into()))?,
        )
        .ok_or_else(|| DriverError::Mmio("Intel GGTT base offset overflow".into()))
}

fn ensure_mmio_access(mmio_size: usize, offset: usize, width: usize, op: &str) -> Result<()> {
    let end = offset
        .checked_add(width)
        .ok_or_else(|| DriverError::Mmio(format!("{op} offset overflow at {offset:#x}")))?;
    if end > mmio_size {
        return Err(DriverError::Mmio(format!(
            "{op} outside MMIO aperture: end={end:#x} size={mmio_size:#x}"
        )));
    }
    Ok(())
}

fn encode_pte(physical_addr: u64, flags: u64) -> u64 {
    (physical_addr & GTT_PTE_ADDR_MASK)
        | (flags & (GTT_PTE_PRESENT | GTT_PTE_WRITE))
        | GTT_PTE_PRESENT
}
