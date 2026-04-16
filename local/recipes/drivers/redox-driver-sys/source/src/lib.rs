//! Safe Rust wrappers for Redox OS scheme-based hardware access.
//!
//! Provides abstractions for physical memory mapping, interrupt handling,
//! PCI device access, port I/O, DMA buffer management, and capability scanning.
//!
//! All hardware access goes through Redox's scheme system:
//! - `scheme:memory` for physical memory mapping and address translation
//! - `scheme:irq` for interrupt delivery
//! - `scheme:pci` for PCI device enumeration and configuration
//!
//! # Example
//!
//! ```no_run
//! use redox_driver_sys::pci::PciDevice;
//! use redox_driver_sys::Result;
//!
//! fn example() -> Result<()> {
//!     // Open a PCI device by location
//!     let mut dev = PciDevice::open(0, 0x10, 0, 0)?;
//!     let _vendor = dev.vendor_id();
//!     let bars = dev.parse_bars()?;
//!     if let Some((addr, size)) = bars[0].memory_info() {
//!         let mmio = dev.map_bar(0, addr, size)?;
//!         let _reg = mmio.read32(0);
//!     }
//!     Ok(())
//! }
//! ```

pub mod dma;
pub mod io;
pub mod irq;
pub mod memory;
pub mod pci;
pub mod pcid_client;

use syscall as redox_syscall;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DriverError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("system call error: {0}")]
    Syscall(#[from] redox_syscall::error::Error),

    #[error("invalid address: {0}")]
    InvalidAddress(u64),

    #[error("invalid parameter: {0}")]
    InvalidParam(&'static str),

    #[error("mapping failed for {phys:#x}+{size:#x}: {reason}")]
    MappingFailed {
        phys: u64,
        size: usize,
        reason: String,
    },

    #[error("device not found: {0}")]
    DeviceNotFound(String),

    #[error("firmware not found: {0}")]
    FirmwareNotFound(String),

    #[error("PCI error: {0}")]
    Pci(String),

    #[error("IRQ error: {0}")]
    Irq(String),

    #[error("capability not found: {0}")]
    CapabilityNotFound(String),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = core::result::Result<T, DriverError>;

impl From<libredox::error::Error> for DriverError {
    fn from(error: libredox::error::Error) -> Self {
        // Preserve the raw errno rather than going through std::io::Error
        // which discards the syscall-specific error code.
        Self::Syscall(redox_syscall::error::Error::new(error.errno()))
    }
}
