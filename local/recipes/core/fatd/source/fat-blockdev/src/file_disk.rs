use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Block device adapter backed by a host file (Linux/macOS).
///
/// Implements `Read + Write + Seek` for use with the `fatfs` crate.
/// Wraps `std::fs::File` and reports total size from filesystem metadata.
pub struct FileDisk {
    file: File,
    size: u64,
}

impl FileDisk {
    /// Open an existing file for read/write.
    pub fn new(file: File) -> Self {
        let size = file.metadata()
            .map(|m| m.len())
            .unwrap_or_else(|e| {
                log::warn!("file_disk: metadata read failed, assuming zero size: {e}");
                0
            });
        Self { file, size }
    }

    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?;
        let size = file.metadata()?.len();
        Ok(Self { file, size })
    }

    /// Create a new file of the given size, zero-filled.
    pub fn create<P: AsRef<Path>>(path: P, size: u64) -> io::Result<Self> {
        let file = File::create(&path)?;
        file.set_len(size)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)?;
        Ok(Self { file, size })
    }

    /// Total size of the backing file in bytes.
    pub fn size(&self) -> u64 {
        self.size
    }
}

impl Read for FileDisk {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file.read(buf)
    }
}

impl Write for FileDisk {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

impl Seek for FileDisk {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.file.seek(pos)
    }
}
