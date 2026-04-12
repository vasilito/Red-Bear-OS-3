use std::collections::HashMap;
use std::ptr;
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

    let ptr = match redox_driver_sys::memory::MmioRegion::map(
        phys,
        size,
        redox_driver_sys::memory::CacheType::DeviceMemory,
        redox_driver_sys::memory::MmioProt::READ_WRITE,
    ) {
        Ok(region) => {
            let p = region.as_ptr() as *mut u8;
            let s = region.size();
            if let Ok(mut tracker) = MMIO_MAP_TRACKER.lock() {
                tracker.insert(p as usize, MappedRegion { size: s });
            }
            std::mem::forget(region);
            p
        }
        Err(e) => {
            log::error!("ioremap: failed to map {:#x}+{:#x}: {:?}", phys, size, e);
            ptr::null_mut()
        }
    };

    ptr
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
