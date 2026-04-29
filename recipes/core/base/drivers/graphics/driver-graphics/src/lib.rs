#![feature(macro_metavar_expr)]

use std::collections::HashMap;
use std::fmt::Debug;
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::BorrowedFd;
use std::sync::{Arc, Mutex};
use std::{cmp, mem};

use drm_fourcc::DrmFourcc;
use drm_sys::{
    drm_mode_property_enum, DRM_MODE_CURSOR_BO, DRM_MODE_CURSOR_MOVE, DRM_MODE_PROP_ATOMIC,
    DRM_MODE_PROP_BITMASK, DRM_MODE_PROP_BLOB, DRM_MODE_PROP_ENUM, DRM_MODE_PROP_IMMUTABLE,
    DRM_MODE_PROP_OBJECT, DRM_MODE_PROP_RANGE, DRM_MODE_PROP_SIGNED_RANGE,
};
use inputd::{DisplayHandle, VtEventKind};
use libredox::Fd;
use redox_scheme::scheme::{register_scheme_inner, SchemeState, SchemeSync};
use redox_scheme::{CallerCtx, OpenResult, RequestKind, SignalBehavior, Socket};
use scheme_utils::{FpathWriter, HandleMap};
use syscall::schemev2::NewFdFlags;
use syscall::{Error, MapFlags, Result, EACCES, EAGAIN, EINVAL, ENOENT, EOPNOTSUPP};

use crate::kms::connector::{KmsConnectorDriver, KmsConnectorState};
use crate::kms::objects::{self, KmsCrtc, KmsCrtcDriver, KmsCrtcState, KmsObjectId, KmsObjects};
use crate::kms::properties::KmsPropertyKind;

pub mod kms;

#[derive(Debug, Copy, Clone)]
#[repr(C, packed)]
pub struct Damage {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Damage {
    fn merge(self, other: Self) -> Self {
        if self.width == 0 || self.height == 0 {
            return other;
        }

        if other.width == 0 || other.height == 0 {
            return self;
        }

        let x = cmp::min(self.x, other.x);
        let y = cmp::min(self.y, other.y);
        let x2 = cmp::max(self.x + self.width, other.x + other.width);
        let y2 = cmp::max(self.y + self.height, other.y + other.height);

        Damage {
            x,
            y,
            width: x2 - x,
            height: y2 - y,
        }
    }

    #[must_use]
    pub fn clip(mut self, width: u32, height: u32) -> Self {
        // Clip damage
        let x2 = self.x + self.width;
        self.x = cmp::min(self.x, width);
        if x2 > width {
            self.width = width - self.x;
        }

        let y2 = self.y + self.height;
        self.y = cmp::min(self.y, height);
        if y2 > height {
            self.height = height - self.y;
        }
        self
    }
}

pub trait GraphicsAdapter: Sized + Debug {
    type Connector: KmsConnectorDriver;
    type Crtc: KmsCrtcDriver;

    type Buffer: Buffer;
    type Framebuffer: Framebuffer;

    fn name(&self) -> &'static [u8];
    fn desc(&self) -> &'static [u8];

    fn init(&mut self, objects: &mut KmsObjects<Self>);

    fn get_cap(&self, cap: u32) -> Result<u64>;
    fn set_client_cap(&self, cap: u32, value: u64) -> Result<()>;

    fn probe_connector(&mut self, objects: &mut KmsObjects<Self>, id: KmsObjectId);

    fn create_dumb_buffer(&mut self, width: u32, height: u32) -> (Self::Buffer, u32);
    fn map_dumb_buffer(&mut self, buffer: &Self::Buffer) -> *mut u8;

    fn create_framebuffer(&mut self, buffer: &Self::Buffer) -> Self::Framebuffer;

    fn set_crtc(
        &mut self,
        objects: &KmsObjects<Self>,
        crtc: &Mutex<KmsCrtc<Self>>,
        new_state: KmsCrtcState<Self>,
        damage: Damage,
    ) -> syscall::Result<()>;

    fn hw_cursor_size(&self) -> Option<(u32, u32)>;
    fn handle_cursor(&mut self, cursor: &CursorPlane<Self::Buffer>, dirty_fb: bool);
}

pub trait Buffer: Debug {
    fn size(&self) -> usize;
}

pub trait Framebuffer: Debug {}

impl Framebuffer for () {}

pub struct CursorPlane<C: Buffer> {
    pub x: i32,
    pub y: i32,
    pub hot_x: i32,
    pub hot_y: i32,
    pub buffer: Option<Arc<C>>,
}

pub struct GraphicsScheme<T: GraphicsAdapter> {
    inner: GraphicsSchemeInner<T>,
    inputd_handle: DisplayHandle,
    state: SchemeState,
}

impl<T: GraphicsAdapter> GraphicsScheme<T> {
    pub fn new(mut adapter: T, scheme_name: String, early: bool) -> Self {
        assert!(scheme_name.starts_with("display"));
        let socket = Socket::nonblock().expect("failed to create graphics scheme");

        let disable_graphical_debug = Some(
            File::open("/scheme/debug/disable-graphical-debug")
                .expect("vesad: Failed to open /scheme/debug/disable-graphical-debug"),
        );

        let mut objects = KmsObjects::new();
        adapter.init(&mut objects);
        for connector_id in objects.connector_ids().to_vec() {
            adapter.probe_connector(&mut objects, connector_id)
        }

        let mut inner = GraphicsSchemeInner {
            adapter,
            scheme_name,
            disable_graphical_debug,
            socket,
            objects,
            handles: HandleMap::new(),
            active_vt: 0,
            vts: HashMap::new(),
        };

        let cap_id = inner.scheme_root().expect("failed to get this scheme root");
        register_scheme_inner(&inner.socket, &inner.scheme_name, cap_id)
            .expect("failed to register graphics scheme root");

        let display_handle = if early {
            DisplayHandle::new_early(&inner.scheme_name).unwrap()
        } else {
            DisplayHandle::new(&inner.scheme_name).unwrap()
        };

        Self {
            inner,
            inputd_handle: display_handle,
            state: SchemeState::new(),
        }
    }

    pub fn event_handle(&self) -> &Fd {
        self.inner.socket.inner()
    }

    pub fn inputd_event_handle(&self) -> BorrowedFd<'_> {
        self.inputd_handle.inner()
    }

    pub fn adapter(&self) -> &T {
        &self.inner.adapter
    }

    pub fn adapter_mut(&mut self) -> &mut T {
        &mut self.inner.adapter
    }

    pub fn kms_objects(&self) -> &KmsObjects<T> {
        &self.inner.objects
    }

    pub fn kms_objects_mut(&mut self) -> &mut KmsObjects<T> {
        &mut self.inner.objects
    }

    pub fn adapter_and_kms_objects_mut(&mut self) -> (&mut T, &mut KmsObjects<T>) {
        (&mut self.inner.adapter, &mut self.inner.objects)
    }

    pub fn handle_vt_events(&mut self) {
        while let Some(vt_event) = self
            .inputd_handle
            .read_vt_event()
            .expect("driver-graphics: failed to read display handle")
        {
            match vt_event.kind {
                VtEventKind::Activate => self.inner.activate_vt(vt_event.vt),
            }
        }
    }

    pub fn notify_displays_changed(&mut self) {
        // FIXME notify clients
    }

    /// Process new scheme requests.
    ///
    /// This needs to be called each time there is a new event on the scheme
    /// file.
    pub fn tick(&mut self) -> io::Result<()> {
        loop {
            let request = match self.inner.socket.next_request(SignalBehavior::Restart) {
                Ok(Some(request)) => request,
                Ok(None) => {
                    // Scheme likely got unmounted
                    std::process::exit(0);
                }
                Err(err) if err.errno == EAGAIN => break,
                Err(err) => panic!("driver-graphics: failed to read display scheme: {err}"),
            };

            match request.kind() {
                RequestKind::Call(call) => {
                    let response = call.handle_sync(&mut self.inner, &mut self.state);
                    self.inner
                        .socket
                        .write_response(response, SignalBehavior::Restart)
                        .expect("driver-graphics: failed to write response");
                }
                RequestKind::OnClose { id } => {
                    self.inner.on_close(id);
                }
                _ => (),
            }
        }

        Ok(())
    }
}

struct GraphicsSchemeInner<T: GraphicsAdapter> {
    adapter: T,

    scheme_name: String,
    disable_graphical_debug: Option<File>,
    socket: Socket,
    objects: KmsObjects<T>,
    handles: HandleMap<Handle<T>>,

    active_vt: usize,
    vts: HashMap<usize, VtState<T>>,
}

struct VtState<T: GraphicsAdapter> {
    connector_state: Vec<KmsConnectorState<T>>,
    crtc_state: Vec<KmsCrtcState<T>>,
    cursor_plane: CursorPlane<T::Buffer>,
}

enum Handle<T: GraphicsAdapter> {
    V2 {
        vt: usize,
        next_id: u32,
        buffers: HashMap<u32, Arc<T::Buffer>>,
    },
    SchemeRoot,
}

impl<T: GraphicsAdapter> GraphicsSchemeInner<T> {
    fn get_or_create_vt<'a>(
        objects: &KmsObjects<T>,
        vts: &'a mut HashMap<usize, VtState<T>>,
        vt: usize,
    ) -> &'a mut VtState<T> {
        vts.entry(vt).or_insert_with(|| VtState {
            connector_state: objects
                .connectors()
                .map(|connector| connector.lock().unwrap().state.clone())
                .collect(),
            crtc_state: objects
                .crtcs()
                .map(|crtc| crtc.lock().unwrap().state.clone())
                .collect(),
            cursor_plane: CursorPlane {
                x: 0,
                y: 0,
                hot_x: 0,
                hot_y: 0,
                buffer: None,
            },
        })
    }

    fn activate_vt(&mut self, vt: usize) {
        log::info!("activate {}", vt);

        // Disable the kernel graphical debug writing once switching vt's for the
        // first time. This way the kernel graphical debug remains enabled if the
        // userspace logging infrastructure doesn't start up because for example a
        // kernel panic happened prior to it starting up or logd crashed.
        if let Some(mut disable_graphical_debug) = self.disable_graphical_debug.take() {
            let _ = disable_graphical_debug.write(&[1]);
        }

        self.active_vt = vt;

        let vt_state = GraphicsSchemeInner::get_or_create_vt(&self.objects, &mut self.vts, vt);

        for (connector_idx, connector_state) in vt_state.connector_state.iter().enumerate() {
            let connector_id = self.objects.connector_ids()[connector_idx];
            let mut connector = self
                .objects
                .get_connector(connector_id)
                .unwrap()
                .lock()
                .unwrap();
            connector.state = connector_state.clone();
        }

        for (crtc_idx, crtc_state) in vt_state.crtc_state.iter().enumerate() {
            let crtc_id = self.objects.crtc_ids()[crtc_idx];
            let crtc = self.objects.get_crtc(crtc_id).unwrap();
            let connector_id = self.objects.connector_ids()[crtc_idx];

            let fb = crtc_state.fb_id.map(|fb_id| {
                self.objects
                    .get_framebuffer(fb_id)
                    .expect("removed framebuffers should be unset")
            });

            self.adapter
                .set_crtc(
                    &self.objects,
                    crtc,
                    crtc_state.clone(),
                    Damage {
                        x: 0,
                        y: 0,
                        width: fb.map_or(0, |fb| fb.width),
                        height: fb.map_or(0, |fb| fb.height),
                    },
                )
                .unwrap();

            self.objects
                .get_connector(connector_id)
                .unwrap()
                .lock()
                .unwrap()
                .state
                .crtc_id = crtc_id;
        }

        if self.adapter.hw_cursor_size().is_some() {
            self.adapter.handle_cursor(&vt_state.cursor_plane, true);
        }
    }
}

const MAP_FAKE_OFFSET_MULTIPLIER: usize = 0x10_000_000;

impl<T: GraphicsAdapter> SchemeSync for GraphicsSchemeInner<T> {
    fn scheme_root(&mut self) -> Result<usize> {
        Ok(self.handles.insert(Handle::SchemeRoot))
    }
    fn openat(
        &mut self,
        dirfd: usize,
        path: &str,
        _flags: usize,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<OpenResult> {
        if !matches!(self.handles.get(dirfd)?, Handle::SchemeRoot) {
            return Err(Error::new(EACCES));
        }
        if path.is_empty() {
            return Err(Error::new(EINVAL));
        }

        let handle = if path.starts_with("v") {
            if !path.starts_with("v2/") {
                return Err(Error::new(ENOENT));
            }
            let vt = path["v2/".len()..]
                .parse::<usize>()
                .map_err(|_| Error::new(EINVAL))?;

            // Ensure the VT exists such that the rest of the methods can freely access it.
            Self::get_or_create_vt(&self.objects, &mut self.vts, vt);

            Handle::V2 {
                vt,
                next_id: 0,
                buffers: HashMap::new(),
            }
        } else {
            return Err(Error::new(EINVAL));
        };
        let id = self.handles.insert(handle);
        Ok(OpenResult::ThisScheme {
            number: id,
            flags: NewFdFlags::empty(),
        })
    }

    fn fpath(&mut self, id: usize, buf: &mut [u8], _ctx: &CallerCtx) -> syscall::Result<usize> {
        FpathWriter::with(buf, &self.scheme_name, |w| {
            match self.handles.get(id)? {
                Handle::V2 {
                    vt,
                    next_id: _,
                    buffers: _,
                } => write!(w, "v2/{vt}").unwrap(),
                Handle::SchemeRoot => return Err(Error::new(EOPNOTSUPP)),
            };
            Ok(())
        })
    }

    fn call(
        &mut self,
        id: usize,
        payload: &mut [u8],
        metadata: &[u64],
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        use redox_ioctl::drm as ipc;

        fn id_index(id: u32) -> u32 {
            id & 0xFF
        }

        fn plane_id(i: u32) -> u32 {
            id_index(i) | (1 << 13)
        }

        match self.handles.get_mut(id)? {
            Handle::SchemeRoot => return Err(Error::new(EOPNOTSUPP)),
            Handle::V2 {
                vt,
                next_id,
                buffers,
            } => match metadata[0] {
                ipc::VERSION => ipc::DrmVersion::with(payload, |mut data| {
                    data.set_version_major(1);
                    data.set_version_minor(4);
                    data.set_version_patchlevel(0);

                    data.set_name(unsafe { mem::transmute(self.adapter.name()) });
                    data.set_date(unsafe { mem::transmute(&b"0"[..]) });
                    data.set_desc(unsafe { mem::transmute(self.adapter.desc()) });

                    Ok(0)
                }),
                ipc::GET_CAP => ipc::DrmGetCap::with(payload, |mut data| {
                    data.set_value(
                        self.adapter.get_cap(
                            data.capability()
                                .try_into()
                                .map_err(|_| syscall::Error::new(EINVAL))?,
                        )?,
                    );
                    Ok(0)
                }),
                ipc::SET_CLIENT_CAP => ipc::DrmSetClientCap::with(payload, |data| {
                    self.adapter.set_client_cap(
                        data.capability()
                            .try_into()
                            .map_err(|_| syscall::Error::new(EINVAL))?,
                        data.value(),
                    )?;
                    Ok(0)
                }),
                ipc::MODE_CARD_RES => ipc::DrmModeCardRes::with(payload, |mut data| {
                    let conn_ids = self
                        .objects
                        .connector_ids()
                        .iter()
                        .map(|id| id.0)
                        .collect::<Vec<_>>();
                    let crtc_ids = self
                        .objects
                        .crtc_ids()
                        .iter()
                        .map(|id| id.0)
                        .collect::<Vec<_>>();
                    let enc_ids = self
                        .objects
                        .encoder_ids()
                        .iter()
                        .map(|id| id.0)
                        .collect::<Vec<_>>();
                    let fb_ids = self
                        .objects
                        .fb_ids()
                        .iter()
                        .map(|id| id.0)
                        .collect::<Vec<_>>();
                    data.set_fb_id_ptr(&fb_ids);
                    data.set_crtc_id_ptr(&crtc_ids);
                    data.set_connector_id_ptr(&conn_ids);
                    data.set_encoder_id_ptr(&enc_ids);
                    data.set_min_width(0);
                    data.set_max_width(16384);
                    data.set_min_height(0);
                    data.set_max_height(16384);
                    Ok(0)
                }),
                ipc::MODE_GET_CRTC => ipc::DrmModeCrtc::with(payload, |mut data| {
                    let crtc = self
                        .objects
                        .get_crtc(KmsObjectId(data.crtc_id()))?
                        .lock()
                        .unwrap();
                    // Don't touch set_connectors, that is only used by MODE_SET_CRTC
                    data.set_fb_id(crtc.state.fb_id.unwrap_or(KmsObjectId::INVALID).0);
                    // FIXME fill x and y with the data from the primary plane
                    data.set_x(0);
                    data.set_y(0);
                    data.set_gamma_size(crtc.gamma_size);
                    if let Some(mode) = crtc.state.mode {
                        data.set_mode_valid(1);
                        data.set_mode(mode);
                    } else {
                        data.set_mode_valid(0);
                        data.set_mode(Default::default());
                    }
                    Ok(0)
                }),
                ipc::MODE_SET_CRTC => ipc::DrmModeCrtc::with(payload, |data| {
                    let crtc = self.objects.get_crtc(KmsObjectId(data.crtc_id()))?;
                    let connector_ids: Vec<KmsObjectId> = data
                        .set_connectors_ptr()
                        .iter()
                        .take(data.count_connectors() as usize)
                        .map(|&id| KmsObjectId(id))
                        .collect();
                    let fb_id = if data.fb_id() != 0 {
                        Some(KmsObjectId(data.fb_id()))
                    } else {
                        None
                    };
                    let mode = if data.mode_valid() != 0 {
                        Some(data.mode())
                    } else {
                        None
                    };
                    let mut new_state = crtc.lock().unwrap().state.clone();
                    new_state.fb_id = fb_id;
                    new_state.mode = mode;
                    if *vt == self.active_vt {
                        self.adapter.set_crtc(
                            &self.objects,
                            crtc,
                            new_state.clone(),
                            Damage {
                                x: data.x(),
                                y: data.y(),
                                width: mode.map_or(0, |m| m.hdisplay as u32),
                                height: mode.map_or(0, |m| m.vdisplay as u32),
                            },
                        )?;

                        for connector in connector_ids {
                            self.objects
                                .get_connector(connector)?
                                .lock()
                                .unwrap()
                                .state
                                .crtc_id = KmsObjectId(data.crtc_id());
                        }
                    }
                    self.vts.get_mut(vt).unwrap().crtc_state
                        [crtc.lock().unwrap().crtc_index as usize] = new_state;
                    Ok(0)
                }),
                ipc::MODE_CURSOR => ipc::DrmModeCursor::with(payload, |data| {
                    let vt_state = self.vts.get_mut(vt).unwrap();

                    let cursor_plane = &mut vt_state.cursor_plane;

                    let update_buffer = data.flags() & DRM_MODE_CURSOR_BO != 0;
                    if update_buffer {
                        cursor_plane.buffer = if data.handle() == 0 {
                            None
                        } else if let Some(buffer) = buffers.get(&data.handle()) {
                            Some(buffer.clone())
                        } else {
                            return Err(Error::new(EINVAL));
                        };
                    }

                    if data.flags() & DRM_MODE_CURSOR_MOVE != 0 {
                        cursor_plane.x = data.x();
                        cursor_plane.y = data.y();
                    }

                    self.adapter.handle_cursor(cursor_plane, update_buffer);

                    Ok(0)
                }),
                ipc::MODE_GET_ENCODER => ipc::DrmModeGetEncoder::with(payload, |mut data| {
                    let encoder = self.objects.get_encoder(KmsObjectId(data.encoder_id()))?;
                    data.set_crtc_id(encoder.crtc_id.0);
                    data.set_possible_crtcs(encoder.possible_crtcs);
                    data.set_possible_clones(encoder.possible_clones);
                    Ok(0)
                }),
                ipc::MODE_GET_CONNECTOR => ipc::DrmModeGetConnector::with(payload, |mut data| {
                    if data.count_modes() == 0 {
                        self.adapter
                            .probe_connector(&mut self.objects, KmsObjectId(data.connector_id()));
                    }
                    let connector = self
                        .objects
                        .get_connector(KmsObjectId(data.connector_id()))?
                        .lock()
                        .unwrap();
                    data.set_encoders_ptr(&[connector.encoder_id.0]);
                    data.set_modes_ptr(&connector.modes);
                    data.set_connector_type(data.connector_type());
                    data.set_connector_type_id(data.connector_type_id());
                    data.set_connection(connector.connection as u32);
                    data.set_mm_width(connector.mm_width);
                    data.set_mm_height(connector.mm_width);
                    data.set_subpixel(connector.subpixel as u32);
                    drop(connector);
                    let (props, prop_vals) = self
                        .objects
                        .get_object_properties_data(KmsObjectId(data.connector_id()))?;
                    data.set_props_ptr(&props);
                    data.set_prop_values_ptr(&prop_vals);
                    Ok(0)
                }),
                ipc::MODE_GET_PROPERTY => ipc::DrmModeGetProperty::with(payload, |mut data| {
                    let property = self.objects.get_property(KmsObjectId(data.prop_id()))?;
                    data.set_name(property.name.0);
                    let mut flags = 0;
                    if property.immutable {
                        flags |= DRM_MODE_PROP_IMMUTABLE;
                    }
                    if property.atomic {
                        flags |= DRM_MODE_PROP_ATOMIC;
                    }
                    match &property.kind {
                        &KmsPropertyKind::Range(start, end) => {
                            data.set_flags(flags | DRM_MODE_PROP_RANGE);
                            data.set_values_ptr(&[start, end]);
                            data.set_enum_blob_ptr(&[]);
                        }
                        KmsPropertyKind::Enum(variants) => {
                            data.set_flags(flags | DRM_MODE_PROP_ENUM);
                            data.set_values_ptr(
                                &variants.iter().map(|&(_, value)| value).collect::<Vec<_>>(),
                            );
                            data.set_enum_blob_ptr(
                                &variants
                                    .iter()
                                    .map(|&(name, value)| drm_mode_property_enum {
                                        name: name.0,
                                        value,
                                    })
                                    .collect::<Vec<_>>(),
                            );
                        }
                        KmsPropertyKind::Blob => {
                            data.set_flags(flags | DRM_MODE_PROP_BLOB);
                            data.set_values_ptr(&[]);
                            data.set_enum_blob_ptr(&[]);
                        }
                        KmsPropertyKind::Bitmask(bitmask_flags) => {
                            data.set_flags(flags | DRM_MODE_PROP_BITMASK);
                            data.set_values_ptr(
                                &bitmask_flags
                                    .iter()
                                    .map(|&(_, value)| value)
                                    .collect::<Vec<_>>(),
                            );
                            data.set_enum_blob_ptr(
                                &bitmask_flags
                                    .iter()
                                    .map(|&(name, value)| drm_mode_property_enum {
                                        name: name.0,
                                        value,
                                    })
                                    .collect::<Vec<_>>(),
                            );
                        }
                        KmsPropertyKind::Object { type_ } => {
                            data.set_flags(flags | DRM_MODE_PROP_OBJECT);
                            data.set_values_ptr(&[u64::from(*type_)]);
                            data.set_enum_blob_ptr(&[]);
                        }
                        &KmsPropertyKind::SignedRange(start, end) => {
                            data.set_flags(flags | DRM_MODE_PROP_SIGNED_RANGE);
                            data.set_values_ptr(&[start as u64, end as u64]);
                            data.set_enum_blob_ptr(&[]);
                        }
                    }
                    Ok(0)
                }),
                ipc::MODE_GET_PROP_BLOB => ipc::DrmModeGetBlob::with(payload, |mut data| {
                    let blob = self.objects.get_blob(KmsObjectId(data.blob_id()))?;
                    data.set_data(&blob);
                    Ok(0)
                }),
                ipc::MODE_GET_FB => ipc::DrmModeFbCmd::with(payload, |mut data| {
                    let fb = self.objects.get_framebuffer(KmsObjectId(data.fb_id()))?;

                    *next_id += 1;
                    buffers.insert(*next_id, fb.buffer.clone());

                    data.set_width(fb.width);
                    data.set_height(fb.height);
                    data.set_pitch(fb.pitch);
                    data.set_bpp(fb.bpp);
                    data.set_depth(fb.depth);
                    data.set_handle(*next_id);
                    Ok(0)
                }),
                ipc::MODE_ADD_FB => ipc::DrmModeFbCmd::with(payload, |mut data| {
                    let buffer = buffers.get(&data.handle()).ok_or(Error::new(EINVAL))?;

                    let fb = self.adapter.create_framebuffer(buffer);

                    let id = self.objects.add_framebuffer(objects::KmsFramebuffer {
                        width: data.width(),
                        height: data.height(),
                        pitch: data.pitch(),
                        bpp: data.bpp(),
                        depth: data.depth(),
                        buffer: buffer.clone(),
                        driver_data: fb,
                    });

                    data.set_fb_id(id.0);

                    Ok(0)
                }),
                ipc::MODE_RM_FB => ipc::StandinForUint::with(payload, |data| {
                    let fb_id = KmsObjectId(data.inner());
                    self.objects.remove_framebuffer(fb_id)?;

                    // Disable planes that use this framebuffer.
                    for (vt, vt_data) in &mut self.vts {
                        for (crtc_idx, crtc_state) in vt_data.crtc_state.iter_mut().enumerate() {
                            if crtc_state.fb_id != Some(fb_id) {
                                continue;
                            }
                            crtc_state.fb_id = None;

                            if *vt != self.active_vt {
                                continue;
                            }
                            let crtc = self.objects.crtcs().nth(crtc_idx).unwrap();
                            self.adapter
                                .set_crtc(
                                    &self.objects,
                                    crtc,
                                    crtc_state.clone(),
                                    Damage {
                                        x: 0,
                                        y: 0,
                                        width: 0,
                                        height: 0,
                                    },
                                )
                                .unwrap();
                        }
                    }

                    Ok(0)
                }),
                ipc::MODE_DIRTYFB => ipc::DrmModeFbDirtyCmd::with(payload, |data| {
                    let fb = self.objects.get_framebuffer(KmsObjectId(data.fb_id()))?;

                    let damage = data
                        .clips_ptr()
                        .iter()
                        .map(|rect| Damage {
                            x: u32::from(rect.x1),
                            y: u32::from(rect.y1),
                            width: u32::from(rect.x2 - rect.x1),
                            height: u32::from(rect.y2 - rect.y1),
                        })
                        .reduce(Damage::merge)
                        .unwrap_or(Damage {
                            x: 0,
                            y: 0,
                            width: fb.width,
                            height: fb.height,
                        });

                    if *vt == self.active_vt {
                        for crtc in self.objects.crtcs() {
                            let state = crtc.lock().unwrap().state.clone();
                            if state.fb_id == Some(KmsObjectId(data.fb_id())) {
                                self.adapter.set_crtc(&self.objects, crtc, state, damage)?;
                            }
                        }
                    }

                    Ok(0)
                }),
                ipc::MODE_CREATE_DUMB => ipc::DrmModeCreateDumb::with(payload, |mut data| {
                    if data.bpp() != 32 || data.flags() != 0 {
                        return Err(Error::new(EINVAL));
                    }

                    let (buffer, pitch) =
                        self.adapter.create_dumb_buffer(data.width(), data.height());

                    data.set_pitch(pitch);
                    data.set_size(buffer.size() as u64);

                    *next_id += 1;
                    buffers.insert(*next_id, Arc::new(buffer));
                    data.set_handle(*next_id as u32);
                    Ok(0)
                }),
                ipc::MODE_MAP_DUMB => ipc::DrmModeMapDumb::with(payload, |mut data| {
                    if data.offset() != 0 {
                        return Err(Error::new(EINVAL));
                    }

                    let buffer_id = data.handle();

                    if !buffers.contains_key(&buffer_id) {
                        return Err(Error::new(EINVAL));
                    }

                    // FIXME use a better scheme for creating map offsets
                    assert!(buffers[&buffer_id].size() < MAP_FAKE_OFFSET_MULTIPLIER);

                    data.set_offset((buffer_id as usize * MAP_FAKE_OFFSET_MULTIPLIER) as u64);

                    Ok(0)
                }),
                ipc::MODE_DESTROY_DUMB => ipc::DrmModeDestroyDumb::with(payload, |data| {
                    if buffers.remove(&data.handle()).is_none() {
                        return Err(Error::new(ENOENT));
                    }
                    Ok(0)
                }),
                ipc::MODE_GET_PLANE_RES => ipc::DrmModeGetPlaneRes::with(payload, |mut data| {
                    let count = self.objects.crtc_ids().len();
                    let mut ids = Vec::with_capacity(count);
                    for i in 0..(count as u32) {
                        ids.push(plane_id(i));
                    }
                    data.set_plane_id_ptr(&ids);
                    Ok(0)
                }),
                ipc::MODE_GET_PLANE => ipc::DrmModeGetPlane::with(payload, |mut data| {
                    let i = id_index(data.plane_id());
                    let crtc_id = self.objects.crtc_ids()[i as usize];
                    let crtc = self.objects.get_crtc(crtc_id).unwrap();
                    data.set_crtc_id(crtc_id.0);
                    data.set_fb_id(
                        crtc.lock()
                            .unwrap()
                            .state
                            .fb_id
                            .unwrap_or(KmsObjectId::INVALID)
                            .0,
                    );
                    data.set_possible_crtcs(1 << i);
                    data.set_format_type_ptr(&[DrmFourcc::Argb8888 as u32]);
                    Ok(0)
                }),
                ipc::MODE_OBJ_GET_PROPERTIES => {
                    ipc::DrmModeObjGetProperties::with(payload, |mut data| {
                        // FIXME remove once all drm objects are materialized in self.objects
                        if data.obj_id() >= 1 << 11 {
                            data.set_props_ptr(&[]);
                            data.set_prop_values_ptr(&[]);
                            return Ok(0);
                        }

                        let (props, prop_vals) = self
                            .objects
                            .get_object_properties_data(KmsObjectId(data.obj_id()))?;
                        data.set_props_ptr(&props);
                        data.set_prop_values_ptr(&prop_vals);
                        data.set_obj_type(self.objects.object_type(KmsObjectId(data.obj_id()))?);
                        Ok(0)
                    })
                }
                ipc::MODE_CURSOR2 => ipc::DrmModeCursor2::with(payload, |data| {
                    let vt_state = self.vts.get_mut(vt).unwrap();

                    let cursor_plane = &mut vt_state.cursor_plane;

                    let update_buffer = data.flags() & DRM_MODE_CURSOR_BO != 0;
                    if update_buffer {
                        cursor_plane.buffer = if data.handle() == 0 {
                            None
                        } else if let Some(buffer) = buffers.get(&data.handle()) {
                            Some(buffer.clone())
                        } else {
                            return Err(Error::new(EINVAL));
                        };
                        cursor_plane.hot_x = data.hot_x();
                        cursor_plane.hot_y = data.hot_y();
                    }

                    if data.flags() & DRM_MODE_CURSOR_MOVE != 0 {
                        cursor_plane.x = data.x();
                        cursor_plane.y = data.y();
                    }

                    self.adapter.handle_cursor(cursor_plane, update_buffer);

                    Ok(0)
                }),
                ipc::MODE_GET_FB2 => ipc::DrmModeFbCmd2::with(payload, |mut data| {
                    let fb = self.objects.get_framebuffer(KmsObjectId(data.fb_id()))?;

                    *next_id += 1;
                    buffers.insert(*next_id, fb.buffer.clone());

                    data.set_width(fb.width);
                    data.set_height(fb.height);
                    data.set_pixel_format(DrmFourcc::Argb8888 as u32);
                    data.set_handles([*next_id, 0, 0, 0]);
                    data.set_pitches([fb.width * 4, 0, 0, 0]);
                    data.set_offsets([0; 4]);
                    data.set_modifier([0; 4]);
                    Ok(0)
                }),
                _ => return Err(Error::new(EINVAL)),
            },
        }
    }

    fn mmap_prep(
        &mut self,
        id: usize,
        offset: u64,
        _size: usize,
        _flags: MapFlags,
        _ctx: &CallerCtx,
    ) -> syscall::Result<usize> {
        // log::trace!("KSMSG MMAP {} {:?} {} {}", id, _flags, _offset, _size);
        let (framebuffer, offset) = match self.handles.get(id)? {
            Handle::V2 {
                vt: _,
                next_id: _,
                buffers,
            } => (
                buffers
                    .get(&((offset as usize / MAP_FAKE_OFFSET_MULTIPLIER) as u32))
                    .ok_or(Error::new(EINVAL))
                    .unwrap(),
                offset & (MAP_FAKE_OFFSET_MULTIPLIER as u64 - 1),
            ),
            Handle::SchemeRoot => return Err(Error::new(EOPNOTSUPP)),
        };
        let ptr = T::map_dumb_buffer(&mut self.adapter, framebuffer);
        Ok(unsafe { ptr.add(offset as usize) } as usize)
    }

    fn on_close(&mut self, id: usize) {
        self.handles.remove(id);
    }
}
