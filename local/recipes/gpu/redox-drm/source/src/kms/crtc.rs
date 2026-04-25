use crate::driver::{DriverError, Result};
use crate::kms::ModeInfo;

#[derive(Clone, Debug)]
pub struct Crtc {
    pub id: u32,
    pub current_fb: u32,
    pub connectors: Vec<u32>,
    pub mode: Option<ModeInfo>,
    #[allow(dead_code)]
    pub x: u32,
    #[allow(dead_code)]
    pub y: u32,
    #[allow(dead_code)]
    pub gamma_size: u32,
}

impl Crtc {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            current_fb: 0,
            connectors: Vec::new(),
            mode: None,
            x: 0,
            y: 0,
            gamma_size: 256,
        }
    }

    pub fn program(&mut self, fb_handle: u32, connectors: &[u32], mode: &ModeInfo) -> Result<()> {
        if connectors.is_empty() {
            return Err(DriverError::InvalidArgument(
                "set_crtc requires at least one connector",
            ));
        }

        self.current_fb = fb_handle;
        self.connectors = connectors.to_vec();
        self.mode = Some(mode.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_mode() -> ModeInfo {
        ModeInfo::default_1080p()
    }

    #[test]
    fn new_initializes_correctly() {
        let crtc = Crtc::new(42);
        assert_eq!(crtc.id, 42);
        assert_eq!(crtc.current_fb, 0);
        assert!(crtc.connectors.is_empty());
        assert!(crtc.mode.is_none());
        assert_eq!(crtc.gamma_size, 256);
    }

    #[test]
    fn program_sets_fb_connectors_and_mode() {
        let mut crtc = Crtc::new(1);
        let mode = test_mode();
        let result = crtc.program(99, &[10, 20], &mode);

        assert!(result.is_ok());
        assert_eq!(crtc.current_fb, 99);
        assert_eq!(crtc.connectors, vec![10, 20]);
        assert!(crtc.mode.is_some());
        let programmed_mode = crtc.mode.unwrap();
        assert_eq!(programmed_mode.hdisplay, 1920);
        assert_eq!(programmed_mode.vdisplay, 1080);
    }

    #[test]
    fn program_empty_connectors_returns_invalid_argument() {
        let mut crtc = Crtc::new(1);
        let mode = test_mode();
        let result = crtc.program(99, &[], &mode);

        assert!(result.is_err());
        match result.unwrap_err() {
            DriverError::InvalidArgument(msg) => {
                assert!(msg.contains("connector"));
            }
            other => panic!("expected InvalidArgument, got {:?}", other),
        }
        // State should be unchanged
        assert_eq!(crtc.current_fb, 0);
        assert!(crtc.connectors.is_empty());
        assert!(crtc.mode.is_none());
    }

    #[test]
    fn program_multiple_connectors_accepted() {
        let mut crtc = Crtc::new(1);
        let mode = test_mode();
        let result = crtc.program(50, &[1, 2, 3, 4, 5], &mode);

        assert!(result.is_ok());
        assert_eq!(crtc.connectors.len(), 5);
        assert_eq!(crtc.connectors, vec![1, 2, 3, 4, 5]);
    }
}
