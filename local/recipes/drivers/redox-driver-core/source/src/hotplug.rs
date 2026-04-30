use alloc::string::String;

use hashbrown::HashMap;

use crate::device::DeviceId;
#[cfg(feature = "hotplug")]
use crate::device::DeviceInfo;

/// A normalized action associated with a userspace device event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UeventAction {
    /// A device or logical function was added.
    Add,
    /// A device or logical function was removed.
    Remove,
    /// A device changed state or metadata.
    Change,
    /// A driver or subsystem bound to the device.
    Bind,
    /// A driver or subsystem detached from the device.
    Unbind,
}

/// Bus-agnostic metadata describing a userspace device event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Uevent {
    /// Event action, normalized across bus implementations.
    pub action: UeventAction,
    /// Stable device identifier associated with the event.
    pub device: DeviceId,
    /// Bus-specific key-value metadata that accompanied the event.
    pub properties: HashMap<String, String>,
}

/// Opaque subscription handle for receiving hotplug notifications.
#[cfg(feature = "hotplug")]
pub type HotplugSubscription = usize;

/// High-level hotplug event delivered by a bus implementation.
#[cfg(feature = "hotplug")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HotplugEvent {
    /// A device appeared on the bus and is ready for probing.
    DeviceAdded(DeviceInfo),
    /// A device disappeared from the bus.
    DeviceRemoved(DeviceId),
}
