use std::collections::HashMap;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

use drm_sys::{
    drm_mode_modeinfo, DRM_MODE_OBJECT_BLOB, DRM_MODE_OBJECT_CONNECTOR, DRM_MODE_OBJECT_CRTC,
    DRM_MODE_OBJECT_ENCODER, DRM_MODE_OBJECT_FB, DRM_MODE_OBJECT_PROPERTY,
};
use syscall::{Error, Result, EINVAL};

use crate::kms::connector::{KmsConnector, KmsEncoder};
use crate::kms::properties::{
    define_object_props, init_standard_props, KmsBlob, KmsProperty, KmsPropertyData,
};
use crate::GraphicsAdapter;

#[derive(Debug)]
pub struct KmsObjects<T: GraphicsAdapter> {
    next_id: KmsObjectId,
    pub(crate) connectors: Vec<KmsObjectId>,
    pub(crate) encoders: Vec<KmsObjectId>,
    crtcs: Vec<KmsObjectId>,
    framebuffers: Vec<KmsObjectId>,
    pub(crate) objects: HashMap<KmsObjectId, KmsObject<T>>,
    _marker: PhantomData<T>,
}

impl<T: GraphicsAdapter> KmsObjects<T> {
    pub(crate) fn new() -> Self {
        let mut objects = KmsObjects {
            next_id: KmsObjectId(1),
            connectors: vec![],
            encoders: vec![],
            crtcs: vec![],
            framebuffers: vec![],
            objects: HashMap::new(),
            _marker: PhantomData,
        };
        init_standard_props(&mut objects);
        objects
    }

    pub(crate) fn add<U: Into<KmsObject<T>>>(&mut self, data: U) -> KmsObjectId {
        let id = self.next_id;
        self.objects.insert(id, data.into());
        self.next_id.0 += 1;

        id
    }

    pub(crate) fn get<'a, U: 'a>(&'a self, id: KmsObjectId) -> Result<&'a U>
    where
        &'a U: TryFrom<&'a KmsObject<T>>,
    {
        let object = self.objects.get(&id).ok_or(Error::new(EINVAL))?;
        if let Ok(object) = object.try_into() {
            Ok(object)
        } else {
            Err(Error::new(EINVAL))
        }
    }

    pub fn object_type(&self, id: KmsObjectId) -> Result<u32> {
        let object = self.objects.get(&id).ok_or(Error::new(EINVAL))?;
        Ok(object.object_type())
    }

    pub fn add_crtc(
        &mut self,
        driver_data: T::Crtc,
        driver_data_state: <T::Crtc as KmsCrtcDriver>::State,
    ) -> KmsObjectId {
        let crtc_index = self.crtcs.len() as u32;
        let id = self.add(Mutex::new(KmsCrtc {
            crtc_index,
            gamma_size: 0,
            properties: KmsCrtc::base_properties(),
            state: KmsCrtcState {
                fb_id: None,
                mode: None,
                driver_data: driver_data_state,
            },
            driver_data,
        }));
        self.crtcs.push(id);

        id
    }

    pub fn crtc_ids(&self) -> &[KmsObjectId] {
        &self.crtcs
    }

    pub fn crtcs(&self) -> impl Iterator<Item = &Mutex<KmsCrtc<T>>> + use<'_, T> {
        self.crtcs
            .iter()
            .map(|&id| self.get::<Mutex<KmsCrtc<T>>>(id).unwrap())
    }

    pub fn get_crtc(&self, id: KmsObjectId) -> Result<&Mutex<KmsCrtc<T>>> {
        self.get(id)
    }

    pub fn add_framebuffer(&mut self, fb: KmsFramebuffer<T>) -> KmsObjectId {
        let id = self.add(fb);
        self.framebuffers.push(id);
        id
    }

    pub fn remove_framebuffer(&mut self, id: KmsObjectId) -> Result<()> {
        let Some(object) = self.objects.get(&id) else {
            return Err(Error::new(EINVAL));
        };
        let KmsObject::Framebuffer(_) = object else {
            return Err(Error::new(EINVAL));
        };
        self.objects.remove(&id).unwrap();

        Ok(())
    }

    pub fn fb_ids(&self) -> &[KmsObjectId] {
        &self.framebuffers
    }

    pub fn get_framebuffer(&self, id: KmsObjectId) -> Result<&KmsFramebuffer<T>> {
        self.get(id)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct KmsObjectId(pub(crate) u32);

impl KmsObjectId {
    pub const INVALID: KmsObjectId = KmsObjectId(0);
}

impl From<KmsObjectId> for u64 {
    fn from(value: KmsObjectId) -> Self {
        value.0.into()
    }
}

macro_rules! define_object_kinds {
    (<$T:ident> $(
        $variant:ident($data:ty) = $type:ident,
    )*) => {
        #[derive(Debug)]
        pub(crate) enum KmsObject<$T: GraphicsAdapter> {
            $($variant($data),)*
        }

        impl<$T: GraphicsAdapter> KmsObject<$T> {
            fn object_type(&self) -> u32 {
                match self {
                    $(Self::$variant(_) => $type,)*
                }
            }
        }

        $(
            impl<$T: GraphicsAdapter> From<$data> for KmsObject<$T> {
                fn from(value: $data) -> Self {
                    Self::$variant(value)
                }
            }

            impl<'a, $T: GraphicsAdapter> TryFrom<&'a KmsObject<$T>> for &'a $data {
                type Error = ();

                fn try_from(value: &'a KmsObject<T>) -> Result<Self, Self::Error> {
                    match value {
                        KmsObject::$variant(data) => Ok(data),
                        _ => Err(()),
                    }
                }
            }
        )*
    };
}

define_object_kinds! { <T>
    Crtc(Mutex<KmsCrtc<T>>) = DRM_MODE_OBJECT_CRTC,
    Connector(Mutex<KmsConnector<T>>) = DRM_MODE_OBJECT_CONNECTOR,
    Encoder(KmsEncoder) = DRM_MODE_OBJECT_ENCODER,
    Property(KmsProperty) = DRM_MODE_OBJECT_PROPERTY,
    Framebuffer(KmsFramebuffer<T>) = DRM_MODE_OBJECT_FB,
    Blob(KmsBlob) = DRM_MODE_OBJECT_BLOB,
}

pub trait KmsCrtcDriver: Debug {
    type State: Clone + Debug;
}

impl KmsCrtcDriver for () {
    type State = ();
}

#[derive(Debug)]
pub struct KmsCrtc<T: GraphicsAdapter> {
    pub crtc_index: u32,
    pub gamma_size: u32,
    pub properties: Vec<KmsPropertyData<Self>>,
    pub state: KmsCrtcState<T>,
    pub driver_data: T::Crtc,
}

#[derive(Debug)]
pub struct KmsCrtcState<T: GraphicsAdapter> {
    pub fb_id: Option<KmsObjectId>,
    pub mode: Option<drm_mode_modeinfo>,
    pub driver_data: <T::Crtc as KmsCrtcDriver>::State,
}

impl<T: GraphicsAdapter> Clone for KmsCrtcState<T> {
    fn clone(&self) -> Self {
        Self {
            fb_id: self.fb_id.clone(),
            mode: self.mode.clone(),
            driver_data: self.driver_data.clone(),
        }
    }
}

define_object_props!(object, KmsCrtc<T: GraphicsAdapter> {});

#[derive(Debug)]
pub struct KmsFramebuffer<T: GraphicsAdapter> {
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub bpp: u32,
    pub depth: u32,
    pub buffer: Arc<T::Buffer>,
    pub driver_data: T::Framebuffer,
}
