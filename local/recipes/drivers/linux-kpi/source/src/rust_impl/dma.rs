use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ptr;

use syscall::CallFlags;

lazy_static::lazy_static! {
    static ref TRANSLATION_FD: Option<usize> = {
        libredox::call::open("/scheme/memory/translation",
            syscall::flag::O_CLOEXEC as i32, 0)
            .ok()
            .map(|fd| fd)
    };
}

fn virt_to_phys(virt: usize) -> usize {
    let raw = match *TRANSLATION_FD {
        Some(fd) => fd,
        None => return 0,
    };
    let mut buf = virt.to_ne_bytes();
    let _ = libredox::call::call_ro(raw, &mut buf, CallFlags::empty(), &[]);
    usize::from_ne_bytes(buf)
}

#[no_mangle]
pub extern "C" fn dma_alloc_coherent(
    _dev: *mut u8,
    size: usize,
    dma_handle: *mut u64,
    _flags: u32,
) -> *mut u8 {
    if size == 0 || dma_handle.is_null() {
        return ptr::null_mut();
    }

    let layout = match Layout::from_size_align(size, 4096) {
        Ok(l) => l,
        Err(_) => return ptr::null_mut(),
    };

    let vaddr = unsafe { alloc_zeroed(layout) };
    if vaddr.is_null() {
        return ptr::null_mut();
    }

    let phys = virt_to_phys(vaddr as usize);
    if phys == 0 {
        unsafe { dealloc(vaddr, layout) };
        return ptr::null_mut();
    }

    unsafe { *dma_handle = phys as u64 };
    log::debug!(
        "dma_alloc_coherent: {} bytes at virt={:#x} phys={:#x}",
        size,
        vaddr as usize,
        phys
    );
    vaddr
}

#[no_mangle]
pub extern "C" fn dma_free_coherent(_dev: *mut u8, size: usize, vaddr: *mut u8, _dma_handle: u64) {
    if vaddr.is_null() || size == 0 {
        return;
    }
    let layout = match Layout::from_size_align(size, 4096) {
        Ok(l) => l,
        Err(_) => return,
    };
    unsafe { dealloc(vaddr, layout) };
}

#[no_mangle]
pub extern "C" fn dma_map_single(_dev: *mut u8, ptr: *mut u8, _size: usize, _dir: u32) -> u64 {
    if ptr.is_null() {
        return 0;
    }
    virt_to_phys(ptr as usize) as u64
}

#[no_mangle]
pub extern "C" fn dma_unmap_single(_dev: *mut u8, _addr: u64, _size: usize, _dir: u32) {}

#[no_mangle]
pub extern "C" fn dma_set_mask(_dev: *mut u8, _mask: u64) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn dma_set_coherent_mask(_dev: *mut u8, _mask: u64) -> i32 {
    0
}
