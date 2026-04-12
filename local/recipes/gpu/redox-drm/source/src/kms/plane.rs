use crate::driver::{DriverError, Result};

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaneKind {
    Primary,
    Cursor,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Plane {
    pub id: u32,
    pub kind: PlaneKind,
    pub fb_handle: Option<u32>,
    pub crtc_id: Option<u32>,
}

impl Plane {
    #[allow(dead_code)]
    pub fn new(id: u32, kind: PlaneKind) -> Self {
        Self {
            id,
            kind,
            fb_handle: None,
            crtc_id: None,
        }
    }

    #[allow(dead_code)]
    pub fn attach(&mut self, crtc_id: u32, fb_handle: u32) -> Result<()> {
        if fb_handle == 0 {
            return Err(DriverError::InvalidArgument(
                "plane attach requires a framebuffer handle",
            ));
        }

        self.crtc_id = Some(crtc_id);
        self.fb_handle = Some(fb_handle);
        Ok(())
    }
}
