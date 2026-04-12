pub mod display;
pub mod gtt;
pub mod ring;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use log::{debug, info, warn};
use redox_driver_sys::memory::MmioRegion;
use redox_driver_sys::pci::{PciBarInfo, PciDevice, PciDeviceInfo};

use crate::driver::{DriverError, GpuDriver, Result};
use crate::drivers::interrupt::InterruptHandle;
use crate::gem::{GemHandle, GemManager};
use crate::kms::connector::{synthetic_edid, Connector};
use crate::kms::crtc::Crtc;
use crate::kms::encoder::Encoder;
use crate::kms::{ConnectorInfo, ModeInfo};

use self::display::DisplayCore;
use self::gtt::GttManager;
use self::ring::RingManager;

const AMD_IH_RB_CNTL: usize = 0x0080;
const AMD_IH_RB_RPTR: usize = 0x0083;
const AMD_IH_RB_WPTR: usize = 0x0084;
const AMD_IH_CNTL: usize = 0x00c0;
const AMD_IH_STATUS: usize = 0x00c2;

const AMD_DCN_DISP_INTERRUPT_STATUS: [usize; 6] = [0x012a, 0x012b, 0x012c, 0x012d, 0x012e, 0x012f];
const AMD_DCN_HPD_INT_STATUS: [usize; 6] = [0x1f14, 0x1f1c, 0x1f24, 0x1f2c, 0x1f34, 0x1f3c];
const AMD_DCN_HPD_CONTROL: [usize; 6] = [0x1f16, 0x1f1e, 0x1f26, 0x1f2e, 0x1f36, 0x1f3e];

const AMD_DISP_INTERRUPT_VBLANK_MASK: u32 = 0x0000_0008;
const AMD_DISP_INTERRUPT_HPD_MASK: u32 = 0x0002_0000;
const AMD_HPD_INT_STATUS_MASK: u32 = 0x0000_0001;
const AMD_HPD_RX_INT_STATUS_MASK: u32 = 0x0000_0100;
const AMD_HPD_INT_ACK_MASK: u32 = 0x0000_0001;
const AMD_HPD_RX_INT_ACK_MASK: u32 = 0x0000_0100;
const AMD_IH_STATUS_INTERRUPT_PENDING_MASK: u32 = 0x0000_0001;
const AMD_IH_STATUS_RING_OVERFLOW_MASK: u32 = 0x0000_0002;

#[derive(Clone, Debug)]
pub enum IrqEvent {
    Vblank { crtc_id: u32, count: u64 },
    Hotplug { connector_id: u32 },
    Unknown,
}

pub struct AmdDriver {
    info: PciDeviceInfo,
    mmio: MmioRegion,
    irq_handle: Option<InterruptHandle>,
    display: DisplayCore,
    gem: Mutex<GemManager>,
    connectors: Mutex<Vec<Connector>>,
    crtcs: Mutex<Vec<Crtc>>,
    encoders: Mutex<Vec<Encoder>>,
    gtt: Mutex<GttManager>,
    ring: Mutex<RingManager>,
    vblank_count: AtomicU64,
    hotplug_pending: AtomicBool,
    firmware: HashMap<String, Vec<u8>>,
}

impl AmdDriver {
    pub fn new(info: PciDeviceInfo, firmware: HashMap<String, Vec<u8>>) -> Result<Self> {
        let bar0 = find_memory_bar0(&info)?;
        let bar2 = info.find_memory_bar(2).copied();
        let mut device = PciDevice::open_location(&info.location)
            .map_err(|e| DriverError::Pci(format!("failed to re-open PCI device: {e}")))?;
        device
            .enable_device()
            .map_err(|e| DriverError::Pci(format!("enable_device failed: {e}")))?;
        let mmio = device
            .map_bar(bar0.index, bar0.addr, bar0.size as usize)
            .map_err(|e| DriverError::Mmio(format!("map_bar failed: {e}")))?;

        let pci_id = mmio.read32(0);
        debug!(
            "redox-drm: mapped AMD MMIO BAR0 addr={:#x} size={:#x} idreg={:#x}",
            bar0.addr, bar0.size, pci_id
        );

        let (fb_phys, fb_size) = match &bar2 {
            Some(bar) => {
                debug!(
                    "redox-drm: AMD VRAM BAR2 addr={:#x} size={:#x}",
                    bar.addr, bar.size
                );
                (bar.addr, bar.size as usize)
            }
            None => {
                return Err(DriverError::Pci(format!(
                    "AMD device {} has no VRAM BAR2 — cannot initialize display without framebuffer aperture",
                    info.location
                )));
            }
        };

        display::set_pci_device_info(
            info.vendor_id,
            info.device_id,
            info.revision,
            info.irq.unwrap_or(0),
            bar0.addr,
            bar0.size,
            bar2.as_ref().map(|b| b.addr).unwrap_or(0),
            bar2.as_ref().map(|b| b.size).unwrap_or(0),
        );

        let irq_handle = Some(InterruptHandle::setup(&info, &mut device).map_err(|e| {
            DriverError::Io(format!(
                "failed to setup interrupt for {}: {e}",
                info.location
            ))
        })?);

        let display = DisplayCore::with_framebuffer(mmio.as_ptr(), mmio.size(), fb_phys, fb_size)?;
        let (connectors, encoders) = detect_display_topology(&display)?;

        RingManager::bind_mmio(&mmio);

        let mut gtt = GttManager::new();
        gtt.initialize()?;
        gtt.program_vm_context(&mmio)?;

        let mut ring = RingManager::new();
        ring.initialize()?;

        let fw_count = firmware.len();
        let dmcub_available = firmware.contains_key("amdgpu/dmcub_dcn31.bin")
            || firmware.contains_key("amdgpu/dcn_3_1_dmcub");
        if !dmcub_available {
            warn!("redox-drm: DMCUB firmware not found in cache — display core may fail to initialize");
        }

        info!(
            "redox-drm: AMD driver ready for {} with {} connector(s), {} firmware blob(s) loaded",
            info.location,
            connectors.len(),
            fw_count
        );

        Ok(Self {
            info,
            mmio,
            irq_handle,
            display,
            gem: Mutex::new(GemManager::new()),
            connectors: Mutex::new(connectors),
            crtcs: Mutex::new(vec![Crtc::new(1)]),
            encoders: Mutex::new(encoders),
            gtt: Mutex::new(gtt),
            ring: Mutex::new(ring),
            vblank_count: AtomicU64::new(0),
            hotplug_pending: AtomicBool::new(false),
            firmware,
        })
    }

    pub fn process_irq(&self) -> Result<IrqEvent> {
        let ih_status = self.read_mmio_reg(AMD_IH_STATUS);
        let ih_cntl = self.read_mmio_reg(AMD_IH_CNTL);
        let ih_rptr = self.read_mmio_reg(AMD_IH_RB_RPTR);
        let ih_wptr = self.read_mmio_reg(AMD_IH_RB_WPTR);
        let ring_pending = ih_rptr != ih_wptr;

        if ih_status & AMD_IH_STATUS_RING_OVERFLOW_MASK != 0 {
            warn!(
                "redox-drm: AMD IH overflow status={:#010x} cntl={:#010x}",
                ih_status, ih_cntl
            );
        }

        if let Some(connector_id) = self.detect_hotplug_interrupt() {
            self.hotplug_pending.store(true, Ordering::SeqCst);
            self.refresh_connectors()?;
            self.hotplug_pending.store(false, Ordering::SeqCst);
            self.acknowledge_ih(ih_wptr);

            debug!(
                "redox-drm: hotplug interrupt on connector {} status={:#010x} cntl={:#010x} rptr={:#010x} wptr={:#010x}",
                connector_id, ih_status, ih_cntl, ih_rptr, ih_wptr
            );

            return Ok(IrqEvent::Hotplug { connector_id });
        }

        if ring_pending || (ih_status & AMD_IH_STATUS_INTERRUPT_PENDING_MASK != 0) {
            if let Some(crtc_id) = self.detect_vblank_interrupt() {
                let count = self.vblank_count.fetch_add(1, Ordering::SeqCst) + 1;
                self.acknowledge_ih(ih_wptr);

                debug!(
                    "redox-drm: vblank interrupt on CRTC {} count={} status={:#010x} cntl={:#010x} rptr={:#010x} wptr={:#010x}",
                    crtc_id, count, ih_status, ih_cntl, ih_rptr, ih_wptr
                );

                return Ok(IrqEvent::Vblank { crtc_id, count });
            }
        }

        self.acknowledge_ih(ih_wptr);
        Ok(IrqEvent::Unknown)
    }

    fn read_mmio_reg(&self, register_index: usize) -> u32 {
        self.mmio.read32(register_index.saturating_mul(4))
    }

    fn write_mmio_reg(&self, register_index: usize, value: u32) {
        self.mmio.write32(register_index.saturating_mul(4), value);
    }

    fn detect_vblank_interrupt(&self) -> Option<u32> {
        let active_crtc_ids = self
            .crtcs
            .lock()
            .map(|crtcs| {
                crtcs
                    .iter()
                    .filter(|crtc| crtc.mode.is_some())
                    .map(|crtc| crtc.id)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|_| vec![1]);

        for (index, register) in AMD_DCN_DISP_INTERRUPT_STATUS.iter().copied().enumerate() {
            let status = self.read_mmio_reg(register);
            if status & AMD_DISP_INTERRUPT_VBLANK_MASK == 0 {
                continue;
            }

            let crtc_id = index as u32 + 1;
            if active_crtc_ids.is_empty() || active_crtc_ids.contains(&crtc_id) {
                return Some(crtc_id);
            }
        }

        None
    }

    fn detect_hotplug_interrupt(&self) -> Option<u32> {
        for (index, register) in AMD_DCN_HPD_INT_STATUS.iter().copied().enumerate() {
            let status = self.read_mmio_reg(register);
            if status & (AMD_HPD_INT_STATUS_MASK | AMD_HPD_RX_INT_STATUS_MASK) != 0 {
                self.acknowledge_hotplug(index, status);
                return Some(index as u32 + 1);
            }
        }

        for (index, register) in AMD_DCN_DISP_INTERRUPT_STATUS.iter().copied().enumerate() {
            let status = self.read_mmio_reg(register);
            if status & AMD_DISP_INTERRUPT_HPD_MASK != 0 {
                let hpd_status = self.read_mmio_reg(AMD_DCN_HPD_INT_STATUS[index]);
                self.acknowledge_hotplug(index, hpd_status);
                return Some(index as u32 + 1);
            }
        }

        None
    }

    fn acknowledge_hotplug(&self, hpd_index: usize, hpd_status: u32) {
        let control_register = AMD_DCN_HPD_CONTROL[hpd_index];
        let control = self.read_mmio_reg(control_register);
        let ack = control
            | if hpd_status & AMD_HPD_INT_STATUS_MASK != 0 {
                AMD_HPD_INT_ACK_MASK
            } else {
                0
            }
            | if hpd_status & AMD_HPD_RX_INT_STATUS_MASK != 0 {
                AMD_HPD_RX_INT_ACK_MASK
            } else {
                0
            };
        self.write_mmio_reg(control_register, ack);
    }

    fn acknowledge_ih(&self, ih_wptr: u32) {
        self.write_mmio_reg(AMD_IH_RB_RPTR, ih_wptr);

        let ih_cntl = self.read_mmio_reg(AMD_IH_CNTL);
        self.write_mmio_reg(AMD_IH_CNTL, ih_cntl);

        let ih_rb_cntl = self.read_mmio_reg(AMD_IH_RB_CNTL);
        self.write_mmio_reg(AMD_IH_RB_CNTL, ih_rb_cntl);
    }

    fn refresh_connectors(&self) -> Result<()> {
        let (connectors, encoders) = detect_display_topology(&self.display)?;

        {
            let mut connector_state = self
                .connectors
                .lock()
                .map_err(|_| DriverError::Initialization("connector state poisoned".to_string()))?;
            *connector_state = connectors;
        }

        {
            let mut encoder_state = self
                .encoders
                .lock()
                .map_err(|_| DriverError::Initialization("encoder state poisoned".to_string()))?;
            *encoder_state = encoders;
        }

        Ok(())
    }

    fn ensure_gem_gpu_mapping(&self, fb_handle: GemHandle) -> Result<u64> {
        {
            let gem = self
                .gem
                .lock()
                .map_err(|_| DriverError::Buffer("GEM manager poisoned".to_string()))?;
            if let Some(addr) = gem.object(fb_handle)?.gpu_addr {
                return Ok(addr);
            }
        }

        let (phys_addr, fb_size) = {
            let gem = self
                .gem
                .lock()
                .map_err(|_| DriverError::Buffer("GEM manager poisoned".to_string()))?;
            let obj = gem.object(fb_handle)?;
            (obj.phys_addr as u64, obj.size)
        };

        let gpu_addr = {
            let mut gtt = self
                .gtt
                .lock()
                .map_err(|_| DriverError::Initialization("GTT manager poisoned".to_string()))?;
            let addr = gtt.alloc_gpu_range(fb_size)?;
            if let Err(e) = gtt.map_range(addr, phys_addr, fb_size, 0) {
                if gtt.unmap_range(addr, fb_size).is_ok() {
                    gtt.release_range(addr, fb_size);
                }
                return Err(e);
            }
            if let Err(e) = gtt.flush_tlb(&self.mmio) {
                if gtt.unmap_range(addr, fb_size).is_ok() {
                    if gtt.flush_tlb(&self.mmio).is_ok() {
                        gtt.release_range(addr, fb_size);
                    }
                }
                return Err(e);
            }
            addr
        };

        if let Err(e) = self
            .gem
            .lock()
            .map_err(|_| DriverError::Buffer("GEM manager poisoned".to_string()))?
            .set_gpu_addr(fb_handle, gpu_addr)
        {
            let mut gtt = self
                .gtt
                .lock()
                .map_err(|_| DriverError::Initialization("GTT manager poisoned".to_string()))?;
            if gtt.flush_tlb(&self.mmio).is_ok() && gtt.unmap_range(gpu_addr, fb_size).is_ok() {
                gtt.release_range(gpu_addr, fb_size);
            } else {
                let _ = gtt.unmap_range(gpu_addr, fb_size);
            }
            return Err(e);
        }

        Ok(gpu_addr)
    }
}

impl GpuDriver for AmdDriver {
    fn driver_name(&self) -> &str {
        "amdgpu-redox"
    }

    fn driver_desc(&self) -> &str {
        "AMD GPU DRM/KMS backend for Redox"
    }

    fn driver_date(&self) -> &str {
        "2026-04-11"
    }

    fn detect_connectors(&self) -> Vec<ConnectorInfo> {
        match self.connectors.lock() {
            Ok(connectors) => connectors
                .iter()
                .map(|connector| connector.info.clone())
                .collect(),
            Err(poisoned) => {
                warn!("redox-drm: connector state poisoned; using inner state");
                poisoned
                    .into_inner()
                    .iter()
                    .map(|connector| connector.info.clone())
                    .collect()
            }
        }
    }

    fn get_modes(&self, connector_id: u32) -> Vec<ModeInfo> {
        self.detect_connectors()
            .into_iter()
            .find(|connector| connector.id == connector_id)
            .map(|connector| connector.modes)
            .unwrap_or_default()
    }

    fn set_crtc(
        &self,
        crtc_id: u32,
        fb_handle: u32,
        connectors: &[u32],
        mode: &ModeInfo,
    ) -> Result<()> {
        let fb_addr = self.ensure_gem_gpu_mapping(fb_handle)?;

        self.display
            .set_crtc(crtc_id, fb_addr, mode.hdisplay as u32, mode.vdisplay as u32)?;

        let mut crtcs = self
            .crtcs
            .lock()
            .map_err(|_| DriverError::Initialization("CRTC state poisoned".to_string()))?;
        let crtc = crtcs
            .iter_mut()
            .find(|candidate| candidate.id == crtc_id)
            .ok_or_else(|| DriverError::NotFound(format!("unknown CRTC {crtc_id}")))?;
        crtc.program(fb_handle, connectors, mode)
    }

    fn page_flip(&self, crtc_id: u32, fb_handle: u32, _flags: u32) -> Result<u64> {
        {
            let crtcs = self
                .crtcs
                .lock()
                .map_err(|_| DriverError::Initialization("CRTC state poisoned".to_string()))?;
            if !crtcs.iter().any(|crtc| crtc.id == crtc_id) {
                return Err(DriverError::NotFound(format!("unknown CRTC {crtc_id}")));
            }
        }

        let fb_addr = self.ensure_gem_gpu_mapping(fb_handle)?;

        self.display.flip_surface(crtc_id, fb_addr)?;

        let mut ring = self
            .ring
            .lock()
            .map_err(|_| DriverError::Initialization("ring manager poisoned".to_string()))?;
        ring.page_flip()
    }

    fn get_vblank(&self, crtc_id: u32) -> Result<u64> {
        let crtcs = self
            .crtcs
            .lock()
            .map_err(|_| DriverError::Initialization("CRTC state poisoned".to_string()))?;
        if !crtcs.iter().any(|crtc| crtc.id == crtc_id) {
            return Err(DriverError::NotFound(format!("unknown CRTC {crtc_id}")));
        }

        Ok(self.vblank_count.load(Ordering::SeqCst))
    }

    fn gem_create(&self, size: u64) -> Result<GemHandle> {
        let mut gem = self
            .gem
            .lock()
            .map_err(|_| DriverError::Buffer("GEM manager poisoned".to_string()))?;
        gem.create(size)
    }

    fn gem_close(&self, handle: GemHandle) -> Result<()> {
        let gpu_info = {
            let gem = self
                .gem
                .lock()
                .map_err(|_| DriverError::Buffer("GEM manager poisoned".to_string()))?;
            let obj = gem.object(handle)?;
            (obj.gpu_addr, obj.size)
        };

        if let (Some(gpu_addr), fb_size) = gpu_info {
            let mut gtt = self
                .gtt
                .lock()
                .map_err(|_| DriverError::Initialization("GTT manager poisoned".to_string()))?;
            gtt.flush_tlb(&self.mmio)?;
            gtt.unmap_range(gpu_addr, fb_size)?;
            gtt.release_range(gpu_addr, fb_size);
        }

        self.gem
            .lock()
            .map_err(|_| DriverError::Buffer("GEM manager poisoned".to_string()))?
            .close(handle)
    }

    fn gem_mmap(&self, handle: GemHandle) -> Result<usize> {
        let gem = self
            .gem
            .lock()
            .map_err(|_| DriverError::Buffer("GEM manager poisoned".to_string()))?;
        gem.mmap(handle)
    }

    fn gem_size(&self, handle: GemHandle) -> Result<u64> {
        let gem = self
            .gem
            .lock()
            .map_err(|_| DriverError::Buffer("GEM manager poisoned".to_string()))?;
        Ok(gem.object(handle)?.size)
    }

    fn gem_export_dmafd(&self, handle: GemHandle) -> Result<i32> {
        let mut gem = self
            .gem
            .lock()
            .map_err(|_| DriverError::Buffer("GEM manager poisoned".to_string()))?;
        gem.export_dmafd(handle)
    }

    fn gem_import_dmafd(&self, fd: i32) -> Result<GemHandle> {
        let gem = self
            .gem
            .lock()
            .map_err(|_| DriverError::Buffer("GEM manager poisoned".to_string()))?;
        gem.import_dmafd(fd)
    }

    fn get_edid(&self, connector_id: u32) -> Vec<u8> {
        match self.connectors.lock() {
            Ok(connectors) => connectors
                .iter()
                .find(|connector| connector.info.id == connector_id)
                .map(|connector| connector.edid.clone())
                .unwrap_or_default(),
            Err(poisoned) => poisoned
                .into_inner()
                .iter()
                .find(|connector| connector.info.id == connector_id)
                .map(|connector| connector.edid.clone())
                .unwrap_or_default(),
        }
    }

    fn handle_irq(&self) -> Result<Option<(u32, u64)>> {
        match self.process_irq()? {
            IrqEvent::Vblank { crtc_id, count } => {
                debug!(
                    "redox-drm: handled AMD vblank IRQ for {} CRTC {} count={} irq={:?}",
                    self.info.location,
                    crtc_id,
                    count,
                    self.irq_handle.as_ref().map(|h| h.irq())
                );
                Ok(Some((crtc_id, count)))
            }
            IrqEvent::Hotplug { connector_id } => {
                info!(
                    "redox-drm: handled AMD hotplug IRQ for {} connector {} irq={:?}",
                    self.info.location,
                    connector_id,
                    self.irq_handle.as_ref().map(|h| h.irq())
                );
                Ok(None)
            }
            IrqEvent::Unknown => {
                debug!(
                    "redox-drm: handled AMD IRQ for {} with no decoded source irq={:?}",
                    self.info.location,
                    self.irq_handle.as_ref().map(|h| h.irq())
                );
                Ok(None)
            }
        }
    }
}

fn detect_display_topology(display: &DisplayCore) -> Result<(Vec<Connector>, Vec<Encoder>)> {
    let detected = display.detect_connectors()?;
    let mut connectors = Vec::new();
    let mut encoders = Vec::new();

    for (idx, connector) in detected.into_iter().enumerate() {
        let encoder_id = connector.encoder_id;
        encoders.push(Encoder::new(encoder_id, 1));
        let edid = display.read_edid(idx as u32);
        connectors.push(Connector {
            info: connector,
            edid: if edid.is_empty() {
                synthetic_edid()
            } else {
                edid
            },
        });
    }

    Ok((connectors, encoders))
}

fn find_memory_bar0(info: &PciDeviceInfo) -> Result<PciBarInfo> {
    info.find_memory_bar(0)
        .copied()
        .ok_or_else(|| DriverError::Pci(format!("device {} has no MMIO BAR0", info.location)))
}
