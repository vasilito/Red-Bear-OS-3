use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::mem::{size_of, MaybeUninit};
use std::os::fd::AsRawFd;
use std::process::ExitCode;
use std::ptr::{self, NonNull};
use std::slice;

const DRM_CARD_PATH: &str = "/scheme/drm/card0";
const GEM_SIZE: usize = 4096;
const MAGIC_PATTERN: [u8; 16] = *b"RBOS-PRIME-TEST!";

const DRM_IOCTL_BASE: usize = 0x00A0;
const DRM_IOCTL_GEM_CREATE: usize = DRM_IOCTL_BASE + 26;
const DRM_IOCTL_GEM_CLOSE: usize = DRM_IOCTL_BASE + 27;
const DRM_IOCTL_GEM_MMAP: usize = DRM_IOCTL_BASE + 28;
const DRM_IOCTL_PRIME_HANDLE_TO_FD: usize = DRM_IOCTL_BASE + 29;
const DRM_IOCTL_PRIME_FD_TO_HANDLE: usize = DRM_IOCTL_BASE + 30;

const MAP_SHARED: i32 = 0x0001;
const PROT_WRITE: i32 = 0x0002;
const PROT_READ: i32 = 0x0004;
const MAP_FAILED: isize = -1;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct GemCreateWire {
    size: u64,
    handle: u32,
    _pad: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct GemCloseWire {
    handle: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct GemMmapWire {
    handle: u32,
    _pad: u32,
    offset: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct PrimeHandleToFdWire {
    handle: u32,
    flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct PrimeHandleToFdResponseWire {
    fd: i32,
    _pad: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct PrimeFdToHandleWire {
    fd: i32,
    _pad: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct PrimeFdToHandleResponseWire {
    handle: u32,
    _pad: u32,
}

unsafe extern "C" {
    fn mmap(
        addr: *mut c_void,
        len: usize,
        prot: i32,
        flags: i32,
        fd: i32,
        offset: isize,
    ) -> *mut c_void;
    fn munmap(addr: *mut c_void, len: usize) -> i32;
}

struct MappedRegion {
    ptr: NonNull<u8>,
    len: usize,
}

impl MappedRegion {
    fn map(file: &File, len: usize, offset: u64) -> io::Result<Self> {
        let offset = isize::try_from(offset).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("mmap offset {offset} does not fit in isize"),
            )
        })?;

        let ptr = unsafe {
            mmap(
                ptr::null_mut(),
                len,
                PROT_READ | PROT_WRITE,
                MAP_SHARED,
                file.as_raw_fd(),
                offset,
            )
        };

        if ptr as isize == MAP_FAILED {
            return Err(io::Error::last_os_error());
        }

        let ptr = NonNull::new(ptr.cast::<u8>())
            .ok_or_else(|| io::Error::other("mmap returned a null pointer"))?;

        Ok(Self { ptr, len })
    }

    fn as_slice(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl Drop for MappedRegion {
    fn drop(&mut self) {
        let _ = unsafe { munmap(self.ptr.as_ptr().cast::<c_void>(), self.len) };
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => {
            println!("PASS: PRIME DMA-BUF test completed");
            ExitCode::SUCCESS
        }
        Err(err) => {
            println!("FAIL: PRIME DMA-BUF test aborted: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> io::Result<()> {
    let mut card = step("open /scheme/drm/card0", || open_card())?;

    let gem = step("allocate GEM buffer", || {
        ioctl::<_, GemCreateWire>(
            &mut card,
            DRM_IOCTL_GEM_CREATE,
            &GemCreateWire {
                size: GEM_SIZE as u64,
                ..GemCreateWire::default()
            },
        )
    })?;
    println!("info: created GEM handle {}", gem.handle);

    let gem_map = step("request GEM mmap offset", || {
        ioctl::<_, GemMmapWire>(
            &mut card,
            DRM_IOCTL_GEM_MMAP,
            &GemMmapWire {
                handle: gem.handle,
                ..GemMmapWire::default()
            },
        )
    })?;
    println!("info: GEM mmap offset/address {:#x}", gem_map.offset);

    let mut gem_region = step("mmap GEM buffer", || {
        MappedRegion::map(&card, GEM_SIZE, gem_map.offset)
    })?;

    step("write magic pattern into GEM buffer", || {
        let bytes = gem_region.as_mut_slice();
        bytes.fill(0);
        bytes[..MAGIC_PATTERN.len()].copy_from_slice(&MAGIC_PATTERN);
        Ok(())
    })?;

    let export = step("export GEM handle via PRIME_HANDLE_TO_FD", || {
        ioctl::<_, PrimeHandleToFdResponseWire>(
            &mut card,
            DRM_IOCTL_PRIME_HANDLE_TO_FD,
            &PrimeHandleToFdWire {
                handle: gem.handle,
                flags: 0,
            },
        )
    })?;
    if export.fd < 0 {
        return Err(io::Error::other(format!(
            "scheme returned a negative PRIME fd token: {}",
            export.fd
        )));
    }
    println!("info: exported PRIME token {}", export.fd);

    let dmabuf_path = format!("{DRM_CARD_PATH}/dmabuf/{}", export.fd);
    let dmabuf = step("open exported dmabuf node", || {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(&dmabuf_path)
            .map_err(|err| {
                io::Error::new(err.kind(), format!("failed to open {dmabuf_path}: {err}"))
            })
    })?;
    println!("info: dmabuf fd {}", dmabuf.as_raw_fd());

    let dmabuf_region = step("mmap exported dmabuf fd", || {
        MappedRegion::map(&dmabuf, GEM_SIZE, 0)
    })?;

    step("verify dmabuf mapping sees magic pattern", || {
        let observed = &dmabuf_region.as_slice()[..MAGIC_PATTERN.len()];
        if observed != MAGIC_PATTERN {
            return Err(io::Error::other(format!(
                "expected {:?}, observed {:?}",
                MAGIC_PATTERN, observed
            )));
        }
        Ok(())
    })?;

    // The scheme's PRIME_FD_TO_HANDLE expects the opaque export token
    // (returned by PRIME_HANDLE_TO_FD), not the raw GEM handle.
    // In production, libdrm extracts the token via redox_fpath() on the dmabuf fd.
    let imported = step("import via PRIME_FD_TO_HANDLE using export token", || {
        ioctl::<_, PrimeFdToHandleResponseWire>(
            &mut card,
            DRM_IOCTL_PRIME_FD_TO_HANDLE,
            &PrimeFdToHandleWire {
                fd: export.fd,
                _pad: 0,
            },
        )
    })?;

    step("verify imported handle matches original GEM handle", || {
        if imported.handle != gem.handle {
            return Err(io::Error::other(format!(
                "imported handle {} did not match original {}",
                imported.handle, gem.handle
            )));
        }
        Ok(())
    })?;

    drop(dmabuf_region);
    drop(dmabuf);
    drop(gem_region);

    step("close GEM handle", || {
        ioctl_no_response(
            &mut card,
            DRM_IOCTL_GEM_CLOSE,
            &GemCloseWire { handle: gem.handle },
        )
    })?;

    test_stale_token_after_gem_close(&mut card)?;

    Ok(())
}

fn test_stale_token_after_gem_close(card: &mut File) -> io::Result<()> {
    let gem2 = step("stale-token: allocate second GEM", || {
        ioctl::<_, GemCreateWire>(
            card,
            DRM_IOCTL_GEM_CREATE,
            &GemCreateWire {
                size: GEM_SIZE as u64,
                ..GemCreateWire::default()
            },
        )
    })?;

    let export2 = step("stale-token: export via PRIME_HANDLE_TO_FD", || {
        ioctl::<_, PrimeHandleToFdResponseWire>(
            card,
            DRM_IOCTL_PRIME_HANDLE_TO_FD,
            &PrimeHandleToFdWire {
                handle: gem2.handle,
                flags: 0,
            },
        )
    })?;
    assert!(export2.fd >= 0);

    step("stale-token: close GEM before opening dmabuf", || {
        ioctl_no_response(
            card,
            DRM_IOCTL_GEM_CLOSE,
            &GemCloseWire {
                handle: gem2.handle,
            },
        )
    })?;

    step(
        "stale-token: open dmabuf with stale token must fail",
        || {
            let stale_path = format!("{DRM_CARD_PATH}/dmabuf/{}", export2.fd);
            match OpenOptions::new().read(true).write(true).open(&stale_path) {
                Ok(_) => Err(io::Error::other(
                    "expected ENOENT for stale token, but open succeeded",
                )),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(io::Error::other(format!("wrong error kind: {e}"))),
            }
        },
    )?;

    step(
        "stale-token: PRIME_FD_TO_HANDLE with stale token must fail",
        || match ioctl::<_, PrimeFdToHandleResponseWire>(
            card,
            DRM_IOCTL_PRIME_FD_TO_HANDLE,
            &PrimeFdToHandleWire {
                fd: export2.fd,
                _pad: 0,
            },
        ) {
            Ok(_) => Err(io::Error::other(
                "expected ENOENT for stale token, but import succeeded",
            )),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io::Error::other(format!("wrong error kind: {e}"))),
        },
    )
}

fn open_card() -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(DRM_CARD_PATH)
        .map_err(|err| io::Error::new(err.kind(), format!("failed to open {DRM_CARD_PATH}: {err}")))
}

fn step<T, F>(name: &str, action: F) -> io::Result<T>
where
    F: FnOnce() -> io::Result<T>,
{
    match action() {
        Ok(value) => {
            println!("PASS: {name}");
            Ok(value)
        }
        Err(err) => {
            println!("FAIL: {name}: {err}");
            Err(err)
        }
    }
}

fn ioctl<TReq, TResp>(file: &mut File, request: usize, payload: &TReq) -> io::Result<TResp>
where
    TReq: Copy,
    TResp: Copy,
{
    write_request(file, request, payload)?;
    read_plain(file)
}

fn ioctl_no_response<TReq>(file: &mut File, request: usize, payload: &TReq) -> io::Result<()>
where
    TReq: Copy,
{
    write_request(file, request, payload)?;
    let mut ack = [0_u8; 1];
    file.read_exact(&mut ack)?;
    Ok(())
}

fn write_request<TReq>(file: &mut File, request: usize, payload: &TReq) -> io::Result<()>
where
    TReq: Copy,
{
    let request = u64::try_from(request).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("request code {request} does not fit in u64"),
        )
    })?;

    file.write_all(&request.to_le_bytes())?;
    file.write_all(as_bytes(payload))?;
    Ok(())
}

fn read_plain<T>(file: &mut File) -> io::Result<T>
where
    T: Copy,
{
    let mut value = MaybeUninit::<T>::uninit();
    let buf = unsafe { slice::from_raw_parts_mut(value.as_mut_ptr().cast::<u8>(), size_of::<T>()) };
    file.read_exact(buf)?;
    Ok(unsafe { value.assume_init() })
}

fn as_bytes<T>(value: &T) -> &[u8] {
    unsafe { slice::from_raw_parts((value as *const T).cast::<u8>(), size_of::<T>()) }
}
