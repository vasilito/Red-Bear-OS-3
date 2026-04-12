pub mod file_disk;

#[cfg(feature = "redox")]
pub mod redox_disk;

pub use file_disk::FileDisk;

#[cfg(feature = "redox")]
pub use redox_disk::RedoxDisk;

pub use rsext4::bmalloc::AbsoluteBN;
pub use rsext4::disknode::Ext4Timestamp;
pub use rsext4::{BlockDevice, Ext4Error, Ext4Result};
