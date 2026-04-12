use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::UNIX_EPOCH;

use rsext4::bmalloc::AbsoluteBN;
use rsext4::disknode::Ext4Timestamp;
use rsext4::{BlockDevice, Ext4Error, Ext4Result};

pub struct FileDisk {
    file: File,
    total_blocks: u64,
    block_size: u32,
}

impl FileDisk {
    pub fn open<P: AsRef<Path>>(path: P, block_size: u32) -> std::io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        let len = file.metadata()?.len();
        Ok(Self {
            file,
            total_blocks: len / block_size as u64,
            block_size,
        })
    }

    pub fn create<P: AsRef<Path>>(path: P, size: u64, block_size: u32) -> std::io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        file.set_len(size)?;
        Ok(Self {
            file,
            total_blocks: size / block_size as u64,
            block_size,
        })
    }
}

impl BlockDevice for FileDisk {
    fn read(&mut self, buffer: &mut [u8], block_id: AbsoluteBN, count: u32) -> Ext4Result<()> {
        let offset = block_id.raw() * self.block_size as u64;
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|_| Ext4Error::io())?;
        let total = count as usize * self.block_size as usize;
        if buffer.len() < total {
            return Err(Ext4Error::invalid_input());
        }
        self.file
            .read_exact(&mut buffer[..total])
            .map_err(|_| Ext4Error::io())?;
        Ok(())
    }

    fn write(&mut self, buffer: &[u8], block_id: AbsoluteBN, count: u32) -> Ext4Result<()> {
        let offset = block_id.raw() * self.block_size as u64;
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|_| Ext4Error::io())?;
        let total = count as usize * self.block_size as usize;
        if buffer.len() < total {
            return Err(Ext4Error::invalid_input());
        }
        self.file
            .write_all(&buffer[..total])
            .map_err(|_| Ext4Error::io())?;
        Ok(())
    }

    fn open(&mut self) -> Ext4Result<()> {
        Ok(())
    }

    fn close(&mut self) -> Ext4Result<()> {
        Ok(())
    }

    fn total_blocks(&self) -> u64 {
        self.total_blocks
    }

    fn block_size(&self) -> u32 {
        self.block_size
    }

    fn flush(&mut self) -> Ext4Result<()> {
        self.file.sync_data().map_err(|_| Ext4Error::io())
    }

    fn current_time(&self) -> Ext4Result<Ext4Timestamp> {
        let dur = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Ext4Error::io())?;
        Ok(Ext4Timestamp::new(dur.as_secs() as i64, dur.subsec_nanos()))
    }
}
