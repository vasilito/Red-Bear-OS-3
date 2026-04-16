use std::alloc::Layout;
use std::collections::HashMap;
use std::sync::Mutex;

const GFP_DMA32: u32 = 2;

/// Wrapper to make raw pointers `Send`, required because `DEVRES_MAP` is a
/// global `Mutex` (which needs `T: Send`). Raw pointers are not `Send` by
/// default since the compiler can't prove thread-safety. Here each tracked
/// pointer is exclusively owned by the device that allocated it — only
/// freed via `devm_kfree` or `devres_free_all` — so sending across threads is
/// safe.
struct TrackedAlloc(*mut u8);
unsafe impl Send for TrackedAlloc {}

lazy_static::lazy_static! {
    static ref DEVRES_MAP: Mutex<HashMap<usize, Vec<TrackedAlloc>>> =
        Mutex::new(HashMap::new());
}

fn align_up(size: usize, align: usize) -> usize {
    (size + align - 1) & !(align - 1)
}

fn tracked_layout(size: usize, flags: u32) -> Option<Layout> {
    if size == 0 {
        return None;
    }

    if flags & GFP_DMA32 != 0 {
        return Layout::from_size_align(size, 4096).ok();
    }

    let aligned_size = align_up(size, 16);
    Layout::from_size_align(aligned_size, 16).ok()
}

#[no_mangle]
pub extern "C" fn devm_kzalloc(dev: *mut u8, size: usize, flags: u32) -> *mut u8 {
    let ptr = super::memory::kzalloc(size, flags);
    if ptr.is_null() || dev.is_null() {
        return ptr;
    }

    if tracked_layout(size, flags).is_none() {
        return ptr;
    }

    if let Ok(mut devres_map) = DEVRES_MAP.lock() {
        devres_map
            .entry(dev as usize)
            .or_default()
            .push(TrackedAlloc(ptr));
    }

    ptr
}

#[no_mangle]
pub extern "C" fn devm_kfree(dev: *mut u8, ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }

    if !dev.is_null() {
        if let Ok(mut devres_map) = DEVRES_MAP.lock() {
            let dev_key = dev as usize;
            let should_remove = if let Some(entries) = devres_map.get_mut(&dev_key) {
                if let Some(index) = entries.iter().position(|alloc| alloc.0 == ptr) {
                    entries.swap_remove(index);
                }
                entries.is_empty()
            } else {
                false
            };

            if should_remove {
                devres_map.remove(&dev_key);
            }
        }
    }

    super::memory::kfree(ptr);
}

#[no_mangle]
pub extern "C" fn devres_free_all(dev: *mut u8) {
    if dev.is_null() {
        return;
    }

    let allocations = match DEVRES_MAP.lock() {
        Ok(mut devres_map) => devres_map.remove(&(dev as usize)),
        Err(_) => None,
    };

    if let Some(allocations) = allocations {
        for alloc in allocations {
            super::memory::kfree(alloc.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devm_kzalloc_and_devm_kfree_round_trip() {
        let dev: *mut u8 = 0xDEAD_0000 as *mut u8;
        let ptr = devm_kzalloc(dev, 64, 0);
        assert!(!ptr.is_null(), "devm_kzalloc should return non-null");

        let bytes = unsafe { std::slice::from_raw_parts(ptr, 64) };
        assert!(
            bytes.iter().all(|&b| b == 0),
            "devm_kzalloc should zero memory"
        );

        devm_kfree(dev, ptr);
    }

    #[test]
    fn devm_kfree_null_is_safe() {
        devm_kfree(0xDEAD_0001 as *mut u8, std::ptr::null_mut());
    }

    #[test]
    fn devres_free_all_cleans_up() {
        let dev: *mut u8 = 0xBEEF_0000 as *mut u8;
        let p1 = devm_kzalloc(dev, 32, 0);
        let p2 = devm_kzalloc(dev, 64, 0);
        assert!(!p1.is_null());
        assert!(!p2.is_null());

        devres_free_all(dev);
    }

    #[test]
    fn devres_free_all_null_is_safe() {
        devres_free_all(std::ptr::null_mut());
    }

    #[test]
    fn devm_kzalloc_null_dev_returns_untracked() {
        let ptr = devm_kzalloc(std::ptr::null_mut(), 32, 0);
        assert!(!ptr.is_null());
        crate::rust_impl::memory::kfree(ptr);
    }

    #[test]
    fn tracked_layout_rejects_zero_size() {
        assert!(tracked_layout(0, 0).is_none());
    }
}
