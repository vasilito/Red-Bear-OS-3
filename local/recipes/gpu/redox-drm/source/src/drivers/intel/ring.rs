use std::thread;
use std::time::Duration;

use log::{debug, info};
use redox_driver_sys::dma::DmaBuffer;
use redox_driver_sys::memory::MmioRegion;

use crate::driver::{DriverError, Result};

use super::gtt::IntelGtt;

const RING_BUFFER_BYTES: usize = 4096;
const RING_ALIGNMENT: usize = 4096;
const RING_WAIT_ATTEMPTS: usize = 2000;
const RING_WAIT_DELAY: Duration = Duration::from_micros(50);

const RBBASE: usize = 0x04;
const RBBASE_HI: usize = 0x08;
const RBTAIL: usize = 0x30;
const RBHEAD: usize = 0x34;
const RBSTART: usize = 0x38;
const RBCTL: usize = 0x3C;

const RING_CTL_ENABLE: u32 = 1 << 0;
const RING_CTL_SIZE_MASK: u32 = !0x0FFF;

const MI_NOOP: u32 = 0x0000_0000;
const MI_FLUSH_DW: u32 = 0x0200_0000;

#[derive(Clone, Copy, Debug)]
pub enum RingType {
    Render,
    Blitter,
    VideoEnhance,
}

pub struct IntelRing {
    mmio: MmioRegion,
    base: usize,
    head: u32,
    tail: u32,
    size: u32,
    ring_type: RingType,
    buffer: DmaBuffer,
    gpu_addr: Option<u64>,
    last_seqno: u64,
}

impl IntelRing {
    pub fn create(mmio: MmioRegion, ring_type: RingType) -> Result<Self> {
        let mut buffer = DmaBuffer::allocate(RING_BUFFER_BYTES, RING_ALIGNMENT)
            .map_err(|e| DriverError::Buffer(format!("Intel ring allocation failed: {e}")))?;
        zero_dma(&mut buffer);

        let ring = Self {
            mmio,
            base: ring_base(ring_type),
            head: 0,
            tail: 0,
            size: RING_BUFFER_BYTES as u32,
            ring_type,
            buffer,
            gpu_addr: None,
            last_seqno: 0,
        };

        ring.ensure_reg_access(RBCTL, core::mem::size_of::<u32>(), "ring control")?;
        ring.write_reg(RBHEAD, 0)?;
        ring.write_reg(RBTAIL, 0)?;
        ring.write_reg(RBSTART, 0)?;

        info!(
            "redox-drm: Intel {:?} ring allocated ({} bytes)",
            ring.ring_type, ring.size
        );
        Ok(ring)
    }

    pub fn bind_gtt(&mut self, gtt: &mut IntelGtt) -> Result<()> {
        if self.gpu_addr.is_some() {
            return Ok(());
        }

        let gpu_addr = gtt.alloc_range(self.size as u64)?;
        if let Err(error) = gtt.map_range(
            gpu_addr,
            self.buffer.physical_address() as u64,
            self.size as u64,
            1 << 1,
        ) {
            let _ = gtt.release_range(gpu_addr, self.size as u64);
            return Err(error);
        }

        self.gpu_addr = Some(gpu_addr);
        self.program_ring_registers(gpu_addr)?;
        Ok(())
    }

    pub fn submit_batch(&mut self, buffer: &[u32]) -> Result<()> {
        if buffer.is_empty() {
            return Ok(());
        }
        if self.gpu_addr.is_none() {
            return Err(DriverError::Initialization(
                "Intel ring must be bound into GGTT before submission".into(),
            ));
        }

        self.wait_for_space(buffer.len())?;

        for &dword in buffer {
            self.write_dword(dword)?;
        }

        self.publish_tail()?;
        self.last_seqno = self.last_seqno.saturating_add(1);
        debug!(
            "redox-drm: Intel {:?} ring submitted {} DWORDs seqno={}",
            self.ring_type,
            buffer.len(),
            self.last_seqno
        );
        Ok(())
    }

    pub fn wait_for_space(&mut self, count: usize) -> Result<()> {
        let required = (count * core::mem::size_of::<u32>()) as u32;
        if required >= self.size {
            return Err(DriverError::Buffer(format!(
                "Intel ring submission too large: {required} bytes >= ring size {}",
                self.size
            )));
        }

        for _ in 0..RING_WAIT_ATTEMPTS {
            self.sync_from_hw()?;
            if required <= self.free_bytes() {
                return Ok(());
            }
            thread::sleep(RING_WAIT_DELAY);
        }

        Err(DriverError::Buffer(format!(
            "Intel {:?} ring did not free {} bytes in time",
            self.ring_type, required
        )))
    }

    pub fn flush(&mut self) -> Result<()> {
        self.submit_batch(&[MI_FLUSH_DW, MI_NOOP])
    }

    pub fn has_activity(&mut self) -> Result<bool> {
        self.sync_from_hw()?;
        Ok(self.head != self.tail)
    }

    pub fn sync_from_hw(&mut self) -> Result<()> {
        self.head = self.read_reg(RBHEAD)? & (self.size - 1);
        self.tail = self.read_reg(RBTAIL)? & (self.size - 1);
        Ok(())
    }

    pub fn last_seqno(&self) -> u64 {
        self.last_seqno
    }

    fn program_ring_registers(&mut self, gpu_addr: u64) -> Result<()> {
        self.write_reg(RBHEAD, 0)?;
        self.write_reg(RBTAIL, 0)?;
        self.write_reg(RBSTART, lower_32(gpu_addr))?;
        self.write_reg(RBBASE, lower_32(gpu_addr))?;
        self.write_reg(RBBASE_HI, upper_32(gpu_addr))?;

        let mut ctl = self.read_reg(RBCTL)?;
        ctl &= !RING_CTL_SIZE_MASK;
        ctl |= (self.size - 0x1000) & RING_CTL_SIZE_MASK;
        ctl |= RING_CTL_ENABLE;
        self.write_reg(RBCTL, ctl)?;
        Ok(())
    }

    fn free_bytes(&self) -> u32 {
        let used = if self.tail >= self.head {
            self.tail - self.head
        } else {
            self.size - (self.head - self.tail)
        };
        self.size.saturating_sub(used).saturating_sub(4)
    }

    fn write_dword(&mut self, value: u32) -> Result<()> {
        let write_offset = self.tail as usize;
        let width = core::mem::size_of::<u32>();
        let end = write_offset
            .checked_add(width)
            .ok_or_else(|| DriverError::Buffer("Intel ring write offset overflow".into()))?;
        if end > self.buffer.len() {
            return Err(DriverError::Buffer(format!(
                "Intel ring write out of bounds: end={end:#x} size={:#x}",
                self.buffer.len()
            )));
        }
        let ptr = unsafe { self.buffer.as_mut_ptr().add(write_offset) as *mut u32 };
        unsafe { core::ptr::write_volatile(ptr, value) };

        self.tail = (self.tail + width as u32) % self.size;
        Ok(())
    }

    fn publish_tail(&self) -> Result<()> {
        self.write_reg(RBTAIL, self.tail)
    }

    fn read_reg(&self, reg: usize) -> Result<u32> {
        let offset = self
            .base
            .checked_add(reg)
            .ok_or_else(|| DriverError::Mmio("Intel ring register offset overflow".into()))?;
        self.ensure_reg_access(offset, core::mem::size_of::<u32>(), "ring read")?;
        Ok(self.mmio.read32(offset))
    }

    fn write_reg(&self, reg: usize, value: u32) -> Result<()> {
        let offset = self
            .base
            .checked_add(reg)
            .ok_or_else(|| DriverError::Mmio("Intel ring register offset overflow".into()))?;
        self.ensure_reg_access(offset, core::mem::size_of::<u32>(), "ring write")?;
        self.mmio.write32(offset, value);
        Ok(())
    }

    fn ensure_reg_access(&self, offset: usize, width: usize, op: &str) -> Result<()> {
        let end = offset.checked_add(width).ok_or_else(|| {
            DriverError::Mmio(format!("Intel {op} offset overflow at {offset:#x}"))
        })?;
        if end > self.mmio.size() {
            return Err(DriverError::Mmio(format!(
                "Intel {op} outside MMIO aperture: end={end:#x} size={:#x}",
                self.mmio.size()
            )));
        }
        Ok(())
    }
}

fn ring_base(ring_type: RingType) -> usize {
    match ring_type {
        RingType::Render => 0x02000,
        RingType::Blitter => 0x22000,
        RingType::VideoEnhance => 0x1A000,
    }
}

fn zero_dma(buffer: &mut DmaBuffer) {
    unsafe { core::ptr::write_bytes(buffer.as_mut_ptr(), 0, buffer.len()) };
}

fn lower_32(value: u64) -> u32 {
    value as u32
}

fn upper_32(value: u64) -> u32 {
    (value >> 32) as u32
}
