use std::ffi::c_char;
use std::fmt::Debug;
use std::mem;

use drm_sys::{
    DRM_MODE_DPMS_OFF, DRM_MODE_DPMS_ON, DRM_MODE_DPMS_STANDBY, DRM_MODE_DPMS_SUSPEND,
    DRM_MODE_OBJECT_CRTC, DRM_MODE_OBJECT_FB, DRM_PLANE_TYPE_CURSOR, DRM_PLANE_TYPE_OVERLAY,
    DRM_PLANE_TYPE_PRIMARY, DRM_PROP_NAME_LEN,
};
use syscall::{Error, Result, EINVAL};

use crate::kms::objects::{KmsObject, KmsObjectId, KmsObjects};
use crate::GraphicsAdapter;

impl<T: GraphicsAdapter> KmsObjects<T> {
    pub fn add_property(
        &mut self,
        name: &str,
        immutable: bool,
        atomic: bool,
        kind: KmsPropertyKind,
    ) -> KmsObjectId {
        match &kind {
            KmsPropertyKind::Range(start, end) => assert!(start < end),
            KmsPropertyKind::Enum(_variants) => {
                // FIXME check duplicate variant numbers
            }
            KmsPropertyKind::Blob => {}
            KmsPropertyKind::Bitmask(_bitmask_flags) => {
                // FIXME check overlapping flag numbers
            }
            KmsPropertyKind::Object { type_: _ } => {}
            KmsPropertyKind::SignedRange(start, end) => assert!(start < end),
        }

        let mut name_bytes = [0; DRM_PROP_NAME_LEN as usize];
        for (to, &from) in name_bytes.iter_mut().zip(name.as_bytes()) {
            *to = from as c_char;
        }

        self.add(KmsProperty {
            name: KmsPropertyName::new("Property name", name),
            immutable,
            atomic,
            kind,
        })
    }

    pub fn get_property(&self, id: KmsObjectId) -> Result<&KmsProperty> {
        self.get(id)
    }

    pub fn get_object_properties_data(&self, id: KmsObjectId) -> Result<(Vec<u32>, Vec<u64>)> {
        let object = self.objects.get(&id).ok_or(Error::new(EINVAL))?;
        match object {
            KmsObject::Crtc(crtc) => {
                let crtc = crtc.lock().unwrap();
                let props = &crtc.properties;
                Ok((
                    props.iter().map(|prop| prop.id.0).collect::<Vec<_>>(),
                    props
                        .iter()
                        .map(|prop| (prop.getter)(&crtc))
                        .collect::<Vec<_>>(),
                ))
            }
            KmsObject::Connector(connector) => {
                let connector = connector.lock().unwrap();
                let props = &connector.properties;
                Ok((
                    props.iter().map(|prop| prop.id.0).collect::<Vec<_>>(),
                    props
                        .iter()
                        .map(|prop| (prop.getter)(&connector))
                        .collect::<Vec<_>>(),
                ))
            }
            KmsObject::Encoder(_)
            | KmsObject::Property(_)
            | KmsObject::Framebuffer(_)
            | KmsObject::Blob(_) => Ok((vec![], vec![])),
        }
    }

    pub fn add_blob(&mut self, data: Vec<u8>) -> KmsObjectId {
        self.add(KmsBlob { data })
    }

    pub fn get_blob(&self, id: KmsObjectId) -> Result<&[u8]> {
        Ok(&self.get::<KmsBlob>(id)?.data)
    }
}

#[derive(Copy, Clone)]
pub struct KmsPropertyName(pub [c_char; DRM_PROP_NAME_LEN as usize]);

impl KmsPropertyName {
    fn new(context: &str, name: &str) -> KmsPropertyName {
        if name.len() > DRM_PROP_NAME_LEN as usize {
            panic!("{context} {name} is too long");
        }

        let mut name_bytes = [0; DRM_PROP_NAME_LEN as usize];
        for (to, &from) in name_bytes.iter_mut().zip(name.as_bytes()) {
            *to = from as c_char;
        }

        KmsPropertyName(name_bytes)
    }
}

impl Debug for KmsPropertyName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let u8_bytes = unsafe { mem::transmute::<&[c_char], &[u8]>(&self.0) };
        f.write_str(&String::from_utf8_lossy(u8_bytes).trim_end_matches('\0'))
    }
}

#[derive(Debug)]
pub struct KmsProperty {
    pub name: KmsPropertyName,
    pub immutable: bool,
    pub atomic: bool,
    pub kind: KmsPropertyKind,
}

#[derive(Debug)]
pub enum KmsPropertyKind {
    Range(u64, u64),
    Enum(Vec<(KmsPropertyName, u64)>),
    Blob,
    Bitmask(Vec<(KmsPropertyName, u64)>),
    Object { type_: u32 },
    SignedRange(i64, i64),
}

#[derive(Debug)]
pub struct KmsPropertyData<T> {
    pub id: KmsObjectId,
    pub getter: fn(&T) -> u64,
}

#[derive(Debug)]
pub struct KmsBlob {
    data: Vec<u8>,
}

macro_rules! define_properties {
    ($($prop:ident $($prop_name:literal)?: $prop_type:ident $({$($prop_content:tt)*})? [$($prop_flag:ident)?],)*) => {
        $(#[allow(non_upper_case_globals)] pub const $prop: KmsObjectId = KmsObjectId(1 + ${index()});)*

        pub(super) fn init_standard_props<T: GraphicsAdapter>(objects: &mut KmsObjects<T>) {
            $(
                assert_eq!(objects.add_property(
                    define_properties!(@prop_name $prop $($prop_name)?),
                    define_properties!(@is_immutable $($prop_flag)?),
                    define_properties!(@is_atomic $($prop_flag)?),
                    define_properties!(@prop_kind $prop_type $({$($prop_content)*})?),
                ), $prop);
            )*
        }
    };
    (@prop_name $prop:ident $prop_name:literal) => { $prop_name };
    (@prop_name $prop:ident) => { stringify!($prop) };
    (@is_immutable) => { false };
    (@is_immutable immutable) => { true };
    (@is_immutable atomic) => { false };
    (@is_atomic) => { false };
    (@is_atomic immutable) => { false };
    (@is_atomic atomic) => { true };
    (@prop_kind range { $start:expr, $end:expr }) => {
        KmsPropertyKind::Range($start, $end)
    };
    (@prop_kind enum { $($variant:ident = $value:expr,)* }) => {
        KmsPropertyKind::Enum(vec![
            $((KmsPropertyName::new("Property variant name", stringify!($variant)), $value)),*]
        )
    };
    (@prop_kind blob) => {
        KmsPropertyKind::Blob
    };
    (@prop_kind object { $type:ident }) => {
        KmsPropertyKind::Object { type_: $type }
    };
    (@prop_kind srange { $start:expr, $end:expr }) => {
        KmsPropertyKind::SignedRange($start, $end)
    };
}

define_properties! {
    // Connector + Plane
    CRTC_ID: object { DRM_MODE_OBJECT_CRTC } [atomic],

    // Connector
    EDID: blob [immutable],
    DPMS: enum {
        On = u64::from(DRM_MODE_DPMS_ON),
        Standby = u64::from(DRM_MODE_DPMS_STANDBY),
        Suspend = u64::from(DRM_MODE_DPMS_SUSPEND),
        Off = u64::from(DRM_MODE_DPMS_OFF),
    } [],

    // CRTC
    ACTIVE: range { 0,1 } [atomic],
    MODE_ID: blob [atomic],

    // Plane
    type_ "type": enum {
        Overlay = u64::from(DRM_PLANE_TYPE_OVERLAY),
        Primary = u64::from(DRM_PLANE_TYPE_PRIMARY),
        Cursor = u64::from(DRM_PLANE_TYPE_CURSOR),
    } [immutable],
    FB_ID: object { DRM_MODE_OBJECT_FB } [atomic],
    CRTC_X: srange { i64::from(i32::MIN), i64::from(i32::MAX) } [atomic],
    CRTC_Y: srange { i64::from(i32::MIN), i64::from(i32::MAX) } [atomic],
    CRTC_W: range { 0, u64::from(u32::MAX) } [atomic],
    CRTC_H: range { 0, u64::from(u32::MAX) } [atomic],
    SRC_X: range { 0, u64::from(u32::MAX) } [atomic],
    SRC_Y: range { 0, u64::from(u32::MAX) } [atomic],
    SRC_W: range { 0, u64::from(u32::MAX) } [atomic],
    SRC_H: range { 0, u64::from(u32::MAX) } [atomic],
    FB_DAMAGE_CLIPS: blob [atomic],
}

macro_rules! define_object_props {
    ($object:ident, $obj:ident$(<$($T:ident$(: $bound:ident)?),*>)? { $(
        $prop:ident {
            get => $get:expr,
        }
    )* }) => {
        impl$(<$($T$(: $bound)?),*>)? $obj$(<$($T),*>)? {
            pub(super) fn base_properties() -> Vec<KmsPropertyData<Self>> {
                vec![$(KmsPropertyData {
                    id: $prop,
                    getter: |$object| $get
                }),*]
            }
        }
    };
}
pub(super) use define_object_props;
