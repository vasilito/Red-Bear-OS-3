use std::collections::HashMap;
use std::ptr;

const EINVAL: i32 = 22;
const ENOSPC: i32 = 28;

#[repr(C)]
pub struct Idr {
    map: HashMap<u32, usize>,
    next_id: u32,
}

#[no_mangle]
pub extern "C" fn idr_init(idr: *mut Idr) {
    if idr.is_null() {
        return;
    }

    unsafe {
        ptr::write(
            idr,
            Idr {
                map: HashMap::new(),
                next_id: 0,
            },
        );
    }
}

fn normalize_id(value: i32) -> Option<u32> {
    if value < 0 {
        None
    } else {
        Some(value as u32)
    }
}

#[no_mangle]
pub extern "C" fn idr_alloc(idr: *mut Idr, ptr: *mut u8, start: i32, end: i32, _gfp: u32) -> i32 {
    if idr.is_null() {
        return -EINVAL;
    }

    let start = match normalize_id(start) {
        Some(start) => start,
        None => return -EINVAL,
    };
    let end = match end {
        0 => None,
        value if value > 0 => Some(value as u32),
        _ => return -EINVAL,
    };

    if let Some(end) = end {
        if start >= end {
            return -EINVAL;
        }
    }

    let idr_ref = unsafe { &mut *idr };
    let initial = idr_ref.next_id.max(start);

    if let Some(end) = end {
        for candidate in initial..end {
            if let std::collections::hash_map::Entry::Vacant(entry) = idr_ref.map.entry(candidate) {
                entry.insert(ptr as usize);
                idr_ref.next_id = candidate.saturating_add(1);
                if idr_ref.next_id >= end {
                    idr_ref.next_id = start;
                }
                return candidate as i32;
            }
        }

        for candidate in start..initial {
            if let std::collections::hash_map::Entry::Vacant(entry) = idr_ref.map.entry(candidate) {
                entry.insert(ptr as usize);
                idr_ref.next_id = candidate.saturating_add(1);
                if idr_ref.next_id >= end {
                    idr_ref.next_id = start;
                }
                return candidate as i32;
            }
        }

        return -ENOSPC;
    }

    for candidate in initial..=u32::MAX {
        if let std::collections::hash_map::Entry::Vacant(entry) = idr_ref.map.entry(candidate) {
            entry.insert(ptr as usize);
            idr_ref.next_id = if candidate == u32::MAX {
                start
            } else {
                candidate.saturating_add(1).max(start)
            };
            return candidate as i32;
        }
    }

    for candidate in start..initial {
        if let std::collections::hash_map::Entry::Vacant(entry) = idr_ref.map.entry(candidate) {
            entry.insert(ptr as usize);
            idr_ref.next_id = if candidate == u32::MAX {
                start
            } else {
                candidate.saturating_add(1).max(start)
            };
            return candidate as i32;
        }
    }

    -ENOSPC
}

#[no_mangle]
pub extern "C" fn idr_find(idr: *mut Idr, id: u32) -> *mut u8 {
    if idr.is_null() {
        return ptr::null_mut();
    }

    let idr_ref = unsafe { &*idr };
    match idr_ref.map.get(&id) {
        Some(value) => *value as *mut u8,
        None => ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn idr_remove(idr: *mut Idr, id: u32) {
    if idr.is_null() {
        return;
    }

    let idr_ref = unsafe { &mut *idr };
    idr_ref.map.remove(&id);
    if id < idr_ref.next_id {
        idr_ref.next_id = id;
    }
}

#[no_mangle]
pub extern "C" fn idr_destroy(idr: *mut Idr) {
    if idr.is_null() {
        return;
    }

    let idr_ref = unsafe { &mut *idr };
    idr_ref.map.clear();
    idr_ref.next_id = 0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idr_alloc_and_find_round_trip() {
        let mut idr = std::mem::MaybeUninit::<Idr>::uninit();
        idr_init(idr.as_mut_ptr());

        let ptr1: *mut u8 = 0x1000 as *mut u8;
        let id1 = idr_alloc(idr.as_mut_ptr(), ptr1, 1, 0, 0);
        assert!(id1 >= 1, "allocated ID should be >= start");

        assert_eq!(idr_find(idr.as_mut_ptr(), id1 as u32), ptr1);
        assert_eq!(idr_find(idr.as_mut_ptr(), 9999), std::ptr::null_mut());

        idr_destroy(idr.as_mut_ptr());
    }

    #[test]
    fn idr_remove_frees_slot() {
        let mut idr = std::mem::MaybeUninit::<Idr>::uninit();
        idr_init(idr.as_mut_ptr());

        let ptr1: *mut u8 = 0x2000 as *mut u8;
        let id1 = idr_alloc(idr.as_mut_ptr(), ptr1, 10, 0, 0);
        assert!(id1 >= 10);

        idr_remove(idr.as_mut_ptr(), id1 as u32);
        assert_eq!(idr_find(idr.as_mut_ptr(), id1 as u32), std::ptr::null_mut());

        idr_destroy(idr.as_mut_ptr());
    }

    #[test]
    fn idr_alloc_with_bounded_range() {
        let mut idr = std::mem::MaybeUninit::<Idr>::uninit();
        idr_init(idr.as_mut_ptr());

        let ptr1: *mut u8 = 0x3000 as *mut u8;
        let id1 = idr_alloc(idr.as_mut_ptr(), ptr1, 5, 8, 0);
        assert!(id1 >= 5 && id1 < 8, "ID should be in [5, 8)");

        idr_destroy(idr.as_mut_ptr());
    }

    #[test]
    fn idr_alloc_returns_enospc_when_full() {
        let mut idr = std::mem::MaybeUninit::<Idr>::uninit();
        idr_init(idr.as_mut_ptr());

        let ptr1: *mut u8 = 0x4000 as *mut u8;
        let id1 = idr_alloc(idr.as_mut_ptr(), ptr1, 1, 2, 0);
        assert_eq!(id1, 1);

        let ptr2: *mut u8 = 0x4001 as *mut u8;
        let id2 = idr_alloc(idr.as_mut_ptr(), ptr2, 1, 2, 0);
        assert_eq!(id2, -ENOSPC);

        idr_destroy(idr.as_mut_ptr());
    }

    #[test]
    fn idr_null_pointers_are_safe() {
        assert_eq!(
            idr_alloc(std::ptr::null_mut(), std::ptr::null_mut(), 1, 0, 0),
            -EINVAL
        );
        assert_eq!(idr_find(std::ptr::null_mut(), 1), std::ptr::null_mut());
        idr_remove(std::ptr::null_mut(), 1);
        idr_destroy(std::ptr::null_mut());
        idr_init(std::ptr::null_mut());
    }

    #[test]
    fn idr_alloc_reuses_removed_id() {
        let mut idr = std::mem::MaybeUninit::<Idr>::uninit();
        idr_init(idr.as_mut_ptr());

        let ptr1: *mut u8 = 0x5000 as *mut u8;
        let id1 = idr_alloc(idr.as_mut_ptr(), ptr1, 1, 0, 0);

        idr_remove(idr.as_mut_ptr(), id1 as u32);

        let ptr2: *mut u8 = 0x5001 as *mut u8;
        let id2 = idr_alloc(idr.as_mut_ptr(), ptr2, 1, 0, 0);
        assert_eq!(id2, id1, "should reuse removed ID");

        idr_destroy(idr.as_mut_ptr());
    }
}
