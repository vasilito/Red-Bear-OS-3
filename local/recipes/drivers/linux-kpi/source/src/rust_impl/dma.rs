use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ffi::{c_char, c_void, CStr};
use std::ptr;
use std::sync::atomic::{fence, Ordering};
use std::sync::Mutex;

lazy_static::lazy_static! {
    static ref TRANSLATION_FD: Option<usize> = {
        libredox::call::open("/scheme/memory/translation", syscall::flag::O_CLOEXEC as i32, 0)
            .ok()
            .map(|fd| fd)
    };
}

#[cfg(target_os = "redox")]
fn virt_to_phys(virt: usize) -> usize {
    let raw = match *TRANSLATION_FD {
        Some(fd) => fd,
        None => return 0,
    };
    let mut buf = virt.to_ne_bytes();
    let _ = libredox::call::call_ro(raw, &mut buf, syscall::CallFlags::empty(), &[]);
    usize::from_ne_bytes(buf)
}

#[cfg(not(target_os = "redox"))]
fn virt_to_phys(virt: usize) -> usize {
    let _ = *TRANSLATION_FD;
    virt
}

fn sanitize_align(align: usize) -> Option<usize> {
    let align = align.max(1);
    if align.is_power_of_two() {
        Some(align)
    } else {
        align.checked_next_power_of_two()
    }
}

fn crosses_boundary(addr: u64, size: usize, boundary: usize) -> bool {
    if boundary == 0 || size == 0 {
        return false;
    }

    let end = match addr.checked_add(size.saturating_sub(1) as u64) {
        Some(end) => end,
        None => return true,
    };
    let mask = !(boundary as u64 - 1);
    (addr & mask) != (end & mask)
}

#[derive(Clone, Copy)]
struct PoolAllocation {
    vaddr: usize,
    dma: u64,
    size: usize,
    align: usize,
}

type AllocationList = Mutex<Vec<PoolAllocation>>;

#[repr(C)]
pub struct DmaPool {
    pub name: *mut u8,
    pub size: usize,
    pub align: usize,
    pub boundary: usize,
    pub allocations: *mut c_void,
    name_len: usize,
}

fn copy_pool_name(name: *const u8) -> (*mut u8, usize) {
    if name.is_null() {
        return (ptr::null_mut(), 0);
    }

    let c_name = unsafe { CStr::from_ptr(name.cast::<c_char>()) };
    let bytes = c_name.to_bytes();
    let mut owned = Vec::with_capacity(bytes.len() + 1);
    owned.extend_from_slice(bytes);
    owned.push(0);
    let len = owned.len();
    let ptr = owned.as_mut_ptr();
    std::mem::forget(owned);
    (ptr, len)
}

fn pool_allocations(pool: *mut DmaPool) -> Option<&'static AllocationList> {
    if pool.is_null() {
        return None;
    }
    let allocations = unsafe { (*pool).allocations.cast::<AllocationList>() };
    if allocations.is_null() {
        None
    } else {
        Some(unsafe { &*allocations })
    }
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

#[no_mangle]
pub extern "C" fn dma_pool_create(
    name: *const u8,
    _dev: *mut u8,
    size: usize,
    align: usize,
    boundary: usize,
) -> *mut DmaPool {
    if size == 0 {
        return ptr::null_mut();
    }

    let Some(align) = sanitize_align(align) else {
        return ptr::null_mut();
    };

    if boundary != 0 && size > boundary {
        return ptr::null_mut();
    }

    let allocations = Box::new(Mutex::new(Vec::<PoolAllocation>::new()));
    let (name_ptr, name_len) = copy_pool_name(name);
    Box::into_raw(Box::new(DmaPool {
        name: name_ptr,
        size,
        align,
        boundary,
        allocations: Box::into_raw(allocations).cast::<c_void>(),
        name_len,
    }))
}

#[no_mangle]
pub extern "C" fn dma_pool_destroy(pool: *mut DmaPool) {
    if pool.is_null() {
        return;
    }

    let allocations_ptr = unsafe { (*pool).allocations.cast::<AllocationList>() };
    if !allocations_ptr.is_null() {
        let allocations = unsafe { Box::from_raw(allocations_ptr) };
        let entries = allocations
            .lock()
            .map(|entries| entries.clone())
            .unwrap_or_default();
        for entry in entries {
            if let Ok(layout) = Layout::from_size_align(entry.size.max(1), entry.align.max(1)) {
                unsafe { dealloc(entry.vaddr as *mut u8, layout) };
            }
        }
    }

    let pool = unsafe { Box::from_raw(pool) };
    if !pool.name.is_null() && pool.name_len != 0 {
        unsafe {
            drop(Vec::from_raw_parts(pool.name, pool.name_len, pool.name_len));
        }
    }
}

#[no_mangle]
pub extern "C" fn dma_pool_alloc(pool: *mut DmaPool, _flags: u32, handle: *mut u64) -> *mut u8 {
    if pool.is_null() || handle.is_null() {
        return ptr::null_mut();
    }

    let pool_ref = unsafe { &*pool };
    if pool_ref.size == 0 {
        return ptr::null_mut();
    }

    let layout = match Layout::from_size_align(pool_ref.size, pool_ref.align.max(1)) {
        Ok(layout) => layout,
        Err(_) => return ptr::null_mut(),
    };

    let vaddr = unsafe { alloc_zeroed(layout) };
    if vaddr.is_null() {
        return ptr::null_mut();
    }

    let dma = virt_to_phys(vaddr as usize) as u64;
    if dma == 0 || crosses_boundary(dma, pool_ref.size, pool_ref.boundary) {
        unsafe { dealloc(vaddr, layout) };
        return ptr::null_mut();
    }

    let Some(allocations) = pool_allocations(pool) else {
        unsafe { dealloc(vaddr, layout) };
        return ptr::null_mut();
    };

    let Ok(mut entries) = allocations.lock() else {
        unsafe { dealloc(vaddr, layout) };
        return ptr::null_mut();
    };

    entries.push(PoolAllocation {
        vaddr: vaddr as usize,
        dma,
        size: pool_ref.size,
        align: pool_ref.align.max(1),
    });
    unsafe { *handle = dma };
    vaddr
}

#[no_mangle]
pub extern "C" fn dma_pool_free(pool: *mut DmaPool, vaddr: *mut u8, addr: u64) {
    if pool.is_null() || vaddr.is_null() {
        return;
    }

    let Some(allocations) = pool_allocations(pool) else {
        return;
    };

    let Ok(mut entries) = allocations.lock() else {
        return;
    };

    let Some(index) = entries
        .iter()
        .position(|entry| entry.vaddr == vaddr as usize || (addr != 0 && entry.dma == addr))
    else {
        return;
    };

    let entry = entries.swap_remove(index);
    if let Ok(layout) = Layout::from_size_align(entry.size.max(1), entry.align.max(1)) {
        unsafe { dealloc(entry.vaddr as *mut u8, layout) };
    }
}

#[no_mangle]
pub extern "C" fn dma_sync_single_for_cpu(_dev: *mut u8, addr: u64, size: usize, _dir: u32) {
    if addr == 0 || size == 0 {
        return;
    }
    fence(Ordering::Acquire);
}

#[no_mangle]
pub extern "C" fn dma_sync_single_for_device(_dev: *mut u8, addr: u64, size: usize, _dir: u32) {
    if addr == 0 || size == 0 {
        return;
    }
    fence(Ordering::Release);
}

#[no_mangle]
pub extern "C" fn dma_map_page(
    _dev: *mut u8,
    page: *mut u8,
    offset: usize,
    size: usize,
    _dir: u32,
) -> u64 {
    if page.is_null() || size == 0 {
        return 0;
    }

    let Some(vaddr) = (page as usize).checked_add(offset) else {
        return 0;
    };
    virt_to_phys(vaddr) as u64
}

#[no_mangle]
pub extern "C" fn dma_unmap_page(_dev: *mut u8, _addr: u64, _size: usize, _dir: u32) {}

#[no_mangle]
pub extern "C" fn dma_mapping_error(_dev: *mut u8, addr: u64) -> i32 {
    if addr == 0 {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn dma_alloc_and_map_work_on_host() {
        let mut handle = 0u64;
        let vaddr = dma_alloc_coherent(ptr::null_mut(), 128, &mut handle, 0);
        assert!(!vaddr.is_null());
        assert_ne!(handle, 0);
        assert_eq!(dma_mapping_error(ptr::null_mut(), handle), 0);
        assert_eq!(dma_map_single(ptr::null_mut(), vaddr, 128, 0), handle);
        dma_sync_single_for_cpu(ptr::null_mut(), handle, 128, 0);
        dma_sync_single_for_device(ptr::null_mut(), handle, 128, 0);
        dma_free_coherent(ptr::null_mut(), 128, vaddr, handle);
    }

    #[test]
    fn dma_pool_lifecycle_tracks_allocations() {
        let name = CString::new("iwlwifi-rx").expect("valid test CString");
        let pool = dma_pool_create(name.as_ptr().cast::<u8>(), ptr::null_mut(), 256, 64, 0);
        assert!(!pool.is_null());

        let mut handle = 0u64;
        let vaddr = dma_pool_alloc(pool, 0, &mut handle);
        assert!(!vaddr.is_null());
        assert_ne!(handle, 0);

        let allocations = unsafe { &*((*pool).allocations.cast::<AllocationList>()) };
        assert_eq!(allocations.lock().expect("lock allocations").len(), 1);

        dma_pool_free(pool, vaddr, handle);
        assert!(allocations.lock().expect("lock allocations").is_empty());
        dma_pool_destroy(pool);
    }

    #[test]
    fn dma_pool_rejects_impossible_boundary() {
        let pool = dma_pool_create(ptr::null(), ptr::null_mut(), 1024, 16, 128);
        assert!(pool.is_null());
    }

    #[test]
    fn dma_map_page_and_error_checks_work() {
        let mut page = [0u8; 64];
        let dma = dma_map_page(ptr::null_mut(), page.as_mut_ptr(), 8, 16, 0);
        assert_ne!(dma, 0);
        assert_eq!(dma_mapping_error(ptr::null_mut(), dma), 0);
        assert_eq!(dma_mapping_error(ptr::null_mut(), 0), 1);
        dma_unmap_page(ptr::null_mut(), dma, 16, 0);
    }
}
