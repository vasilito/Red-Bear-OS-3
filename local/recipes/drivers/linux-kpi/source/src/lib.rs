#![doc = "Linux Kernel API compatibility layer for Redox OS (LinuxKPI-style).\n\nProvides C headers and Rust FFI implementations that translate Linux kernel APIs\nto Redox OS primitives, enabling porting of Linux C drivers as Redox userspace daemons."]

pub mod rust_impl;

pub use rust_impl::device;
pub use rust_impl::dma;
pub use rust_impl::drm_shim;
pub use rust_impl::firmware;
pub use rust_impl::io;
pub use rust_impl::irq;
pub use rust_impl::memory;
pub use rust_impl::pci;
pub use rust_impl::sync;
pub use rust_impl::workqueue;
