mod file_disk;

pub use file_disk::FileDisk;

#[cfg(feature = "redox")]
mod redox_disk;

#[cfg(feature = "redox")]
pub use redox_disk::RedoxDisk;
