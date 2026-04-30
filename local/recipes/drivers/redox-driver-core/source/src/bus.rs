use alloc::vec::Vec;

use crate::device::DeviceInfo;
#[cfg(feature = "hotplug")]
use crate::hotplug::HotplugSubscription;

/// A hardware bus that can enumerate devices.
pub trait Bus: Send + Sync {
    /// Returns a human-readable bus name such as `"pci"`, `"usb"`, or `"acpi"`.
    fn name(&self) -> &str;

    /// Enumerates all devices currently visible on this bus.
    ///
    /// Implementations must be safe to call repeatedly so that the manager can perform
    /// re-scans after topology changes or deferred-probe retries.
    fn enumerate_devices(&self) -> Result<Vec<DeviceInfo>, BusError>;

    /// Subscribes to bus hotplug notifications.
    ///
    /// The returned subscription is intentionally opaque so concrete bus implementations can
    /// map it to a file descriptor, channel, or other event source.
    #[cfg(feature = "hotplug")]
    fn subscribe_hotplug(&self) -> Result<HotplugSubscription, BusError>;
}

/// Errors produced by a [`Bus`] implementation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BusError {
    /// The bus has not finished initializing and cannot currently enumerate devices.
    NotReady,
    /// A transport or I/O failure occurred while talking to the bus.
    IoError,
    /// The requested capability is not supported by this bus implementation.
    Unsupported,
    /// An implementation-specific static error message.
    Other(&'static str),
}
