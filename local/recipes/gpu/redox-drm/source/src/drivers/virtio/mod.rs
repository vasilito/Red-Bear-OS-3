use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use log::{info, warn};
use redox_driver_sys::memory::MmioRegion;
use redox_driver_sys::pci::{PciBarInfo, PciDevice, PciDeviceInfo};

use crate::driver::{DriverError, DriverEvent, GpuDriver, Result};
use crate::drivers::interrupt::InterruptHandle;
use crate::gem::{GemHandle, GemManager};
use crate::kms::connector::{synthetic_edid, Connector};
use crate::kms::crtc::Crtc;
use crate::kms::{ConnectorInfo, ConnectorStatus, ConnectorType, ModeInfo};

pub struct VirtioDriver {
    info: PciDeviceInfo,
    _mmio: MmioRegion,
    irq_handle: Mutex<Option<InterruptHandle>>,
    width: u32,
    height: u32,
    gem: Mutex<GemManager>,
    connectors: Mutex<Vec<Connector>>,
    crtcs: Mutex<Vec<Crtc>>,
    vblank_count: AtomicU64,
}

fn find_fb_bar(info: &PciDeviceInfo) -> Result<PciBarInfo> {
    info.bars.iter()
        .find(|bar| bar.addr != 0 && bar.size > 0)
        .cloned()
        .ok_or_else(|| DriverError::Pci("VirtIO GPU has no valid framebuffer BAR".into()))
}

fn map_bar(device: &mut PciDevice, bar: &PciBarInfo, name: &str) -> Result<MmioRegion> {
    device
        .map_bar(bar.index, bar.addr, bar.size as usize)
        .map_err(|e| DriverError::Mmio(format!("failed to map {name}: {e}")))
}

impl VirtioDriver {
    pub fn new(info: PciDeviceInfo, _firmware: HashMap<String, Vec<u8>>) -> Result<Self> {
        if info.vendor_id != 0x1AF4 {
            return Err(DriverError::Pci(format!(
                "device {} is not a VirtIO GPU (vendor {:#06x})",
                info.location, info.vendor_id
            )));
        }

        let fb_bar = find_fb_bar(&info)?;
        let mut device = PciDevice::open_location(&info.location)
            .map_err(|e| DriverError::Pci(format!("open PCI: {e}")))?;
        let _mmio = map_bar(&mut device, &fb_bar, "VirtIO FB BAR")?;
        drop(device);

        info!(
            "redox-drm: VirtIO GPU at {}: {} MiB BAR at {:#x}",
            info.location,
            fb_bar.size / 1024 / 1024,
            fb_bar.addr,
        );

        Ok(Self {
            info,
            _mmio,
            irq_handle: Mutex::new(None),
            width: 1280,
            height: 720,
            gem: Mutex::new(GemManager::new()),
            connectors: Mutex::new(Vec::new()),
            crtcs: Mutex::new(Vec::new()),
            vblank_count: AtomicU64::new(0),
        })
    }

    fn refresh_connectors(&self) -> Result<Vec<ConnectorInfo>> {
        let mode = ModeInfo {
            name: String::from("1280x720"),
            clock: 0,
            hdisplay: self.width as u16,
            hsync_start: (self.width + 16) as u16,
            hsync_end: (self.width + 48) as u16,
            htotal: (self.width + 160) as u16,
            vdisplay: self.height as u16,
            vsync_start: (self.height + 3) as u16,
            vsync_end: (self.height + 6) as u16,
            vtotal: (self.height + 30) as u16,
            hskew: 0,
            vscan: 0,
            vrefresh: 60,
            type_: 0,
            flags: 0,
        };
        let info = ConnectorInfo {
            id: 1,
            connector_type: ConnectorType::Unknown,
            connector_type_id: 1,
            connection: ConnectorStatus::Connected,
            mm_width: 0,
            mm_height: 0,
            modes: vec![mode],
            encoder_id: 0,
        };
        let mut connectors = self.connectors.lock()
            .map_err(|_| DriverError::Initialization("connector lock poisoned".into()))?;
        connectors.clear();
        let result = info.clone();
        connectors.push(Connector {
            edid: synthetic_edid(),
            info,
        });
        let mut crtcs = self.crtcs.lock()
            .map_err(|_| DriverError::Initialization("crtc lock poisoned".into()))?;
        crtcs.clear();
        crtcs.push(Crtc::new(1));
        Ok(vec![result])
    }

    fn cached_connectors(&self) -> Vec<ConnectorInfo> {
        self.connectors.lock()
            .ok()
            .map(|c| c.iter().map(|c| c.info.clone()).collect())
            .unwrap_or_default()
    }
}

impl GpuDriver for VirtioDriver {
    fn driver_name(&self) -> &str { "virtio-gpu-redox" }
    fn driver_desc(&self) -> &str { "VirtIO GPU DRM/KMS backend for QEMU" }
    fn driver_date(&self) -> &str { "2026-04-27" }

    fn detect_connectors(&self) -> Vec<ConnectorInfo> {
        match self.refresh_connectors() {
            Ok(connectors) => connectors,
            Err(error) => {
                warn!("redox-drm: VirtIO connector refresh failed: {}", error);
                self.cached_connectors()
            }
        }
    }

    fn get_modes(&self, connector_id: u32) -> Vec<ModeInfo> {
        self.detect_connectors()
            .into_iter()
            .find(|c| c.id == connector_id)
            .map(|c| c.modes)
            .unwrap_or_default()
    }

    fn set_crtc(&self, crtc_id: u32, fb_handle: u32, connectors: &[u32], mode: &ModeInfo) -> Result<()> {
        let mut crtcs = self.crtcs.lock()
            .map_err(|_| DriverError::Initialization("crtc lock poisoned".into()))?;
        let crtc = crtcs.iter_mut()
            .find(|c| c.id == crtc_id)
            .ok_or_else(|| DriverError::NotFound(format!("unknown CRTC {crtc_id}")))?;
        crtc.program(fb_handle, connectors, mode)
    }

    fn page_flip(&self, crtc_id: u32, _fb_handle: u32, _flags: u32) -> Result<u64> {
        let crtcs = self.crtcs.lock()
            .map_err(|_| DriverError::Initialization("crtc lock poisoned".into()))?;
        if !crtcs.iter().any(|c| c.id == crtc_id) {
            return Err(DriverError::NotFound(format!("unknown CRTC {crtc_id}")));
        }
        self.vblank_count.fetch_add(1, Ordering::SeqCst);
        Ok(self.vblank_count.load(Ordering::SeqCst))
    }

    fn get_vblank(&self, crtc_id: u32) -> Result<u64> {
        let crtcs = self.crtcs.lock()
            .map_err(|_| DriverError::Initialization("crtc lock poisoned".into()))?;
        if !crtcs.iter().any(|c| c.id == crtc_id) {
            return Err(DriverError::NotFound(format!("unknown CRTC {crtc_id}")));
        }
        Ok(self.vblank_count.load(Ordering::SeqCst))
    }

    fn gem_create(&self, size: u64) -> Result<GemHandle> {
        self.gem.lock()
            .map_err(|_| DriverError::Buffer("VirtIO GEM poisoned".into()))?
            .create(size)
    }

    fn gem_close(&self, handle: GemHandle) -> Result<()> {
        self.gem.lock()
            .map_err(|_| DriverError::Buffer("VirtIO GEM poisoned".into()))?
            .close(handle)
    }

    fn gem_mmap(&self, handle: GemHandle) -> Result<usize> {
        self.gem.lock()
            .map_err(|_| DriverError::Buffer("VirtIO GEM poisoned".into()))?
            .mmap(handle)
    }

    fn gem_size(&self, handle: GemHandle) -> Result<u64> {
        self.gem.lock()
            .map_err(|_| DriverError::Buffer("VirtIO GEM poisoned".into()))?
            .object(handle)
            .map(|o| o.size)
    }

    fn get_edid(&self, connector_id: u32) -> Vec<u8> {
        match self.connectors.lock() {
            Ok(connectors) => connectors.iter()
                .find(|c| c.info.id == connector_id)
                .map(|c| c.edid.clone())
                .unwrap_or_else(synthetic_edid),
            Err(_) => synthetic_edid(),
        }
    }

    fn handle_irq(&self) -> Result<Option<DriverEvent>> {
        self.vblank_count.fetch_add(1, Ordering::SeqCst);
        Ok(None)
    }
}
