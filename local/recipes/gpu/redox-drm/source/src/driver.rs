use thiserror::Error;

use crate::gem::GemHandle;
use crate::kms::{ConnectorInfo, ModeInfo};

pub type Result<T> = std::result::Result<T, DriverError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriverEvent {
    Vblank { crtc_id: u32, count: u64 },
    Hotplug { connector_id: u32 },
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RedoxPrivateCsSubmit {
    pub src_handle: GemHandle,
    pub dst_handle: GemHandle,
    pub src_offset: u64,
    pub dst_offset: u64,
    pub byte_count: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RedoxPrivateCsSubmitResult {
    pub seqno: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RedoxPrivateCsWait {
    pub seqno: u64,
    pub timeout_ns: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RedoxPrivateCsWaitResult {
    pub completed: bool,
    pub completed_seqno: u64,
}

#[derive(Debug, Error)]
pub enum DriverError {
    #[error("driver initialization failed: {0}")]
    Initialization(String),

    #[error("invalid argument: {0}")]
    InvalidArgument(&'static str),

    #[error("resource not found: {0}")]
    NotFound(String),

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
    fn get_vblank(&self, crtc_id: u32) -> Result<u64>;

    fn gem_create(&self, size: u64) -> Result<GemHandle>;
    fn gem_close(&self, handle: GemHandle) -> Result<()>;
    fn gem_mmap(&self, handle: GemHandle) -> Result<usize>;
    fn gem_size(&self, handle: GemHandle) -> Result<u64>;

    #[allow(dead_code)]
    fn get_edid(&self, connector_id: u32) -> Vec<u8>;
    fn handle_irq(&self) -> Result<Option<DriverEvent>>;

    fn redox_private_cs_submit(
        &self,
        _submit: &RedoxPrivateCsSubmit,
    ) -> Result<RedoxPrivateCsSubmitResult> {
        Err(DriverError::Unsupported(
            "private command submission is unavailable on this backend",
        ))
    }

    fn redox_private_cs_wait(
        &self,
        _wait: &RedoxPrivateCsWait,
    ) -> Result<RedoxPrivateCsWaitResult> {
        Err(DriverError::Unsupported(
            "private command completion waits are unavailable on this backend",
        ))
    }
}
