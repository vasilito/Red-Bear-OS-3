use core::mem::size_of;
use core::slice;

use redox_driver_sys::dma::DmaBuffer;

pub const IRTE_SIZE: usize = 16;
pub const MAX_INTERRUPT_REMAP_ENTRIES: usize = 4096;

const DMA_ALIGNMENT: usize = 4096;
const IRTE_REMAP_ENABLE: u64 = 1 << 0;
const IRTE_SUPPRESS_IOPF: u64 = 1 << 1;
const IRTE_INT_TYPE_SHIFT: u64 = 2;
const IRTE_INT_TYPE_MASK: u64 = 0x7 << IRTE_INT_TYPE_SHIFT;
const IRTE_DEST_MODE: u64 = 1 << 8;
const IRTE_DEST_LOW_SHIFT: u64 = 16;
const IRTE_DEST_LOW_MASK: u64 = 0xFFFF << IRTE_DEST_LOW_SHIFT;
const IRTE_VECTOR_SHIFT: u64 = 32;
const IRTE_VECTOR_MASK: u64 = 0xFF << IRTE_VECTOR_SHIFT;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct AmdIrte {
    data: [u64; 2],
}

impl AmdIrte {
    pub const fn new() -> Self {
        Self { data: [0; 2] }
    }

    pub fn remap_enabled(&self) -> bool {
        self.data[0] & IRTE_REMAP_ENABLE != 0
    }

    pub fn set_remap_enabled(&mut self, value: bool) {
        if value {
            self.data[0] |= IRTE_REMAP_ENABLE;
        } else {
            self.data[0] &= !IRTE_REMAP_ENABLE;
        }
    }

    pub fn suppress_io_page_faults(&self) -> bool {
        self.data[0] & IRTE_SUPPRESS_IOPF != 0
    }

    pub fn set_suppress_io_page_faults(&mut self, value: bool) {
        if value {
            self.data[0] |= IRTE_SUPPRESS_IOPF;
        } else {
            self.data[0] &= !IRTE_SUPPRESS_IOPF;
        }
    }

    pub fn interrupt_type(&self) -> u8 {
        ((self.data[0] & IRTE_INT_TYPE_MASK) >> IRTE_INT_TYPE_SHIFT) as u8
    }

    pub fn set_interrupt_type(&mut self, value: u8) {
        self.data[0] = (self.data[0] & !IRTE_INT_TYPE_MASK)
            | ((u64::from(value) & 0x7) << IRTE_INT_TYPE_SHIFT);
    }

    pub fn destination_mode(&self) -> bool {
        self.data[0] & IRTE_DEST_MODE != 0
    }

    pub fn set_destination_mode(&mut self, logical: bool) {
        if logical {
            self.data[0] |= IRTE_DEST_MODE;
        } else {
            self.data[0] &= !IRTE_DEST_MODE;
        }
    }

    pub fn destination(&self) -> u32 {
        (((self.data[1] & 0xFFFF_FFFF) as u32) << 16)
            | (((self.data[0] & IRTE_DEST_LOW_MASK) >> IRTE_DEST_LOW_SHIFT) as u32)
    }

    pub fn set_destination(&mut self, apic_id: u32) {
        self.data[0] = (self.data[0] & !IRTE_DEST_LOW_MASK)
            | ((u64::from(apic_id & 0xFFFF)) << IRTE_DEST_LOW_SHIFT);
        self.data[1] = (self.data[1] & !0xFFFF_FFFF) | u64::from(apic_id >> 16);
    }

    pub fn vector(&self) -> u8 {
        ((self.data[0] & IRTE_VECTOR_MASK) >> IRTE_VECTOR_SHIFT) as u8
    }

    pub fn set_vector(&mut self, vector: u8) {
        self.data[0] =
            (self.data[0] & !IRTE_VECTOR_MASK) | (u64::from(vector) << IRTE_VECTOR_SHIFT);
    }
}

const _: () = assert!(size_of::<AmdIrte>() == IRTE_SIZE);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IrteConfig {
    pub vector: u8,
    pub destination: u32,
    pub logical_destination: bool,
    pub interrupt_type: u8,
    pub suppress_io_page_faults: bool,
}

pub struct InterruptRemapTable {
    buffer: DmaBuffer,
    capacity: usize,
}

impl InterruptRemapTable {
    pub fn new(entry_count: usize) -> Result<Self, &'static str> {
        if !(2..=MAX_INTERRUPT_REMAP_ENTRIES).contains(&entry_count) {
            return Err("interrupt remap table entry count must be between 2 and 4096");
        }
        if !entry_count.is_power_of_two() {
            return Err("interrupt remap table entry count must be a power of two");
        }

        let byte_len = entry_count
            .checked_mul(IRTE_SIZE)
            .ok_or("interrupt remap table size overflow")?;
        let buffer = DmaBuffer::allocate(byte_len, DMA_ALIGNMENT)
            .map_err(|_| "failed to allocate interrupt remap table")?;
        if buffer.len() < byte_len {
            return Err("interrupt remap table allocation was smaller than requested");
        }
        if !buffer.is_physically_contiguous() {
            return Err("interrupt remap table allocation is not physically contiguous");
        }

        Ok(Self {
            buffer,
            capacity: entry_count,
        })
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn len_encoding(&self) -> u8 {
        self.capacity.ilog2() as u8 - 1
    }

    pub fn physical_address(&self) -> usize {
        self.buffer.physical_address()
    }

    pub fn entry(&self, index: usize) -> AmdIrte {
        assert!(
            index < self.capacity,
            "interrupt remap table index out of bounds"
        );
        self.entries()[index]
    }

    pub fn set_entry(&mut self, index: usize, entry: AmdIrte) {
        assert!(
            index < self.capacity,
            "interrupt remap table index out of bounds"
        );
        self.entries_mut()[index] = entry;
    }

    pub fn clear_entry(&mut self, index: usize) {
        self.set_entry(index, AmdIrte::new());
    }

    pub fn configure(&mut self, index: usize, config: IrteConfig) {
        let mut entry = AmdIrte::new();
        entry.set_remap_enabled(true);
        entry.set_suppress_io_page_faults(config.suppress_io_page_faults);
        entry.set_interrupt_type(config.interrupt_type);
        entry.set_destination_mode(config.logical_destination);
        entry.set_destination(config.destination);
        entry.set_vector(config.vector);
        self.set_entry(index, entry);
    }

    fn entries(&self) -> &[AmdIrte] {
        unsafe { slice::from_raw_parts(self.buffer.as_ptr().cast::<AmdIrte>(), self.capacity) }
    }

    fn entries_mut(&mut self) -> &mut [AmdIrte] {
        unsafe {
            slice::from_raw_parts_mut(self.buffer.as_mut_ptr().cast::<AmdIrte>(), self.capacity)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AmdIrte;

    #[test]
    fn irte_accessors_round_trip() {
        let mut irte = AmdIrte::new();
        irte.set_remap_enabled(true);
        irte.set_suppress_io_page_faults(true);
        irte.set_interrupt_type(3);
        irte.set_destination_mode(true);
        irte.set_destination(0x1234_5678);
        irte.set_vector(0x52);

        assert!(irte.remap_enabled());
        assert!(irte.suppress_io_page_faults());
        assert_eq!(irte.interrupt_type(), 3);
        assert!(irte.destination_mode());
        assert_eq!(irte.destination(), 0x1234_5678);
        assert_eq!(irte.vector(), 0x52);
    }
}
