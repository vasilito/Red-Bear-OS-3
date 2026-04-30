#![cfg_attr(not(feature = "std"), no_std)]
#![doc = "Core device-model traits and orchestration primitives for Red Bear OS drivers."]

#[cfg(not(any(feature = "std", feature = "alloc", test)))]
compile_error!("redox-driver-core requires either the `std` or `alloc` feature");

extern crate alloc;

/// Bus abstractions and related error types.
pub mod bus;
/// Device descriptors and bound-device state.
pub mod device;
/// Driver traits and probe outcomes.
pub mod driver;
/// Hotplug and uevent metadata types.
pub mod hotplug;
/// Device-manager orchestration.
pub mod manager;
/// Match-table primitives.
pub mod r#match;
/// Driver parameter definitions and runtime values.
pub mod params;

pub use bus::{Bus, BusError};
pub use device::{BoundDevice, Device, DeviceId, DeviceInfo};
pub use driver::{Driver, DriverError, ProbeResult};
pub use hotplug::{Uevent, UeventAction};
#[cfg(feature = "hotplug")]
pub use hotplug::{HotplugEvent, HotplugSubscription};
pub use manager::{DeviceManager, ManagerConfig, ProbeEvent};
pub use params::{DriverParams, ParamDef, ParamValue};
pub use r#match::{DriverMatch, MatchPriority, MatchTable};
