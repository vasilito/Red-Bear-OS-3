use rsext4::bmalloc::AbsoluteBN;
use rsext4::disknode::Ext4Timestamp;
use rsext4::{BlockDevice, Ext4Error, Ext4Result};

pub struct RedoxDisk {
    fd: usize,
    total_blocks: u64,
    block_size: u32,
}

impl RedoxDisk {
    pub fn open(disk_path: &str, block_size: u32) -> syscall::error::Result<Self> {
        let fd = libredox::call::open(disk_path, libredox::flag::O_RDWR, 0)?;
        let mut stat = syscall::data::Stat::default();
        syscall::call::fstat(fd, &mut stat)?;
        let total_blocks = stat.st_size / block_size as u64;
        Ok(Self {
            fd,
            total_blocks,
            block_size,
        })
    }
}

impl BlockDevice for RedoxDisk {
    fn read(&mut self, buffer: &mut [u8], block_id: AbsoluteBN, count: u32) -> Ext4Result<()> {
        let offset = block_id.raw() * self.block_size as u64;
        let total = count as usize * self.block_size as usize;
        if buffer.len() < total {
            return Err(Ext4Error::invalid_input());
        }
        syscall::call::lseek(self.fd, offset as isize, syscall::flag::SEEK_SET)
            .map_err(|_| Ext4Error::io())?;
        let mut read_total = 0;
        while read_total < total {
            let n = syscall::call::read(self.fd, &mut buffer[read_total..total])
                .map_err(|_| Ext4Error::io())?;
            if n == 0 {
                return Err(Ext4Error::io());
            }
            read_total += n;
        }
        Ok(())
    }

    fn write(&mut self, buffer: &[u8], block_id: AbsoluteBN, count: u32) -> Ext4Result<()> {
        let offset = block_id.raw() * self.block_size as u64;
        let total = count as usize * self.block_size as usize;
        if buffer.len() < total {
            return Err(Ext4Error::invalid_input());
        }
        syscall::call::lseek(self.fd, offset as isize, syscall::flag::SEEK_SET)
            .map_err(|_| Ext4Error::io())?;
        let mut written_total = 0;
        while written_total < total {
            let n = syscall::call::write(self.fd, &buffer[written_total..total])
                .map_err(|_| Ext4Error::io())?;
            if n == 0 {
                return Err(Ext4Error::io());
            }
            written_total += n;
        }
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
        syscall::call::fsync(self.fd).map_err(|_| Ext4Error::io())?;
        Ok(())
    }

    fn current_time(&self) -> Ext4Result<Ext4Timestamp> {
        let mut ts = syscall::data::TimeSpec::default();
        syscall::call::clock_gettime(syscall::flag::CLOCK_REALTIME, &mut ts)
            .map_err(|_| Ext4Error::io())?;
        Ok(Ext4Timestamp::new(ts.tv_sec, ts.tv_nsec as u32))
    }
}
