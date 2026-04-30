use alloc::collections::BTreeMap;
use alloc::string::String;

/// Unique identifier for a device on a specific bus.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId {
    /// The bus namespace for this device, such as `"pci"` or `"usb"`.
    pub bus: String,
    /// The bus-local path for the device, such as `"0000:00:02.0"` or `"1-2"`.
    pub path: String,
}

/// Information about a discovered device, used for driver matching and probing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceInfo {
    /// Stable device identifier within the manager.
    pub id: DeviceId,
    /// Optional vendor identifier reported by the bus or firmware.
    pub vendor: Option<u16>,
    /// Optional device identifier reported by the bus or firmware.
    pub device: Option<u16>,
    /// Optional base class code.
    pub class: Option<u8>,
    /// Optional subclass code.
    pub subclass: Option<u8>,
    /// Optional programming-interface code.
    pub prog_if: Option<u8>,
    /// Optional hardware revision code.
    pub revision: Option<u8>,
    /// Optional subsystem vendor identifier.
    pub subsystem_vendor: Option<u16>,
    /// Optional subsystem device identifier.
    pub subsystem_device: Option<u16>,
    /// Raw bus-specific device handle for detailed access.
    pub raw_path: String,
    /// Optional human-readable description provided by firmware or the bus layer.
    pub description: Option<String>,
}

/// Generic interface for an owned device handle.
pub trait Device: Send + Sync {
    /// Returns the stable identifier for this device.
    fn id(&self) -> &DeviceId;

    /// Returns the immutable descriptor used for matching and lifecycle actions.
    fn info(&self) -> &DeviceInfo;
}

/// A device that has been successfully matched and bound to a driver.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundDevice {
    /// Static information captured at discovery time.
    pub info: DeviceInfo,
    /// The name of the driver that currently owns the device.
    pub driver_name: String,
    /// Key-value parameters associated with the active binding.
    pub parameters: BTreeMap<String, String>,
}
