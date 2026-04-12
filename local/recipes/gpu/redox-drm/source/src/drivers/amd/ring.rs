use core::sync::atomic::{fence, AtomicPtr, AtomicUsize, Ordering};

use log::{info, warn};
use redox_driver_sys::dma::DmaBuffer;
use redox_driver_sys::memory::MmioRegion;

use crate::driver::{DriverError, Result};

const RING_BUFFER_BYTES: usize = 4096;
const RING_BUFFER_DWORDS: usize = RING_BUFFER_BYTES / 4;
const RING_ALIGNMENT_BYTES: usize = 4096;
const FENCE_BUFFER_BYTES: usize = 16;
const WPTR_STRIDE_DWORDS: usize = 1;

const SDMA_OP_NOP: u32 = 0;
const SDMA_OP_FENCE: u32 = 5;
const SDMA_OP_TRAP: u32 = 6;

const SDMA0_GFX_RB_CNTL: usize = 0x0080 * 4;
const SDMA0_GFX_RB_BASE: usize = 0x0081 * 4;
const SDMA0_GFX_RB_BASE_HI: usize = 0x0082 * 4;
const SDMA0_GFX_RB_RPTR: usize = 0x0083 * 4;
const SDMA0_GFX_RB_RPTR_HI: usize = 0x0084 * 4;
const SDMA0_GFX_RB_WPTR: usize = 0x0085 * 4;
const SDMA0_GFX_RB_WPTR_HI: usize = 0x0086 * 4;
const SDMA0_GFX_RB_WPTR_POLL_CNTL: usize = 0x0087 * 4;
const SDMA0_GFX_RB_RPTR_ADDR_HI: usize = 0x0088 * 4;
const SDMA0_GFX_RB_RPTR_ADDR_LO: usize = 0x0089 * 4;
const SDMA0_GFX_IB_CNTL: usize = 0x008a * 4;
const SDMA0_GFX_RB_WPTR_POLL_ADDR_HI: usize = 0x00b2 * 4;
const SDMA0_GFX_RB_WPTR_POLL_ADDR_LO: usize = 0x00b3 * 4;
const SDMA0_GFX_MINOR_PTR_UPDATE: usize = 0x00b5 * 4;

const SDMA_RB_CNTL_RB_ENABLE: u32 = 1 << 0;
const SDMA_RB_CNTL_RB_SIZE_SHIFT: u32 = 1;
const SDMA_RB_CNTL_RB_SIZE_MASK: u32 = 0x1f << SDMA_RB_CNTL_RB_SIZE_SHIFT;
const SDMA_RB_CNTL_RPTR_WRITEBACK_ENABLE: u32 = 1 << 12;
const SDMA_IB_CNTL_IB_ENABLE: u32 = 1 << 0;

const FENCE_OFFSET_BYTES: usize = 0;
const WPTR_POLL_OFFSET_BYTES: usize = 8;

static MMIO_BASE: AtomicPtr<u8> = AtomicPtr::new(core::ptr::null_mut());
static MMIO_SIZE: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug)]
struct MmioBinding {
    base: usize,
    size: usize,
}

// Safety: MmioBinding holds raw address integers, not pointers.
// It is safe to send between threads because register access is volatile.
unsafe impl Send for MmioBinding {}
unsafe impl Sync for MmioBinding {}

impl MmioBinding {
    fn try_load() -> Option<Self> {
        let base = MMIO_BASE.load(Ordering::Acquire);
        let size = MMIO_SIZE.load(Ordering::Acquire);
        if base.is_null() {
            return None;
        }
        Some(Self {
            base: base as usize,
            size,
        })
    }

    fn read32(&self, offset: usize) -> Result<u32> {
        if offset.checked_add(4).is_none_or(|end| end > self.size) {
            return Err(DriverError::Mmio(format!(
                "AMD ring MMIO read out of bounds: offset={offset:#x} size={:#x}",
                self.size
            )));
        }

        let ptr = (self.base + offset) as *const u32;
        Ok(unsafe { core::ptr::read_volatile(ptr) })
    }

    fn write32(&self, offset: usize, value: u32) -> Result<()> {
        if offset.checked_add(4).is_none_or(|end| end > self.size) {
            return Err(DriverError::Mmio(format!(
                "AMD ring MMIO write out of bounds: offset={offset:#x} size={:#x}",
                self.size
            )));
        }

        let ptr = (self.base + offset) as *mut u32;
        unsafe { core::ptr::write_volatile(ptr, value) };
        Ok(())
    }
}

#[derive(Default)]
pub struct RingManager {
    initialized: bool,
    ring_buffer: Option<DmaBuffer>,
    fence_buffer: Option<DmaBuffer>,
    mmio: Option<MmioBinding>,
    ring_size_dwords: u32,
    read_ptr: u64,
    write_ptr: u64,
    next_seqno: u64,
    last_signaled_seqno: u64,
}

impl RingManager {
    pub fn new() -> Self {
        Self {
            initialized: false,
            ring_buffer: None,
            fence_buffer: None,
            mmio: None,
            ring_size_dwords: RING_BUFFER_DWORDS as u32,
            read_ptr: 0,
            write_ptr: 0,
            next_seqno: 1,
            last_signaled_seqno: 0,
        }
    }

    pub fn initialize(&mut self) -> Result<()> {
        let mut ring_buffer = DmaBuffer::allocate(RING_BUFFER_BYTES, RING_ALIGNMENT_BYTES)
            .map_err(|e| DriverError::Buffer(format!("ring buffer allocation failed: {e}")))?;
        let mut fence_buffer =
            DmaBuffer::allocate(FENCE_BUFFER_BYTES, core::mem::align_of::<u64>())
                .map_err(|e| DriverError::Buffer(format!("fence buffer allocation failed: {e}")))?;

        Self::zero_dma(&mut ring_buffer);
        Self::zero_dma(&mut fence_buffer);

        self.mmio = MmioBinding::try_load();
        self.program_ring(&ring_buffer, &fence_buffer)?;

        self.ring_buffer = Some(ring_buffer);
        self.fence_buffer = Some(fence_buffer);
        self.read_ptr = 0;
        self.write_ptr = 0;
        self.next_seqno = 1;
        self.last_signaled_seqno = 0;
        self.initialized = true;

        info!(
            "redox-drm: AMD ring manager initialized with {} DW ring buffer{}",
            self.ring_size_dwords,
            if self.mmio.is_some() {
                " and SDMA MMIO programming"
            } else {
                " (MMIO binding unavailable; submissions stay software-tracked)"
            }
        );

        Ok(())
    }

    pub fn page_flip(&mut self) -> Result<u64> {
        self.ensure_initialized()?;

        let seqno = self.next_seqno;
        self.next_seqno = self.next_seqno.saturating_add(1);

        let mut packet = Vec::with_capacity(16);
        self.emit_flip(&mut packet, seqno);
        self.emit_fence(&mut packet, seqno)?;

        self.submit(&packet, seqno)
    }

    pub(crate) fn bind_mmio(mmio: &MmioRegion) {
        MMIO_BASE.store(mmio.as_ptr() as *mut u8, Ordering::Release);
        MMIO_SIZE.store(mmio.size(), Ordering::Release);
    }

    fn ensure_initialized(&self) -> Result<()> {
        if self.initialized {
            Ok(())
        } else {
            Err(DriverError::Initialization(
                "ring manager must be initialized before page flips".to_string(),
            ))
        }
    }

    fn program_ring(&self, ring_buffer: &DmaBuffer, fence_buffer: &DmaBuffer) -> Result<()> {
        let Some(mmio) = self.mmio else {
            warn!(
                "redox-drm: AMD ring manager has no MMIO binding; skipping SDMA register programming"
            );
            return Ok(());
        };

        let ring_addr = ring_buffer.physical_address() as u64;
        let fence_addr = fence_buffer.physical_address() as u64 + FENCE_OFFSET_BYTES as u64;
        let wptr_poll_addr = fence_buffer.physical_address() as u64 + WPTR_POLL_OFFSET_BYTES as u64;

        let mut rb_cntl = mmio.read32(SDMA0_GFX_RB_CNTL)?;
        rb_cntl &= !(SDMA_RB_CNTL_RB_ENABLE | SDMA_RB_CNTL_RB_SIZE_MASK);
        rb_cntl |=
            (self.ring_size_order() << SDMA_RB_CNTL_RB_SIZE_SHIFT) & SDMA_RB_CNTL_RB_SIZE_MASK;
        mmio.write32(SDMA0_GFX_RB_CNTL, rb_cntl)?;

        mmio.write32(SDMA0_GFX_RB_RPTR, 0)?;
        mmio.write32(SDMA0_GFX_RB_RPTR_HI, 0)?;
        mmio.write32(SDMA0_GFX_RB_WPTR, 0)?;
        mmio.write32(SDMA0_GFX_RB_WPTR_HI, 0)?;

        mmio.write32(SDMA0_GFX_RB_RPTR_ADDR_HI, upper_32(fence_addr))?;
        mmio.write32(SDMA0_GFX_RB_RPTR_ADDR_LO, lower_32(fence_addr) & !0x3)?;

        rb_cntl |= SDMA_RB_CNTL_RPTR_WRITEBACK_ENABLE;
        mmio.write32(SDMA0_GFX_RB_CNTL, rb_cntl)?;

        mmio.write32(SDMA0_GFX_RB_BASE, lower_32(ring_addr >> 8))?;
        mmio.write32(SDMA0_GFX_RB_BASE_HI, lower_32(ring_addr >> 40))?;

        mmio.write32(SDMA0_GFX_MINOR_PTR_UPDATE, 1)?;
        mmio.write32(SDMA0_GFX_RB_WPTR, 0)?;
        mmio.write32(SDMA0_GFX_RB_WPTR_HI, 0)?;
        mmio.write32(SDMA0_GFX_MINOR_PTR_UPDATE, 0)?;

        mmio.write32(SDMA0_GFX_RB_WPTR_POLL_ADDR_LO, lower_32(wptr_poll_addr))?;
        mmio.write32(SDMA0_GFX_RB_WPTR_POLL_ADDR_HI, upper_32(wptr_poll_addr))?;
        mmio.write32(SDMA0_GFX_RB_WPTR_POLL_CNTL, 0)?;

        rb_cntl |= SDMA_RB_CNTL_RB_ENABLE;
        mmio.write32(SDMA0_GFX_RB_CNTL, rb_cntl)?;

        let mut ib_cntl = mmio.read32(SDMA0_GFX_IB_CNTL)?;
        ib_cntl |= SDMA_IB_CNTL_IB_ENABLE;
        mmio.write32(SDMA0_GFX_IB_CNTL, ib_cntl)?;

        Ok(())
    }

    fn submit(&mut self, commands: &[u32], seqno: u64) -> Result<u64> {
        self.refresh_read_ptr();
        self.ensure_space(commands.len())?;

        for &command in commands {
            self.write_ring_dword(command)?;
        }

        fence(Ordering::Release);
        self.publish_wptr()?;

        if self.mmio.is_none() {
            self.write_completed_seqno(seqno)?;
        }

        Ok(seqno)
    }

    fn refresh_read_ptr(&mut self) {
        if let Some(mmio) = self.mmio {
            let low = mmio.read32(SDMA0_GFX_RB_RPTR).unwrap_or(0) as u64;
            let high = mmio.read32(SDMA0_GFX_RB_RPTR_HI).unwrap_or(0) as u64;
            self.read_ptr = ((high << 32) | low) >> 2;
        } else {
            self.read_ptr = self.write_ptr;
        }
    }

    fn ensure_space(&self, required_dwords: usize) -> Result<()> {
        if required_dwords >= self.ring_capacity() {
            return Err(DriverError::Buffer(format!(
                "ring submission too large: {} DW exceeds capacity {} DW",
                required_dwords,
                self.ring_capacity() - 1
            )));
        }

        let used = self.used_dwords();
        let free = self.ring_capacity().saturating_sub(used).saturating_sub(1);
        if required_dwords <= free {
            Ok(())
        } else {
            Err(DriverError::Buffer(format!(
                "ring buffer full: required {} DW, free {} DW",
                required_dwords, free
            )))
        }
    }

    fn used_dwords(&self) -> usize {
        let size = self.ring_capacity() as u64;
        ((self.write_ptr + size).wrapping_sub(self.read_ptr) % size) as usize
    }

    fn write_ring_dword(&mut self, value: u32) -> Result<()> {
        let capacity = self.ring_capacity();
        let ring_buffer = self
            .ring_buffer
            .as_mut()
            .ok_or_else(|| DriverError::Initialization("ring buffer missing".to_string()))?;

        let index = (self.write_ptr as usize) % capacity;
        let ptr = unsafe {
            ring_buffer
                .as_mut_ptr()
                .add(index * core::mem::size_of::<u32>()) as *mut u32
        };
        unsafe { core::ptr::write_volatile(ptr, value) };

        self.write_ptr = (self.write_ptr + WPTR_STRIDE_DWORDS as u64) % capacity as u64;
        Ok(())
    }

    fn publish_wptr(&mut self) -> Result<()> {
        self.write_wptr_shadow(self.write_ptr)?;

        let Some(mmio) = self.mmio else {
            return Ok(());
        };

        mmio.write32(SDMA0_GFX_MINOR_PTR_UPDATE, 1)?;
        mmio.write32(SDMA0_GFX_RB_WPTR, lower_32(self.write_ptr << 2))?;
        mmio.write32(SDMA0_GFX_RB_WPTR_HI, upper_32(self.write_ptr << 2))?;
        mmio.write32(SDMA0_GFX_MINOR_PTR_UPDATE, 0)?;
        Ok(())
    }

    fn emit_nop(&self, packet: &mut Vec<u32>, count: u32) {
        for _ in 0..count {
            packet.push(SDMA_OP_NOP);
        }
    }

    fn emit_flip(&self, packet: &mut Vec<u32>, seqno: u64) {
        self.emit_nop(packet, 2);
        packet.push(0x5049_4c46);
        packet.push(lower_32(seqno));
        packet.push(upper_32(seqno));
    }

    fn emit_fence(&self, packet: &mut Vec<u32>, seqno: u64) -> Result<()> {
        let fence_addr = self.fence_address()?;

        packet.push(SDMA_OP_FENCE);
        packet.push(lower_32(fence_addr));
        packet.push(upper_32(fence_addr));
        packet.push(lower_32(seqno));

        packet.push(SDMA_OP_FENCE);
        packet.push(lower_32(fence_addr + 4));
        packet.push(upper_32(fence_addr + 4));
        packet.push(upper_32(seqno));

        packet.push(SDMA_OP_TRAP);
        packet.push(0);

        Ok(())
    }

    fn fence_address(&self) -> Result<u64> {
        let fence_buffer = self
            .fence_buffer
            .as_ref()
            .ok_or_else(|| DriverError::Initialization("fence buffer missing".to_string()))?;
        Ok(fence_buffer.physical_address() as u64 + FENCE_OFFSET_BYTES as u64)
    }

    fn write_completed_seqno(&mut self, seqno: u64) -> Result<()> {
        let fence_buffer = self
            .fence_buffer
            .as_mut()
            .ok_or_else(|| DriverError::Initialization("fence buffer missing".to_string()))?;
        let ptr = unsafe { fence_buffer.as_mut_ptr().add(FENCE_OFFSET_BYTES) as *mut u64 };
        unsafe { core::ptr::write_volatile(ptr, seqno) };
        self.last_signaled_seqno = seqno;
        Ok(())
    }

    fn write_wptr_shadow(&mut self, wptr_dwords: u64) -> Result<()> {
        let fence_buffer = self
            .fence_buffer
            .as_mut()
            .ok_or_else(|| DriverError::Initialization("fence buffer missing".to_string()))?;
        let ptr = unsafe { fence_buffer.as_mut_ptr().add(WPTR_POLL_OFFSET_BYTES) as *mut u64 };
        unsafe { core::ptr::write_volatile(ptr, wptr_dwords << 2) };
        Ok(())
    }

    fn ring_size_order(&self) -> u32 {
        self.ring_size_dwords.ilog2()
    }

    fn ring_capacity(&self) -> usize {
        self.ring_size_dwords as usize
    }

    fn zero_dma(buffer: &mut DmaBuffer) {
        unsafe { core::ptr::write_bytes(buffer.as_mut_ptr(), 0, buffer.len()) };
    }
}

fn lower_32(value: u64) -> u32 {
    value as u32
}

fn upper_32(value: u64) -> u32 {
    (value >> 32) as u32
}
