use crate::types::{PortStatus, SetupPacket, TransferDirection};

/// Trait that all USB host controller drivers must implement.
/// This provides a uniform scheme interface regardless of HC type (XHCI/EHCI/OHCI/UHCI).
pub trait UsbHostController {
    /// Get the number of ports on this controller
    fn port_count(&self) -> usize;

    /// Get status of a specific port
    fn port_status(&self, port: usize) -> Option<PortStatus>;

    /// Reset a port (USB enumeration step 1)
    fn port_reset(&mut self, port: usize) -> bool;

    /// Submit a control transfer to endpoint 0
    fn control_transfer(
        &mut self,
        device_address: u8,
        setup: &SetupPacket,
        data: &mut [u8],
    ) -> Result<usize, UsbError>;

    /// Submit a bulk transfer
    fn bulk_transfer(
        &mut self,
        device_address: u8,
        endpoint: u8,
        data: &mut [u8],
        direction: TransferDirection,
    ) -> Result<usize, UsbError>;

    /// Submit an interrupt transfer
    fn interrupt_transfer(
        &mut self,
        device_address: u8,
        endpoint: u8,
        data: &mut [u8],
    ) -> Result<usize, UsbError>;

    /// Set device address (after reset, before config)
    fn set_address(&mut self, device_address: u8) -> bool;

    /// Get the controller name for logging
    fn name(&self) -> &str;
}

#[derive(Debug)]
pub enum UsbError {
    Timeout,
    Stall,
    DataError,
    Babble,
    NoDevice,
    NotConfigured,
    IoError,
    Unsupported,
}
