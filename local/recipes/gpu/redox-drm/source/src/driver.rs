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

#[cfg(test)]
mod tests {
    use std::mem::{discriminant, offset_of, size_of};

    use super::*;

    #[test]
    fn redox_private_cs_submit_size() {
        // src_handle(u32) + dst_handle(u32) + src_offset(u64) + dst_offset(u64) + byte_count(u64)
        // = 4 + 4 + 8 + 8 + 8 = 32 bytes
        assert_eq!(size_of::<RedoxPrivateCsSubmit>(), 32);
    }

    #[test]
    fn redox_private_cs_submit_result_size() {
        // seqno(u64) = 8 bytes
        assert_eq!(size_of::<RedoxPrivateCsSubmitResult>(), 8);
    }

    #[test]
    fn redox_private_cs_wait_size() {
        // seqno(u64) + timeout_ns(u64) = 16 bytes
        assert_eq!(size_of::<RedoxPrivateCsWait>(), 16);
    }

    #[test]
    fn redox_private_cs_wait_result_size() {
        // completed(bool) + 7 padding + completed_seqno(u64) = 16 bytes
        assert_eq!(size_of::<RedoxPrivateCsWaitResult>(), 16);
    }

    #[test]
    fn driver_event_vblank_size() {
        let event = DriverEvent::Vblank {
            crtc_id: 0xDEADBEEF,
            count: 0x1234_5678_9ABC_DEF0,
        };
        match event {
            DriverEvent::Vblank { crtc_id, count } => {
                assert_eq!(crtc_id, 0xDEADBEEF);
                assert_eq!(count, 0x1234_5678_9ABC_DEF0);
            }
            DriverEvent::Hotplug { .. } => panic!("expected Vblank, got Hotplug"),
        }
        let enum_size = size_of::<DriverEvent>();
        assert!(enum_size > 0, "DriverEvent must be non-zero-sized");
    }

    #[test]
    fn driver_event_hotplug_size() {
        let event = DriverEvent::Hotplug {
            connector_id: 0xCAFEBABE,
        };
        match event {
            DriverEvent::Hotplug { connector_id } => {
                assert_eq!(connector_id, 0xCAFEBABE);
            }
            DriverEvent::Vblank { .. } => panic!("expected Hotplug, got Vblank"),
        }
        let vblank = DriverEvent::Vblank {
            crtc_id: 0,
            count: 0,
        };
        let hotplug = DriverEvent::Hotplug { connector_id: 0 };
        assert_ne!(
            discriminant(&vblank),
            discriminant(&hotplug),
            "Vblank and Hotplug must have distinct discriminants"
        );
    }

    #[test]
    fn redox_private_cs_submit_is_repr_c() {
        assert_eq!(offset_of!(RedoxPrivateCsSubmit, src_handle), 0);
        assert_eq!(offset_of!(RedoxPrivateCsSubmit, dst_handle), 4);
        assert_eq!(offset_of!(RedoxPrivateCsSubmit, src_offset), 8);
        assert_eq!(offset_of!(RedoxPrivateCsSubmit, dst_offset), 16);
        assert_eq!(offset_of!(RedoxPrivateCsSubmit, byte_count), 24);
    }

    #[test]
    fn redox_private_cs_wait_is_repr_c() {
        assert_eq!(offset_of!(RedoxPrivateCsWait, seqno), 0);
        assert_eq!(offset_of!(RedoxPrivateCsWait, timeout_ns), 8);
    }
}
