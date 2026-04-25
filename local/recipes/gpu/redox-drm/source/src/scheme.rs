use std::collections::{BTreeMap, HashSet, VecDeque};
use std::mem::size_of;
use std::sync::Arc;

use getrandom::getrandom;
use log::{debug, warn};
use redox_scheme::SchemeBlockMut;
use syscall04::data::Stat;
use syscall04::error::{Error, Result, EBADF, EBUSY, EINVAL, ENOENT, EOPNOTSUPP};
use syscall04::flag::{EventFlags, MapFlags, MunmapFlags, MODE_FILE};

use crate::driver::{
    DriverEvent, GpuDriver, RedoxPrivateCsSubmit, RedoxPrivateCsSubmitResult, RedoxPrivateCsWait,
    RedoxPrivateCsWaitResult,
};
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
const DRM_IOCTL_REDOX_PRIVATE_CS_SUBMIT: usize = DRM_IOCTL_BASE + 31;
const DRM_IOCTL_REDOX_PRIVATE_CS_WAIT: usize = DRM_IOCTL_BASE + 32;
const DRM_IOCTL_REDOX_AMD_SDMA_SUBMIT: usize = DRM_IOCTL_BASE + 0x40;
const DRM_IOCTL_REDOX_AMD_SDMA_WAIT: usize = DRM_IOCTL_BASE + 0x41;

const MAX_SCHEME_GEM_BYTES: u64 = 256 * 1024 * 1024;

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

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct RedoxAmdSdmaSubmitWire {
    src_handle: u32,
    dst_handle: u32,
    flags: u32,
    _pad: u32,
    src_offset: u64,
    dst_offset: u64,
    size: u64,
    seqno: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct RedoxAmdSdmaWaitWire {
    seqno: u64,
    timeout_ns: u64,
    flags: u32,
    completed: u32,
    completed_seqno: u64,
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
    event_queue: VecDeque<Vec<u8>>,
    mapped_gem: Option<GemHandle>,
    mapped_gem_refs: usize,
    owned_fbs: Vec<u32>,
    owned_gems: Vec<GemHandle>,
    imported_gems: HashSet<GemHandle>,
    closing: bool,
}

pub struct DrmScheme {
    driver: Arc<dyn GpuDriver>,
    next_id: usize,
    next_fb_id: u32,
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

    fn is_fb_active(&self, fb_id: u32) -> bool {
        self.active_crtc_fb.values().any(|&id| id == fb_id)
            || self.pending_flip_fb.values().any(|&(_, id)| id == fb_id)
    }

    fn handle_has_gem_ref(handle: &Handle, gem_handle: GemHandle) -> bool {
        handle.owned_gems.contains(&gem_handle)
    }

    fn handle_has_local_gem_ref(handle: &Handle, gem_handle: GemHandle) -> bool {
        Self::handle_has_gem_ref(handle, gem_handle) && !handle.imported_gems.contains(&gem_handle)
    }

    fn handle_has_imported_gem_ref(handle: &Handle, gem_handle: GemHandle) -> bool {
        Self::handle_has_gem_ref(handle, gem_handle) && handle.imported_gems.contains(&gem_handle)
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

    fn allocate_export_token(&self) -> Result<u32> {
        for _ in 0..64 {
            let mut bytes = [0u8; 4];
            getrandom(&mut bytes).map_err(|e| {
                warn!("redox-drm: failed to draw PRIME export token entropy: {e}");
                Error::new(syscall04::error::EIO)
            })?;

            let token = u32::from_le_bytes(bytes) & 0x7fff_ffff;
            if token == 0 || self.prime_exports.contains_key(&token) {
                continue;
            }

            return Ok(token);
        }

        warn!("redox-drm: unable to allocate unique PRIME export token");
        Err(Error::new(EBUSY))
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

    fn validate_private_cs_handles(
        &self,
        id: usize,
        src_handle: GemHandle,
        dst_handle: GemHandle,
        operation: &str,
    ) -> Result<()> {
        let handle = self.handles.get(&id).ok_or_else(|| Error::new(EBADF))?;

        if !Self::handle_has_gem_ref(handle, src_handle)
            || !Self::handle_has_gem_ref(handle, dst_handle)
        {
            warn!(
                "redox-drm: {} rejected — src={} dst={} not owned by this fd",
                operation, src_handle, dst_handle
            );
            return Err(Error::new(EBADF));
        }

        if Self::handle_has_imported_gem_ref(handle, src_handle)
            || Self::handle_has_imported_gem_ref(handle, dst_handle)
        {
            warn!(
                "redox-drm: {} rejected — imported DMA-BUF handles are outside the bounded private CS path",
                operation
            );
            return Err(Error::new(EOPNOTSUPP));
        }

        Ok(())
    }

    fn validate_private_cs_ranges(
        &self,
        submit: &RedoxPrivateCsSubmit,
        operation: &str,
    ) -> Result<()> {
        if submit.byte_count == 0 {
            warn!("redox-drm: {} rejected — zero-sized submission", operation);
            return Err(Error::new(EINVAL));
        }

        let src_size = self
            .driver
            .gem_size(submit.src_handle)
            .map_err(driver_to_syscall)?;
        let dst_size = self
            .driver
            .gem_size(submit.dst_handle)
            .map_err(driver_to_syscall)?;

        let src_end = submit
            .src_offset
            .checked_add(submit.byte_count)
            .ok_or_else(|| {
                warn!("redox-drm: {} rejected — source range overflow", operation);
                Error::new(EINVAL)
            })?;
        if src_end > src_size {
            warn!(
                "redox-drm: {} rejected — source range {}..{} exceeds GEM size {}",
                operation,
                submit.src_offset,
                src_end,
                src_size
            );
            return Err(Error::new(EINVAL));
        }

        let dst_end = submit
            .dst_offset
            .checked_add(submit.byte_count)
            .ok_or_else(|| {
                warn!("redox-drm: {} rejected — destination range overflow", operation);
                Error::new(EINVAL)
            })?;
        if dst_end > dst_size {
            warn!(
                "redox-drm: {} rejected — destination range {}..{} exceeds GEM size {}",
                operation,
                submit.dst_offset,
                dst_end,
                dst_size
            );
            return Err(Error::new(EINVAL));
        }

        Ok(())
    }

    fn validate_gem_create_size(&self, size: u64, operation: &str) -> Result<()> {
        if size == 0 {
            warn!("redox-drm: {} rejected — zero-sized GEM allocation", operation);
            return Err(Error::new(EINVAL));
        }
        if size > MAX_SCHEME_GEM_BYTES {
            warn!(
                "redox-drm: {} rejected — size {} exceeds trusted shared-core cap {}",
                operation,
                size,
                MAX_SCHEME_GEM_BYTES
            );
            return Err(Error::new(EINVAL));
        }
        Ok(())
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
                event_queue: VecDeque::new(),
                mapped_gem: None,
                mapped_gem_refs: 0,
                owned_fbs: Vec::new(),
                owned_gems: Vec::new(),
                imported_gems: HashSet::new(),
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

    pub fn handle_driver_event(&mut self, event: DriverEvent) {
        match event {
            DriverEvent::Vblank { crtc_id, count } => {
                self.retire_vblank(crtc_id, count);
                self.queue_card_event(format!("vblank:{crtc_id}:{count}\n").into_bytes());
            }
            DriverEvent::Hotplug { connector_id } => self.queue_hotplug_event(connector_id),
        }
    }

    fn queue_card_event(&mut self, payload: Vec<u8>) {
        for handle in self.handles.values_mut() {
            if let NodeKind::Card = handle.node {
                handle.event_queue.push_back(payload.clone());
            }
        }
    }

    fn queue_hotplug_event(&mut self, connector_id: u32) {
        let payload = format!("hotplug:{}\n", connector_id).into_bytes();
        for handle in self.handles.values_mut() {
            match handle.node {
                NodeKind::Card => {
                    handle.event_queue.push_back(payload.clone());
                }
                NodeKind::Connector(id) if id == connector_id => {
                    handle.event_queue.push_back(payload.clone());
                }
                _ => {}
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
        let mut out = bytes_of(&payload);
        for connector in connectors {
            out.extend_from_slice(&bytes_of(&connector.id));
        }
        out
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
                self.validate_gem_create_size(req.size, "CREATE_DUMB")?;
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
                    handle.imported_gems.remove(&req.handle);
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
                let required_size = match (pitch as u64).checked_mul(req.height as u64) {
                    Some(s) => s,
                    None => {
                        warn!(
                            "redox-drm: ADDFB pitch * height overflows pitch={} height={}",
                            pitch, req.height
                        );
                        return Err(Error::new(EINVAL));
                    }
                };
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
                if required_size > actual_size {
                    warn!(
                        "redox-drm: ADDFB requires {} bytes but GEM {} is {} bytes",
                        required_size,
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
                self.validate_gem_create_size(req.size, "GEM_CREATE")?;
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
                    handle.imported_gems.remove(&req.handle);
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

            DRM_IOCTL_REDOX_AMD_SDMA_SUBMIT => {
                let mut req = decode_wire::<RedoxAmdSdmaSubmitWire>(payload)?;
                if req.flags != 0 {
                    warn!(
                        "redox-drm: AMD SDMA submit rejected — unsupported flags {:#x}",
                        req.flags
                    );
                    return Err(Error::new(EINVAL));
                }
                if req.size == 0 {
                    warn!("redox-drm: AMD SDMA submit rejected — zero-sized copy");
                    return Err(Error::new(EINVAL));
                }

                self.validate_private_cs_handles(
                    id,
                    req.src_handle,
                    req.dst_handle,
                    "AMD SDMA submit",
                )?;

                let submit = RedoxPrivateCsSubmit {
                    src_handle: req.src_handle,
                    dst_handle: req.dst_handle,
                    src_offset: req.src_offset,
                    dst_offset: req.dst_offset,
                    byte_count: req.size,
                };
                self.validate_private_cs_ranges(&submit, "AMD SDMA submit")?;
                req.seqno = self
                    .driver
                    .redox_private_cs_submit(&submit)
                    .map_err(driver_to_syscall)?
                    .seqno;
                bytes_of(&req)
            }

            DRM_IOCTL_REDOX_AMD_SDMA_WAIT => {
                let mut req = decode_wire::<RedoxAmdSdmaWaitWire>(payload)?;
                if req.flags != 0 {
                    warn!(
                        "redox-drm: AMD SDMA wait rejected — unsupported flags {:#x}",
                        req.flags
                    );
                    return Err(Error::new(EINVAL));
                }

                let result = self
                    .driver
                    .redox_private_cs_wait(&RedoxPrivateCsWait {
                        seqno: req.seqno,
                        timeout_ns: req.timeout_ns,
                    })
                    .map_err(driver_to_syscall)?;
                req.completed = u32::from(result.completed);
                req.completed_seqno = result.completed_seqno;
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

                let token = self.allocate_export_token()?;
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
                    handle.imported_gems.insert(gem_handle);
                }

                let resp = DrmPrimeFdToHandleResponseWire {
                    handle: gem_handle,
                    _pad: 0,
                };
                bytes_of(&resp)
            }

            DRM_IOCTL_REDOX_PRIVATE_CS_SUBMIT => {
                let req = decode_wire::<RedoxPrivateCsSubmit>(payload)?;
                self.validate_private_cs_handles(
                    id,
                    req.src_handle,
                    req.dst_handle,
                    "private CS submit",
                )?;
                self.validate_private_cs_ranges(&req, "private CS submit")?;
                let resp: RedoxPrivateCsSubmitResult = self
                    .driver
                    .redox_private_cs_submit(&req)
                    .map_err(driver_to_syscall)?;
                bytes_of(&resp)
            }

            DRM_IOCTL_REDOX_PRIVATE_CS_WAIT => {
                let req = decode_wire::<RedoxPrivateCsWait>(payload)?;
                let resp: RedoxPrivateCsWaitResult = self
                    .driver
                    .redox_private_cs_wait(&req)
                    .map_err(driver_to_syscall)?;
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
        if !handle.response.is_empty() {
            let len = handle.response.len().min(buf.len());
            buf[..len].copy_from_slice(&handle.response[..len]);
            return Ok(Some(len));
        }

        if let Some(event) = handle.event_queue.pop_front() {
            let len = event.len().min(buf.len());
            buf[..len].copy_from_slice(&event[..len]);
            return Ok(Some(len));
        }

        Ok(Some(0))
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
        stat.st_size = if !handle.response.is_empty() {
            handle.response.len() as u64
        } else {
            handle.event_queue.front().map(|payload| payload.len()).unwrap_or(0) as u64
        };
        stat.st_blksize = 4096;
        Ok(Some(0))
    }

    fn fsync(&mut self, id: usize) -> Result<Option<usize>> {
        let _ = self.handles.get(&id).ok_or_else(|| Error::new(EBADF))?;
        warn!(
            "redox-drm: fsync rejected — shared core has no implicit render-fence sync contract"
        );
        Err(Error::new(EOPNOTSUPP))
    }

    fn fevent(&mut self, id: usize, flags: EventFlags) -> Result<Option<EventFlags>> {
        let handle = self.handles.get(&id).ok_or_else(|| Error::new(EBADF))?;
        let readiness = if handle.event_queue.is_empty() {
            EventFlags::empty()
        } else {
            flags & EventFlags::EVENT_READ
        };
        Ok(Some(readiness))
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
    match error {
        crate::driver::DriverError::Unsupported(_) => Error::new(EOPNOTSUPP),
        crate::driver::DriverError::InvalidArgument(_) => Error::new(EINVAL),
        crate::driver::DriverError::NotFound(_) => Error::new(ENOENT),
        crate::driver::DriverError::Initialization(_)
        | crate::driver::DriverError::Mmio(_)
        | crate::driver::DriverError::Pci(_)
        | crate::driver::DriverError::Buffer(_)
        | crate::driver::DriverError::Io(_) => Error::new(EINVAL),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use redox_scheme::SchemeBlockMut;

    use super::*;
    use crate::driver::{DriverError, DriverEvent, GpuDriver};
    use crate::kms::{ConnectorInfo, ModeInfo};

    #[derive(Default)]
    struct FakeDriverState {
        next_handle: GemHandle,
        gem_sizes: BTreeMap<GemHandle, u64>,
        submit_calls: usize,
    }

    struct FakeDriver {
        state: Mutex<FakeDriverState>,
        support_private_cs: bool,
    }

    impl FakeDriver {
        fn new(support_private_cs: bool) -> Self {
            Self {
                state: Mutex::new(FakeDriverState {
                    next_handle: 1,
                    ..FakeDriverState::default()
                }),
                support_private_cs,
            }
        }

        fn submit_calls(&self) -> usize {
            self.state.lock().unwrap().submit_calls
        }
    }

    impl GpuDriver for FakeDriver {
        fn driver_name(&self) -> &str {
            "fake"
        }

        fn driver_desc(&self) -> &str {
            "fake"
        }

        fn driver_date(&self) -> &str {
            "1970-01-01"
        }

        fn detect_connectors(&self) -> Vec<ConnectorInfo> {
            Vec::new()
        }

        fn get_modes(&self, _connector_id: u32) -> Vec<ModeInfo> {
            Vec::new()
        }

        fn set_crtc(
            &self,
            _crtc_id: u32,
            _fb_handle: u32,
            _connectors: &[u32],
            _mode: &ModeInfo,
        ) -> crate::driver::Result<()> {
            Ok(())
        }

        fn page_flip(&self, _crtc_id: u32, _fb_handle: u32, _flags: u32) -> crate::driver::Result<u64> {
            Ok(0)
        }

        fn get_vblank(&self, _crtc_id: u32) -> crate::driver::Result<u64> {
            Ok(0)
        }

        fn gem_create(&self, size: u64) -> crate::driver::Result<GemHandle> {
            let mut state = self.state.lock().unwrap();
            let handle = state.next_handle;
            state.next_handle = state.next_handle.saturating_add(1);
            state.gem_sizes.insert(handle, size);
            Ok(handle)
        }

        fn gem_close(&self, handle: GemHandle) -> crate::driver::Result<()> {
            let removed = self.state.lock().unwrap().gem_sizes.remove(&handle);
            if removed.is_some() {
                Ok(())
            } else {
                Err(DriverError::NotFound(format!("unknown GEM handle {handle}")))
            }
        }

        fn gem_mmap(&self, handle: GemHandle) -> crate::driver::Result<usize> {
            if self.state.lock().unwrap().gem_sizes.contains_key(&handle) {
                Ok((handle as usize).saturating_mul(4096))
            } else {
                Err(DriverError::NotFound(format!("unknown GEM handle {handle}")))
            }
        }

        fn gem_size(&self, handle: GemHandle) -> crate::driver::Result<u64> {
            self.state
                .lock()
                .unwrap()
                .gem_sizes
                .get(&handle)
                .copied()
                .ok_or_else(|| DriverError::NotFound(format!("unknown GEM handle {handle}")))
        }

        fn get_edid(&self, _connector_id: u32) -> Vec<u8> {
            Vec::new()
        }

        fn handle_irq(&self) -> crate::driver::Result<Option<DriverEvent>> {
            Ok(None)
        }

        fn redox_private_cs_submit(
            &self,
            _submit: &RedoxPrivateCsSubmit,
        ) -> crate::driver::Result<RedoxPrivateCsSubmitResult> {
            if !self.support_private_cs {
                return Err(DriverError::Unsupported(
                    "private command submission is unavailable on this backend",
                ));
            }

            let mut state = self.state.lock().unwrap();
            state.submit_calls = state.submit_calls.saturating_add(1);
            Ok(RedoxPrivateCsSubmitResult { seqno: 7 })
        }
    }

    fn open_card(scheme: &mut DrmScheme) -> usize {
        scheme.open("card0", 0, 0, 0).unwrap().unwrap()
    }

    fn open_connector(scheme: &mut DrmScheme, connector_id: u32) -> usize {
        scheme
            .open(&format!("card0Connector/{connector_id}"), 0, 0, 0)
            .unwrap()
            .unwrap()
    }

    fn write_ioctl<T>(scheme: &mut DrmScheme, id: usize, request: usize, payload: &T) -> Result<usize> {
        let mut buf = request.to_le_bytes().to_vec();
        buf.extend_from_slice(&bytes_of(payload));
        scheme.write(id, &buf).map(|written| written.unwrap_or(0))
    }

    fn read_response<T: Copy>(scheme: &mut DrmScheme, id: usize) -> T {
        let mut buf = vec![0; size_of::<T>()];
        let len = scheme.read(id, &mut buf).unwrap().unwrap();
        assert_eq!(len, size_of::<T>());
        decode_wire::<T>(&buf).unwrap()
    }

    #[test]
    fn private_cs_submit_rejects_imported_dma_buf_handles() {
        let driver = Arc::new(FakeDriver::new(true));
        let mut scheme = DrmScheme::new(driver.clone());

        let exporter = open_card(&mut scheme);
        let importer = open_card(&mut scheme);

        let create = DrmGemCreateWire {
            size: 4096,
            ..DrmGemCreateWire::default()
        };
        write_ioctl(&mut scheme, exporter, DRM_IOCTL_GEM_CREATE, &create).unwrap();
        let created = read_response::<DrmGemCreateWire>(&mut scheme, exporter);

        let export = DrmPrimeHandleToFdWire {
            handle: created.handle,
            flags: 0,
        };
        write_ioctl(&mut scheme, exporter, DRM_IOCTL_PRIME_HANDLE_TO_FD, &export).unwrap();
        let exported = read_response::<DrmPrimeHandleToFdResponseWire>(&mut scheme, exporter);

        let import = DrmPrimeFdToHandleWire {
            fd: exported.fd,
            _pad: 0,
        };
        write_ioctl(&mut scheme, importer, DRM_IOCTL_PRIME_FD_TO_HANDLE, &import).unwrap();
        let imported = read_response::<DrmPrimeFdToHandleResponseWire>(&mut scheme, importer);

        let submit = RedoxPrivateCsSubmit {
            src_handle: imported.handle,
            dst_handle: imported.handle,
            src_offset: 0,
            dst_offset: 0,
            byte_count: 64,
        };
        let err = write_ioctl(
            &mut scheme,
            importer,
            DRM_IOCTL_REDOX_PRIVATE_CS_SUBMIT,
            &submit,
        )
        .unwrap_err();

        assert_eq!(err.errno, EOPNOTSUPP);
        assert_eq!(driver.submit_calls(), 0);
    }

    #[test]
    fn prime_handle_to_fd_returns_distinct_nonzero_tokens() {
        let mut scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));
        let card = open_card(&mut scheme);

        for _ in 0..2 {
            let create = DrmGemCreateWire {
                size: 4096,
                ..DrmGemCreateWire::default()
            };
            write_ioctl(&mut scheme, card, DRM_IOCTL_GEM_CREATE, &create).unwrap();
            let _ = read_response::<DrmGemCreateWire>(&mut scheme, card);
        }

        let handles = scheme.handles.get(&card).unwrap().owned_gems.clone();

        let export_a = DrmPrimeHandleToFdWire {
            handle: handles[0],
            flags: 0,
        };
        write_ioctl(&mut scheme, card, DRM_IOCTL_PRIME_HANDLE_TO_FD, &export_a).unwrap();
        let token_a = read_response::<DrmPrimeHandleToFdResponseWire>(&mut scheme, card).fd;

        let export_b = DrmPrimeHandleToFdWire {
            handle: handles[1],
            flags: 0,
        };
        write_ioctl(&mut scheme, card, DRM_IOCTL_PRIME_HANDLE_TO_FD, &export_b).unwrap();
        let token_b = read_response::<DrmPrimeHandleToFdResponseWire>(&mut scheme, card).fd;

        assert_ne!(token_a, 0);
        assert_ne!(token_b, 0);
        assert_ne!(token_a, token_b);
    }

    #[test]
    fn private_cs_wait_is_explicitly_unsupported_without_backend_support() {
        let mut scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));
        let card = open_card(&mut scheme);
        let wait = RedoxPrivateCsWait {
            seqno: 1,
            timeout_ns: 0,
        };

        let err = write_ioctl(&mut scheme, card, DRM_IOCTL_REDOX_PRIVATE_CS_WAIT, &wait).unwrap_err();

        assert_eq!(err.errno, EOPNOTSUPP);
    }

    #[test]
    fn fsync_is_not_a_fake_render_sync_success() {
        let mut scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));
        let card = open_card(&mut scheme);

        let err = scheme.fsync(card).unwrap_err();

        assert_eq!(err.errno, EOPNOTSUPP);
    }

    #[test]
    fn private_cs_submit_still_reaches_backend_for_local_gems() {
        let driver = Arc::new(FakeDriver::new(true));
        let mut scheme = DrmScheme::new(driver.clone());
        let card = open_card(&mut scheme);

        for _ in 0..2 {
            let create = DrmGemCreateWire {
                size: 4096,
                ..DrmGemCreateWire::default()
            };
            write_ioctl(&mut scheme, card, DRM_IOCTL_GEM_CREATE, &create).unwrap();
            let _ = read_response::<DrmGemCreateWire>(&mut scheme, card);
        }

        let handles = match scheme.handles.get(&card) {
            Some(handle) => handle.owned_gems.clone(),
            None => panic!("missing fake card handle"),
        };
        let submit = RedoxPrivateCsSubmit {
            src_handle: handles[0],
            dst_handle: handles[1],
            src_offset: 0,
            dst_offset: 0,
            byte_count: 128,
        };

        write_ioctl(&mut scheme, card, DRM_IOCTL_REDOX_PRIVATE_CS_SUBMIT, &submit).unwrap();
        let response = read_response::<RedoxPrivateCsSubmitResult>(&mut scheme, card);

        assert_eq!(response.seqno, 7);
        assert_eq!(driver.submit_calls(), 1);
    }

    #[test]
    fn private_cs_submit_rejects_out_of_bounds_ranges() {
        let driver = Arc::new(FakeDriver::new(true));
        let mut scheme = DrmScheme::new(driver.clone());
        let card = open_card(&mut scheme);

        for _ in 0..2 {
            let create = DrmGemCreateWire {
                size: 4096,
                ..DrmGemCreateWire::default()
            };
            write_ioctl(&mut scheme, card, DRM_IOCTL_GEM_CREATE, &create).unwrap();
            let _ = read_response::<DrmGemCreateWire>(&mut scheme, card);
        }

        let handles = scheme.handles.get(&card).unwrap().owned_gems.clone();
        let submit = RedoxPrivateCsSubmit {
            src_handle: handles[0],
            dst_handle: handles[1],
            src_offset: 4090,
            dst_offset: 0,
            byte_count: 64,
        };

        let err = write_ioctl(&mut scheme, card, DRM_IOCTL_REDOX_PRIVATE_CS_SUBMIT, &submit)
            .unwrap_err();

        assert_eq!(err.errno, EINVAL);
        assert_eq!(driver.submit_calls(), 0);
    }

    #[test]
    fn vblank_driver_event_retires_pending_page_flip() {
        let mut scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));

        scheme.fb_registry.insert(
            7,
            FbInfo {
                gem_handle: 41,
                width: 0,
                height: 0,
                pitch: 0,
                bpp: 0,
            },
        );
        scheme.pending_flip_fb.insert(3, (5, 7));

        scheme.handle_driver_event(DriverEvent::Vblank {
            crtc_id: 3,
            count: 5,
        });

        assert!(!scheme.pending_flip_fb.contains_key(&3));
        assert!(!scheme.fb_registry.contains_key(&7));
    }

    #[test]
    fn non_vblank_driver_event_does_not_retire_pending_page_flip() {
        let mut scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));
        let card = open_card(&mut scheme);

        scheme.fb_registry.insert(
            9,
            FbInfo {
                gem_handle: 99,
                width: 0,
                height: 0,
                pitch: 0,
                bpp: 0,
            },
        );
        scheme.pending_flip_fb.insert(1, (2, 9));

        scheme.handle_driver_event(DriverEvent::Hotplug { connector_id: 1 });

        assert_eq!(scheme.pending_flip_fb.get(&1), Some(&(2, 9)));
        assert!(scheme.fb_registry.contains_key(&9));
        assert_eq!(
            scheme.fevent(card, EventFlags::EVENT_READ).unwrap(),
            Some(EventFlags::EVENT_READ)
        );
    }

    #[test]
    fn hotplug_event_is_readable_from_card_handle() {
        let mut scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));
        let card = open_card(&mut scheme);

        scheme.handle_driver_event(DriverEvent::Hotplug { connector_id: 7 });

        assert_eq!(
            scheme.fevent(card, EventFlags::EVENT_READ).unwrap(),
            Some(EventFlags::EVENT_READ)
        );

        let mut buf = [0u8; 32];
        let len = scheme.read(card, &mut buf).unwrap().unwrap();
        assert_eq!(&buf[..len], b"hotplug:7\n");
        assert_eq!(
            scheme.fevent(card, EventFlags::EVENT_READ).unwrap(),
            Some(EventFlags::empty())
        );
    }

    #[test]
    fn hotplug_event_targets_matching_connector_handle_only() {
        let mut scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));
        let connector_a = open_connector(&mut scheme, 1);
        let connector_b = open_connector(&mut scheme, 2);

        scheme.handle_driver_event(DriverEvent::Hotplug { connector_id: 2 });

        assert_eq!(
            scheme.fevent(connector_a, EventFlags::EVENT_READ).unwrap(),
            Some(EventFlags::empty())
        );
        assert_eq!(
            scheme.fevent(connector_b, EventFlags::EVENT_READ).unwrap(),
            Some(EventFlags::EVENT_READ)
        );
    }

    #[test]
    fn vblank_event_is_readable_from_card_handle() {
        let mut scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));
        let card = open_card(&mut scheme);

        scheme.handle_driver_event(DriverEvent::Vblank {
            crtc_id: 4,
            count: 12,
        });

        assert_eq!(
            scheme.fevent(card, EventFlags::EVENT_READ).unwrap(),
            Some(EventFlags::EVENT_READ)
        );

        let mut buf = [0u8; 32];
        let len = scheme.read(card, &mut buf).unwrap().unwrap();
        assert_eq!(&buf[..len], b"vblank:4:12\n");
        assert_eq!(
            scheme.fevent(card, EventFlags::EVENT_READ).unwrap(),
            Some(EventFlags::empty())
        );
    }

    #[test]
    fn gem_create_rejects_oversized_allocations() {
        let mut scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));
        let card = open_card(&mut scheme);
        let create = DrmGemCreateWire {
            size: MAX_SCHEME_GEM_BYTES + 1,
            ..DrmGemCreateWire::default()
        };

        let err = write_ioctl(&mut scheme, card, DRM_IOCTL_GEM_CREATE, &create).unwrap_err();

        assert_eq!(err.errno, EINVAL);
    }

    #[test]
    fn create_dumb_rejects_oversized_allocations() {
        let mut scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));
        let card = open_card(&mut scheme);
        let create = DrmCreateDumbWire {
            width: 16384,
            height: 16384,
            bpp: 32,
            ..DrmCreateDumbWire::default()
        };

        let err = write_ioctl(&mut scheme, card, DRM_IOCTL_MODE_CREATE_DUMB, &create).unwrap_err();

        assert_eq!(err.errno, EINVAL);
    }

    #[test]
    fn gem_has_other_refs_returns_false_when_only_current_handle_owns_gem() {
        let mut scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));
        let card = open_card(&mut scheme);

        let create = DrmGemCreateWire {
            size: 4096,
            ..DrmGemCreateWire::default()
        };
        write_ioctl(&mut scheme, card, DRM_IOCTL_GEM_CREATE, &create).unwrap();
        let created = read_response::<DrmGemCreateWire>(&mut scheme, card);

        let gem_handle = created.handle;
        assert!(
            !scheme.gem_has_other_refs(card, gem_handle),
            "only one handle owns the GEM, so gem_has_other_refs should be false"
        );
    }

    #[test]
    fn gem_has_other_refs_returns_true_when_another_handle_owns_gem() {
        let mut scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));
        let card_a = open_card(&mut scheme);
        let card_b = open_card(&mut scheme);

        let create = DrmGemCreateWire {
            size: 4096,
            ..DrmGemCreateWire::default()
        };
        write_ioctl(&mut scheme, card_a, DRM_IOCTL_GEM_CREATE, &create).unwrap();
        let created = read_response::<DrmGemCreateWire>(&mut scheme, card_a);
        let gem_handle = created.handle;

        let export = DrmPrimeHandleToFdWire {
            handle: gem_handle,
            flags: 0,
        };
        write_ioctl(&mut scheme, card_a, DRM_IOCTL_PRIME_HANDLE_TO_FD, &export).unwrap();
        let exported = read_response::<DrmPrimeHandleToFdResponseWire>(&mut scheme, card_a);

        let import = DrmPrimeFdToHandleWire {
            fd: exported.fd,
            _pad: 0,
        };
        write_ioctl(&mut scheme, card_b, DRM_IOCTL_PRIME_FD_TO_HANDLE, &import).unwrap();
        let imported = read_response::<DrmPrimeFdToHandleResponseWire>(&mut scheme, card_b);

        assert!(
            scheme.gem_has_other_refs(card_a, imported.handle),
            "card_b owns the same GEM, so gem_has_other_refs from card_a should be true"
        );
    }

    #[test]
    fn gem_is_mapped_returns_false_initially() {
        let mut scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));
        let card = open_card(&mut scheme);

        let create = DrmGemCreateWire {
            size: 4096,
            ..DrmGemCreateWire::default()
        };
        write_ioctl(&mut scheme, card, DRM_IOCTL_GEM_CREATE, &create).unwrap();
        let created = read_response::<DrmGemCreateWire>(&mut scheme, card);

        assert!(
            !scheme.gem_is_mapped(created.handle),
            "freshly created GEM should not be mapped"
        );
    }

    #[test]
    fn gem_is_mapped_returns_true_after_pin() {
        let mut scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));
        let card = open_card(&mut scheme);

        let create = DrmGemCreateWire {
            size: 4096,
            ..DrmGemCreateWire::default()
        };
        write_ioctl(&mut scheme, card, DRM_IOCTL_GEM_CREATE, &create).unwrap();
        let created = read_response::<DrmGemCreateWire>(&mut scheme, card);
        let gem_handle = created.handle;

        scheme.pin_mapped_gem(card, gem_handle).unwrap();

        assert!(
            scheme.gem_is_mapped(gem_handle),
            "GEM should be mapped after pin_mapped_gem"
        );
    }

    #[test]
    fn gem_export_refcount_starts_at_zero() {
        let scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));

        assert_eq!(
            scheme.gem_export_refcount(9999),
            0,
            "unknown GEM should have refcount 0"
        );
    }

    #[test]
    fn bump_export_ref_increments_from_zero_to_one() {
        let mut scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));
        let gem_handle: GemHandle = 42;

        scheme.bump_export_ref(gem_handle);

        assert_eq!(
            scheme.gem_export_refcount(gem_handle),
            1,
            "bumping an unknown GEM should set its refcount to 1"
        );
    }

    #[test]
    fn bump_export_ref_saturates_on_overflow() {
        let mut scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));
        let gem_handle: GemHandle = 42;

        scheme.gem_export_refs.insert(gem_handle, usize::MAX);

        scheme.bump_export_ref(gem_handle);

        assert_eq!(
            scheme.gem_export_refcount(gem_handle),
            usize::MAX,
            "saturating add should keep refcount at usize::MAX"
        );
    }

    #[test]
    fn drop_export_ref_decrements_count() {
        let mut scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));
        let gem_handle: GemHandle = 42;

        scheme.bump_export_ref(gem_handle);
        scheme.bump_export_ref(gem_handle);
        assert_eq!(scheme.gem_export_refcount(gem_handle), 2);

        scheme.drop_export_ref(gem_handle);

        assert_eq!(
            scheme.gem_export_refcount(gem_handle),
            1,
            "dropping once should decrement from 2 to 1"
        );
    }

    #[test]
    fn drop_export_ref_removes_entry_when_count_reaches_zero() {
        let mut scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));
        let gem_handle: GemHandle = 42;

        scheme.bump_export_ref(gem_handle);
        assert_eq!(scheme.gem_export_refcount(gem_handle), 1);

        scheme.drop_export_ref(gem_handle);

        assert_eq!(
            scheme.gem_export_refcount(gem_handle),
            0,
            "dropping the last ref should remove the entry"
        );
    }

    #[test]
    fn drop_export_ref_cleans_up_prime_exports() {
        let mut scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));
        let gem_handle: GemHandle = 77;
        let export_token: u32 = 100;

        scheme.prime_exports.insert(export_token, gem_handle);
        scheme.bump_export_ref(gem_handle);
        assert_eq!(scheme.gem_export_refcount(gem_handle), 1);
        assert!(scheme.prime_exports.contains_key(&export_token));

        scheme.drop_export_ref(gem_handle);

        assert_eq!(scheme.gem_export_refcount(gem_handle), 0);
        assert!(
            !scheme.prime_exports.values().any(|&h| h == gem_handle),
            "drop_export_ref should clean up prime_exports entries for this GEM"
        );
    }

    #[test]
    fn gem_can_close_returns_false_when_gem_backs_framebuffer() {
        let mut scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));
        let card = open_card(&mut scheme);

        let create = DrmGemCreateWire {
            size: 16384,
            ..DrmGemCreateWire::default()
        };
        write_ioctl(&mut scheme, card, DRM_IOCTL_GEM_CREATE, &create).unwrap();
        let created = read_response::<DrmGemCreateWire>(&mut scheme, card);
        let gem_handle = created.handle;

        let addfb = DrmAddFbWire {
            width: 64,
            height: 64,
            pitch: 256,
            bpp: 32,
            depth: 24,
            handle: gem_handle,
            fb_id: 0,
        };
        write_ioctl(&mut scheme, card, DRM_IOCTL_MODE_ADDFB, &addfb).unwrap();
        let fb_resp = read_response::<DrmAddFbWire>(&mut scheme, card);

        if let Some(handle) = scheme.handles.get_mut(&card) {
            handle.owned_gems.retain(|&h| h != gem_handle);
        }

        assert!(
            !scheme.gem_can_close(gem_handle),
            "GEM backing a framebuffer should not be closeable"
        );
        assert!(scheme.fb_registry.contains_key(&fb_resp.fb_id));
    }

    #[test]
    fn gem_can_close_returns_false_when_gem_is_mapped() {
        let mut scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));
        let card = open_card(&mut scheme);

        let create = DrmGemCreateWire {
            size: 4096,
            ..DrmGemCreateWire::default()
        };
        write_ioctl(&mut scheme, card, DRM_IOCTL_GEM_CREATE, &create).unwrap();
        let created = read_response::<DrmGemCreateWire>(&mut scheme, card);
        let gem_handle = created.handle;

        scheme.pin_mapped_gem(card, gem_handle).unwrap();

        if let Some(handle) = scheme.handles.get_mut(&card) {
            handle.owned_gems.retain(|&h| h != gem_handle);
        }

        assert!(
            !scheme.gem_can_close(gem_handle),
            "mapped GEM should not be closeable"
        );
    }

    #[test]
    fn gem_can_close_returns_true_when_unreferenced() {
        let mut scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));
        let card = open_card(&mut scheme);

        let create = DrmGemCreateWire {
            size: 4096,
            ..DrmGemCreateWire::default()
        };
        write_ioctl(&mut scheme, card, DRM_IOCTL_GEM_CREATE, &create).unwrap();
        let created = read_response::<DrmGemCreateWire>(&mut scheme, card);
        let gem_handle = created.handle;

        if let Some(handle) = scheme.handles.get_mut(&card) {
            handle.owned_gems.retain(|&h| h != gem_handle);
        }

        assert!(
            scheme.gem_can_close(gem_handle),
            "unreferenced, unmapped GEM with no FB or export refs should be closeable"
        );
    }

    #[test]
    fn allocate_handle_returns_sequential_ids() {
        let mut scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));

        let id_a = scheme.allocate_handle(NodeKind::Card);
        let id_b = scheme.allocate_handle(NodeKind::Card);

        assert!(
            id_b > id_a,
            "second allocated handle ID ({id_b}) should be greater than first ({id_a})"
        );
    }

    #[test]
    fn is_fb_active_returns_false_for_unknown_fb() {
        let scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));

        assert!(
            !scheme.is_fb_active(12345),
            "unknown fb_id should not be active"
        );
    }

    #[test]
    fn is_fb_active_returns_true_for_active_crtc_fb() {
        let mut scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));
        let card = open_card(&mut scheme);

        let create = DrmGemCreateWire {
            size: 640 * 480 * 4,
            ..DrmGemCreateWire::default()
        };
        write_ioctl(&mut scheme, card, DRM_IOCTL_GEM_CREATE, &create).unwrap();
        let created = read_response::<DrmGemCreateWire>(&mut scheme, card);
        let gem_handle = created.handle;

        let addfb = DrmAddFbWire {
            width: 640,
            height: 480,
            pitch: 2560,
            bpp: 32,
            depth: 24,
            handle: gem_handle,
            fb_id: 0,
        };
        write_ioctl(&mut scheme, card, DRM_IOCTL_MODE_ADDFB, &addfb).unwrap();
        let fb_resp = read_response::<DrmAddFbWire>(&mut scheme, card);
        let fb_id = fb_resp.fb_id;

        let mode = DrmModeWire {
            clock: 25200,
            hdisplay: 640,
            hsync_start: 656,
            hsync_end: 752,
            htotal: 800,
            vdisplay: 480,
            vsync_start: 490,
            vsync_end: 492,
            vtotal: 525,
            vrefresh: 60,
            ..DrmModeWire::default()
        };
        let setcrtc = DrmSetCrtcWire {
            crtc_id: 0,
            fb_handle: fb_id,
            connector_count: 0,
            connectors: [0; 8],
            mode,
        };
        write_ioctl(&mut scheme, card, DRM_IOCTL_MODE_SETCRTC, &setcrtc).unwrap();

        assert!(
            scheme.is_fb_active(fb_id),
            "FB programmed on a CRTC should be active"
        );
    }

    #[test]
    fn validate_gem_create_size_rejects_zero() {
        let scheme = DrmScheme::new(Arc::new(FakeDriver::new(false)));

        let err = scheme
            .validate_gem_create_size(0, "test-zero-size")
            .unwrap_err();

        assert_eq!(err.errno, EINVAL, "zero-sized GEM creation should return EINVAL");
    }
}
