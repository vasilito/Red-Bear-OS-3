use std::collections::{BTreeMap, HashSet};
use std::mem::size_of;
use std::sync::Arc;

use log::{debug, warn};
use redox_scheme::SchemeBlockMut;
use syscall04::data::Stat;
use syscall04::error::{Error, Result, EBADF, EBUSY, EINVAL, ENOENT, EOPNOTSUPP};
use syscall04::flag::{EventFlags, MapFlags, MunmapFlags, MODE_FILE};

use crate::driver::GpuDriver;
use crate::gem::GemHandle;
use crate::kms::ModeInfo;

#[derive(Clone, Debug)]
struct FbInfo {
    gem_handle: GemHandle,
    width: u32,
    height: u32,
    pitch: u32,
    bpp: u32,
}

// ---- DRM ioctl request codes ----
const DRM_IOCTL_BASE: usize = 0x00A0;
const DRM_IOCTL_MODE_GETRESOURCES: usize = DRM_IOCTL_BASE;
const DRM_IOCTL_MODE_GETCONNECTOR: usize = DRM_IOCTL_BASE + 7;
const DRM_IOCTL_MODE_GETMODES: usize = DRM_IOCTL_BASE + 8;
const DRM_IOCTL_MODE_SETCRTC: usize = DRM_IOCTL_BASE + 2;
const DRM_IOCTL_MODE_GETCRTC: usize = DRM_IOCTL_BASE + 3;
const DRM_IOCTL_MODE_GETENCODER: usize = DRM_IOCTL_BASE + 6;
const DRM_IOCTL_MODE_PAGE_FLIP: usize = DRM_IOCTL_BASE + 16;
const DRM_IOCTL_MODE_CREATE_DUMB: usize = DRM_IOCTL_BASE + 18;
const DRM_IOCTL_MODE_MAP_DUMB: usize = DRM_IOCTL_BASE + 19;
const DRM_IOCTL_MODE_DESTROY_DUMB: usize = DRM_IOCTL_BASE + 20;
const DRM_IOCTL_MODE_ADDFB: usize = DRM_IOCTL_BASE + 21;
const DRM_IOCTL_MODE_RMFB: usize = DRM_IOCTL_BASE + 22;
const DRM_IOCTL_GET_CAP: usize = DRM_IOCTL_BASE + 23;
const DRM_IOCTL_SET_CLIENT_CAP: usize = DRM_IOCTL_BASE + 24;
const DRM_IOCTL_VERSION: usize = DRM_IOCTL_BASE + 25;
const DRM_IOCTL_GEM_CREATE: usize = DRM_IOCTL_BASE + 26;
const DRM_IOCTL_GEM_CLOSE: usize = DRM_IOCTL_BASE + 27;
const DRM_IOCTL_GEM_MMAP: usize = DRM_IOCTL_BASE + 28;
const DRM_IOCTL_PRIME_HANDLE_TO_FD: usize = DRM_IOCTL_BASE + 29;
const DRM_IOCTL_PRIME_FD_TO_HANDLE: usize = DRM_IOCTL_BASE + 30;

// ---- Wire types for DRM ioctls ----
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmResourcesWire {
    connector_count: u32,
    crtc_count: u32,
    encoder_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmConnectorWire {
    connector_id: u32,
    connection: u32,
    connector_type: u32,
    mm_width: u32,
    mm_height: u32,
    encoder_id: u32,
    mode_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmModeWire {
    clock: u32,
    hdisplay: u16,
    hsync_start: u16,
    hsync_end: u16,
    htotal: u16,
    hskew: u16,
    vdisplay: u16,
    vsync_start: u16,
    vsync_end: u16,
    vtotal: u16,
    vscan: u16,
    vrefresh: u32,
    flags: u32,
    type_: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmSetCrtcWire {
    crtc_id: u32,
    fb_handle: u32,
    connector_count: u32,
    connectors: [u32; 8],
    mode: DrmModeWire,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmPageFlipWire {
    crtc_id: u32,
    fb_handle: u32,
    flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmCreateDumbWire {
    width: u32,
    height: u32,
    bpp: u32,
    flags: u32,
    pitch: u32,
    size: u64,
    handle: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmMapDumbWire {
    handle: u32,
    offset: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmDestroyDumbWire {
    handle: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmGetEncoderWire {
    encoder_id: u32,
    encoder_type: u32,
    crtc_id: u32,
    possible_crtcs: u32,
    possible_clones: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmAddFbWire {
    width: u32,
    height: u32,
    pitch: u32,
    bpp: u32,
    depth: u32,
    handle: u32,
    fb_id: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmRmFbWire {
    fb_id: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmGetCrtcWire {
    crtc_id: u32,
    fb_id: u32,
    x: u32,
    y: u32,
    mode_valid: u32,
    mode: DrmModeWire,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmVersionWire {
    major: i32,
    minor: i32,
    patch: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmGetCapWire {
    capability: u64,
    value: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmSetClientCapWire {
    capability: u64,
    value: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmGemCreateWire {
    size: u64,
    handle: u32,
    _pad: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmGemCloseWire {
    handle: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmGemMmapWire {
    handle: u32,
    _pad: u32,
    offset: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmPrimeHandleToFdWire {
    handle: u32,
    flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmPrimeFdToHandleWire {
    fd: i32,
    _pad: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmPrimeHandleToFdResponseWire {
    fd: i32,
    _pad: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmPrimeFdToHandleResponseWire {
    handle: u32,
    _pad: u32,
}

// ---- Internal handle types ----

#[derive(Clone, Debug)]
enum NodeKind {
    Card,
    Connector(u32),
    DmaBuf {
        gem_handle: GemHandle,
        export_token: u32,
    },
}

struct Handle {
    node: NodeKind,
    response: Vec<u8>,
    mapped_gem: Option<GemHandle>,
    mapped_gem_refs: usize,
    owned_fbs: Vec<u32>,
    owned_gems: Vec<GemHandle>,
    closing: bool,
}

pub struct DrmScheme {
    driver: Arc<dyn GpuDriver>,
    next_id: usize,
    next_fb_id: u32,
    next_export_token: u32,
    handles: BTreeMap<usize, Handle>,
    active_crtc_fb: BTreeMap<u32, u32>,
    active_crtc_mode: BTreeMap<u32, ModeInfo>,
    pending_flip_fb: BTreeMap<u32, (u64, u32)>,
    fb_registry: BTreeMap<u32, FbInfo>,
    active_gem_maps: BTreeMap<GemHandle, usize>,
    gem_export_refs: BTreeMap<GemHandle, usize>,
    prime_exports: BTreeMap<u32, GemHandle>,
}

impl DrmScheme {
    pub fn new(driver: Arc<dyn GpuDriver>) -> Self {
        Self {
            driver,
            next_id: 0,
            next_fb_id: 1,
            next_export_token: 1,
            handles: BTreeMap::new(),
            active_crtc_fb: BTreeMap::new(),
            active_crtc_mode: BTreeMap::new(),
            pending_flip_fb: BTreeMap::new(),
            fb_registry: BTreeMap::new(),
            active_gem_maps: BTreeMap::new(),
            gem_export_refs: BTreeMap::new(),
            prime_exports: BTreeMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn on_close(&mut self, id: usize) {
        let mapped = self
            .handles
            .get(&id)
            .map(|handle| handle.mapped_gem_refs != 0)
            .unwrap_or(false);
        if mapped {
            if let Some(handle) = self.handles.get_mut(&id) {
                handle.closing = true;
            }
            return;
        }

        if let Some(handle) = self.handles.remove(&id) {
            self.finalize_handle_close(handle);
        }
    }

    fn is_fb_active(&self, fb_id: u32) -> bool {
        self.active_crtc_fb.values().any(|&id| id == fb_id)
            || self.pending_flip_fb.values().any(|&(_, id)| id == fb_id)
    }

    fn handle_has_gem_ref(handle: &Handle, gem_handle: GemHandle) -> bool {
        handle.owned_gems.contains(&gem_handle)
    }

    fn gem_is_still_referenced(&self, gem_handle: GemHandle) -> bool {
        self.handles
            .values()
            .any(|handle| Self::handle_has_gem_ref(handle, gem_handle))
    }

    fn gem_has_other_refs(&self, current_id: usize, gem_handle: GemHandle) -> bool {
        self.handles.iter().any(|(&other_id, handle)| {
            other_id != current_id && Self::handle_has_gem_ref(handle, gem_handle)
        })
    }

    fn gem_is_mapped(&self, gem_handle: GemHandle) -> bool {
        self.active_gem_maps.get(&gem_handle).copied().unwrap_or(0) != 0
    }

    fn gem_export_refcount(&self, gem_handle: GemHandle) -> usize {
        self.gem_export_refs.get(&gem_handle).copied().unwrap_or(0)
    }

    fn bump_export_ref(&mut self, gem_handle: GemHandle) {
        let entry = self.gem_export_refs.entry(gem_handle).or_insert(0);
        *entry = entry.saturating_add(1);
    }

    fn drop_export_ref(&mut self, gem_handle: GemHandle) {
        let remove_entry = match self.gem_export_refs.get_mut(&gem_handle) {
            Some(count) if *count > 1 => {
                *count -= 1;
                false
            }
            Some(_) => true,
            None => false,
        };
        if remove_entry {
            self.gem_export_refs.remove(&gem_handle);
            self.prime_exports.retain(|_, &mut h| h != gem_handle);
        }
    }

    fn gem_can_close(&self, gem_handle: GemHandle) -> bool {
        let backs_fb = self
            .fb_registry
            .values()
            .any(|info| info.gem_handle == gem_handle);
        !backs_fb
            && !self.gem_is_still_referenced(gem_handle)
            && !self.gem_is_mapped(gem_handle)
            && self.gem_export_refcount(gem_handle) == 0
    }

    fn maybe_close_gem(&mut self, gem_handle: GemHandle, context: &str) -> bool {
        if !self.gem_can_close(gem_handle) {
            return false;
        }

        match self.driver.gem_close(gem_handle) {
            Ok(()) => {
                self.prime_exports.retain(|_, &mut h| h != gem_handle);
                true
            }
            Err(e) => {
                warn!(
                    "redox-drm: {} gem_close({}) failed: {}",
                    context, gem_handle, e
                );
                false
            }
        }
    }

    fn allocate_handle(&mut self, node: NodeKind) -> usize {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.handles.insert(
            id,
            Handle {
                node,
                response: Vec::new(),
                mapped_gem: None,
                mapped_gem_refs: 0,
                owned_fbs: Vec::new(),
                owned_gems: Vec::new(),
                closing: false,
            },
        );
        id
    }

    fn finalize_handle_close(&mut self, handle: Handle) {
        if let NodeKind::DmaBuf { gem_handle, .. } = handle.node {
            self.drop_export_ref(gem_handle);
            let _ = self.maybe_close_gem(gem_handle, "close dmabuf");
            return;
        }

        let mut auto_closed_gems = HashSet::new();
        for fb_id in &handle.owned_fbs {
            if self.is_fb_active(*fb_id) {
                continue;
            }
            if let Some(fb_info) = self.fb_registry.remove(fb_id) {
                if self.maybe_close_gem(fb_info.gem_handle, "close") {
                    auto_closed_gems.insert(fb_info.gem_handle);
                }
            }
        }
        for gem_handle in handle.owned_gems {
            if auto_closed_gems.contains(&gem_handle) {
                continue;
            }
            let backs_fb = self
                .fb_registry
                .values()
                .any(|info| info.gem_handle == gem_handle);
            if !backs_fb && self.maybe_close_gem(gem_handle, "close gem") {
                auto_closed_gems.insert(gem_handle);
            }
        }
    }

    fn pin_mapped_gem(&mut self, id: usize, gem_handle: GemHandle) -> Result<()> {
        let handle = self.handles.get_mut(&id).ok_or_else(|| Error::new(EBADF))?;
        handle.mapped_gem = Some(gem_handle);
        handle.mapped_gem_refs = handle.mapped_gem_refs.saturating_add(1);
        let entry = self.active_gem_maps.entry(gem_handle).or_insert(0);
        *entry = entry.saturating_add(1);
        Ok(())
    }

    fn unpin_mapped_gem(&mut self, id: usize) -> Result<()> {
        let handle = self.handles.get_mut(&id).ok_or_else(|| Error::new(EBADF))?;
        let gem_handle = match handle.mapped_gem {
            Some(gem_handle) if handle.mapped_gem_refs != 0 => gem_handle,
            _ => return Ok(()),
        };
        handle.mapped_gem_refs -= 1;
        if handle.mapped_gem_refs == 0 {
            handle.mapped_gem = None;
        }

        let remove_entry = match self.active_gem_maps.get_mut(&gem_handle) {
            Some(count) if *count > 1 => {
                *count -= 1;
                false
            }
            Some(_) => true,
            None => false,
        };
        if remove_entry {
            self.active_gem_maps.remove(&gem_handle);
        }
        Ok(())
    }

    pub fn retire_vblank(&mut self, crtc_id: u32, vblank_count: u64) {
        if let Some((expected, fb_id)) = self.pending_flip_fb.get(&crtc_id).copied() {
            if expected <= vblank_count {
                self.pending_flip_fb.remove(&crtc_id);
                self.try_reap_fb(fb_id);
            }
        }
    }

    fn try_reap_fb(&mut self, fb_id: u32) {
        let gem_handle = match self.fb_registry.get(&fb_id) {
            Some(info) => info.gem_handle,
            None => return,
        };
        let still_owned = self.handles.values().any(|h| h.owned_fbs.contains(&fb_id));
        if still_owned {
            return;
        }
        self.fb_registry.remove(&fb_id);
        let _ = self.maybe_close_gem(gem_handle, "try_reap_fb");
    }

    // ---- Encode helpers ----

    fn encode_resources(&self) -> Vec<u8> {
        let connectors = self.driver.detect_connectors();
        let payload = DrmResourcesWire {
            connector_count: connectors.len() as u32,
            crtc_count: 1,
            encoder_count: connectors.len() as u32,
        };
        bytes_of(&payload)
    }

    fn encode_connector(&self, connector_id: u32) -> Result<Vec<u8>> {
        let connector = self
            .driver
            .detect_connectors()
            .into_iter()
            .find(|c| c.id == connector_id)
            .ok_or_else(|| Error::new(ENOENT))?;

        let header = DrmConnectorWire {
            connector_id: connector.id,
            connection: match connector.connection {
                crate::kms::ConnectorStatus::Connected => 1,
                crate::kms::ConnectorStatus::Disconnected => 2,
                crate::kms::ConnectorStatus::Unknown => 0,
            },
            connector_type: connector_type_to_u32(connector.connector_type),
            mm_width: connector.mm_width,
            mm_height: connector.mm_height,
            encoder_id: connector.encoder_id,
            mode_count: connector.modes.len() as u32,
        };

        let mut out = bytes_of(&header);
        for mode in &connector.modes {
            out.extend_from_slice(&bytes_of(&mode_to_wire(mode)));
            out.extend_from_slice(mode.name.as_bytes());
            out.push(0);
        }
        Ok(out)
    }

    // ---- ioctl dispatch ----

    fn handle_ioctl(&mut self, id: usize, request: usize, payload: &[u8]) -> Result<usize> {
        let response = match request {
            DRM_IOCTL_MODE_GETRESOURCES => self.encode_resources(),

            DRM_IOCTL_MODE_GETCONNECTOR => {
                let connector_id = if payload.len() >= size_of::<u32>() {
                    read_u32(payload, 0)?
                } else {
                    match self.handles.get(&id).map(|h| &h.node) {
                        Some(NodeKind::Connector(cid)) => *cid,
                        _ => return Err(Error::new(EINVAL)),
                    }
                };
                self.encode_connector(connector_id)?
            }

            DRM_IOCTL_MODE_GETMODES => {
                let connector_id = read_u32(payload, 0)?;
                let modes = self.driver.get_modes(connector_id);
                encode_modes(&modes)
            }

            DRM_IOCTL_MODE_SETCRTC => {
                let req = decode_wire::<DrmSetCrtcWire>(payload)?;
                if req.fb_handle == 0 && req.connector_count == 0 {
                    let completed_flip = self.pending_flip_fb.remove(&req.crtc_id);
                    let prev_fb_id = self.active_crtc_fb.remove(&req.crtc_id);
                    self.active_crtc_mode.remove(&req.crtc_id);
                    if let Some((_, fb_id)) = completed_flip {
                        self.try_reap_fb(fb_id);
                    }
                    if let Some(fb_id) = prev_fb_id {
                        self.try_reap_fb(fb_id);
                    }
                    return Ok(1);
                }
                let count = req.connector_count as usize;
                if count > req.connectors.len() {
                    return Err(Error::new(EINVAL));
                }
                let conns = req.connectors[..count].to_vec();
                let fb_info = self.fb_registry.get(&req.fb_handle).ok_or_else(|| {
                    warn!("redox-drm: SETCRTC with unknown fb_id {}", req.fb_handle);
                    Error::new(ENOENT)
                })?;
                let mode = wire_to_mode(&req.mode);
                let fb_pitch = fb_info.pitch as u64;
                let required_fb_lines = mode.vdisplay as u64;
                let fb_height = fb_info.height as u64;
                let fb_width = fb_info.width as u64;
                let mode_width = mode.hdisplay as u64;
                if fb_pitch.checked_mul(required_fb_lines).is_none() {
                    warn!("redox-drm: SETCRTC FB pitch * mode_height overflows");
                    return Err(Error::new(EINVAL));
                }
                if fb_pitch == 0 || fb_height < required_fb_lines || fb_width < mode_width {
                    warn!(
                        "redox-drm: SETCRTC FB {}x{} pitch={} too small for mode {}x{}",
                        fb_info.width, fb_info.height, fb_info.pitch, mode.hdisplay, mode.vdisplay
                    );
                    return Err(Error::new(EINVAL));
                }
                let gem_handle = fb_info.gem_handle;
                self.driver
                    .set_crtc(req.crtc_id, gem_handle, &conns, &mode)
                    .map_err(driver_to_syscall)?;
                let completed_flip = self.pending_flip_fb.remove(&req.crtc_id);
                let prev_fb = self.active_crtc_fb.insert(req.crtc_id, req.fb_handle);
                self.active_crtc_mode.insert(req.crtc_id, mode);
                if let Some((_, fb_id)) = completed_flip {
                    self.try_reap_fb(fb_id);
                }
                if let Some(prev) = prev_fb {
                    if prev != req.fb_handle {
                        self.try_reap_fb(prev);
                    }
                }
                Vec::new()
            }

            DRM_IOCTL_MODE_PAGE_FLIP => {
                let req = decode_wire::<DrmPageFlipWire>(payload)?;
                if self.pending_flip_fb.contains_key(&req.crtc_id) {
                    warn!(
                        "redox-drm: PAGE_FLIP rejected — flip already pending on CRTC {}",
                        req.crtc_id
                    );
                    return Err(Error::new(EBUSY));
                }
                let fb_info = self.fb_registry.get(&req.fb_handle).ok_or_else(|| {
                    warn!("redox-drm: PAGE_FLIP with unknown fb_id {}", req.fb_handle);
                    Error::new(ENOENT)
                })?;
                if let Some(active_mode) = self.active_crtc_mode.get(&req.crtc_id) {
                    let fb_pitch = fb_info.pitch as u64;
                    let required_lines = active_mode.vdisplay as u64;
                    let required_width = active_mode.hdisplay as u64;
                    if fb_pitch == 0
                        || (fb_info.height as u64) < required_lines
                        || (fb_info.width as u64) < required_width
                    {
                        warn!(
                            "redox-drm: PAGE_FLIP FB {}x{} pitch={} too small for active mode {}x{}",
                            fb_info.width, fb_info.height, fb_info.pitch,
                            active_mode.hdisplay, active_mode.vdisplay
                        );
                        return Err(Error::new(EINVAL));
                    }
                }
                let gem_handle = fb_info.gem_handle;
                let seqno = self
                    .driver
                    .page_flip(req.crtc_id, gem_handle, req.flags)
                    .map_err(driver_to_syscall)?;
                let current_vblank = self.driver.get_vblank(req.crtc_id).unwrap_or(0);
                let prev = self.active_crtc_fb.insert(req.crtc_id, req.fb_handle);
                if let Some(old_fb) = prev {
                    if old_fb != req.fb_handle {
                        self.pending_flip_fb
                            .insert(req.crtc_id, (current_vblank.saturating_add(1), old_fb));
                    }
                }
                seqno.to_le_bytes().to_vec()
            }

            DRM_IOCTL_MODE_CREATE_DUMB => {
                let mut req = decode_wire::<DrmCreateDumbWire>(payload)?;
                let pitch = (req.width.saturating_mul(req.bpp).saturating_add(7)) / 8;
                req.pitch = pitch;
                req.size = (pitch as u64).saturating_mul(req.height as u64);
                req.handle = self
                    .driver
                    .gem_create(req.size)
                    .map_err(driver_to_syscall)?;
                if let Some(handle) = self.handles.get_mut(&id) {
                    handle.owned_gems.push(req.handle);
                }
                bytes_of(&req)
            }

            DRM_IOCTL_MODE_MAP_DUMB => {
                let mut req = decode_wire::<DrmMapDumbWire>(payload)?;
                let owned = self
                    .handles
                    .get(&id)
                    .map(|h| Self::handle_has_gem_ref(h, req.handle))
                    .unwrap_or(false);
                if !owned {
                    warn!(
                        "redox-drm: MAP_DUMB handle {} not owned by this fd",
                        req.handle
                    );
                    return Err(Error::new(EBADF));
                }
                if let Some(handle) = self.handles.get(&id) {
                    if handle.mapped_gem_refs != 0 && handle.mapped_gem != Some(req.handle) {
                        warn!(
                            "redox-drm: MAP_DUMB handle {} rejected — another GEM is still mapped",
                            req.handle
                        );
                        return Err(Error::new(EBUSY));
                    }
                }
                req.offset = self
                    .driver
                    .gem_mmap(req.handle)
                    .map_err(driver_to_syscall)? as u64;
                if let Some(handle) = self.handles.get_mut(&id) {
                    handle.mapped_gem = Some(req.handle);
                }
                bytes_of(&req)
            }

            DRM_IOCTL_MODE_DESTROY_DUMB => {
                let req = decode_wire::<DrmDestroyDumbWire>(payload)?;
                let owned = self
                    .handles
                    .get(&id)
                    .map(|h| h.owned_gems.contains(&req.handle))
                    .unwrap_or(false);
                if !owned {
                    warn!(
                        "redox-drm: DESTROY_DUMB handle {} not owned by this fd",
                        req.handle
                    );
                    return Err(Error::new(EBADF));
                }
                let backs_fb = self
                    .fb_registry
                    .values()
                    .any(|info| info.gem_handle == req.handle);
                if backs_fb {
                    warn!(
                        "redox-drm: DESTROY_DUMB handle {} rejected — backs an active framebuffer",
                        req.handle
                    );
                    return Err(Error::new(EBUSY));
                }
                if self.gem_is_mapped(req.handle) {
                    warn!(
                        "redox-drm: DESTROY_DUMB handle {} rejected — still mapped",
                        req.handle
                    );
                    return Err(Error::new(EBUSY));
                }
                let close_now = !self.gem_has_other_refs(id, req.handle)
                    && self.gem_export_refcount(req.handle) == 0;
                if close_now {
                    self.driver
                        .gem_close(req.handle)
                        .map_err(driver_to_syscall)?;
                    self.prime_exports.retain(|_, &mut h| h != req.handle);
                }
                if let Some(handle) = self.handles.get_mut(&id) {
                    handle.owned_gems.retain(|&h| h != req.handle);
                }
                Vec::new()
            }

            DRM_IOCTL_MODE_GETENCODER => {
                let _req = decode_wire::<DrmGetEncoderWire>(payload)?;
                let resp = DrmGetEncoderWire {
                    encoder_id: _req.encoder_id,
                    encoder_type: 0,
                    crtc_id: 1,
                    possible_crtcs: 1,
                    possible_clones: 0,
                };
                bytes_of(&resp)
            }

            DRM_IOCTL_MODE_GETCRTC => {
                let req = decode_wire::<DrmGetCrtcWire>(payload)?;
                let (fb_id, mode_valid, mode) = match (
                    self.active_crtc_fb.get(&req.crtc_id),
                    self.active_crtc_mode.get(&req.crtc_id),
                ) {
                    (Some(&fb), Some(m)) if self.fb_registry.contains_key(&fb) => {
                        (fb, 1u32, mode_to_wire(m))
                    }
                    _ => (0u32, 0u32, DrmModeWire::default()),
                };
                let resp = DrmGetCrtcWire {
                    crtc_id: req.crtc_id,
                    fb_id,
                    x: 0,
                    y: 0,
                    mode_valid,
                    mode,
                };
                bytes_of(&resp)
            }

            DRM_IOCTL_MODE_ADDFB => {
                let req = decode_wire::<DrmAddFbWire>(payload)?;
                if req.handle == 0 {
                    return Err(Error::new(EINVAL));
                }
                if req.width == 0 || req.height == 0 || req.bpp == 0 {
                    warn!(
                        "redox-drm: ADDFB zero dimension width={} height={} bpp={}",
                        req.width, req.height, req.bpp
                    );
                    return Err(Error::new(EINVAL));
                }
                let min_stride = (req.width.saturating_mul(req.bpp).saturating_add(7)) / 8;
                let pitch = if req.pitch != 0 {
                    req.pitch
                } else {
                    min_stride
                };
                if pitch == 0 || pitch < min_stride {
                    warn!(
                        "redox-drm: ADDFB pitch {} below minimum stride {} ({}x{})",
                        pitch, min_stride, req.width, req.bpp
                    );
                    return Err(Error::new(EINVAL));
                }
                let required_size = (pitch as u64).checked_mul(req.height as u64);
                if required_size.is_none() {
                    warn!(
                        "redox-drm: ADDFB pitch * height overflows pitch={} height={}",
                        pitch, req.height
                    );
                    return Err(Error::new(EINVAL));
                }
                let owned = self
                    .handles
                    .get(&id)
                    .map(|h| Self::handle_has_gem_ref(h, req.handle))
                    .unwrap_or(false);
                if !owned {
                    warn!(
                        "redox-drm: ADDFB handle {} not owned by this fd",
                        req.handle
                    );
                    return Err(Error::new(EBADF));
                }
                let actual_size = self.driver.gem_size(req.handle).map_err(|e| {
                    warn!("redox-drm: ADDFB handle {} not found: {}", req.handle, e);
                    Error::new(ENOENT)
                })?;
                if required_size.unwrap() > actual_size {
                    warn!(
                        "redox-drm: ADDFB requires {} bytes but GEM {} is {} bytes",
                        required_size.unwrap(),
                        req.handle,
                        actual_size
                    );
                    return Err(Error::new(EINVAL));
                }
                let fb_id = self.next_fb_id;
                self.next_fb_id = self.next_fb_id.saturating_add(1);
                self.fb_registry.insert(
                    fb_id,
                    FbInfo {
                        gem_handle: req.handle,
                        width: req.width,
                        height: req.height,
                        pitch,
                        bpp: req.bpp,
                    },
                );
                if let Some(handle) = self.handles.get_mut(&id) {
                    handle.owned_fbs.push(fb_id);
                }
                let mut resp = req;
                resp.fb_id = fb_id;
                bytes_of(&resp)
            }

            DRM_IOCTL_MODE_RMFB => {
                let req = decode_wire::<DrmRmFbWire>(payload)?;
                let owned = self
                    .handles
                    .get(&id)
                    .map(|h| h.owned_fbs.contains(&req.fb_id))
                    .unwrap_or(false);
                if !owned {
                    warn!("redox-drm: RMFB {} not owned by this fd", req.fb_id);
                    return Err(Error::new(EBADF));
                }
                let in_use = self.is_fb_active(req.fb_id);
                if in_use {
                    warn!(
                        "redox-drm: RMFB {} rejected — still active on a CRTC",
                        req.fb_id
                    );
                    return Err(Error::new(EBUSY));
                }
                if let Some(fb_info) = self.fb_registry.remove(&req.fb_id) {
                    let _ = self.maybe_close_gem(fb_info.gem_handle, "RMFB");
                }
                if let Some(handle) = self.handles.get_mut(&id) {
                    handle.owned_fbs.retain(|&fb| fb != req.fb_id);
                }
                Vec::new()
            }

            DRM_IOCTL_GET_CAP => {
                let mut req = decode_wire::<DrmGetCapWire>(payload)?;
                req.value = match req.capability {
                    0 => 1,
                    1 => 1,
                    _ => 0,
                };
                bytes_of(&req)
            }

            DRM_IOCTL_SET_CLIENT_CAP => Vec::new(),

            DRM_IOCTL_VERSION => {
                let resp = DrmVersionWire {
                    major: 1,
                    minor: 0,
                    patch: 0,
                };
                bytes_of(&resp)
            }

            DRM_IOCTL_GEM_CREATE => {
                let mut req = decode_wire::<DrmGemCreateWire>(payload)?;
                if req.size == 0 {
                    return Err(Error::new(EINVAL));
                }
                req.handle = self
                    .driver
                    .gem_create(req.size)
                    .map_err(driver_to_syscall)?;
                if let Some(handle) = self.handles.get_mut(&id) {
                    handle.owned_gems.push(req.handle);
                }
                bytes_of(&req)
            }

            DRM_IOCTL_GEM_CLOSE => {
                let req = decode_wire::<DrmGemCloseWire>(payload)?;
                let owned = self
                    .handles
                    .get(&id)
                    .map(|h| h.owned_gems.contains(&req.handle))
                    .unwrap_or(false);
                if !owned {
                    warn!(
                        "redox-drm: GEM_CLOSE handle {} not owned by this fd",
                        req.handle
                    );
                    return Err(Error::new(EBADF));
                }
                let backs_fb = self
                    .fb_registry
                    .values()
                    .any(|info| info.gem_handle == req.handle);
                if backs_fb {
                    warn!(
                        "redox-drm: GEM_CLOSE handle {} rejected — backs an active framebuffer",
                        req.handle
                    );
                    return Err(Error::new(EBUSY));
                }
                if self.gem_is_mapped(req.handle) {
                    warn!(
                        "redox-drm: GEM_CLOSE handle {} rejected — still mapped",
                        req.handle
                    );
                    return Err(Error::new(EBUSY));
                }
                let close_now = !self.gem_has_other_refs(id, req.handle)
                    && self.gem_export_refcount(req.handle) == 0;
                if close_now {
                    self.driver
                        .gem_close(req.handle)
                        .map_err(driver_to_syscall)?;
                    self.prime_exports.retain(|_, &mut h| h != req.handle);
                }
                if let Some(handle) = self.handles.get_mut(&id) {
                    handle.owned_gems.retain(|&h| h != req.handle);
                }
                Vec::new()
            }

            DRM_IOCTL_GEM_MMAP => {
                let mut req = decode_wire::<DrmGemMmapWire>(payload)?;
                let owned = self
                    .handles
                    .get(&id)
                    .map(|h| Self::handle_has_gem_ref(h, req.handle))
                    .unwrap_or(false);
                if !owned {
                    warn!(
                        "redox-drm: GEM_MMAP handle {} not owned by this fd",
                        req.handle
                    );
                    return Err(Error::new(EBADF));
                }
                if let Some(handle) = self.handles.get(&id) {
                    if handle.mapped_gem_refs != 0 && handle.mapped_gem != Some(req.handle) {
                        warn!(
                            "redox-drm: GEM_MMAP handle {} rejected — another GEM is still mapped",
                            req.handle
                        );
                        return Err(Error::new(EBUSY));
                    }
                }
                req.offset = self
                    .driver
                    .gem_mmap(req.handle)
                    .map_err(driver_to_syscall)? as u64;
                if let Some(handle) = self.handles.get_mut(&id) {
                    handle.mapped_gem = Some(req.handle);
                }
                bytes_of(&req)
            }

            DRM_IOCTL_PRIME_HANDLE_TO_FD => {
                let req = decode_wire::<DrmPrimeHandleToFdWire>(payload)?;
                let owned = self
                    .handles
                    .get(&id)
                    .map(|h| Self::handle_has_gem_ref(h, req.handle))
                    .unwrap_or(false);
                if !owned {
                    warn!(
                        "redox-drm: PRIME_HANDLE_TO_FD handle {} not owned by this fd",
                        req.handle
                    );
                    return Err(Error::new(EBADF));
                }

                let token = self.next_export_token;
                self.next_export_token = self.next_export_token.saturating_add(1);
                self.prime_exports.insert(token, req.handle);

                let resp = DrmPrimeHandleToFdResponseWire {
                    fd: token as i32,
                    _pad: 0,
                };
                bytes_of(&resp)
            }

            DRM_IOCTL_PRIME_FD_TO_HANDLE => {
                let req = decode_wire::<DrmPrimeFdToHandleWire>(payload)?;
                let token = if req.fd >= 0 {
                    req.fd as u32
                } else {
                    warn!("redox-drm: PRIME_FD_TO_HANDLE invalid token {}", req.fd);
                    return Err(Error::new(EBADF));
                };

                // The token comes from fpath() on the dmabuf fd, which embeds
                // the opaque export token (not the raw GEM handle).
                let gem_handle = match self.prime_exports.get(&token).copied() {
                    Some(h) => h,
                    None => {
                        warn!("redox-drm: PRIME_FD_TO_HANDLE token {} not found", token);
                        return Err(Error::new(ENOENT));
                    }
                };

                // Verify the GEM is still live — the exporter may have closed it
                // before any dmabuf fd was opened, leaving a stale token.
                self.driver.gem_size(gem_handle).map_err(|_| {
                    warn!(
                        "redox-drm: PRIME_FD_TO_HANDLE token {} maps to dead GEM {}",
                        token, gem_handle
                    );
                    // Clean up the stale token so future calls fail fast.
                    self.prime_exports.remove(&token);
                    Error::new(ENOENT)
                })?;

                let handle = self.handles.get_mut(&id).ok_or_else(|| Error::new(EBADF))?;
                if !handle.owned_gems.contains(&gem_handle) {
                    handle.owned_gems.push(gem_handle);
                }

                let resp = DrmPrimeFdToHandleResponseWire {
                    handle: gem_handle,
                    _pad: 0,
                };
                bytes_of(&resp)
            }

            _ => {
                warn!("redox-drm: unsupported ioctl {:#x}", request);
                return Err(Error::new(EOPNOTSUPP));
            }
        };

        let response = if response.is_empty() {
            vec![0]
        } else {
            response
        };

        let handle = self.handles.get_mut(&id).ok_or_else(|| Error::new(EBADF))?;
        let len = response.len();
        handle.response = response;
        Ok(len)
    }
}

// ---- SchemeBlockMut implementation ----

impl SchemeBlockMut for DrmScheme {
    fn open(&mut self, path: &str, _flags: usize, _uid: u32, _gid: u32) -> Result<Option<usize>> {
        let node = match path.trim_matches('/') {
            "card0" => NodeKind::Card,
            p if p.starts_with("card0Connector/") => {
                let tail = p.trim_start_matches("card0Connector/");
                let connector_id = tail.parse::<u32>().map_err(|_| Error::new(ENOENT))?;
                NodeKind::Connector(connector_id)
            }
            p if p.starts_with("card0/dmabuf/") => {
                let tail = p.trim_start_matches("card0/dmabuf/");
                let token = tail.parse::<u32>().map_err(|_| Error::new(ENOENT))?;
                let gem_handle = match self.prime_exports.get(&token).copied() {
                    Some(h) => h,
                    None => return Err(Error::new(ENOENT)),
                };
                self.driver.gem_size(gem_handle).map_err(|_| {
                    warn!(
                        "redox-drm: open dmabuf token {} maps to dead GEM {}",
                        token, gem_handle
                    );
                    self.prime_exports.remove(&token);
                    Error::new(ENOENT)
                })?;
                NodeKind::DmaBuf {
                    gem_handle,
                    export_token: token,
                }
            }
            _ => return Err(Error::new(ENOENT)),
        };

        if let NodeKind::DmaBuf { gem_handle, .. } = &node {
            self.bump_export_ref(*gem_handle);
        }

        let id = self.allocate_handle(node);
        Ok(Some(id))
    }

    fn read(&mut self, id: usize, buf: &mut [u8]) -> Result<Option<usize>> {
        let handle = self.handles.get_mut(&id).ok_or_else(|| Error::new(EBADF))?;
        let len = handle.response.len().min(buf.len());
        buf[..len].copy_from_slice(&handle.response[..len]);
        Ok(Some(len))
    }

    fn write(&mut self, id: usize, buf: &[u8]) -> Result<Option<usize>> {
        let (request_bytes, payload) = match buf.split_first_chunk::<8>() {
            Some(pair) => pair,
            None => {
                let _ = self.handles.get(&id).ok_or_else(|| Error::new(EBADF))?;
                return Ok(Some(0));
            }
        };
        let request = usize::from_le_bytes(*request_bytes);
        let written = self.handle_ioctl(id, request, payload)?;
        Ok(Some(written))
    }

    fn fpath(&mut self, id: usize, buf: &mut [u8]) -> Result<Option<usize>> {
        let handle = self.handles.get(&id).ok_or_else(|| Error::new(EBADF))?;
        let path = match handle.node {
            NodeKind::Card => "drm:card0".to_string(),
            NodeKind::Connector(cid) => format!("drm:card0Connector/{cid}"),
            NodeKind::DmaBuf { export_token, .. } => format!("drm:card0/dmabuf/{export_token}"),
        };
        let bytes = path.as_bytes();
        let len = bytes.len().min(buf.len());
        buf[..len].copy_from_slice(&bytes[..len]);
        Ok(Some(len))
    }

    fn fstat(&mut self, id: usize, stat: &mut Stat) -> Result<Option<usize>> {
        let handle = self.handles.get(&id).ok_or_else(|| Error::new(EBADF))?;
        stat.st_mode = MODE_FILE | 0o666;
        stat.st_size = handle.response.len() as u64;
        stat.st_blksize = 4096;
        Ok(Some(0))
    }

    fn fsync(&mut self, id: usize) -> Result<Option<usize>> {
        let _ = self.handles.get(&id).ok_or_else(|| Error::new(EBADF))?;
        Ok(Some(0))
    }

    fn fevent(&mut self, id: usize, _flags: EventFlags) -> Result<Option<EventFlags>> {
        let _ = self.handles.get(&id).ok_or_else(|| Error::new(EBADF))?;
        Ok(Some(EventFlags::empty()))
    }

    fn close(&mut self, id: usize) -> Result<Option<usize>> {
        let mapped = self
            .handles
            .get(&id)
            .ok_or_else(|| Error::new(EBADF))?
            .mapped_gem_refs;
        if mapped != 0 {
            let handle = self.handles.get_mut(&id).ok_or_else(|| Error::new(EBADF))?;
            handle.closing = true;
            return Ok(Some(0));
        }

        if let Some(handle) = self.handles.remove(&id) {
            self.finalize_handle_close(handle);
        } else {
            return Err(Error::new(EBADF));
        }
        Ok(Some(0))
    }

    fn mmap_prep(
        &mut self,
        id: usize,
        offset: u64,
        size: usize,
        _flags: MapFlags,
    ) -> Result<Option<usize>> {
        let gem_handle = {
            let handle = self.handles.get(&id).ok_or_else(|| Error::new(EBADF))?;
            match handle.node {
                NodeKind::DmaBuf { gem_handle, .. } => gem_handle,
                _ => handle.mapped_gem.ok_or_else(|| Error::new(EINVAL))?,
            }
        };

        let gem_size = self
            .driver
            .gem_size(gem_handle)
            .map_err(driver_to_syscall)?;

        if offset > gem_size {
            return Err(Error::new(EINVAL));
        }
        let remaining = gem_size - offset;
        if size as u64 > remaining {
            return Err(Error::new(EINVAL));
        }

        let base_addr = self
            .driver
            .gem_mmap(gem_handle)
            .map_err(driver_to_syscall)?;
        let addr = base_addr + offset as usize;
        self.pin_mapped_gem(id, gem_handle)?;
        debug!(
            "redox-drm: mmap_prep GEM handle {} offset={} size={} at addr={:#x}",
            gem_handle, offset, size, addr
        );
        Ok(Some(addr))
    }

    fn munmap(
        &mut self,
        id: usize,
        offset: u64,
        size: usize,
        _flags: MunmapFlags,
    ) -> Result<Option<usize>> {
        let _ = self.handles.get(&id).ok_or_else(|| Error::new(EBADF))?;
        self.unpin_mapped_gem(id)?;
        debug!(
            "redox-drm: munmap id={} offset={} size={}",
            id, offset, size
        );
        let should_finalize = self
            .handles
            .get(&id)
            .map(|handle| handle.closing && handle.mapped_gem_refs == 0)
            .unwrap_or(false);
        if should_finalize {
            if let Some(handle) = self.handles.remove(&id) {
                self.finalize_handle_close(handle);
            }
        }
        Ok(Some(0))
    }
}

// ---- Conversion helpers ----

fn connector_type_to_u32(ct: crate::kms::ConnectorType) -> u32 {
    match ct {
        crate::kms::ConnectorType::Unknown => 0,
        crate::kms::ConnectorType::VGA => 1,
        crate::kms::ConnectorType::DVII => 2,
        crate::kms::ConnectorType::DVID => 3,
        crate::kms::ConnectorType::DVIA => 4,
        crate::kms::ConnectorType::Composite => 5,
        crate::kms::ConnectorType::SVideo => 6,
        crate::kms::ConnectorType::LVDS => 7,
        crate::kms::ConnectorType::Component => 8,
        crate::kms::ConnectorType::NinePinDIN => 9,
        crate::kms::ConnectorType::DisplayPort => 10,
        crate::kms::ConnectorType::HDMIA => 11,
        crate::kms::ConnectorType::HDMIB => 12,
        crate::kms::ConnectorType::TV => 13,
        crate::kms::ConnectorType::EDP => 14,
        crate::kms::ConnectorType::Virtual => 15,
    }
}

fn mode_to_wire(mode: &ModeInfo) -> DrmModeWire {
    DrmModeWire {
        clock: mode.clock,
        hdisplay: mode.hdisplay,
        hsync_start: mode.hsync_start,
        hsync_end: mode.hsync_end,
        htotal: mode.htotal,
        hskew: mode.hskew,
        vdisplay: mode.vdisplay,
        vsync_start: mode.vsync_start,
        vsync_end: mode.vsync_end,
        vtotal: mode.vtotal,
        vscan: mode.vscan,
        vrefresh: mode.vrefresh,
        flags: mode.flags,
        type_: mode.type_,
    }
}

fn wire_to_mode(w: &DrmModeWire) -> ModeInfo {
    ModeInfo {
        clock: w.clock,
        hdisplay: w.hdisplay,
        hsync_start: w.hsync_start,
        hsync_end: w.hsync_end,
        htotal: w.htotal,
        hskew: w.hskew,
        vdisplay: w.vdisplay,
        vsync_start: w.vsync_start,
        vsync_end: w.vsync_end,
        vtotal: w.vtotal,
        vscan: w.vscan,
        vrefresh: w.vrefresh,
        flags: w.flags,
        type_: w.type_,
        name: format!("{}x{}@{}", w.hdisplay, w.vdisplay, w.vrefresh),
    }
}

fn encode_modes(modes: &[ModeInfo]) -> Vec<u8> {
    let mut out = Vec::new();
    for mode in modes {
        out.extend_from_slice(&bytes_of(&mode_to_wire(mode)));
        out.extend_from_slice(mode.name.as_bytes());
        out.push(0);
    }
    if out.is_empty() {
        out.push(0);
    }
    out
}

fn bytes_of<T>(value: &T) -> Vec<u8> {
    let ptr = value as *const T as *const u8;
    let len = size_of::<T>();
    unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec()
}

fn read_u32(buf: &[u8], offset: usize) -> Result<u32> {
    let end = offset.saturating_add(size_of::<u32>());
    let bytes = buf.get(offset..end).ok_or_else(|| Error::new(EINVAL))?;
    let array: [u8; 4] = bytes.try_into().map_err(|_| Error::new(EINVAL))?;
    Ok(u32::from_le_bytes(array))
}

fn decode_wire<T: Copy>(buf: &[u8]) -> Result<T> {
    if buf.len() < size_of::<T>() {
        return Err(Error::new(EINVAL));
    }
    let ptr = buf.as_ptr() as *const T;
    Ok(unsafe { ptr.read_unaligned() })
}

fn driver_to_syscall(error: crate::driver::DriverError) -> Error {
    warn!("redox-drm: driver error: {}", error);
    Error::new(EINVAL)
}
