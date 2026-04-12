use std::sync::Mutex;

use log::{debug, info};
use redox_driver_sys::memory::MmioRegion;

use crate::driver::{DriverError, Result};
use crate::kms::connector::synthetic_edid;
use crate::kms::{ConnectorInfo, ConnectorStatus, ConnectorType, ModeInfo};

const PIPE_COUNT: usize = 3;
const PORT_COUNT: usize = 5;

const PP_STATUS: usize = 0xC7200;
const PIPECONF_BASE: usize = 0x70008;
const DSPCNTR_BASE: usize = 0x70180;
const DSPSURF_BASE: usize = 0x7019C;
const DDI_BUF_CTL_BASE: usize = 0x64000;

const HTOTAL_BASE: usize = 0x60000;
const HBLANK_BASE: usize = 0x60004;
const HSYNC_BASE: usize = 0x60008;
const VTOTAL_BASE: usize = 0x6000C;
const VBLANK_BASE: usize = 0x60010;
const VSYNC_BASE: usize = 0x60014;
const PIPE_SRC_BASE: usize = 0x6001C;
const PLANE_SIZE_BASE: usize = 0x70190;

const PIPE_STRIDE: usize = 0x1000;
const PORT_STRIDE: usize = 0x100;

const PIPECONF_ENABLE: u32 = 1 << 31;
const DSPCNTR_ENABLE: u32 = 1 << 31;
const DDI_BUF_CTL_ENABLE: u32 = 1 << 31;

#[derive(Clone, Copy, Debug)]
pub struct DisplayPipe {
    pub index: u8,
    pub enabled: bool,
    pub port: Option<u8>,
}

pub struct IntelDisplay {
    mmio: MmioRegion,
    pipes: Mutex<Vec<DisplayPipe>>,
}

impl IntelDisplay {
    pub fn new(mmio: MmioRegion) -> Result<Self> {
        let pipes = Self::detect_pipes(&mmio)?;
        info!(
            "redox-drm: Intel display initialized with {} pipe(s)",
            pipes.len()
        );
        Ok(Self {
            mmio,
            pipes: Mutex::new(pipes),
        })
    }

    pub fn pipes(&self) -> Result<Vec<DisplayPipe>> {
        self.refresh_pipes()
    }

    pub fn pipe_for_crtc(&self, crtc_id: u32) -> Result<DisplayPipe> {
        let index = crtc_id
            .checked_sub(1)
            .ok_or(DriverError::InvalidArgument("invalid Intel CRTC id"))?
            as usize;
        self.refresh_pipes()?
            .get(index)
            .copied()
            .ok_or_else(|| DriverError::NotFound(format!("unknown Intel pipe for CRTC {crtc_id}")))
    }

    pub fn detect_pipes(mmio: &MmioRegion) -> Result<Vec<DisplayPipe>> {
        let mut pipes = Vec::with_capacity(PIPE_COUNT);
        let pp_status = read32(mmio, PP_STATUS).unwrap_or(0);
        let connected_ports = connected_ports(mmio);

        for index in 0..PIPE_COUNT {
            let conf = read32(mmio, pipe_offset(PIPECONF_BASE, index))?;
            let enabled = conf & PIPECONF_ENABLE != 0;
            let mut port = connected_ports.get(index).copied();

            if port.is_none() && index == 0 && pp_status != 0 {
                port = Some(0);
            }
            if port.is_none() && enabled {
                port = Some(index as u8);
            }

            pipes.push(DisplayPipe {
                index: index as u8,
                enabled,
                port,
            });
        }

        if pipes.iter().all(|pipe| pipe.port.is_none()) {
            if let Some(pipe) = pipes.first_mut() {
                pipe.port = Some(0);
            }
        }

        Ok(pipes)
    }

    pub fn detect_connectors(&self) -> Result<Vec<ConnectorInfo>> {
        let pp_status = self.read32(PP_STATUS).unwrap_or(0);
        let pipes = self.refresh_pipes()?;
        let mut connectors = Vec::with_capacity(PORT_COUNT);

        for port in 0..PORT_COUNT as u8 {
            let status = self.read32(ddi_offset(port)).unwrap_or(0);
            let connected = status & DDI_BUF_CTL_ENABLE != 0
                || pipes
                    .iter()
                    .any(|pipe| pipe.port == Some(port) && pipe.enabled)
                || (port == 0 && pp_status != 0);
            let connector_type = connector_type_for_port(port, pp_status);
            let modes = self.modes_for_port(port, connector_type);

            connectors.push(ConnectorInfo {
                id: port as u32 + 1,
                connector_type,
                connector_type_id: port as u32 + 1,
                connection: if connected {
                    ConnectorStatus::Connected
                } else {
                    ConnectorStatus::Disconnected
                },
                mm_width: 600,
                mm_height: 340,
                encoder_id: port as u32 + 1,
                modes,
            });
        }

        Ok(connectors)
    }

    pub fn modes_for_connector(&self, connector: &ConnectorInfo) -> Vec<ModeInfo> {
        let port = connector
            .connector_type_id
            .saturating_sub(1)
            .min((PORT_COUNT - 1) as u32) as u8;
        self.modes_for_port(port, connector.connector_type)
    }

    pub fn read_edid(&self, port: u8) -> Vec<u8> {
        debug!("redox-drm: Intel HDMI/DVI EDID fallback on port {}", port);
        synthetic_edid()
    }

    pub fn read_dpcd(&self, port: u8) -> Vec<u8> {
        let status = self.read32(ddi_offset(port)).unwrap_or(0);
        if status & DDI_BUF_CTL_ENABLE == 0 {
            return Vec::new();
        }

        debug!("redox-drm: Intel AUX/DPCD skeleton read on port {}", port);
        vec![0x12, 0x0A, 0x84, 0x01]
    }

    pub fn set_mode(&self, pipe: &DisplayPipe, mode: &ModeInfo) -> Result<()> {
        let index = usize::from(pipe.index);
        self.write32(
            pipe_offset(HTOTAL_BASE, index),
            pack_pair(mode.htotal, mode.hdisplay),
        )?;
        self.write32(
            pipe_offset(HBLANK_BASE, index),
            pack_pair(mode.htotal, mode.hdisplay),
        )?;
        self.write32(
            pipe_offset(HSYNC_BASE, index),
            pack_pair(mode.hsync_end, mode.hsync_start),
        )?;
        self.write32(
            pipe_offset(VTOTAL_BASE, index),
            pack_pair(mode.vtotal, mode.vdisplay),
        )?;
        self.write32(
            pipe_offset(VBLANK_BASE, index),
            pack_pair(mode.vtotal, mode.vdisplay),
        )?;
        self.write32(
            pipe_offset(VSYNC_BASE, index),
            pack_pair(mode.vsync_end, mode.vsync_start),
        )?;
        self.write32(
            pipe_offset(PIPE_SRC_BASE, index),
            pack_pair(mode.vdisplay, mode.hdisplay),
        )?;
        self.write32(
            pipe_offset(PLANE_SIZE_BASE, index),
            pack_pair(mode.vdisplay, mode.hdisplay),
        )?;

        let mut dspcntr = self.read32(pipe_offset(DSPCNTR_BASE, index))?;
        dspcntr |= DSPCNTR_ENABLE;
        self.write32(pipe_offset(DSPCNTR_BASE, index), dspcntr)?;

        let mut pipeconf = self.read32(pipe_offset(PIPECONF_BASE, index))?;
        pipeconf |= PIPECONF_ENABLE;
        self.write32(pipe_offset(PIPECONF_BASE, index), pipeconf)?;

        if let Some(port) = pipe.port {
            let mut ddi = self.read32(ddi_offset(port))?;
            ddi |= DDI_BUF_CTL_ENABLE;
            self.write32(ddi_offset(port), ddi)?;
        }

        self.update_pipe(pipe.index, true, pipe.port)?;

        Ok(())
    }

    pub fn page_flip(&self, pipe: &DisplayPipe, fb_addr: u64) -> Result<()> {
        if fb_addr > u64::from(u32::MAX) {
            return Err(DriverError::Buffer(format!(
                "Intel DSPSURF supports 32-bit GGTT offsets in this skeleton, got {fb_addr:#x}"
            )));
        }
        let index = usize::from(pipe.index);
        self.write32(pipe_offset(DSPSURF_BASE, index), fb_addr as u32)
    }

    fn refresh_pipes(&self) -> Result<Vec<DisplayPipe>> {
        let detected = Self::detect_pipes(&self.mmio)?;
        let mut cached = self
            .pipes
            .lock()
            .map_err(|_| DriverError::Initialization("Intel display pipe state poisoned".into()))?;

        let previous = cached.clone();
        let mut refreshed = Vec::with_capacity(detected.len());

        for mut pipe in detected {
            if let Some(existing) = previous
                .iter()
                .find(|existing| existing.index == pipe.index)
            {
                if pipe.port.is_none() {
                    pipe.port = existing.port;
                }
                pipe.enabled |= existing.enabled;
            }
            refreshed.push(pipe);
        }

        *cached = refreshed.clone();
        Ok(refreshed)
    }

    fn update_pipe(&self, index: u8, enabled: bool, port: Option<u8>) -> Result<()> {
        let mut cached = self
            .pipes
            .lock()
            .map_err(|_| DriverError::Initialization("Intel display pipe state poisoned".into()))?;

        if let Some(pipe) = cached.iter_mut().find(|pipe| pipe.index == index) {
            pipe.enabled = enabled;
            pipe.port = port.or(pipe.port);
            return Ok(());
        }

        cached.push(DisplayPipe {
            index,
            enabled,
            port,
        });
        Ok(())
    }

    fn modes_for_port(&self, port: u8, connector_type: ConnectorType) -> Vec<ModeInfo> {
        let mut modes = match connector_type {
            ConnectorType::DisplayPort | ConnectorType::EDP => {
                modes_from_dpcd(&self.read_dpcd(port))
            }
            _ => ModeInfo::from_edid(&self.read_edid(port)),
        };

        if modes.is_empty() {
            modes = ModeInfo::from_edid(&synthetic_edid());
        }
        if modes.is_empty() {
            modes.push(ModeInfo::default_1080p());
        }
        modes
    }

    fn read32(&self, offset: usize) -> Result<u32> {
        read32(&self.mmio, offset)
    }

    fn write32(&self, offset: usize, value: u32) -> Result<()> {
        write32(&self.mmio, offset, value)
    }
}

fn connected_ports(mmio: &MmioRegion) -> Vec<u8> {
    let mut ports = Vec::new();
    for port in 0..PORT_COUNT as u8 {
        if read32(mmio, ddi_offset(port)).unwrap_or(0) & DDI_BUF_CTL_ENABLE != 0 {
            ports.push(port);
        }
    }
    ports
}

fn read32(mmio: &MmioRegion, offset: usize) -> Result<u32> {
    ensure_access(
        mmio.size(),
        offset,
        core::mem::size_of::<u32>(),
        "Intel display read",
    )?;
    Ok(mmio.read32(offset))
}

fn write32(mmio: &MmioRegion, offset: usize, value: u32) -> Result<()> {
    ensure_access(
        mmio.size(),
        offset,
        core::mem::size_of::<u32>(),
        "Intel display write",
    )?;
    mmio.write32(offset, value);
    Ok(())
}

fn ensure_access(mmio_size: usize, offset: usize, width: usize, op: &str) -> Result<()> {
    let end = offset
        .checked_add(width)
        .ok_or_else(|| DriverError::Mmio(format!("{op} offset overflow at {offset:#x}")))?;
    if end > mmio_size {
        return Err(DriverError::Mmio(format!(
            "{op} outside MMIO aperture: end={end:#x} size={mmio_size:#x}"
        )));
    }
    Ok(())
}

fn pipe_offset(base: usize, index: usize) -> usize {
    base + index * PIPE_STRIDE
}

fn ddi_offset(port: u8) -> usize {
    DDI_BUF_CTL_BASE + usize::from(port) * PORT_STRIDE
}

fn pack_pair(upper: u16, lower: u16) -> u32 {
    ((u32::from(upper).saturating_sub(1)) << 16) | u32::from(lower).saturating_sub(1)
}

fn connector_type_for_port(port: u8, pp_status: u32) -> ConnectorType {
    match port {
        0 if pp_status != 0 => ConnectorType::EDP,
        0 | 1 => ConnectorType::HDMIA,
        2 | 3 => ConnectorType::DisplayPort,
        _ => ConnectorType::VGA,
    }
}

fn modes_from_dpcd(dpcd: &[u8]) -> Vec<ModeInfo> {
    if dpcd.is_empty() {
        return Vec::new();
    }

    vec![ModeInfo::default_1080p(), mode_1440p()]
}

fn mode_1440p() -> ModeInfo {
    ModeInfo {
        clock: 241_500,
        hdisplay: 2560,
        hsync_start: 2608,
        hsync_end: 2640,
        htotal: 2720,
        hskew: 0,
        vdisplay: 1440,
        vsync_start: 1443,
        vsync_end: 1448,
        vtotal: 1481,
        vscan: 0,
        vrefresh: 60,
        flags: 0,
        type_: 0,
        name: "2560x1440@60".to_string(),
    }
}
