use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::collections::HashMap;
use std::ptr;
use std::sync::Mutex;

use syscall::{flag, CallFlags};

struct SendU8Ptr(*mut u8);

impl SendU8Ptr {
    #[allow(dead_code)]
    fn as_ptr(&self) -> *mut u8 {
        self.0
    }
}

unsafe impl Send for SendU8Ptr {}

impl PartialEq for SendU8Ptr {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for SendU8Ptr {}

impl std::hash::Hash for SendU8Ptr {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (self.0 as usize).hash(state);
    }
}

lazy_static::lazy_static! {
    static ref ALLOC_TRACKER: Mutex<HashMap<SendU8Ptr, Layout>> = Mutex::new(HashMap::new());
    static ref DMA32_TRACKER: Mutex<HashMap<SendU8Ptr, Layout>> = Mutex::new(HashMap::new());
}

fn align_up(size: usize, align: usize) -> usize {
    (size + align - 1) & !(align - 1)
}

/// Translate virtual address to physical address via scheme:memory/translation.
/// Returns 0 on failure.
fn virt_to_phys(virt: usize) -> usize {
    let fd = match libredox::Fd::open("/scheme/memory/translation", flag::O_CLOEXEC as i32, 0) {
        Ok(f) => f,
        Err(_) => return 0,
    };

    let mut buf = virt.to_ne_bytes();
    let _ = libredox::call::call_ro(fd.raw(), &mut buf, CallFlags::empty(), &[]);
    usize::from_ne_bytes(buf)
}

const GFP_DMA32_RETRIES: usize = 8;
const DMA32_LIMIT: u64 = 0x1_0000_0000;

/// Allocate memory with physical address below 4GB (GFP_DMA32).
/// Tries up to GFP_DMA32_RETRIES allocations; if none land below 4GB,
/// returns null rather than giving a buffer the device can't DMA to.
fn dma32_alloc(size: usize) -> *mut u8 {
    let layout = match Layout::from_size_align(size, 4096) {
        Ok(l) => l,
        Err(_) => return ptr::null_mut(),
    };

    for attempt in 0..GFP_DMA32_RETRIES {
        let candidate = unsafe { alloc_zeroed(layout) };
        if candidate.is_null() {
            return ptr::null_mut();
        }

        let phys = virt_to_phys(candidate as usize);
        if phys == 0 {
            log::warn!(
                "dma32_alloc: virt_to_phys failed for {:#x}",
                candidate as usize
            );
            unsafe { dealloc(candidate, layout) };
            continue;
        }

        if phys as u64 >= DMA32_LIMIT {
            log::debug!(
                "dma32_alloc: attempt {} phys={:#x} >= 4GB, retrying",
                attempt,
                phys
            );
            unsafe { dealloc(candidate, layout) };
            continue;
        }

        log::debug!(
            "dma32_alloc: {} bytes at virt={:#x} phys={:#x} (< 4GB)",
            size,
            candidate as usize,
            phys
        );

        if let Ok(mut tracker) = DMA32_TRACKER.lock() {
            tracker.insert(SendU8Ptr(candidate), layout);
        } else {
            unsafe { dealloc(candidate, layout) };
            return ptr::null_mut();
        }
        return candidate;
    }

    log::warn!(
        "dma32_alloc: failed to get <4GB physical address after {} retries for {} bytes",
        GFP_DMA32_RETRIES,
        size
    );
    ptr::null_mut()
}

const GFP_KERNEL: u32 = 0;
const GFP_ATOMIC: u32 = 1;
const GFP_DMA32: u32 = 2;

#[no_mangle]
/// Allocate kernel memory.
/// GFP_DMA32 flag routes through a dedicated path with physical address verification
/// to ensure allocations are suitable for devices with 32-bit DMA limitations.
pub extern "C" fn kmalloc(size: usize, flags: u32) -> *mut u8 {
    if size == 0 {
        return ptr::null_mut();
    }

    // Handle GFP_DMA32 allocations via dedicated path
    if flags & GFP_DMA32 != 0 {
        return dma32_alloc(size);
    }

    let aligned_size = align_up(size, 16);
    let layout = match Layout::from_size_align(aligned_size, 16) {
        Ok(l) => l,
        Err(_) => return ptr::null_mut(),
    };
    let ptr = unsafe { alloc_zeroed(layout) };
    if ptr.is_null() {
        return ptr::null_mut();
    }
    if let Ok(mut tracker) = ALLOC_TRACKER.lock() {
        tracker.insert(SendU8Ptr(ptr), layout);
    }
    ptr
}

#[no_mangle]
pub extern "C" fn kzalloc(size: usize, flags: u32) -> *mut u8 {
    let ptr = kmalloc(size, flags);
    if !ptr.is_null() {
        unsafe { ptr::write_bytes(ptr, 0, size) };
    }
    ptr
}

#[no_mangle]
pub extern "C" fn kfree(ptr: *const u8) {
    if ptr.is_null() {
        return;
    }

    // Check DMA32 tracker first
    {
        let mut dma32_tracker = match DMA32_TRACKER.lock() {
            Ok(t) => t,
            Err(_) => return,
        };
        if let Some(layout) = dma32_tracker.remove(&SendU8Ptr(ptr as *mut u8)) {
            unsafe { dealloc(ptr as *mut u8, layout) };
            return;
        }
    }

    // Check regular allocator tracker
    let layout = {
        let mut tracker = match ALLOC_TRACKER.lock() {
            Ok(t) => t,
            Err(_) => return,
        };
        match tracker.remove(&SendU8Ptr(ptr as *mut u8)) {
            Some(l) => l,
            None => return,
        }
    };
    unsafe { dealloc(ptr as *mut u8, layout) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kmalloc_basic() {
        let p = kmalloc(64, GFP_KERNEL);
        assert!(!p.is_null());
        kfree(p);
    }

    #[test]
    fn test_kzalloc_zeroed() {
        let p = kzalloc(64, GFP_KERNEL);
        assert!(!p.is_null());
        for i in 0..64 {
            assert_eq!(unsafe { *p.add(i) }, 0);
        }
        kfree(p);
    }

    #[test]
    fn test_kfree_null() {
        kfree(ptr::null());
    }

    #[test]
    fn test_kmalloc_zero_size() {
        assert!(kmalloc(0, GFP_KERNEL).is_null());
    }

    #[test]
    fn test_kmalloc_dma32_basic() {
        let p = kmalloc(64, GFP_DMA32);
        assert!(!p.is_null(), "GFP_DMA32 allocation should succeed");
        kfree(p);
    }

    #[test]
    fn test_kmalloc_dma32_zero_size() {
        assert!(
            kmalloc(0, GFP_DMA32).is_null(),
            "GFP_DMA32 with size 0 should return null"
        );
    }

    #[test]
    fn test_kfree_dma32_null() {
        // kfree(null) should not crash
        kfree(ptr::null());
    }

    #[test]
    fn test_kmalloc_dma32_multiple() {
        // Allocate and free multiple DMA32 buffers
        let p1 = kmalloc(128, GFP_DMA32);
        let p2 = kmalloc(256, GFP_DMA32);
        assert!(!p1.is_null());
        assert!(!p2.is_null());
        kfree(p1);
        kfree(p2);
    }
}
