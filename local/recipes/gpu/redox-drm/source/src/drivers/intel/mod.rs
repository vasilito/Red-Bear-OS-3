pub mod display;
pub mod gtt;
pub mod ring;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
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
use crate::kms::{ConnectorInfo, ConnectorType, ModeInfo};

use self::display::{DisplayPipe, IntelDisplay};
use self::gtt::IntelGtt;
use self::ring::{IntelRing, RingType};

const FORCEWAKE: usize = 0xA18C;
const PP_STATUS: usize = 0xC7200;
const PIPECONF_BASE: usize = 0x70008;
const PIPE_STRIDE: usize = 0x1000;
const DDI_BUF_CTL_BASE: usize = 0x64000;
const DDI_PORT_STRIDE: usize = 0x100;
const GFX_FLSH_CNTL_REG: usize = 0x101008;

const RENDER_RING_BASE: usize = 0x02000;
const RING_TAIL_OFFSET: usize = 0x30;
const RING_HEAD_OFFSET: usize = 0x34;

pub struct IntelDriver {
    info: PciDeviceInfo,
    mmio: MmioRegion,
    irq_handle: Mutex<Option<InterruptHandle>>,
    display: IntelDisplay,
    gem: Mutex<GemManager>,
    connectors: Mutex<Vec<Connector>>,
    crtcs: Mutex<Vec<Crtc>>,
    encoders: Mutex<Vec<Encoder>>,
    gtt: Mutex<IntelGtt>,
    ring: Mutex<IntelRing>,
    vblank_count: AtomicU64,
}

impl IntelDriver {
    pub fn new(info: PciDeviceInfo, firmware: HashMap<String, Vec<u8>>) -> Result<Self> {
        if !info.is_intel_gpu() {
            return Err(DriverError::Pci(format!(
                "device {} is not an Intel display-class GPU",
                info.location
            )));
        }

        let gtt_bar = find_memory_bar(&info, 0, "GGTT BAR0")?;
        let mmio_bar = find_memory_bar(&info, 2, "MMIO BAR2")?;
        validate_intel_bars(&info, &gtt_bar, &mmio_bar)?;

        let mut device = PciDevice::open_location(&info.location)
            .map_err(|e| DriverError::Pci(format!("failed to re-open PCI device: {e}")))?;
        device
            .enable_device()
            .map_err(|e| DriverError::Pci(format!("enable_device failed: {e}")))?;

        let mmio = map_bar(&mut device, &mmio_bar, "Intel MMIO BAR2")?;
        let display_mmio = map_bar(&mut device, &mmio_bar, "Intel display MMIO")?;
        let ring_mmio = map_bar(&mut device, &mmio_bar, "Intel ring MMIO")?;
        let gtt_control_mmio = map_bar(&mut device, &mmio_bar, "Intel GGTT control MMIO")?;
        let gtt_mmio = map_bar(&mut device, &gtt_bar, "Intel GGTT BAR0")?;

        enable_forcewake(&mmio)?;

        let display = IntelDisplay::new(display_mmio)?;
        let mut gtt = IntelGtt::init(gtt_mmio, gtt_control_mmio)?;
        let mut ring = IntelRing::create(ring_mmio, RingType::Render)?;
        ring.bind_gtt(&mut gtt)?;

        let (connectors, encoders) = detect_display_topology(&display)?;
        let crtcs = build_crtcs(&display)?;

        let irq_handle = match InterruptHandle::setup(&info, &mut device) {
            Ok(handle) => Some(handle),
            Err(e) => {
                warn!(
                    "redox-drm: Intel device {} interrupt setup failed: {e}",
                    info.location
                );
                None
            }
        };

        if !firmware.is_empty() {
            warn!(
                "redox-drm: Intel driver ignores {} firmware blob(s); i915-class GPUs usually boot without scheme:firmware blobs",
                firmware.len()
            );
        }

        info!(
            "redox-drm: Intel driver ready for {} with {} connector(s)",
            info.location,
            connectors.len()
        );

        Ok(Self {
            info,
            mmio,
            irq_handle: Mutex::new(irq_handle),
            display,
            gem: Mutex::new(GemManager::new()),
            connectors: Mutex::new(connectors),
            crtcs: Mutex::new(crtcs),
            encoders: Mutex::new(encoders),
            gtt: Mutex::new(gtt),
            ring: Mutex::new(ring),
            vblank_count: AtomicU64::new(0),
        })
    }

    fn refresh_connectors(&self) -> Result<Vec<ConnectorInfo>> {
        let (connectors, encoders) = detect_display_topology(&self.display)?;
        let infos = connectors
            .iter()
            .map(|connector| connector.info.clone())
            .collect();

        {
            let mut connector_state = self.connectors.lock().map_err(|_| {
                DriverError::Initialization("Intel connector state poisoned".into())
            })?;
            *connector_state = connectors;
        }

        {
            let mut encoder_state = self
                .encoders
                .lock()
                .map_err(|_| DriverError::Initialization("Intel encoder state poisoned".into()))?;
            *encoder_state = encoders;
        }

        Ok(infos)
    }

    fn cached_connectors(&self) -> Vec<ConnectorInfo> {
        match self.connectors.lock() {
            Ok(connectors) => connectors
                .iter()
                .map(|connector| connector.info.clone())
                .collect(),
            Err(poisoned) => {
                warn!("redox-drm: Intel connector state poisoned; using inner state");
                poisoned
                    .into_inner()
                    .iter()
                    .map(|connector| connector.info.clone())
                    .collect()
            }
        }
    }

    fn connector_port(&self, connector_id: u32) -> Result<u8> {
        let connectors = self
            .connectors
            .lock()
            .map_err(|_| DriverError::Initialization("Intel connector state poisoned".into()))?;
        let connector = connectors
            .iter()
            .find(|connector| connector.info.id == connector_id)
            .ok_or_else(|| DriverError::NotFound(format!("unknown connector {connector_id}")))?;

        Ok(connector.info.connector_type_id.saturating_sub(1) as u8)
    }

    fn process_irq(&self) -> Result<Option<(u32, u64)>> {
        let previous = self.cached_connectors();
        let current = self.refresh_connectors()?;

        if connector_status_changed(&previous, &current) {
            info!(
                "redox-drm: Intel hotplug event detected on {}",
                self.info.location
            );
        }

        let ring_busy = self
            .ring
            .lock()
            .map_err(|_| DriverError::Initialization("Intel ring state poisoned".into()))?
            .has_activity()?;

        if let Some(crtc_id) = self.active_crtc_id()? {
            let count = self.vblank_count.fetch_add(1, Ordering::SeqCst) + 1;
            debug!(
                "redox-drm: Intel IRQ decoded as display event crtc={} ring_busy={}",
                crtc_id, ring_busy
            );
            return Ok(Some((crtc_id, count)));
        }

        if ring_busy {
            debug!("redox-drm: Intel IRQ signaled command stream activity without active CRTC");
        }

        Ok(None)
    }

    fn active_crtc_id(&self) -> Result<Option<u32>> {
        let crtcs = self
            .crtcs
            .lock()
            .map_err(|_| DriverError::Initialization("Intel CRTC state poisoned".into()))?;

        if let Some(active) = crtcs.iter().find(|crtc| crtc.mode.is_some()) {
            return Ok(Some(active.id));
        }

        Ok(self
            .display
            .pipes()?
            .into_iter()
            .find(|pipe| pipe.enabled)
            .map(|pipe| u32::from(pipe.index) + 1))
    }

    fn ensure_gem_gpu_mapping(&self, handle: GemHandle) -> Result<u64> {
        {
            let gem = self
                .gem
                .lock()
                .map_err(|_| DriverError::Buffer("Intel GEM manager poisoned".into()))?;
            if let Some(gpu_addr) = gem.gpu_addr(handle)? {
                return Ok(gpu_addr);
            }
        }

        let (phys_addr, size) = {
            let gem = self
                .gem
                .lock()
                .map_err(|_| DriverError::Buffer("Intel GEM manager poisoned".into()))?;
            let object = gem.object(handle)?;
            (object.phys_addr as u64, object.size)
        };

        let gpu_addr = {
            let mut gtt = self
                .gtt
                .lock()
                .map_err(|_| DriverError::Initialization("Intel GGTT state poisoned".into()))?;
            let gpu_addr = gtt.alloc_range(size)?;
            if let Err(error) = gtt.map_range(gpu_addr, phys_addr, size, 1 << 1) {
                let _ = gtt.release_range(gpu_addr, size);
                return Err(error);
            }
            gpu_addr
        };

        if let Err(error) = self
            .gem
            .lock()
            .map_err(|_| DriverError::Buffer("Intel GEM manager poisoned".into()))?
            .set_gpu_addr(handle, gpu_addr)
        {
            let mut gtt = self
                .gtt
                .lock()
                .map_err(|_| DriverError::Initialization("Intel GGTT state poisoned".into()))?;
            let _ = gtt.unmap_range(gpu_addr, size);
            let _ = gtt.release_range(gpu_addr, size);
            return Err(error);
        }

        Ok(gpu_addr)
    }

    fn read_mmio(&self, offset: usize) -> Result<u32> {
        let end = offset
            .checked_add(core::mem::size_of::<u32>())
            .ok_or_else(|| {
                DriverError::Mmio(format!("Intel MMIO offset overflow at {offset:#x}"))
            })?;
        if end > self.mmio.size() {
            return Err(DriverError::Mmio(format!(
                "Intel MMIO read outside BAR2 aperture: end={end:#x} size={:#x}",
                self.mmio.size()
            )));
        }
        Ok(self.mmio.read32(offset))
    }
}

impl GpuDriver for IntelDriver {
    fn driver_name(&self) -> &str {
        "i915-redox"
    }

    fn driver_desc(&self) -> &str {
        "Intel i915-class DRM/KMS backend for Redox"
    }

    fn driver_date(&self) -> &str {
        "2026-04-12"
    }

    fn detect_connectors(&self) -> Vec<ConnectorInfo> {
        match self.refresh_connectors() {
            Ok(connectors) => connectors,
            Err(error) => {
                warn!("redox-drm: Intel connector refresh failed: {}", error);
                self.cached_connectors()
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
        if connectors.is_empty() {
            return Err(DriverError::InvalidArgument(
                "set_crtc requires at least one connector",
            ));
        }

        let fb_addr = self.ensure_gem_gpu_mapping(fb_handle)?;
        let mut pipe = self.display.pipe_for_crtc(crtc_id)?;
        pipe.port = Some(self.connector_port(connectors[0])?);

        self.display.set_mode(&pipe, mode)?;
        self.display.page_flip(&pipe, fb_addr)?;

        let mut crtcs = self
            .crtcs
            .lock()
            .map_err(|_| DriverError::Initialization("Intel CRTC state poisoned".into()))?;
        let crtc = crtcs
            .iter_mut()
            .find(|crtc| crtc.id == crtc_id)
            .ok_or_else(|| DriverError::NotFound(format!("unknown CRTC {crtc_id}")))?;
        crtc.program(fb_handle, connectors, mode)
    }

    fn page_flip(&self, crtc_id: u32, fb_handle: u32, _flags: u32) -> Result<u64> {
        let fb_addr = self.ensure_gem_gpu_mapping(fb_handle)?;
        let pipe = self.display.pipe_for_crtc(crtc_id)?;
        self.display.page_flip(&pipe, fb_addr)?;

        let mut ring = self
            .ring
            .lock()
            .map_err(|_| DriverError::Initialization("Intel ring state poisoned".into()))?;
        ring.flush()?;
        Ok(ring.last_seqno())
    }

    fn get_vblank(&self, crtc_id: u32) -> Result<u64> {
        let crtcs = self
            .crtcs
            .lock()
            .map_err(|_| DriverError::Initialization("Intel CRTC state poisoned".into()))?;
        if !crtcs.iter().any(|crtc| crtc.id == crtc_id) {
            return Err(DriverError::NotFound(format!("unknown CRTC {crtc_id}")));
        }
        Ok(self.vblank_count.load(Ordering::SeqCst))
    }

    fn gem_create(&self, size: u64) -> Result<GemHandle> {
        let handle = {
            let mut gem = self
                .gem
                .lock()
                .map_err(|_| DriverError::Buffer("Intel GEM manager poisoned".into()))?;
            gem.create(size)?
        };

        if let Err(error) = self.ensure_gem_gpu_mapping(handle) {
            let _ = self
                .gem
                .lock()
                .map_err(|_| DriverError::Buffer("Intel GEM manager poisoned".into()))?
                .close(handle);
            return Err(error);
        }

        Ok(handle)
    }

    fn gem_close(&self, handle: GemHandle) -> Result<()> {
        let (gpu_addr, size) = {
            let gem = self
                .gem
                .lock()
                .map_err(|_| DriverError::Buffer("Intel GEM manager poisoned".into()))?;
            let object = gem.object(handle)?;
            (object.gpu_addr, object.size)
        };

        if let Some(gpu_addr) = gpu_addr {
            let mut gtt = self
                .gtt
                .lock()
                .map_err(|_| DriverError::Initialization("Intel GGTT state poisoned".into()))?;
            gtt.unmap_range(gpu_addr, size)?;
            gtt.release_range(gpu_addr, size)?;
        }

        self.gem
            .lock()
            .map_err(|_| DriverError::Buffer("Intel GEM manager poisoned".into()))?
            .close(handle)
    }

    fn gem_mmap(&self, handle: GemHandle) -> Result<usize> {
        let gem = self
            .gem
            .lock()
            .map_err(|_| DriverError::Buffer("Intel GEM manager poisoned".into()))?;
        gem.mmap(handle)
    }

    fn gem_size(&self, handle: GemHandle) -> Result<u64> {
        let gem = self
            .gem
            .lock()
            .map_err(|_| DriverError::Buffer("Intel GEM manager poisoned".into()))?;
        Ok(gem.object(handle)?.size)
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
        let irq_event = {
            let mut irq_handle = self
                .irq_handle
                .lock()
                .map_err(|_| DriverError::Initialization("Intel IRQ state poisoned".into()))?;
            match irq_handle.as_mut() {
                Some(handle) => handle
                    .try_wait()
                    .map_err(|e| DriverError::Io(format!("Intel IRQ poll failed: {e}")))?,
                None => return Ok(None),
            }
        };

        if !irq_event {
            return Ok(None);
        }

        self.process_irq()
    }
}

fn detect_display_topology(display: &IntelDisplay) -> Result<(Vec<Connector>, Vec<Encoder>)> {
    let detected = display.detect_connectors()?;
    let mut connectors = Vec::with_capacity(detected.len());
    let mut encoders = Vec::with_capacity(detected.len());

    for connector in detected {
        let port = connector.connector_type_id.saturating_sub(1) as u8;
        let edid = match connector.connector_type {
            ConnectorType::DisplayPort | ConnectorType::EDP => display.read_edid(port),
            _ => display.read_edid(port),
        };

        encoders.push(Encoder::new(
            connector.encoder_id,
            pipe_id_for_port(display, port),
        ));
        connectors.push(Connector {
            edid: if edid.is_empty() {
                synthetic_edid()
            } else {
                edid
            },
            info: ConnectorInfo {
                modes: display.modes_for_connector(&connector),
                ..connector
            },
        });
    }

    Ok((connectors, encoders))
}

fn build_crtcs(display: &IntelDisplay) -> Result<Vec<Crtc>> {
    let mut crtcs: Vec<Crtc> = display
        .pipes()?
        .into_iter()
        .map(|pipe| Crtc::new(u32::from(pipe.index) + 1))
        .collect();

    if crtcs.is_empty() {
        crtcs.push(Crtc::new(1));
    }

    Ok(crtcs)
}

fn pipe_id_for_port(display: &IntelDisplay, port: u8) -> u32 {
    display
        .pipes()
        .ok()
        .and_then(|pipes| {
            pipes
                .into_iter()
                .find(|pipe| pipe.port == Some(port))
                .map(|pipe| u32::from(pipe.index) + 1)
        })
        .unwrap_or(1)
}

fn connector_status_changed(previous: &[ConnectorInfo], current: &[ConnectorInfo]) -> bool {
    if previous.len() != current.len() {
        return true;
    }

    previous.iter().zip(current.iter()).any(|(old, new)| {
        old.id != new.id
            || old.connection != new.connection
            || old.connector_type != new.connector_type
    })
}

fn enable_forcewake(mmio: &MmioRegion) -> Result<()> {
    let end = FORCEWAKE
        .checked_add(core::mem::size_of::<u32>())
        .ok_or_else(|| DriverError::Mmio("Intel FORCEWAKE offset overflow".into()))?;
    if end > mmio.size() {
        return Err(DriverError::Mmio(format!(
            "Intel FORCEWAKE register outside MMIO aperture: end={end:#x} size={:#x}",
            mmio.size()
        )));
    }

    mmio.write32(FORCEWAKE, 1);
    let _ = mmio.read32(FORCEWAKE);
    Ok(())
}

fn validate_intel_bars(
    info: &PciDeviceInfo,
    gtt_bar: &PciBarInfo,
    mmio_bar: &PciBarInfo,
) -> Result<()> {
    if !gtt_bar.is_memory() {
        return Err(DriverError::Pci(format!(
            "device {} GGTT BAR{} is not a memory BAR",
            info.location, gtt_bar.index
        )));
    }
    if !mmio_bar.is_memory() {
        return Err(DriverError::Pci(format!(
            "device {} MMIO BAR{} is not a memory BAR",
            info.location, mmio_bar.index
        )));
    }

    if gtt_bar.size < core::mem::size_of::<u64>() as u64 {
        return Err(DriverError::Pci(format!(
            "device {} GGTT BAR{} is too small ({:#x})",
            info.location, gtt_bar.index, gtt_bar.size
        )));
    }
    if gtt_bar.size % core::mem::size_of::<u64>() as u64 != 0 {
        return Err(DriverError::Pci(format!(
            "device {} GGTT BAR{} size {:#x} is not 8-byte aligned",
            info.location, gtt_bar.index, gtt_bar.size
        )));
    }

    let required_mmio_end = [
        FORCEWAKE + core::mem::size_of::<u32>(),
        PP_STATUS + core::mem::size_of::<u32>(),
        GFX_FLSH_CNTL_REG + core::mem::size_of::<u32>(),
        RENDER_RING_BASE + RING_TAIL_OFFSET + core::mem::size_of::<u32>(),
        RENDER_RING_BASE + RING_HEAD_OFFSET + core::mem::size_of::<u32>(),
    ]
    .into_iter()
    .max()
    .unwrap_or(0);

    if mmio_bar.size < required_mmio_end as u64 {
        return Err(DriverError::Pci(format!(
            "device {} MMIO BAR{} is too small ({:#x}) for required register window ending at {:#x}",
            info.location, mmio_bar.index, mmio_bar.size, required_mmio_end
        )));
    }

    Ok(())
}

fn find_memory_bar(info: &PciDeviceInfo, index: usize, name: &str) -> Result<PciBarInfo> {
    info.find_memory_bar(index)
        .copied()
        .ok_or_else(|| DriverError::Pci(format!("device {} has no {}", info.location, name)))
}

fn map_bar(device: &mut PciDevice, bar: &PciBarInfo, name: &str) -> Result<MmioRegion> {
    device
        .map_bar(bar.index, bar.addr, bar.size as usize)
        .map_err(|e| DriverError::Mmio(format!("failed to map {name}: {e}")))
}

#[allow(dead_code)]
fn ddi_buf_ctl(port: u8) -> usize {
    DDI_BUF_CTL_BASE + usize::from(port) * DDI_PORT_STRIDE
}

#[allow(dead_code)]
fn pipeconf(pipe: &DisplayPipe) -> usize {
    PIPECONF_BASE + usize::from(pipe.index) * PIPE_STRIDE
}
