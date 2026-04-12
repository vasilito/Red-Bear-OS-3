use thiserror::Error;

use crate::gem::GemHandle;
use crate::kms::{ConnectorInfo, ModeInfo};

pub type Result<T> = std::result::Result<T, DriverError>;

#[derive(Debug, Error)]
pub enum DriverError {
    #[error("driver initialization failed: {0}")]
    Initialization(String),

    #[error("invalid argument: {0}")]
    InvalidArgument(&'static str),

    #[error("resource not found: {0}")]
    NotFound(String),

    #[allow(dead_code)]
    #[error("operation not supported: {0}")]
    Unsupported(&'static str),

    #[error("MMIO failure: {0}")]
    Mmio(String),

    #[error("PCI failure: {0}")]
    Pci(String),

    #[error("buffer failure: {0}")]
    Buffer(String),

    #[error("I/O failure: {0}")]
    Io(String),
}

pub trait GpuDriver: Send + Sync {
    fn driver_name(&self) -> &str;
    fn driver_desc(&self) -> &str;
    #[allow(dead_code)]
    fn driver_date(&self) -> &str;

    fn detect_connectors(&self) -> Vec<ConnectorInfo>;
    fn get_modes(&self, connector_id: u32) -> Vec<ModeInfo>;
    fn set_crtc(
        &self,
        crtc_id: u32,
        fb_handle: u32,
        connectors: &[u32],
        mode: &ModeInfo,
    ) -> Result<()>;
    fn page_flip(&self, crtc_id: u32, fb_handle: u32, flags: u32) -> Result<u64>;
    #[allow(dead_code)]
    fn get_vblank(&self, crtc_id: u32) -> Result<u64>;

    fn gem_create(&self, size: u64) -> Result<GemHandle>;
    fn gem_close(&self, handle: GemHandle) -> Result<()>;
    fn gem_mmap(&self, handle: GemHandle) -> Result<usize>;
    fn gem_size(&self, handle: GemHandle) -> Result<u64>;
    #[allow(dead_code)]
    fn gem_export_dmafd(&self, handle: GemHandle) -> Result<i32>;
    #[allow(dead_code)]
    fn gem_import_dmafd(&self, fd: i32) -> Result<GemHandle>;

    #[allow(dead_code)]
    fn get_edid(&self, connector_id: u32) -> Vec<u8>;
    fn handle_irq(&self) -> Result<Option<(u32, u64)>>;
}
