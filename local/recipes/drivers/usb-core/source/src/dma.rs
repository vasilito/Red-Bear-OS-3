use alloc::boxed::Box;
use alloc::vec::Vec;

/// DMA buffer for USB transfers.
///
/// This crate intentionally avoids Redox-specific allocation APIs, so `allocate`
/// produces an owned staging buffer and leaves `physical_addr` unset (`0`).
/// Controller-specific code must install a real physical address before using
/// the buffer for hardware DMA.
pub struct DmaBuffer {
    pub virtual_addr: usize,
    pub physical_addr: u64,
    pub size: usize,
    data: Box<[u8]>,
    mapped: bool,
}

impl DmaBuffer {
    /// Allocate an owned staging buffer for a future DMA mapping.
    ///
    /// The returned buffer is not DMA-mapped yet. `physical_addr` remains `0`
    /// until the caller supplies a real mapping with `set_physical_addr`.
    pub fn allocate(size: usize) -> Result<Self, DmaError> {
        if size > isize::MAX as usize {
            return Err(DmaError::TooLarge);
        }

        if size == 0 {
            return Ok(Self {
                virtual_addr: 0,
                physical_addr: 0,
                size: 0,
                data: Vec::new().into_boxed_slice(),
                mapped: true,
            });
        }

        let mut data = Vec::new();
        if data.try_reserve_exact(size).is_err() {
            return Err(DmaError::NoMemory);
        }

        data.resize(size, 0);
        let mut data = data.into_boxed_slice();
        let virtual_addr = data.as_mut_ptr() as usize;

        Ok(Self {
            virtual_addr,
            physical_addr: 0,
            size,
            data,
            mapped: false,
        })
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// Returns `true` once the controller-specific layer has installed a
    /// hardware-usable physical address for this buffer.
    pub fn is_dma_mapped(&self) -> bool {
        self.mapped
    }

    /// Attach a controller/OS-specific physical address to the staging buffer.
    pub fn set_physical_addr(&mut self, physical_addr: u64) -> Result<(), DmaError> {
        if self.size != 0 && physical_addr == 0 {
            return Err(DmaError::AllocFailed);
        }

        self.physical_addr = physical_addr;
        self.mapped = true;
        Ok(())
    }
}

#[derive(Debug)]
pub enum DmaError {
    AllocFailed,
    TooLarge,
    NoMemory,
}

#[cfg(test)]
mod tests {
    use super::{DmaBuffer, DmaError};

    #[test]
    fn allocated_buffers_start_unmapped() {
        let buffer = match DmaBuffer::allocate(16) {
            Ok(buffer) => buffer,
            Err(error) => panic!("expected allocation to succeed: {error:?}"),
        };

        assert_eq!(buffer.physical_addr, 0);
        assert!(!buffer.is_dma_mapped());
        assert_eq!(buffer.as_slice().len(), 16);
    }

    #[test]
    fn attaching_a_physical_address_marks_the_buffer_mapped() {
        let mut buffer = match DmaBuffer::allocate(16) {
            Ok(buffer) => buffer,
            Err(error) => panic!("expected allocation to succeed: {error:?}"),
        };

        let result = buffer.set_physical_addr(0x1000);
        assert!(result.is_ok());
        assert_eq!(buffer.physical_addr, 0x1000);
        assert!(buffer.is_dma_mapped());
    }

    #[test]
    fn nonempty_buffers_reject_a_zero_physical_address() {
        let mut buffer = match DmaBuffer::allocate(8) {
            Ok(buffer) => buffer,
            Err(error) => panic!("expected allocation to succeed: {error:?}"),
        };

        let result = buffer.set_physical_addr(0);
        assert!(matches!(result, Err(DmaError::AllocFailed)));
    }

    #[test]
    fn direct_field_mutation_does_not_forge_mapping_state() {
        let mut buffer = match DmaBuffer::allocate(8) {
            Ok(buffer) => buffer,
            Err(error) => panic!("expected allocation to succeed: {error:?}"),
        };

        buffer.physical_addr = 0x2000;

        assert_eq!(buffer.physical_addr, 0x2000);
        assert!(!buffer.is_dma_mapped());
    }
}
