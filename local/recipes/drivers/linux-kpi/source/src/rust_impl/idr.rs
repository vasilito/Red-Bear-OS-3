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
