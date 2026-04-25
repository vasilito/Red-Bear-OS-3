use log::{info, warn};
use std::ptr;
#[cfg(no_amdgpu_c)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use crate::driver::{DriverError, Result};
use crate::kms::connector::synthetic_edid;
use crate::kms::{ConnectorInfo, ConnectorStatus, ConnectorType, ModeInfo};

#[repr(C)]
pub struct ConnectorInfoFFI {
    pub id: i32,
    pub connector_type: i32,
    pub connector_type_id: i32,
    pub connection: i32,
    pub mm_width: i32,
    pub mm_height: i32,
    pub encoder_id: i32,
}

#[cfg(not(no_amdgpu_c))]
unsafe extern "C" {
    #[link_name = "amdgpu_redox_init"]
    fn ffi_amdgpu_redox_init(
        mmio_base: *const u8,
        mmio_size: usize,
        fb_phys: u64,
        fb_size: usize,
    ) -> i32;

    #[link_name = "amdgpu_dc_detect_connectors"]
    fn ffi_amdgpu_dc_detect_connectors() -> i32;
    #[link_name = "amdgpu_dc_get_connector_info"]
    fn ffi_amdgpu_dc_get_connector_info(idx: i32, info: *mut ConnectorInfoFFI) -> i32;
    #[link_name = "amdgpu_dc_set_crtc"]
    fn ffi_amdgpu_dc_set_crtc(crtc_id: i32, fb_addr: u64, width: u32, height: u32) -> i32;

    #[link_name = "amdgpu_redox_cleanup"]
    fn ffi_amdgpu_redox_cleanup();

    #[link_name = "redox_pci_set_device_info"]
    fn ffi_redox_pci_set_device_info(
        vendor: u16,
        device: u16,
        bus_number: u8,
        dev_number: u8,
        func_number: u8,
        revision: u8,
        irq: u32,
        bar0_addr: u64,
        bar0_size: u64,
        bar2_addr: u64,
        bar2_size: u64,
    );
}

#[cfg(no_amdgpu_c)]
static FALLBACK_MMIO_BASE: AtomicUsize = AtomicUsize::new(0);
#[cfg(no_amdgpu_c)]
static FALLBACK_MMIO_SIZE: AtomicUsize = AtomicUsize::new(0);

#[cfg(no_amdgpu_c)]
const FALLBACK_ENOENT: i32 = 2;

#[cfg(no_amdgpu_c)]
fn amdgpu_dc_init(mmio_base: *const u8, mmio_size: usize) -> i32 {
    FALLBACK_MMIO_BASE.store(mmio_base as usize, Ordering::Relaxed);
    FALLBACK_MMIO_SIZE.store(mmio_size, Ordering::Relaxed);
    0
}

#[cfg(no_amdgpu_c)]
fn amdgpu_dc_init_with_fb(
    mmio_base: *const u8,
    mmio_size: usize,
    _fb_phys: u64,
    _fb_size: usize,
) -> i32 {
    FALLBACK_MMIO_BASE.store(mmio_base as usize, Ordering::Relaxed);
    FALLBACK_MMIO_SIZE.store(mmio_size, Ordering::Relaxed);
    0
}

#[cfg(no_amdgpu_c)]
fn amdgpu_dc_detect_connectors() -> i32 {
    warn!("redox-drm: compiled without AMD C backend (no_amdgpu_c); no real connector detection available");
    0
}

#[cfg(no_amdgpu_c)]
fn amdgpu_dc_get_connector_info(_idx: i32, _info: *mut ConnectorInfoFFI) -> i32 {
    -FALLBACK_ENOENT
}

#[cfg(no_amdgpu_c)]
fn amdgpu_dc_set_crtc(_crtc_id: i32, _fb_addr: u64, _width: u32, _height: u32) -> i32 {
    0
}

#[cfg(no_amdgpu_c)]
fn amdgpu_dc_cleanup() {
    FALLBACK_MMIO_BASE.store(0, Ordering::Relaxed);
    FALLBACK_MMIO_SIZE.store(0, Ordering::Relaxed);
}

pub fn set_pci_device_info(
    vendor: u16,
    device: u16,
    bus_number: u8,
    dev_number: u8,
    func_number: u8,
    revision: u8,
    irq: u32,
    bar0_addr: u64,
    bar0_size: u64,
    bar2_addr: u64,
    bar2_size: u64,
) {
    #[cfg(not(no_amdgpu_c))]
    unsafe {
        ffi_redox_pci_set_device_info(
            vendor,
            device,
            bus_number,
            dev_number,
            func_number,
            revision,
            irq,
            bar0_addr,
            bar0_size,
            bar2_addr,
            bar2_size,
        );
    }
    let _ = (
        vendor,
        device,
        bus_number,
        dev_number,
        func_number,
        revision,
        irq,
        bar0_addr,
        bar0_size,
        bar2_addr,
        bar2_size,
    );
}

#[cfg(not(no_amdgpu_c))]
fn amdgpu_dc_init(mmio_base: *const u8, mmio_size: usize) -> i32 {
    unsafe { ffi_amdgpu_redox_init(mmio_base, mmio_size, 0, 0) }
}

#[cfg(not(no_amdgpu_c))]
fn amdgpu_dc_init_with_fb(
    mmio_base: *const u8,
    mmio_size: usize,
    fb_phys: u64,
    fb_size: usize,
) -> i32 {
    unsafe { ffi_amdgpu_redox_init(mmio_base, mmio_size, fb_phys, fb_size) }
}

#[cfg(not(no_amdgpu_c))]
fn amdgpu_dc_detect_connectors() -> i32 {
    unsafe { ffi_amdgpu_dc_detect_connectors() }
}

#[cfg(not(no_amdgpu_c))]
fn amdgpu_dc_get_connector_info(idx: i32, info: *mut ConnectorInfoFFI) -> i32 {
    unsafe { ffi_amdgpu_dc_get_connector_info(idx, info) }
}

#[cfg(not(no_amdgpu_c))]
fn amdgpu_dc_set_crtc(crtc_id: i32, fb_addr: u64, width: u32, height: u32) -> i32 {
    unsafe { ffi_amdgpu_dc_set_crtc(crtc_id, fb_addr, width, height) }
}

#[cfg(not(no_amdgpu_c))]
fn amdgpu_dc_cleanup() {
    unsafe { ffi_amdgpu_redox_cleanup() }
}

pub struct DisplayCore {
    initialized: bool,
    mmio_base: usize,
    mmio_size: usize,
    fb_phys: u64,
    fb_size: usize,
}

impl DisplayCore {
    pub fn new(mmio_base: *const u8, mmio_size: usize) -> Result<Self> {
        Self::with_framebuffer(mmio_base, mmio_size, 0, 0)
    }

    pub fn with_framebuffer(
        mmio_base: *const u8,
        mmio_size: usize,
        fb_phys: u64,
        fb_size: usize,
    ) -> Result<Self> {
        let rc = if fb_phys != 0 && fb_size != 0 {
            amdgpu_dc_init_with_fb(mmio_base, mmio_size, fb_phys, fb_size)
        } else {
            amdgpu_dc_init(mmio_base, mmio_size)
        };
        if rc < 0 {
            return Err(DriverError::Initialization(format!(
                "amdgpu display init failed with status {}",
                rc
            )));
        }

        info!(
            "redox-drm: AMD DC initialized with {} bytes of MMIO, fb_phys={:#x}, fb_size={}",
            mmio_size, fb_phys, fb_size
        );
        Ok(Self {
            initialized: true,
            mmio_base: mmio_base as usize,
            mmio_size,
            fb_phys,
            fb_size,
        })
    }

    pub fn fb_phys(&self) -> u64 {
        self.fb_phys
    }

    pub fn fb_size(&self) -> usize {
        self.fb_size
    }

    pub fn detect_connectors(&self) -> Result<Vec<ConnectorInfo>> {
        if !self.initialized {
            return Err(DriverError::Initialization(
                "display core not initialized".to_string(),
            ));
        }

        let count = amdgpu_dc_detect_connectors();
        if count < 0 {
            return Err(DriverError::Mmio(format!(
                "AMD DC connector detection failed with status {}",
                count
            )));
        }
        if count == 0 {
            warn!("redox-drm: AMD DC reported 0 connected displays");
            return Ok(Vec::new());
        }

        let mut connectors = Vec::new();
        for idx in 0..count {
            let mut raw = ConnectorInfoFFI {
                id: 0,
                connector_type: 0,
                connector_type_id: 0,
                connection: 2,
                mm_width: 0,
                mm_height: 0,
                encoder_id: 0,
            };

            let rc = amdgpu_dc_get_connector_info(idx, &mut raw as *mut ConnectorInfoFFI);
            if rc < 0 {
                warn!(
                    "redox-drm: failed to fetch connector {} from AMD DC (status {})",
                    idx, rc
                );
                continue;
            }

            connectors.push(ConnectorInfo {
                id: raw.id.max(0) as u32,
                connector_type: map_connector_type(raw.connector_type),
                connector_type_id: raw.connector_type_id.max(0) as u32,
                connection: map_connection_status(raw.connection),
                mm_width: raw.mm_width.max(0) as u32,
                mm_height: raw.mm_height.max(0) as u32,
                encoder_id: raw.encoder_id.max(0) as u32,
                modes: self.modes_for_connector(idx as u32),
            });
        }

        Ok(connectors)
    }

    pub fn set_crtc(&self, crtc_id: u32, fb_addr: u64, width: u32, height: u32) -> Result<()> {
        if !self.initialized {
            return Err(DriverError::Initialization(
                "display core must be initialized before modesetting".to_string(),
            ));
        }

        let rc = amdgpu_dc_set_crtc(crtc_id as i32, fb_addr, width, height);
        if rc < 0 {
            return Err(DriverError::Mmio(format!(
                "amdgpu_dc_set_crtc failed for CRTC {} with status {}",
                crtc_id, rc
            )));
        }

        Ok(())
    }

    pub fn flip_surface(&self, crtc_id: u32, fb_addr: u64) -> Result<()> {
        if !self.initialized {
            return Err(DriverError::Initialization(
                "display core must be initialized before page flip".to_string(),
            ));
        }

        const HUBP_FLIP_ADDR_LOW: usize = 0x5800;
        const HUBP_FLIP_ADDR_HIGH: usize = 0x5804;

        let hubp_base = HUBP_FLIP_ADDR_LOW + (crtc_id as usize) * 0x400;
        let hubp_high = HUBP_FLIP_ADDR_HIGH + (crtc_id as usize) * 0x400;

        self.write_reg(hubp_high, (fb_addr >> 32) as u32)?;
        self.write_reg(hubp_base, fb_addr as u32)?;

        let flip_control = 0x5834 + (crtc_id as usize) * 0x400;
        self.write_reg(flip_control, 1)?;

        Ok(())
    }

    pub fn read_edid(&self, connector_index: u32) -> Vec<u8> {
        if !self.initialized {
            return Vec::new();
        }

        match self.read_edid_block(connector_index, 0x00) {
            Ok(edid) if edid.len() >= 128 => edid,
            Ok(short) => {
                log::warn!(
                    "redox-drm: short EDID ({} bytes) from AMD connector {}",
                    short.len(),
                    connector_index
                );
                Vec::new()
            }
            Err(e) => {
                log::warn!(
                    "redox-drm: EDID read failed for AMD connector {}: {}",
                    connector_index,
                    e
                );
                Vec::new()
            }
        }
    }

    fn modes_for_connector(&self, connector_index: u32) -> Vec<ModeInfo> {
        let real_edid = self.read_edid(connector_index);
        let mut modes = ModeInfo::from_edid(&real_edid);
        if modes.is_empty() {
            modes = ModeInfo::from_edid(&synthetic_edid());
        }
        if modes.is_empty() {
            modes.push(ModeInfo::default_1080p());
        }
        modes
    }

    fn read_edid_block(&self, connector_index: u32, offset: u8) -> Result<Vec<u8>> {
        const MM_DC_I2C_CONTROL: usize = 0x1e98;
        const MM_DC_I2C_ARBITRATION: usize = 0x1e99;
        const MM_DC_I2C_SW_STATUS: usize = 0x1e9b;
        const MM_DC_I2C_DDC1_SPEED: usize = 0x1ea2;
        const MM_DC_I2C_DDC1_SETUP: usize = 0x1ea3;
        const MM_DC_I2C_TRANSACTION0: usize = 0x1eae;
        const MM_DC_I2C_TRANSACTION1: usize = 0x1eaf;
        const MM_DC_I2C_DATA: usize = 0x1eb2;

        const CONTROL_GO: u32 = 0x0000_0001;
        const CONTROL_SOFT_RESET: u32 = 0x0000_0002;
        const CONTROL_SW_STATUS_RESET: u32 = 0x0000_0008;
        const CONTROL_DDC_SELECT_MASK: u32 = 0x0000_0700;
        const CONTROL_DDC_SELECT_SHIFT: u32 = 8;
        const CONTROL_TRANSACTION_COUNT_MASK: u32 = 0x0030_0000;
        const CONTROL_TRANSACTION_COUNT_SHIFT: u32 = 20;

        const ARBITRATION_STATUS_MASK: u32 = 0x0000_000c;
        const ARBITRATION_STATUS_SHIFT: u32 = 2;
        const ARBITRATION_REQ: u32 = 0x0010_0000;
        const ARBITRATION_DONE: u32 = 0x0020_0000;

        const SW_STATUS_DONE: u32 = 0x0000_0004;
        const SW_STATUS_ABORTED: u32 = 0x0000_0010;
        const SW_STATUS_TIMEOUT: u32 = 0x0000_0020;
        const SW_STATUS_NACK: u32 = 0x0000_0100;

        const SETUP_ENABLE: u32 = 0x0000_0040;
        const SETUP_SEND_RESET_LENGTH: u32 = 0x0000_0004;
        const SETUP_TIME_LIMIT_SHIFT: u32 = 24;

        const SPEED_THRESHOLD: u32 = 0x0000_0002;
        const SPEED_PRESCALE_SHIFT: u32 = 16;
        const SPEED_START_STOP_TIMING: u32 = 0x0000_0200;

        const TX_RW: u32 = 0x0000_0001;
        const TX_STOP_ON_NACK: u32 = 0x0000_0100;
        const TX_START: u32 = 0x0000_1000;
        const TX_STOP: u32 = 0x0000_2000;
        const TX_COUNT_SHIFT: u32 = 16;

        const DATA_RW: u32 = 0x0000_0001;
        const DATA_VALUE_SHIFT: u32 = 8;
        const DATA_VALUE_MASK: u32 = 0x0000_ff00;
        const DATA_INDEX_SHIFT: u32 = 16;
        const DATA_INDEX_WRITE: u32 = 0x8000_0000;

        const EDID_WRITE_ADDR: u8 = 0xa0;
        const EDID_READ_ADDR: u8 = 0xa1;
        const EDID_BLOCK_SIZE: usize = 128;
        const I2C_STATUS_IDLE: u32 = 0;
        const I2C_STATUS_USED_BY_SW: u32 = 1;
        const I2C_WAIT_RETRIES: usize = 200;

        self.ensure_mmio_reg(MM_DC_I2C_DATA)?;
        self.ensure_mmio_reg(MM_DC_I2C_TRANSACTION1)?;

        let connector_select = connector_index & 0x7;
        let arbitration = self.read_reg(MM_DC_I2C_ARBITRATION)?;
        let status = (arbitration & ARBITRATION_STATUS_MASK) >> ARBITRATION_STATUS_SHIFT;
        if status == I2C_STATUS_IDLE {
            self.write_reg(MM_DC_I2C_ARBITRATION, arbitration | ARBITRATION_REQ)?;
        } else if status != I2C_STATUS_USED_BY_SW {
            return Err(DriverError::Mmio(format!(
                "AMD I2C engine unavailable for connector {} (status {})",
                connector_index, status
            )));
        }

        let control = self.read_reg(MM_DC_I2C_CONTROL)?;
        self.write_reg(
            MM_DC_I2C_CONTROL,
            (control
                & !(CONTROL_SOFT_RESET | CONTROL_DDC_SELECT_MASK | CONTROL_TRANSACTION_COUNT_MASK))
                | CONTROL_SW_STATUS_RESET
                | (connector_select << CONTROL_DDC_SELECT_SHIFT),
        )?;

        self.write_reg(
            MM_DC_I2C_DDC1_SETUP,
            SETUP_ENABLE | SETUP_SEND_RESET_LENGTH | (3 << SETUP_TIME_LIMIT_SHIFT),
        )?;
        self.write_reg(
            MM_DC_I2C_DDC1_SPEED,
            SPEED_THRESHOLD | SPEED_START_STOP_TIMING | (40 << SPEED_PRESCALE_SHIFT),
        )?;
        self.write_reg(
            MM_DC_I2C_TRANSACTION0,
            TX_START | TX_STOP_ON_NACK | (1 << TX_COUNT_SHIFT),
        )?;
        self.write_reg(
            MM_DC_I2C_TRANSACTION1,
            TX_RW
                | TX_START
                | TX_STOP
                | TX_STOP_ON_NACK
                | ((EDID_BLOCK_SIZE as u32) << TX_COUNT_SHIFT),
        )?;

        self.write_reg(
            MM_DC_I2C_DATA,
            ((EDID_WRITE_ADDR as u32) << DATA_VALUE_SHIFT) | DATA_INDEX_WRITE,
        )?;
        self.write_reg(MM_DC_I2C_DATA, (offset as u32) << DATA_VALUE_SHIFT)?;
        self.write_reg(MM_DC_I2C_DATA, (EDID_READ_ADDR as u32) << DATA_VALUE_SHIFT)?;

        let control = self.read_reg(MM_DC_I2C_CONTROL)?;
        self.write_reg(
            MM_DC_I2C_CONTROL,
            (control & !CONTROL_TRANSACTION_COUNT_MASK)
                | (1 << CONTROL_TRANSACTION_COUNT_SHIFT)
                | CONTROL_GO,
        )?;

        let mut final_status = 0;
        for _ in 0..I2C_WAIT_RETRIES {
            final_status = self.read_reg(MM_DC_I2C_SW_STATUS)?;
            if (final_status
                & (SW_STATUS_DONE | SW_STATUS_ABORTED | SW_STATUS_TIMEOUT | SW_STATUS_NACK))
                != 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }

        self.write_reg(MM_DC_I2C_ARBITRATION, ARBITRATION_DONE)?;

        if (final_status & SW_STATUS_DONE) == 0 {
            return Err(DriverError::Mmio(format!(
                "AMD I2C EDID read did not complete for connector {} (status {:#x})",
                connector_index, final_status
            )));
        }
        if (final_status & (SW_STATUS_ABORTED | SW_STATUS_TIMEOUT | SW_STATUS_NACK)) != 0 {
            return Err(DriverError::Mmio(format!(
                "AMD I2C EDID read failed for connector {} (status {:#x})",
                connector_index, final_status
            )));
        }

        self.write_reg(
            MM_DC_I2C_DATA,
            DATA_RW | DATA_INDEX_WRITE | ((2_u32) << DATA_INDEX_SHIFT),
        )?;

        let mut edid = Vec::with_capacity(EDID_BLOCK_SIZE);
        for _ in 0..EDID_BLOCK_SIZE {
            let value = self.read_reg(MM_DC_I2C_DATA)?;
            edid.push(((value & DATA_VALUE_MASK) >> DATA_VALUE_SHIFT) as u8);
        }

        Ok(edid)
    }

    fn ensure_mmio_reg(&self, reg: usize) -> Result<()> {
        let offset = reg.checked_mul(4).ok_or_else(|| {
            DriverError::Mmio(format!("AMD register offset overflow for {reg:#x}"))
        })?;
        if offset + 4 > self.mmio_size {
            return Err(DriverError::Mmio(format!(
                "AMD register {reg:#x} outside MMIO aperture {:#x}",
                self.mmio_size
            )));
        }
        Ok(())
    }

    fn read_reg(&self, reg: usize) -> Result<u32> {
        self.ensure_mmio_reg(reg)?;
        let offset = reg * 4;
        let ptr = (self.mmio_base + offset) as *const u32;
        let value = unsafe { ptr::read_volatile(ptr) };
        Ok(u32::from_le(value))
    }

    fn write_reg(&self, reg: usize, value: u32) -> Result<()> {
        self.ensure_mmio_reg(reg)?;
        let offset = reg * 4;
        let ptr = (self.mmio_base + offset) as *mut u32;
        unsafe { ptr::write_volatile(ptr, value.to_le()) };
        Ok(())
    }
}

impl Drop for DisplayCore {
    fn drop(&mut self) {
        if self.initialized {
            amdgpu_dc_cleanup();
        }
    }
}

fn map_connector_type(value: i32) -> ConnectorType {
    match value {
        1 => ConnectorType::VGA,
        2 => ConnectorType::DVII,
        3 => ConnectorType::DVID,
        4 => ConnectorType::DVIA,
        10 => ConnectorType::DisplayPort,
        11 => ConnectorType::HDMIA,
        14 => ConnectorType::EDP,
        15 => ConnectorType::Virtual,
        _ => ConnectorType::Unknown,
    }
}

fn map_connection_status(value: i32) -> ConnectorStatus {
    match value {
        1 => ConnectorStatus::Connected,
        2 => ConnectorStatus::Disconnected,
        _ => ConnectorStatus::Unknown,
    }
}
