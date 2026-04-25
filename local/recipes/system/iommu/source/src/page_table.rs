use core::alloc::Layout;
use core::mem::size_of;
use core::ptr::NonNull;
use core::slice;
use std::collections::BTreeMap;

use redox_driver_sys::dma::DmaBuffer;

pub const PAGE_SIZE: u64 = 4096;
pub const PTES_PER_PAGE: usize = 512;
pub const DEFAULT_IOMMU_LEVELS: u8 = 4;
pub const DEFAULT_IOVA_BASE: u64 = 0x1_0000_0000;
pub const DEFAULT_IOVA_LIMIT: u64 = 0x0000_FFFF_FFFF_F000;

const PTE_PRESENT: u64 = 1 << 0;
const PTE_USER: u64 = 1 << 1;
const PTE_WRITE: u64 = 1 << 2;
const PTE_READ: u64 = 1 << 3;
const PTE_NEXT_LEVEL_SHIFT: u64 = 9;
const PTE_NEXT_LEVEL_MASK: u64 = 0x7 << PTE_NEXT_LEVEL_SHIFT;
const PTE_OUTPUT_ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;
const PTE_FORCE_COHERENT: u64 = 1 << 59;
const PTE_IRQ_REMAP: u64 = 1 << 61;
const PTE_IRQ_WRITE: u64 = 1 << 62;
const PTE_NO_EXECUTE: u64 = 1 << 63;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct AmdPte(pub u64);

impl AmdPte {
    pub const fn new() -> Self {
        Self(0)
    }

    pub fn present(&self) -> bool {
        self.0 & PTE_PRESENT != 0
    }

    pub fn set_present(&mut self, value: bool) {
        if value {
            self.0 |= PTE_PRESENT;
        } else {
            self.0 &= !PTE_PRESENT;
        }
    }

    pub fn user(&self) -> bool {
        self.0 & PTE_USER != 0
    }

    pub fn set_user(&mut self, value: bool) {
        if value {
            self.0 |= PTE_USER;
        } else {
            self.0 &= !PTE_USER;
        }
    }

    pub fn writable(&self) -> bool {
        self.0 & PTE_WRITE != 0
    }

    pub fn set_writable(&mut self, value: bool) {
        if value {
            self.0 |= PTE_WRITE;
        } else {
            self.0 &= !PTE_WRITE;
        }
    }

    pub fn readable(&self) -> bool {
        self.0 & PTE_READ != 0
    }

    pub fn set_readable(&mut self, value: bool) {
        if value {
            self.0 |= PTE_READ;
        } else {
            self.0 &= !PTE_READ;
        }
    }

    pub fn next_level(&self) -> u8 {
        ((self.0 & PTE_NEXT_LEVEL_MASK) >> PTE_NEXT_LEVEL_SHIFT) as u8
    }

    pub fn set_next_level(&mut self, level: u8) {
        self.0 =
            (self.0 & !PTE_NEXT_LEVEL_MASK) | ((u64::from(level) & 0x7) << PTE_NEXT_LEVEL_SHIFT);
    }

    pub fn output_addr(&self) -> u64 {
        self.0 & PTE_OUTPUT_ADDR_MASK
    }

    pub fn set_output_addr(&mut self, addr: u64) {
        self.0 = (self.0 & !PTE_OUTPUT_ADDR_MASK) | (addr & PTE_OUTPUT_ADDR_MASK);
    }

    pub fn force_coherent(&self) -> bool {
        self.0 & PTE_FORCE_COHERENT != 0
    }

    pub fn set_force_coherent(&mut self, value: bool) {
        if value {
            self.0 |= PTE_FORCE_COHERENT;
        } else {
            self.0 &= !PTE_FORCE_COHERENT;
        }
    }

    pub fn interrupt_remap(&self) -> bool {
        self.0 & PTE_IRQ_REMAP != 0
    }

    pub fn set_interrupt_remap(&mut self, value: bool) {
        if value {
            self.0 |= PTE_IRQ_REMAP;
        } else {
            self.0 &= !PTE_IRQ_REMAP;
        }
    }

    pub fn interrupt_write(&self) -> bool {
        self.0 & PTE_IRQ_WRITE != 0
    }

    pub fn set_interrupt_write(&mut self, value: bool) {
        if value {
            self.0 |= PTE_IRQ_WRITE;
        } else {
            self.0 &= !PTE_IRQ_WRITE;
        }
    }

    pub fn no_execute(&self) -> bool {
        self.0 & PTE_NO_EXECUTE != 0
    }

    pub fn set_no_execute(&mut self, value: bool) {
        if value {
            self.0 |= PTE_NO_EXECUTE;
        } else {
            self.0 &= !PTE_NO_EXECUTE;
        }
    }

    pub fn leaf(addr: u64, flags: MappingFlags) -> Self {
        let mut entry = Self::new();
        entry.set_present(true);
        entry.set_output_addr(addr);
        entry.set_readable(flags.readable);
        entry.set_writable(flags.writable);
        entry.set_user(flags.user);
        entry.set_force_coherent(flags.force_coherent);
        entry.set_no_execute(!flags.executable);
        entry
    }

    pub fn pointer(addr: u64, next_level: u8) -> Self {
        let mut entry = Self::new();
        entry.set_present(true);
        entry.set_next_level(next_level);
        entry.set_output_addr(addr);
        entry
    }
}

const _: () = assert!(size_of::<AmdPte>() == 8);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MappingFlags {
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    pub force_coherent: bool,
    pub user: bool,
}

impl Default for MappingFlags {
    fn default() -> Self {
        Self::read_write()
    }
}

impl MappingFlags {
    pub const fn read_write() -> Self {
        Self {
            readable: true,
            writable: true,
            executable: false,
            force_coherent: false,
            user: false,
        }
    }
}

enum PageStorage {
    Dma(DmaBuffer),
    Host {
        ptr: NonNull<u8>,
        layout: Layout,
        len: usize,
    },
}

struct PageBuffer {
    storage: PageStorage,
    phys_addr: usize,
}

impl PageBuffer {
    fn allocate(len: usize, align: usize) -> Result<Self, &'static str> {
        match DmaBuffer::allocate(len, align) {
            Ok(buffer) => Ok(Self {
                phys_addr: buffer.physical_address(),
                storage: PageStorage::Dma(buffer),
            }),
            Err(_) => {
                let layout = Layout::from_size_align(len, align)
                    .map_err(|_| "invalid page-table allocation layout")?;
                let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
                let ptr = NonNull::new(ptr).ok_or("failed to allocate host page-table memory")?;
                Ok(Self {
                    phys_addr: ptr.as_ptr() as usize,
                    storage: PageStorage::Host { ptr, layout, len },
                })
            }
        }
    }

    fn as_ptr(&self) -> *const u8 {
        match &self.storage {
            PageStorage::Dma(buffer) => buffer.as_ptr(),
            PageStorage::Host { ptr, .. } => ptr.as_ptr(),
        }
    }

    fn as_mut_ptr(&mut self) -> *mut u8 {
        match &mut self.storage {
            PageStorage::Dma(buffer) => buffer.as_mut_ptr(),
            PageStorage::Host { ptr, .. } => ptr.as_ptr(),
        }
    }

    fn physical_address(&self) -> usize {
        self.phys_addr
    }

    fn len(&self) -> usize {
        match &self.storage {
            PageStorage::Dma(buffer) => buffer.len(),
            PageStorage::Host { len, .. } => *len,
        }
    }
}

impl Drop for PageBuffer {
    fn drop(&mut self) {
        if let PageStorage::Host { ptr, layout, .. } = &self.storage {
            unsafe {
                std::alloc::dealloc(ptr.as_ptr(), *layout);
            }
        }
    }
}

unsafe impl Send for PageBuffer {}
unsafe impl Sync for PageBuffer {}

struct PageTablePage {
    buffer: PageBuffer,
}

impl PageTablePage {
    fn new() -> Result<Self, &'static str> {
        let buffer = PageBuffer::allocate(PAGE_SIZE as usize, PAGE_SIZE as usize)?;
        if buffer.len() < PAGE_SIZE as usize {
            return Err("page-table allocation smaller than one page");
        }
        Ok(Self { buffer })
    }

    fn physical_address(&self) -> u64 {
        self.buffer.physical_address() as u64
    }

    fn entry(&self, index: usize) -> AmdPte {
        self.entries()[index]
    }

    fn set_entry(&mut self, index: usize, entry: AmdPte) {
        self.entries_mut()[index] = entry;
    }

    fn entries(&self) -> &[AmdPte] {
        unsafe { slice::from_raw_parts(self.buffer.as_ptr().cast::<AmdPte>(), PTES_PER_PAGE) }
    }

    fn entries_mut(&mut self) -> &mut [AmdPte] {
        unsafe {
            slice::from_raw_parts_mut(self.buffer.as_mut_ptr().cast::<AmdPte>(), PTES_PER_PAGE)
        }
    }
}

struct PageTableNode {
    page: PageTablePage,
    children: BTreeMap<usize, Box<PageTableNode>>,
}

impl PageTableNode {
    fn new() -> Result<Self, &'static str> {
        Ok(Self {
            page: PageTablePage::new()?,
            children: BTreeMap::new(),
        })
    }

    fn phys_addr(&self) -> u64 {
        self.page.physical_address()
    }
}

pub struct PageTable {
    levels: u8,
    root: Box<PageTableNode>,
}

impl PageTable {
    pub fn new(levels: u8) -> Result<Self, &'static str> {
        if !(1..=6).contains(&levels) {
            return Err("AMD-Vi page tables support between 1 and 6 levels");
        }

        Ok(Self {
            levels,
            root: Box::new(PageTableNode::new()?),
        })
    }

    pub fn levels(&self) -> u8 {
        self.levels
    }

    pub fn root_address(&self) -> u64 {
        self.root.phys_addr()
    }

    pub fn map_page(
        &mut self,
        iova: u64,
        phys: u64,
        flags: MappingFlags,
    ) -> Result<(), &'static str> {
        if iova & (PAGE_SIZE - 1) != 0 || phys & (PAGE_SIZE - 1) != 0 {
            return Err("IOMMU mappings must be 4KiB-aligned");
        }

        let mut node = self.root.as_mut();
        for level in (2..=self.levels).rev() {
            let index = page_table_index(level, iova);
            if !node.children.contains_key(&index) {
                let child = Box::new(PageTableNode::new()?);
                let child_phys = child.phys_addr();
                node.page
                    .set_entry(index, AmdPte::pointer(child_phys, level - 1));
                node.children.insert(index, child);
            }
            let child = node
                .children
                .get_mut(&index)
                .ok_or("failed to descend page table")?;
            node = child.as_mut();
        }

        let leaf_index = page_table_index(1, iova);
        node.page.set_entry(leaf_index, AmdPte::leaf(phys, flags));
        Ok(())
    }

    pub fn unmap_page(&mut self, iova: u64) -> bool {
        Self::unmap_in_node(self.root.as_mut(), self.levels, iova)
    }

    pub fn translate(&self, iova: u64) -> Option<u64> {
        let page_base = iova & !(PAGE_SIZE - 1);
        let page_offset = iova & (PAGE_SIZE - 1);
        let mut node = self.root.as_ref();

        for level in (2..=self.levels).rev() {
            let index = page_table_index(level, page_base);
            let entry = node.page.entry(index);
            if !entry.present() {
                return None;
            }
            node = node.children.get(&index)?.as_ref();
        }

        let leaf = node.page.entry(page_table_index(1, page_base));
        if !leaf.present() {
            return None;
        }

        Some(leaf.output_addr() + page_offset)
    }

    fn unmap_in_node(node: &mut PageTableNode, level: u8, iova: u64) -> bool {
        if level == 1 {
            let index = page_table_index(1, iova);
            let present = node.page.entry(index).present();
            if present {
                node.page.set_entry(index, AmdPte::new());
            }
            return present;
        }

        let index = page_table_index(level, iova);
        let Some(child) = node.children.get_mut(&index) else {
            return false;
        };
        Self::unmap_in_node(child.as_mut(), level - 1, iova)
    }
}

fn page_table_index(level: u8, address: u64) -> usize {
    ((address >> (12 + ((u64::from(level) - 1) * 9))) & 0x1FF) as usize
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomainMapping {
    pub iova: u64,
    pub phys: u64,
    pub size: u64,
    pub flags: MappingFlags,
}

pub struct IovaAllocator {
    base: u64,
    limit: u64,
    allocations: BTreeMap<u64, u64>,
}

impl IovaAllocator {
    pub fn new(base: u64, limit: u64) -> Self {
        Self {
            base,
            limit,
            allocations: BTreeMap::new(),
        }
    }

    pub fn allocate(&mut self, size: u64, align: u64) -> Option<u64> {
        let size = align_up(size.max(PAGE_SIZE), PAGE_SIZE)?;
        let align = align.max(PAGE_SIZE).next_power_of_two();
        let mut cursor = align_up(self.base, align)?;

        for (&start, &length) in &self.allocations {
            if cursor.checked_add(size)? <= start {
                self.allocations.insert(cursor, size);
                return Some(cursor);
            }
            cursor = align_up(start.checked_add(length)?, align)?;
        }

        if cursor.checked_add(size)? > self.limit {
            return None;
        }

        self.allocations.insert(cursor, size);
        Some(cursor)
    }

    pub fn reserve(&mut self, start: u64, size: u64) -> bool {
        let Some(end) = start.checked_add(size) else {
            return false;
        };
        if start < self.base || end > self.limit {
            return false;
        }

        let prev = self.allocations.range(..=start).next_back();
        if let Some((&prev_start, &prev_len)) = prev {
            let Some(prev_end) = prev_start.checked_add(prev_len) else {
                return false;
            };
            if prev_end > start {
                return false;
            }
        }

        let next = self.allocations.range(start..).next();
        if let Some((&next_start, _)) = next {
            if next_start < end {
                return false;
            }
        }

        self.allocations.insert(start, size);
        true
    }

    pub fn free(&mut self, start: u64) -> bool {
        self.allocations.remove(&start).is_some()
    }

    pub fn allocated_size(&self, start: u64) -> Option<u64> {
        self.allocations.get(&start).copied()
    }

    pub fn allocation_count(&self) -> usize {
        self.allocations.len()
    }
}

pub struct DomainPageTables {
    domain_id: u16,
    page_table: PageTable,
    allocator: IovaAllocator,
    mappings: BTreeMap<u64, DomainMapping>,
}

impl DomainPageTables {
    pub fn new(domain_id: u16) -> Result<Self, &'static str> {
        Self::with_range(domain_id, DEFAULT_IOVA_BASE, DEFAULT_IOVA_LIMIT)
    }

    pub fn with_range(domain_id: u16, base: u64, limit: u64) -> Result<Self, &'static str> {
        Ok(Self {
            domain_id,
            page_table: PageTable::new(DEFAULT_IOMMU_LEVELS)?,
            allocator: IovaAllocator::new(base, limit),
            mappings: BTreeMap::new(),
        })
    }

    pub fn domain_id(&self) -> u16 {
        self.domain_id
    }

    pub fn root_address(&self) -> u64 {
        self.page_table.root_address()
    }

    pub fn levels(&self) -> u8 {
        self.page_table.levels()
    }

    pub fn map_range(
        &mut self,
        phys: u64,
        size: u64,
        flags: MappingFlags,
        preferred_iova: Option<u64>,
    ) -> Result<u64, &'static str> {
        if size == 0 {
            return Err("IOMMU map size must be non-zero");
        }
        if phys & (PAGE_SIZE - 1) != 0 {
            return Err("IOMMU physical mappings must be page-aligned");
        }

        let size = align_up(size, PAGE_SIZE).ok_or("IOMMU map size overflow")?;
        let iova = if let Some(requested) = preferred_iova {
            if requested & (PAGE_SIZE - 1) != 0 {
                return Err("IOMMU IOVA mappings must be page-aligned");
            }
            if !self.allocator.reserve(requested, size) {
                return Err("requested IOVA range is unavailable");
            }
            requested
        } else {
            self.allocator
                .allocate(size, PAGE_SIZE)
                .ok_or("unable to allocate an IOVA range")?
        };

        let mut mapped = 0u64;
        while mapped < size {
            if let Err(err) = self
                .page_table
                .map_page(iova + mapped, phys + mapped, flags)
            {
                let mut rollback = 0u64;
                while rollback < mapped {
                    let _ = self.page_table.unmap_page(iova + rollback);
                    rollback += PAGE_SIZE;
                }
                let _ = self.allocator.free(iova);
                return Err(err);
            }
            mapped += PAGE_SIZE;
        }

        self.mappings.insert(
            iova,
            DomainMapping {
                iova,
                phys,
                size,
                flags,
            },
        );

        Ok(iova)
    }

    pub fn unmap_range(&mut self, iova: u64) -> Result<u64, &'static str> {
        let mapping = self
            .mappings
            .remove(&iova)
            .ok_or("IOMMU mapping does not exist")?;

        let mut offset = 0u64;
        while offset < mapping.size {
            let _ = self.page_table.unmap_page(mapping.iova + offset);
            offset += PAGE_SIZE;
        }
        let _ = self.allocator.free(mapping.iova);
        Ok(mapping.size)
    }

    pub fn mapping(&self, iova: u64) -> Option<&DomainMapping> {
        self.mappings.get(&iova)
    }

    pub fn mapping_count(&self) -> usize {
        self.mappings.len()
    }

    pub fn translate(&self, iova: u64) -> Option<u64> {
        self.page_table.translate(iova)
    }
}

fn align_up(value: u64, align: u64) -> Option<u64> {
    let mask = align.checked_sub(1)?;
    value.checked_add(mask).map(|rounded| rounded & !mask)
}

#[cfg(test)]
mod tests {
    use super::{AmdPte, DomainPageTables, IovaAllocator, MappingFlags, PageTable, PAGE_SIZE};

    #[test]
    fn amd_pte_leaf_sets_permissions() {
        let pte = AmdPte::leaf(0x1234_5000, MappingFlags::read_write());
        assert!(pte.present());
        assert!(pte.readable());
        assert!(pte.writable());
        assert!(pte.no_execute());
        assert_eq!(pte.output_addr(), 0x1234_5000);
    }

    #[test]
    fn iova_allocator_finds_gap_and_reuses_freed_ranges() {
        let mut allocator = IovaAllocator::new(0x1000, 0x10_0000);
        let first = allocator.allocate(PAGE_SIZE, PAGE_SIZE).unwrap_or(0);
        let second = allocator.allocate(PAGE_SIZE * 2, PAGE_SIZE).unwrap_or(0);
        assert_eq!(first, 0x1000);
        assert_eq!(second, 0x2000);
        assert!(allocator.free(first));
        let reused = allocator.allocate(PAGE_SIZE, PAGE_SIZE).unwrap_or(0);
        assert_eq!(reused, first);
    }

    #[test]
    fn page_table_translate_round_trips_mapping() {
        let mut table =
            PageTable::new(4).unwrap_or_else(|err| panic!("page table create failed: {err}"));
        table
            .map_page(0x4000, 0x2000_0000, MappingFlags::read_write())
            .unwrap_or_else(|err| panic!("page table map failed: {err}"));
        assert_eq!(table.translate(0x4123), Some(0x2000_0123));
        assert!(table.unmap_page(0x4000));
        assert_eq!(table.translate(0x4123), None);
    }

    #[test]
    fn domain_page_tables_allocate_iova_and_unmap() {
        let mut domain = DomainPageTables::new(7)
            .unwrap_or_else(|err| panic!("domain page table create failed: {err}"));
        let iova = domain
            .map_range(0x3000_0000, PAGE_SIZE * 2, MappingFlags::read_write(), None)
            .unwrap_or_else(|err| panic!("domain mapping failed: {err}"));
        let mapping = domain
            .mapping(iova)
            .unwrap_or_else(|| panic!("mapping missing"));
        assert_eq!(mapping.size, PAGE_SIZE * 2);
        assert!(domain.unmap_range(iova).is_ok());
        assert!(domain.mapping(iova).is_none());
    }
}
