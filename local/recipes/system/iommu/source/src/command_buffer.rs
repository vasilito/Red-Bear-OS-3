use core::mem::size_of;
use core::slice;

use redox_driver_sys::dma::DmaBuffer;

pub const COMMAND_ENTRY_SIZE: usize = 16;
pub const EVENT_LOG_ENTRY_SIZE: usize = 16;

const DMA_ALIGNMENT: usize = 4096;

pub const CMD_COMPLETION_WAIT: u32 = 0x01;
pub const CMD_INVALIDATE_DEVTAB_ENTRY: u32 = 0x02;
pub const CMD_INVALIDATE_IOMMU_PAGES: u32 = 0x03;
pub const CMD_INVALIDATE_INTERRUPT_TABLE: u32 = 0x04;
pub const CMD_INVALIDATE_IOMMU_ALL: u32 = 0x05;

pub const EVENT_IO_PAGE_FAULT: u32 = 0x01;
pub const EVENT_INVALIDATE_DEVICE_TABLE: u32 = 0x02;

const COMPLETION_WAIT_STORE_BIT: u32 = 1 << 4;
const COMPLETION_WAIT_INTERRUPT_BIT: u32 = 1 << 5;
const INVALIDATE_PAGES_PDE_BIT: u32 = 1 << 12;
const INVALIDATE_PAGES_SIZE_BIT: u32 = 1 << 13;

/// Command buffer entry (128 bits = 16 bytes = 4 × u32).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct CommandEntry {
    words: [u32; 4],
}

impl CommandEntry {
    pub const fn new() -> Self {
        Self { words: [0; 4] }
    }

    pub const fn from_words(words: [u32; 4]) -> Self {
        Self { words }
    }

    pub fn words(&self) -> [u32; 4] {
        self.words
    }

    pub fn opcode(&self) -> u32 {
        self.words[0] & 0xF
    }

    /// COMPLETION_WAIT (opcode 0x01).
    pub fn completion_wait(store_addr: u64, store_data: u32) -> Self {
        debug_assert_eq!(
            store_addr & 0x7,
            0,
            "completion wait store address must be 8-byte aligned"
        );

        Self {
            words: [
                CMD_COMPLETION_WAIT | COMPLETION_WAIT_STORE_BIT,
                store_addr as u32,
                (store_addr >> 32) as u32,
                store_data,
            ],
        }
    }

    /// INVALIDATE_DEVTAB_ENTRY (opcode 0x02).
    pub fn invalidate_devtab_entry(device_id: u16) -> Self {
        Self {
            words: [CMD_INVALIDATE_DEVTAB_ENTRY, device_id as u32, 0, 0],
        }
    }

    pub fn invalidate_pages(domain_id: u16, addr: u64) -> Self {
        Self::invalidate_pages_with_flags(domain_id, addr, false, false)
    }

    pub fn invalidate_pages_with_flags(domain_id: u16, addr: u64, pde: bool, size: bool) -> Self {
        let mut word0 = CMD_INVALIDATE_IOMMU_PAGES;
        if pde {
            word0 |= INVALIDATE_PAGES_PDE_BIT;
        }
        if size {
            word0 |= INVALIDATE_PAGES_SIZE_BIT;
        }

        Self {
            words: [word0, domain_id as u32, addr as u32, (addr >> 32) as u32],
        }
    }

    pub fn invalidate_interrupt_table(device_id: u16) -> Self {
        Self {
            words: [CMD_INVALIDATE_INTERRUPT_TABLE, device_id as u32, 0, 0],
        }
    }

    /// INVALIDATE_IOMMU_ALL (opcode 0x05).
    pub fn invalidate_all() -> Self {
        Self {
            words: [CMD_INVALIDATE_IOMMU_ALL, 0, 0, 0],
        }
    }

    pub fn completion_wait_store(&self) -> bool {
        self.words[0] & COMPLETION_WAIT_STORE_BIT != 0
    }

    pub fn completion_wait_interrupt(&self) -> bool {
        self.words[0] & COMPLETION_WAIT_INTERRUPT_BIT != 0
    }

    pub fn completion_wait_store_address(&self) -> u64 {
        (self.words[1] as u64) | ((self.words[2] as u64) << 32)
    }

    pub fn completion_wait_store_data(&self) -> u32 {
        self.words[3]
    }

    pub fn invalidate_device_id(&self) -> u16 {
        self.words[1] as u16
    }

    pub fn invalidate_pages_pde(&self) -> bool {
        self.words[0] & INVALIDATE_PAGES_PDE_BIT != 0
    }

    pub fn invalidate_pages_size(&self) -> bool {
        self.words[0] & INVALIDATE_PAGES_SIZE_BIT != 0
    }

    pub fn invalidate_pages_address(&self) -> u64 {
        (self.words[2] as u64) | ((self.words[3] as u64) << 32)
    }
}

const _: () = assert!(size_of::<CommandEntry>() == COMMAND_ENTRY_SIZE);

pub struct CommandBuffer {
    buffer: DmaBuffer,
    capacity: usize,
}

impl CommandBuffer {
    pub const RESERVED_COMPLETION_INDEX: usize = 0;
    pub const FIRST_COMMAND_INDEX: usize = 1;

    pub fn new(entry_count: usize) -> Result<Self, &'static str> {
        if entry_count <= Self::FIRST_COMMAND_INDEX {
            return Err("IOMMU command buffer entry count must leave room for command entries");
        }

        let byte_len = entry_count
            .checked_mul(COMMAND_ENTRY_SIZE)
            .ok_or("IOMMU command buffer size overflow")?;

        let buffer = DmaBuffer::allocate(byte_len, DMA_ALIGNMENT)
            .map_err(|_| "failed to allocate IOMMU command buffer")?;

        if buffer.len() < byte_len {
            return Err("IOMMU command buffer allocation was smaller than requested");
        }

        if !buffer.is_physically_contiguous() {
            return Err("IOMMU command buffer allocation is not physically contiguous");
        }

        if buffer.physical_address() & (DMA_ALIGNMENT - 1) != 0 {
            return Err("IOMMU command buffer allocation is not 4KiB-aligned");
        }

        Ok(Self {
            buffer,
            capacity: entry_count,
        })
    }

    pub fn physical_address(&self) -> usize {
        self.buffer.physical_address()
    }

    /// Write a command at the given index.
    pub fn write_command(&mut self, index: usize, cmd: &CommandEntry) {
        assert!(index < self.capacity, "IOMMU command index out of bounds");
        self.commands_mut()[index] = *cmd;
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn completion_store_dma_addr(&self) -> u64 {
        self.buffer.physical_address() as u64
    }

    pub fn clear_completion_store(&mut self) {
        self.commands_mut()[0] = CommandEntry::default();
    }

    pub fn read_completion_store(&self) -> u32 {
        unsafe { core::ptr::read_volatile(self.buffer.as_ptr() as *const u32) }
    }

    pub fn completion_store_cpu_ptr(&self) -> *mut u32 {
        self.buffer.as_ptr() as *mut u32
    }

    fn commands_mut(&mut self) -> &mut [CommandEntry] {
        unsafe {
            slice::from_raw_parts_mut(self.buffer.as_mut_ptr() as *mut CommandEntry, self.capacity)
        }
    }
}

/// Event log entry (128 bits = 16 bytes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct EventLogEntry {
    words: [u32; 4],
}

impl EventLogEntry {
    pub const fn new() -> Self {
        Self { words: [0; 4] }
    }

    pub const fn from_words(words: [u32; 4]) -> Self {
        Self { words }
    }

    pub fn words(&self) -> [u32; 4] {
        self.words
    }

    pub fn event_type(&self) -> u32 {
        self.words[0] & 0xFFFF
    }

    pub fn event_flags(&self) -> u16 {
        ((self.words[0] >> 16) & 0xFFFF) as u16
    }

    pub fn device_id(&self) -> u16 {
        self.words[1] as u16
    }

    pub fn virtual_address(&self) -> u64 {
        ((self.words[3] as u64) << 32) | (self.words[2] as u64)
    }
}

const _: () = assert!(size_of::<EventLogEntry>() == EVENT_LOG_ENTRY_SIZE);

pub struct EventLog {
    buffer: DmaBuffer,
    capacity: usize,
}

impl EventLog {
    pub fn new(entry_count: usize) -> Result<Self, &'static str> {
        if entry_count == 0 {
            return Err("IOMMU event log entry count must be non-zero");
        }

        let byte_len = entry_count
            .checked_mul(EVENT_LOG_ENTRY_SIZE)
            .ok_or("IOMMU event log size overflow")?;

        let buffer = DmaBuffer::allocate(byte_len, DMA_ALIGNMENT)
            .map_err(|_| "failed to allocate IOMMU event log")?;

        if buffer.len() < byte_len {
            return Err("IOMMU event log allocation was smaller than requested");
        }

        if !buffer.is_physically_contiguous() {
            return Err("IOMMU event log allocation is not physically contiguous");
        }

        if buffer.physical_address() & (DMA_ALIGNMENT - 1) != 0 {
            return Err("IOMMU event log allocation is not 4KiB-aligned");
        }

        Ok(Self {
            buffer,
            capacity: entry_count,
        })
    }

    pub fn physical_address(&self) -> usize {
        self.buffer.physical_address()
    }

    pub fn read_entry(&self, index: usize) -> EventLogEntry {
        assert!(index < self.capacity, "IOMMU event log index out of bounds");
        self.entries()[index]
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn completion_store_dma_addr(&self) -> u64 {
        let offset = (self.capacity - 1) * EVENT_LOG_ENTRY_SIZE;
        (self.buffer.physical_address() + offset) as u64
    }

    pub fn completion_store_cpu_ptr(&self) -> *mut u32 {
        let offset = (self.capacity - 1) * EVENT_LOG_ENTRY_SIZE;
        unsafe { self.buffer.as_ptr().add(offset) as *mut u32 }
    }

    pub fn clear_completion_store(&mut self) {
        let offset = (self.capacity - 1) * EVENT_LOG_ENTRY_SIZE;
        unsafe {
            core::ptr::write_bytes(
                self.buffer.as_mut_ptr().add(offset),
                0,
                EVENT_LOG_ENTRY_SIZE,
            )
        };
    }

    pub fn read_completion_store(&self) -> u32 {
        unsafe { core::ptr::read_volatile(self.completion_store_cpu_ptr() as *const u32) }
    }

    fn entries(&self) -> &[EventLogEntry] {
        unsafe {
            slice::from_raw_parts(self.buffer.as_ptr() as *const EventLogEntry, self.capacity)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CommandEntry, EventLogEntry, CMD_COMPLETION_WAIT, CMD_INVALIDATE_DEVTAB_ENTRY,
        CMD_INVALIDATE_IOMMU_ALL, CMD_INVALIDATE_IOMMU_PAGES, EVENT_IO_PAGE_FAULT,
    };

    #[test]
    fn test_completion_wait_command() {
        let store_addr = 0x1234_5000_0000_1000;
        let store_data = 0xabcdefff;
        let cmd = CommandEntry::completion_wait(store_addr, store_data);
        let words = cmd.words();

        assert_eq!(cmd.opcode(), CMD_COMPLETION_WAIT);
        assert!(cmd.completion_wait_store());
        assert!(!cmd.completion_wait_interrupt());
        assert_eq!(words[1], store_addr as u32);
        assert_eq!(words[2], (store_addr >> 32) as u32);
        assert_eq!(words[3], store_data);
        assert_eq!(cmd.completion_wait_store_address(), store_addr);
        assert_eq!(cmd.completion_wait_store_data(), store_data);
    }

    #[test]
    fn test_invalidate_devtab_command() {
        let device_id = 0x1234;
        let cmd = CommandEntry::invalidate_devtab_entry(device_id);
        let words = cmd.words();

        assert_eq!(cmd.opcode(), CMD_INVALIDATE_DEVTAB_ENTRY);
        assert_eq!(cmd.invalidate_device_id(), device_id);
        assert_eq!(words[1], device_id as u32);
        assert_eq!(words[2], 0);
        assert_eq!(words[3], 0);
    }

    #[test]
    fn test_invalidate_pages_command() {
        let device_id = 0x4321;
        let addr = 0xfeed_cafe_b000;
        let cmd = CommandEntry::invalidate_pages(device_id, addr);
        let words = cmd.words();

        assert_eq!(cmd.opcode(), CMD_INVALIDATE_IOMMU_PAGES);
        assert_eq!(cmd.invalidate_device_id(), device_id);
        assert!(!cmd.invalidate_pages_pde());
        assert!(!cmd.invalidate_pages_size());
        assert_eq!(words[1], device_id as u32);
        assert_eq!(cmd.invalidate_pages_address(), addr);
    }

    #[test]
    fn test_invalidate_all_command() {
        let cmd = CommandEntry::invalidate_all();
        let words = cmd.words();

        assert_eq!(cmd.opcode(), CMD_INVALIDATE_IOMMU_ALL);
        assert_eq!(words[1], 0);
        assert_eq!(words[2], 0);
        assert_eq!(words[3], 0);
    }

    #[test]
    fn test_event_entry_parsing() {
        let device_id = 0x2468;
        let address = 0x0123_4567_89ab_cdef;
        let entry = EventLogEntry::from_words([
            EVENT_IO_PAGE_FAULT | ((0x5a as u32) << 16),
            device_id as u32,
            address as u32,
            (address >> 32) as u32,
        ]);

        assert_eq!(entry.event_type(), EVENT_IO_PAGE_FAULT);
        assert_eq!(entry.event_flags(), 0x5a);
        assert_eq!(entry.device_id(), device_id);
        assert_eq!(entry.virtual_address(), address);
    }
}
