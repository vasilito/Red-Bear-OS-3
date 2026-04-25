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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_primary_initializes_correctly() {
        let plane = Plane::new(7, PlaneKind::Primary);
        assert_eq!(plane.id, 7);
        assert_eq!(plane.kind, PlaneKind::Primary);
        assert_eq!(plane.fb_handle, None);
        assert_eq!(plane.crtc_id, None);
    }

    #[test]
    fn new_cursor_initializes_correctly() {
        let plane = Plane::new(3, PlaneKind::Cursor);
        assert_eq!(plane.id, 3);
        assert_eq!(plane.kind, PlaneKind::Cursor);
        assert!(plane.fb_handle.is_none());
        assert!(plane.crtc_id.is_none());
    }

    #[test]
    fn attach_sets_crtc_id_and_fb_handle() {
        let mut plane = Plane::new(1, PlaneKind::Primary);
        let result = plane.attach(10, 20);

        assert!(result.is_ok());
        assert_eq!(plane.crtc_id, Some(10));
        assert_eq!(plane.fb_handle, Some(20));
    }

    #[test]
    fn attach_zero_fb_handle_returns_invalid_argument() {
        let mut plane = Plane::new(1, PlaneKind::Primary);
        let result = plane.attach(10, 0);

        assert!(result.is_err());
        match result.unwrap_err() {
            DriverError::InvalidArgument(msg) => {
                assert!(msg.contains("framebuffer"));
            }
            other => panic!("expected InvalidArgument, got {:?}", other),
        }
        assert_eq!(plane.crtc_id, None);
        assert_eq!(plane.fb_handle, None);
    }
}
