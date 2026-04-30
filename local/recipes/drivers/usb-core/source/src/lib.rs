#![cfg_attr(not(feature = "std"), no_std)]
#![doc = "Shared USB types and primitives for Red Bear host controller drivers."]

extern crate alloc;

pub mod dma;
pub mod scheme;
pub mod spawn;
pub mod transfer;
pub mod types;

pub use dma::{DmaBuffer, DmaError};
pub use scheme::{UsbError, UsbHostController};
pub use spawn::spawn_usb_driver;
pub use transfer::{
    control_transfer, parse_config_descriptor, parse_device_descriptor, parse_endpoint_descriptor,
};
pub use types::{
    ConfigDescriptor, DeviceDescriptor, EndpointDescriptor, PortStatus, SetupPacket,
    TransferDirection, TransferType, Urb, UrbStatus,
};
