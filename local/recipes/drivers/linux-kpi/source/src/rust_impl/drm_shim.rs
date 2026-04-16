use std::collections::{BTreeMap, HashMap};
use std::ptr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

static NEXT_GEM_HANDLE: AtomicU32 = AtomicU32::new(1);

#[repr(C)]
struct CallerGemObject {
    dev: *mut u8,
    handle_count: u32,
    _pad: u32,
    size: usize,
    driver_private: *mut u8,
}

unsafe fn write_handle_count(obj: *mut u8, count: u32) {
    let cobj = obj as *mut CallerGemObject;
    unsafe {
        (*cobj).handle_count = count;
    }
}

unsafe fn write_size(obj: *mut u8, size: usize) {
    let cobj = obj as *mut CallerGemObject;
    unsafe {
        (*cobj).size = size;
    }
}

struct ObjectState {
    size: usize,
    handle_count: u32,
    handles: Vec<u32>,
}

static OBJECTS: Mutex<Option<HashMap<usize, ObjectState>>> = Mutex::new(None);
static HANDLES: Mutex<Option<BTreeMap<u32, usize>>> = Mutex::new(None);

fn with_objects<F, R>(f: F) -> R
where
    F: FnOnce(&mut HashMap<usize, ObjectState>) -> R,
{
    let mut guard = OBJECTS.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Some(HashMap::new());
    }
    match guard.as_mut() {
        Some(map) => f(map),
        None => f(&mut HashMap::new()),
    }
}

fn with_handles<F, R>(f: F) -> R
where
    F: FnOnce(&mut BTreeMap<u32, usize>) -> R,
{
    let mut guard = HANDLES.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Some(BTreeMap::new());
    }
    match guard.as_mut() {
        Some(map) => f(map),
        None => f(&mut BTreeMap::new()),
    }
}

fn next_gem_handle() -> u32 {
    NEXT_GEM_HANDLE.fetch_add(1, Ordering::Relaxed)
}

#[no_mangle]
pub extern "C" fn drm_dev_register(_dev: *mut u8, _flags: u64) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn drm_dev_unregister(_dev: *mut u8) {}

#[no_mangle]
pub extern "C" fn drm_gem_object_init(_dev: *mut u8, obj: *mut u8, size: usize) -> i32 {
    let key = obj as usize;
    unsafe {
        write_size(obj, size);
        write_handle_count(obj, 0);
    }
    with_objects(|objects| {
        objects.insert(
            key,
            ObjectState {
                size,
                handle_count: 0,
                handles: Vec::new(),
            },
        );
    });
    log::debug!("drm_gem_object_init: obj={:#x} size={}", key, size);
    0
}

#[no_mangle]
pub extern "C" fn drm_gem_object_release(obj: *mut u8) {
    let key = obj as usize;
    with_objects(|objects| {
        if let Some(state) = objects.remove(&key) {
            for h in &state.handles {
                with_handles(|handles| {
                    handles.remove(h);
                });
            }
            log::debug!(
                "drm_gem_object_release: obj={:#x} handles_dropped={}",
                key,
                state.handles.len()
            );
        }
    });
}

#[no_mangle]
pub extern "C" fn drm_gem_handle_create(_file: *mut u8, obj: *mut u8, handlep: *mut u32) -> i32 {
    if handlep.is_null() {
        return -22;
    }

    let key = obj as usize;
    let handle = with_objects(|objects| match objects.get_mut(&key) {
        Some(state) => {
            let handle = next_gem_handle();
            state.handle_count += 1;
            unsafe {
                write_handle_count(obj, state.handle_count);
            }
            state.handles.push(handle);
            Some(handle)
        }
        None => {
            log::error!(
                "drm_gem_handle_create: obj={:#x} not initialized (drm_gem_object_init not called)",
                key
            );
            None
        }
    });

    let handle = match handle {
        Some(h) => h,
        None => return -22,
    };

    with_handles(|handles| {
        handles.insert(handle, key);
    });

    unsafe { *handlep = handle };
    log::debug!("drm_gem_handle_create: handle={} obj={:#x}", handle, key);
    0
}

#[no_mangle]
pub extern "C" fn drm_gem_handle_delete(_file: *mut u8, handle: u32) {
    let obj_key = with_handles(|handles| handles.remove(&handle));

    if let Some(key) = obj_key {
        with_objects(|objects| {
            if let Some(state) = objects.get_mut(&key) {
                state.handles.retain(|h| *h != handle);
                state.handle_count = state.handle_count.saturating_sub(1);
                unsafe {
                    write_handle_count(key as *mut u8, state.handle_count);
                }
            }
        });
    }
    log::debug!("drm_gem_handle_delete: handle={}", handle);
}

#[no_mangle]
pub extern "C" fn drm_gem_handle_lookup(_file: *mut u8, handle: u32) -> *mut u8 {
    let obj_key = with_handles(|handles| handles.get(&handle).copied());

    match obj_key {
        Some(key) => {
            let found = with_objects(|objects| objects.contains_key(&key));
            if found {
                key as *mut u8
            } else {
                log::warn!(
                    "drm_gem_handle_lookup: handle={} maps to obj={:#x} but object released",
                    handle,
                    key
                );
                ptr::null_mut()
            }
        }
        None => {
            log::warn!("drm_gem_handle_lookup: handle={} not found", handle);
            ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "C" fn drm_gem_object_lookup(_file: *mut u8, handle: u32) -> *mut u8 {
    let obj_key = with_handles(|handles| handles.get(&handle).copied());

    match obj_key {
        Some(key) => {
            let found = with_objects(|objects| {
                if let Some(state) = objects.get_mut(&key) {
                    state.handle_count += 1;
                    unsafe {
                        write_handle_count(key as *mut u8, state.handle_count);
                    }
                    true
                } else {
                    false
                }
            });
            if found {
                key as *mut u8
            } else {
                log::warn!(
                    "drm_gem_object_lookup: handle={} maps to obj={:#x} but object released",
                    handle,
                    key
                );
                ptr::null_mut()
            }
        }
        None => {
            log::warn!("drm_gem_object_lookup: handle={} not found", handle);
            ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "C" fn drm_gem_object_put(obj: *mut u8) {
    if obj.is_null() {
        return;
    }
    let key = obj as usize;
    with_objects(|objects| {
        if let Some(state) = objects.get_mut(&key) {
            state.handle_count = state.handle_count.saturating_sub(1);
            unsafe {
                write_handle_count(obj, state.handle_count);
            }
        }
    });
}

#[no_mangle]
pub extern "C" fn drm_ioctl(_dev: *mut u8, cmd: u32, _data: *mut u8, _file: *mut u8) -> i32 {
    log::trace!("drm_ioctl: cmd={:#x}", cmd);
    0
}

#[no_mangle]
pub extern "C" fn drm_mode_config_reset(_dev: *mut u8) {}

#[no_mangle]
pub extern "C" fn drm_connector_register(_connector: *mut u8) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn drm_crtc_handle_vblank(_crtc: *mut u8) -> u32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alloc_fake_obj() -> *mut u8 {
        let layout = std::alloc::Layout::from_size_align(256, 8).unwrap();
        let ptr = unsafe { std::alloc::alloc(layout) };
        assert!(!ptr.is_null());
        ptr
    }

    fn free_fake_obj(ptr: *mut u8) {
        let layout = std::alloc::Layout::from_size_align(256, 8).unwrap();
        unsafe { std::alloc::dealloc(ptr, layout) };
    }

    #[test]
    fn drm_gem_object_init_and_release() {
        let obj = alloc_fake_obj();
        let rc = drm_gem_object_init(std::ptr::null_mut(), obj, 4096);
        assert_eq!(rc, 0);
        drm_gem_object_release(obj);
        free_fake_obj(obj);
    }

    #[test]
    fn drm_gem_handle_create_and_lookup() {
        let file: *mut u8 = 0x1100 as *mut u8;
        let obj = alloc_fake_obj();
        drm_gem_object_init(std::ptr::null_mut(), obj, 1024);

        let mut handle: u32 = 0;
        let rc = drm_gem_handle_create(file, obj, &mut handle);
        assert_eq!(rc, 0, "handle_create should succeed");
        assert!(handle > 0, "handle should be nonzero");

        let found = drm_gem_object_lookup(file, handle);
        assert_eq!(found, obj, "lookup should return the original object");

        let not_found = drm_gem_object_lookup(file, 99999);
        assert_eq!(
            not_found,
            std::ptr::null_mut(),
            "invalid handle should return null"
        );

        drm_gem_object_release(obj);
        free_fake_obj(obj);
    }

    #[test]
    fn drm_gem_handle_delete_removes_mapping() {
        let file: *mut u8 = 0x1200 as *mut u8;
        let obj = alloc_fake_obj();
        drm_gem_object_init(std::ptr::null_mut(), obj, 2048);

        let mut handle: u32 = 0;
        drm_gem_handle_create(file, obj, &mut handle);
        assert!(handle > 0);

        drm_gem_handle_delete(file, handle);
        let found = drm_gem_object_lookup(file, handle);
        assert_eq!(
            found,
            std::ptr::null_mut(),
            "deleted handle should return null"
        );

        drm_gem_object_release(obj);
        free_fake_obj(obj);
    }

    #[test]
    fn drm_dev_register_and_unregister_are_callable() {
        let dev: *mut u8 = 0x1300 as *mut u8;
        assert_eq!(drm_dev_register(dev, 0), 0);
        drm_dev_unregister(dev);
    }

    #[test]
    fn drm_ioctl_returns_zero() {
        assert_eq!(
            drm_ioctl(
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut()
            ),
            0
        );
    }

    #[test]
    fn drm_null_pointers_are_safe() {
        drm_gem_object_release(std::ptr::null_mut());
        drm_dev_unregister(std::ptr::null_mut());
        drm_mode_config_reset(std::ptr::null_mut());
        assert_eq!(drm_connector_register(std::ptr::null_mut()), 0);
        assert_eq!(drm_crtc_handle_vblank(std::ptr::null_mut()), 0);
    }
}
