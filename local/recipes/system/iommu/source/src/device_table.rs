use core::mem::size_of;
use core::slice;

use redox_driver_sys::dma::DmaBuffer;

/// AMD-Vi Device Table: 65536 entries × 32 bytes = 2 MiB.
pub const DEVICE_TABLE_ENTRIES: usize = 65_536;
pub const DTE_SIZE: usize = 32;

pub const DEVICE_TABLE_BYTES: usize = DEVICE_TABLE_ENTRIES * DTE_SIZE;

const DTE_VALID_BIT: u64 = 1 << 0;
const DTE_TRANSLATION_VALID_BIT: u64 = 1 << 1;
const DTE_WRITE_PERMISSION_BIT: u64 = 1 << 4;
const DTE_READ_PERMISSION_BIT: u64 = 1 << 5;
const DTE_SNOOP_ENABLE_BIT: u64 = 1 << 8;
const DTE_MODE_SHIFT: u32 = 9;
const DTE_MODE_MASK: u64 = 0x7 << DTE_MODE_SHIFT;
const DTE_PAGE_TABLE_ROOT_MASK: u64 = ((1u64 << 40) - 1) << 12;
const DTE_INTERRUPT_REMAP_BIT: u64 = 1 << 61;
const DTE_INTERRUPT_WRITE_BIT: u64 = 1 << 62;

const DTE_INT_TABLE_LEN_MASK: u64 = 0xF;
const DTE_INT_CONTROL_SHIFT: u32 = 4;
const DTE_INT_CONTROL_MASK: u64 = 0x3 << DTE_INT_CONTROL_SHIFT;
const DTE_INT_REMAP_TABLE_PTR_SHIFT: u32 = 6;
const DTE_INT_REMAP_TABLE_PTR_MASK: u64 = ((1u64 << 46) - 1) << DTE_INT_REMAP_TABLE_PTR_SHIFT;

/// Device Table Entry (DTE) — 256 bits (32 bytes = 4 × u64).
///
/// Layout follows AMD IOMMU Spec 48882 Rev 3.10, Section 3.2.2.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct DeviceTableEntry {
    data: [u64; 4],
}

impl DeviceTableEntry {
    pub const fn new() -> Self {
        Self { data: [0; 4] }
    }

    pub fn valid(&self) -> bool {
        self.data[0] & DTE_VALID_BIT != 0
    }

    pub fn set_valid(&mut self, value: bool) {
        if value {
            self.data[0] |= DTE_VALID_BIT;
        } else {
            self.data[0] &= !DTE_VALID_BIT;
        }
    }

    pub fn translation_valid(&self) -> bool {
        self.data[0] & DTE_TRANSLATION_VALID_BIT != 0
    }

    pub fn set_translation_valid(&mut self, value: bool) {
        if value {
            self.data[0] |= DTE_TRANSLATION_VALID_BIT;
        } else {
            self.data[0] &= !DTE_TRANSLATION_VALID_BIT;
        }
    }

    pub fn write_permission(&self) -> bool {
        self.data[0] & DTE_WRITE_PERMISSION_BIT != 0
    }

    pub fn set_write_permission(&mut self, value: bool) {
        if value {
            self.data[0] |= DTE_WRITE_PERMISSION_BIT;
        } else {
            self.data[0] &= !DTE_WRITE_PERMISSION_BIT;
        }
    }

    pub fn read_permission(&self) -> bool {
        self.data[0] & DTE_READ_PERMISSION_BIT != 0
    }

    pub fn set_read_permission(&mut self, value: bool) {
        if value {
            self.data[0] |= DTE_READ_PERMISSION_BIT;
        } else {
            self.data[0] &= !DTE_READ_PERMISSION_BIT;
        }
    }

    pub fn snoop_enable(&self) -> bool {
        self.data[0] & DTE_SNOOP_ENABLE_BIT != 0
    }

    pub fn set_snoop_enable(&mut self, value: bool) {
        if value {
            self.data[0] |= DTE_SNOOP_ENABLE_BIT;
        } else {
            self.data[0] &= !DTE_SNOOP_ENABLE_BIT;
        }
    }

    pub fn mode(&self) -> u8 {
        ((self.data[0] & DTE_MODE_MASK) >> DTE_MODE_SHIFT) as u8
    }

    pub fn set_mode(&mut self, mode: u8) {
        self.data[0] = (self.data[0] & !DTE_MODE_MASK) | (((mode as u64) & 0x7) << DTE_MODE_SHIFT);
    }

    /// Returns the full, 4KiB-aligned physical address stored in bits 12:51.
    pub fn page_table_root(&self) -> u64 {
        self.data[0] & DTE_PAGE_TABLE_ROOT_MASK
    }

    pub fn set_page_table_root(&mut self, phys: u64) {
        self.data[0] =
            (self.data[0] & !DTE_PAGE_TABLE_ROOT_MASK) | (phys & DTE_PAGE_TABLE_ROOT_MASK);
    }

    /// Interrupt remapping enable (bit 61 of word 0 in the AMD-Vi DTE).
    pub fn interrupt_remap(&self) -> bool {
        self.data[0] & DTE_INTERRUPT_REMAP_BIT != 0
    }

    pub fn set_interrupt_remap(&mut self, value: bool) {
        if value {
            self.data[0] |= DTE_INTERRUPT_REMAP_BIT;
        } else {
            self.data[0] &= !DTE_INTERRUPT_REMAP_BIT;
        }
    }

    /// Interrupt write permission (bit 62 of word 0 in the AMD-Vi DTE).
    pub fn interrupt_write(&self) -> bool {
        self.data[0] & DTE_INTERRUPT_WRITE_BIT != 0
    }

    pub fn set_interrupt_write(&mut self, value: bool) {
        if value {
            self.data[0] |= DTE_INTERRUPT_WRITE_BIT;
        } else {
            self.data[0] &= !DTE_INTERRUPT_WRITE_BIT;
        }
    }

    pub fn int_table_len(&self) -> u8 {
        (self.data[1] & DTE_INT_TABLE_LEN_MASK) as u8
    }

    pub fn set_int_table_len(&mut self, len: u8) {
        self.data[1] =
            (self.data[1] & !DTE_INT_TABLE_LEN_MASK) | ((len as u64) & DTE_INT_TABLE_LEN_MASK);
    }

    pub fn interrupt_control(&self) -> u8 {
        ((self.data[1] & DTE_INT_CONTROL_MASK) >> DTE_INT_CONTROL_SHIFT) as u8
    }

    pub fn set_interrupt_control(&mut self, control: u8) {
        self.data[1] = (self.data[1] & !DTE_INT_CONTROL_MASK)
            | (((control as u64) & 0x3) << DTE_INT_CONTROL_SHIFT);
    }

    /// Returns the interrupt remap table pointer bits stored in word 1.
    pub fn int_remap_table_ptr(&self) -> u64 {
        self.data[1] & DTE_INT_REMAP_TABLE_PTR_MASK
    }

    pub fn set_int_remap_table_ptr(&mut self, phys: u64) {
        self.data[1] =
            (self.data[1] & !DTE_INT_REMAP_TABLE_PTR_MASK) | (phys & DTE_INT_REMAP_TABLE_PTR_MASK);
    }
}

const _: () = assert!(size_of::<DeviceTableEntry>() == DTE_SIZE);

/// Device Table — manages the 65536-entry device table.
pub struct DeviceTable {
    buffer: DmaBuffer,
}

impl DeviceTable {
    /// Allocate a new device table (65536 × 32 bytes = 2 MiB).
    pub fn new() -> Result<Self, &'static str> {
        let buffer = DmaBuffer::allocate(DEVICE_TABLE_BYTES, 4096)
            .map_err(|_| "failed to allocate IOMMU device table")?;

        if buffer.len() < DEVICE_TABLE_BYTES {
            return Err("IOMMU device table allocation was smaller than requested");
        }

        if !buffer.is_physically_contiguous() {
            return Err("IOMMU device table allocation is not physically contiguous");
        }

        Ok(Self { buffer })
    }

    pub fn get_entry(&self, device_id: u16) -> DeviceTableEntry {
        self.entries()[device_id as usize]
    }

    pub fn set_entry(&mut self, device_id: u16, entry: &DeviceTableEntry) {
        self.entries_mut()[device_id as usize] = *entry;
    }

    pub fn clear_entry(&mut self, device_id: u16) {
        self.entries_mut()[device_id as usize] = DeviceTableEntry::new();
    }

    pub fn physical_address(&self) -> usize {
        self.buffer.physical_address()
    }

    /// Convert PCI BDF to device ID.
    /// Bus: bits 8:15, Device: bits 3:7, Function: bits 0:2.
    pub fn bdf_to_device_id(bus: u8, device: u8, function: u8) -> u16 {
        ((bus as u16) << 8) | ((device as u16) << 3) | (function as u16)
    }

    fn entries(&self) -> &[DeviceTableEntry] {
        unsafe {
            slice::from_raw_parts(
                self.buffer.as_ptr() as *const DeviceTableEntry,
                DEVICE_TABLE_ENTRIES,
            )
        }
    }

    fn entries_mut(&mut self) -> &mut [DeviceTableEntry] {
        unsafe {
            slice::from_raw_parts_mut(
                self.buffer.as_mut_ptr() as *mut DeviceTableEntry,
                DEVICE_TABLE_ENTRIES,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DeviceTable, DeviceTableEntry, DTE_PAGE_TABLE_ROOT_MASK};

    fn try_allocate_table() -> Option<DeviceTable> {
        match DeviceTable::new() {
            Ok(table) => Some(table),
            Err(err) => {
                eprintln!("skipping DeviceTable allocation-dependent test: {err}");
                None
            }
        }
    }

    #[test]
    fn test_dte_valid_bit() {
        let mut entry = DeviceTableEntry::new();
        assert!(!entry.valid());

        entry.set_valid(true);
        assert!(entry.valid());

        entry.set_valid(false);
        assert!(!entry.valid());
    }

    #[test]
    fn test_dte_translation_valid() {
        let mut entry = DeviceTableEntry::new();
        assert!(!entry.translation_valid());

        entry.set_translation_valid(true);
        assert!(entry.translation_valid());

        entry.set_translation_valid(false);
        assert!(!entry.translation_valid());
    }

    #[test]
    fn test_dte_mode_4level() {
        let mut entry = DeviceTableEntry::new();
        entry.set_mode(4);

        assert_eq!(entry.mode(), 4);
    }

    #[test]
    fn test_dte_permissions_and_interrupt_control() {
        let mut entry = DeviceTableEntry::new();
        entry.set_read_permission(true);
        entry.set_write_permission(true);
        entry.set_snoop_enable(true);
        entry.set_interrupt_control(0x02);

        assert!(entry.read_permission());
        assert!(entry.write_permission());
        assert!(entry.snoop_enable());
        assert_eq!(entry.interrupt_control(), 0x02);
    }

    #[test]
    fn test_dte_page_table_root() {
        let mut entry = DeviceTableEntry::new();
        entry.set_page_table_root(0x1234_5000);

        assert_eq!(entry.page_table_root(), 0x1234_5000);
        assert_eq!(entry.data[0] & DTE_PAGE_TABLE_ROOT_MASK, 0x1234_5000);
    }

    #[test]
    fn test_bdf_encoding() {
        assert_eq!(DeviceTable::bdf_to_device_id(0x12, 0x05, 0x03), 0x122b);
        assert_eq!(DeviceTable::bdf_to_device_id(0xff, 0x1f, 0x07), 0xffff);
    }

    #[test]
    fn test_clear_entry() -> Result<(), &'static str> {
        let Some(mut table) = try_allocate_table() else {
            return Ok(());
        };

        let device_id = DeviceTable::bdf_to_device_id(0x02, 0x00, 0x00);
        let mut entry = DeviceTableEntry::new();
        entry.set_valid(true);
        entry.set_translation_valid(true);
        entry.set_mode(4);
        entry.set_page_table_root(0x1234_5000);

        table.set_entry(device_id, &entry);
        assert_eq!(table.get_entry(device_id), entry);

        table.clear_entry(device_id);
        assert_eq!(table.get_entry(device_id), DeviceTableEntry::new());

        Ok(())
    }
}
