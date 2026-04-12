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
