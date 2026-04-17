use std::io::{self, Read, Seek, SeekFrom, Write};

pub struct RedoxDisk {
    fd: usize,
    size: u64,
}

impl RedoxDisk {
    pub fn open(disk_path: &str) -> syscall::error::Result<Self> {
        let fd = libredox::call::open(disk_path, libredox::flag::O_RDWR, 0)?;
        let mut stat = syscall::data::Stat::default();
        syscall::call::fstat(fd, &mut stat)?;
        Ok(Self {
            fd,
            size: stat.st_size,
        })
    }

    pub fn size(&self) -> u64 {
        self.size
    }
}

impl Read for RedoxDisk {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        syscall::call::read(self.fd, buf)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("redox read: {e:?}")))
    }
}

impl Write for RedoxDisk {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        syscall::call::write(self.fd, buf)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("redox write: {e:?}")))
    }

    fn flush(&mut self) -> io::Result<()> {
        syscall::call::fsync(self.fd)
            .map(|_| ())
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("redox flush: {e:?}")))
    }
}

impl Seek for RedoxDisk {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let offset = match pos {
            SeekFrom::Start(off) => off as isize,
            SeekFrom::Current(off) => off as isize,
            SeekFrom::End(off) => (self.size as isize) + (off as isize),
        };
        let result = syscall::call::lseek(self.fd, offset, syscall::flag::SEEK_SET)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("redox seek: {e:?}")))?;
        Ok(result as u64)
    }
}
