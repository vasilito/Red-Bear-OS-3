use std::collections::HashMap;
use std::ptr;
use std::sync::atomic::{fence, Ordering};
use std::sync::Mutex;

type PhysAddr = u64;

struct MappedRegion {
    size: usize,
}

lazy_static::lazy_static! {
    static ref MMIO_MAP_TRACKER: Mutex<HashMap<usize, MappedRegion>> = Mutex::new(HashMap::new());
}

#[no_mangle]
pub extern "C" fn ioremap(phys: PhysAddr, size: usize) -> *mut u8 {
    if size == 0 || phys == 0 {
        return ptr::null_mut();
    }

    log::info!(
        "ioremap(phys={:#x}, size={}) — mapping via scheme:memory",
        phys,
        size
    );

    match redox_driver_sys::memory::MmioRegion::map(
        phys,
        size,
        redox_driver_sys::memory::CacheType::DeviceMemory,
        redox_driver_sys::memory::MmioProt::READ_WRITE,
    ) {
        Ok(region) => {
            let ptr = region.as_ptr() as *mut u8;
            let size = region.size();
            if let Ok(mut tracker) = MMIO_MAP_TRACKER.lock() {
                tracker.insert(ptr as usize, MappedRegion { size });
            }
            std::mem::forget(region);
            ptr
        }
        Err(e) => {
            log::error!("ioremap: failed to map {:#x}+{:#x}: {:?}", phys, size, e);
            ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "C" fn iounmap(addr: *mut u8, size: usize) {
    if addr.is_null() || size == 0 {
        return;
    }

    if let Ok(mut tracker) = MMIO_MAP_TRACKER.lock() {
        if let Some(region) = tracker.remove(&(addr as usize)) {
            let _ = unsafe { libredox::call::munmap(addr as *mut (), region.size) };
        }
    }
}

#[no_mangle]
pub extern "C" fn readl(addr: *const u8) -> u32 {
    if addr.is_null() {
        return 0;
    }
    unsafe { ptr::read_volatile(addr as *const u32) }
}

#[no_mangle]
pub extern "C" fn writel(val: u32, addr: *mut u8) {
    if addr.is_null() {
        return;
    }
    unsafe { ptr::write_volatile(addr as *mut u32, val) };
}

#[no_mangle]
pub extern "C" fn readq(addr: *const u8) -> u64 {
    if addr.is_null() {
        return 0;
    }
    unsafe { ptr::read_volatile(addr as *const u64) }
}

#[no_mangle]
pub extern "C" fn writeq(val: u64, addr: *mut u8) {
    if addr.is_null() {
        return;
    }
    unsafe { ptr::write_volatile(addr as *mut u64, val) };
}

#[no_mangle]
pub extern "C" fn readb(addr: *const u8) -> u8 {
    if addr.is_null() {
        return 0;
    }
    unsafe { ptr::read_volatile(addr) }
}

#[no_mangle]
pub extern "C" fn writeb(val: u8, addr: *mut u8) {
    if addr.is_null() {
        return;
    }
    unsafe { ptr::write_volatile(addr, val) };
}

#[no_mangle]
pub extern "C" fn readw(addr: *const u8) -> u16 {
    if addr.is_null() {
        return 0;
    }
    unsafe { ptr::read_volatile(addr as *const u16) }
}

#[no_mangle]
pub extern "C" fn writew(val: u16, addr: *mut u8) {
    if addr.is_null() {
        return;
    }
    unsafe { ptr::write_volatile(addr as *mut u16, val) };
}

#[no_mangle]
pub extern "C" fn memcpy_toio(dst: *mut u8, src: *const u8, count: usize) {
    if dst.is_null() || src.is_null() || count == 0 {
        return;
    }
    unsafe { ptr::copy_nonoverlapping(src, dst, count) };
}

#[no_mangle]
pub extern "C" fn memcpy_fromio(dst: *mut u8, src: *const u8, count: usize) {
    if dst.is_null() || src.is_null() || count == 0 {
        return;
    }
    unsafe { ptr::copy_nonoverlapping(src, dst, count) };
}

#[no_mangle]
pub extern "C" fn memset_io(dst: *mut u8, val: u8, count: usize) {
    if dst.is_null() || count == 0 {
        return;
    }
    unsafe { ptr::write_bytes(dst, val, count) };
}

#[no_mangle]
pub extern "C" fn mb() {
    fence(Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn rmb() {
    fence(Ordering::Acquire);
}

#[no_mangle]
pub extern "C" fn wmb() {
    fence(Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_copy_helpers_move_bytes() {
        let mut dst = [0u8; 8];
        let src = [1u8, 2, 3, 4, 5, 6, 7, 8];
        memcpy_toio(dst.as_mut_ptr(), src.as_ptr(), src.len());
        assert_eq!(dst, src);

        let mut second = [0u8; 8];
        memcpy_fromio(second.as_mut_ptr(), dst.as_ptr(), dst.len());
        assert_eq!(second, src);

        memset_io(second.as_mut_ptr(), 0xaa, second.len());
        assert_eq!(second, [0xaa; 8]);
    }

    #[test]
    fn io_barriers_are_callable() {
        mb();
        rmb();
        wmb();
    }
}
