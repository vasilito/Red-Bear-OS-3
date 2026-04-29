use redox_scheme::{CallerCtx, OpenResult};
use scheme_utils::HandleMap;
use std::collections::VecDeque;
use std::str;
use syscall::error::{Error, Result, EACCES, EBADF, EINVAL, ENOENT, EWOULDBLOCK};

use redox_scheme::scheme::SchemeSync;
use syscall::schemev2::NewFdFlags;

// The strict buffer size of the audiohw: driver
const HW_BUFFER_SIZE: usize = 512;
// The desired buffer size of each handle
const HANDLE_BUFFER_SIZE: usize = 4096;

enum Handle {
    Audio { buffer: VecDeque<(i16, i16)> },
    // TODO: move volume to audiohw:?
    // TODO: Use SYS_CALL to handle this better?
    Volume,
    SchemeRoot,
}

pub struct AudioScheme {
    handles: HandleMap<Handle>,
    volume: i32,
}

impl AudioScheme {
    pub fn new() -> Self {
        AudioScheme {
            handles: HandleMap::new(),
            volume: 50,
        }
    }

    pub fn buffer(&mut self) -> [(i16, i16); HW_BUFFER_SIZE] {
        let mut mix_buffer = [(0i16, 0i16); HW_BUFFER_SIZE];

        // Multiply each sample by the cube of volume divided by 100
        // This mimics natural perception of loudness
        let volume_factor = ((self.volume as f32) / 100.0).powi(3);
        for (_id, handle) in self.handles.iter_mut() {
            match handle {
                Handle::Audio { ref mut buffer } => {
                    let mut i = 0;
                    while i < mix_buffer.len() {
                        if let Some(sample) = buffer.pop_front() {
                            let left = (sample.0 as f32 * volume_factor) as i16;
                            let right = (sample.1 as f32 * volume_factor) as i16;
                            mix_buffer[i].0 = mix_buffer[i].0.saturating_add(left);
                            mix_buffer[i].1 = mix_buffer[i].1.saturating_add(right);
                        } else {
                            break;
                        }
                        i += 1;
                    }
                }
                _ => (),
            }
        }

        mix_buffer
    }
}

impl SchemeSync for AudioScheme {
    fn scheme_root(&mut self) -> Result<usize> {
        Ok(self.handles.insert(Handle::SchemeRoot))
    }
    fn openat(
        &mut self,
        dirfd: usize,
        path: &str,
        _flags: usize,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<OpenResult> {
        if !matches!(self.handles.get(dirfd)?, Handle::SchemeRoot) {
            return Err(Error::new(EACCES));
        }

        let (handle, flags) = match path.trim_matches('/') {
            "" => (
                Handle::Audio {
                    buffer: VecDeque::new(),
                },
                NewFdFlags::empty(),
            ),
            "volume" => (Handle::Volume, NewFdFlags::POSITIONED),
            _ => return Err(Error::new(ENOENT)),
        };

        let id = self.handles.insert(handle);

        Ok(OpenResult::ThisScheme { number: id, flags })
    }

    fn read(
        &mut self,
        id: usize,
        buf: &mut [u8],
        off: u64,
        _flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        //TODO: check flags for readable
        match self.handles.get_mut(id)? {
            Handle::Audio { buffer: _ } => {
                //TODO: audio input?
                Err(Error::new(EBADF))
            }
            Handle::Volume => {
                let Ok(off) = usize::try_from(off) else {
                    return Ok(0);
                };
                //TODO: should we allocate every time?
                let bytes = format!("{}", self.volume).into_bytes();
                let src = bytes.get(off..).unwrap_or(&[]);
                let len = src.len().min(buf.len());
                buf[..len].copy_from_slice(&src[..len]);

                Ok(len)
            }
            Handle::SchemeRoot => Err(Error::new(EBADF)),
        }
    }

    fn write(
        &mut self,
        id: usize,
        buf: &[u8],
        offset: u64,
        _flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        //TODO: check flags for writable
        match self.handles.get_mut(id)? {
            Handle::Audio { ref mut buffer } => {
                if buffer.len() >= HANDLE_BUFFER_SIZE {
                    Err(Error::new(EWOULDBLOCK))
                } else {
                    let mut i = 0;
                    while i + 4 <= buf.len() {
                        buffer.push_back((
                            (buf[i] as i16) | ((buf[i + 1] as i16) << 8),
                            (buf[i + 2] as i16) | ((buf[i + 3] as i16) << 8),
                        ));

                        i += 4;
                    }

                    Ok(i)
                }
            }
            Handle::Volume => {
                //TODO: support other offsets?
                if offset == 0 {
                    let value = str::from_utf8(buf)
                        .map_err(|_| Error::new(EINVAL))?
                        .trim()
                        .parse::<i32>()
                        .map_err(|_| Error::new(EINVAL))?;
                    if value >= 0 && value <= 100 {
                        self.volume = value;
                        Ok(buf.len())
                    } else {
                        Err(Error::new(EINVAL))
                    }
                } else {
                    // EOF
                    Ok(0)
                }
            }
            Handle::SchemeRoot => Err(Error::new(EBADF)),
        }
    }
}
