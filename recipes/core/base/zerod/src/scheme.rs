use redox_scheme::scheme::SchemeSync;
use redox_scheme::{CallerCtx, OpenResult};
use scheme_utils::FpathWriter;
use syscall::{error::*, schemev2::NewFdFlags, MODE_CHR};

use crate::Ty;

pub struct ZeroScheme(pub Ty);

const SCHEME_ROOT_ID: usize = 1;

impl SchemeSync for ZeroScheme {
    fn scheme_root(&mut self) -> Result<usize> {
        Ok(SCHEME_ROOT_ID)
    }
    fn openat(
        &mut self,
        dirfd: usize,
        _path: &str,
        _flags: usize,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<OpenResult> {
        if dirfd != SCHEME_ROOT_ID {
            return Err(Error::new(EACCES));
        }
        Ok(OpenResult::ThisScheme {
            number: 0,
            flags: NewFdFlags::empty(),
        })
    }
    fn read(
        &mut self,
        _id: usize,
        buf: &mut [u8],
        _offset: u64,
        _flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        match self.0 {
            Ty::Null => Ok(0),
            Ty::Zero => {
                buf.fill(0);
                Ok(buf.len())
            }
        }
    }

    fn write(
        &mut self,
        _id: usize,
        buf: &[u8],
        _offset: u64,
        _flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        Ok(buf.len())
    }

    fn fcntl(&mut self, _id: usize, _cmd: usize, _arg: usize, _ctx: &CallerCtx) -> Result<usize> {
        Ok(0)
    }
    fn fsize(&mut self, _id: usize, _ctx: &CallerCtx) -> Result<u64> {
        Ok(0)
    }
    fn ftruncate(&mut self, _id: usize, _len: u64, _ctx: &CallerCtx) -> Result<()> {
        Ok(())
    }

    fn fpath(&mut self, _id: usize, buf: &mut [u8], _ctx: &CallerCtx) -> Result<usize> {
        FpathWriter::with(buf, "zero", |_| Ok(()))
    }

    fn fsync(&mut self, _id: usize, _ctx: &CallerCtx) -> Result<()> {
        Ok(())
    }

    fn fstat(&mut self, _id: usize, stat: &mut syscall::Stat, _ctx: &CallerCtx) -> Result<()> {
        stat.st_mode = 0o666 | MODE_CHR;
        stat.st_size = 0;
        stat.st_blocks = 0;
        stat.st_blksize = 4096;
        stat.st_nlink = 1;

        Ok(())
    }
}
