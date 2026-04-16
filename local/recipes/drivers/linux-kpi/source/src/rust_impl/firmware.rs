use std::ptr;

fn firmware_search_roots() -> Vec<std::path::PathBuf> {
    let mut roots = Vec::new();
    if let Some(root) = std::env::var_os("REDBEAR_LINUX_KPI_FIRMWARE_ROOT") {
        roots.push(root.into());
    }
    roots.push("/scheme/firmware".into());
    roots.push("/lib/firmware".into());
    roots
}

fn firmware_name(name: *const u8) -> Result<String, i32> {
    if name.is_null() {
        return Err(-22);
    }

    let name_str = unsafe {
        let len = {
            let mut l = 0;
            while *name.add(l) != 0 {
                l += 1;
            }
            l
        };
        let slice = std::slice::from_raw_parts(name, len);
        match std::str::from_utf8(slice) {
            Ok(s) => s.to_string(),
            Err(_) => return Err(-22),
        }
    };

    Ok(name_str)
}

fn load_firmware_bytes(name: &str) -> Result<Vec<u8>, i32> {
    for root in firmware_search_roots() {
        let candidate = root.join(name);
        match std::fs::read(&candidate) {
            Ok(bytes) => {
                log::info!(
                    "request_firmware: loaded '{}' via {}",
                    name,
                    candidate.display()
                );
                return Ok(bytes);
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                log::error!(
                    "request_firmware: failed to load '{}' via {}: {}",
                    name,
                    candidate.display(),
                    err
                );
                return Err(-5);
            }
        }
    }

    log::error!("request_firmware: failed to locate '{}'", name);
    Err(-2)
}

fn install_firmware(fw: *mut *mut Firmware, data: Vec<u8>) -> i32 {
    let size = data.len();
    let layout = match std::alloc::Layout::from_size_align(size, 1) {
        Ok(l) => l,
        Err(_) => return -12,
    };
    let ptr = unsafe { std::alloc::alloc(layout) };
    if ptr.is_null() {
        return -12;
    }
    unsafe { ptr::copy_nonoverlapping(data.as_ptr(), ptr, size) };

    let firmware = Box::new(Firmware {
        size,
        data: ptr as *const u8,
    });
    unsafe { *fw = Box::into_raw(firmware) };
    0
}

#[repr(C)]
pub struct Firmware {
    pub size: usize,
    pub data: *const u8,
}

impl Default for Firmware {
    fn default() -> Self {
        Firmware {
            size: 0,
            data: ptr::null(),
        }
    }
}

impl Drop for Firmware {
    fn drop(&mut self) {
        if !self.data.is_null() && self.size > 0 {
            let layout = match std::alloc::Layout::from_size_align(self.size, 1) {
                Ok(l) => l,
                Err(_) => return,
            };
            unsafe { std::alloc::dealloc(self.data as *mut u8, layout) };
            self.data = ptr::null();
            self.size = 0;
        }
    }
}

#[no_mangle]
pub extern "C" fn request_firmware(fw: *mut *mut Firmware, name: *const u8, _dev: *mut u8) -> i32 {
    if fw.is_null() || name.is_null() {
        return -22;
    }

    let name_str = match firmware_name(name) {
        Ok(name_str) => name_str,
        Err(err) => return err,
    };

    match load_firmware_bytes(&name_str) {
        Ok(data) => install_firmware(fw, data),
        Err(err) => err,
    }
}

#[no_mangle]
pub extern "C" fn request_firmware_direct(
    fw: *mut *mut Firmware,
    name: *const u8,
    dev: *mut u8,
) -> i32 {
    request_firmware(fw, name, dev)
}

#[no_mangle]
pub extern "C" fn request_firmware_nowait(
    _dev: *mut u8,
    _uevent: i32,
    name: *const u8,
    context: *mut u8,
    cont: Option<extern "C" fn(*const Firmware, *mut u8)>,
) -> i32 {
    let Some(cont) = cont else {
        return -22;
    };

    let name_str = match firmware_name(name) {
        Ok(name_str) => name_str,
        Err(err) => return err,
    };

    let fw_ptr = match load_firmware_bytes(&name_str) {
        Ok(data) => {
            let mut fw_ptr: *mut Firmware = ptr::null_mut();
            let rc = install_firmware(&mut fw_ptr, data);
            if rc != 0 {
                return rc;
            }
            fw_ptr
        }
        Err(err) => {
            log::warn!(
                "request_firmware_nowait: unable to pre-load '{}': {}",
                name_str,
                err
            );
            ptr::null_mut()
        }
    };

    let fw_addr = fw_ptr as usize;
    let context_addr = context as usize;
    std::thread::spawn(move || {
        cont(fw_addr as *const Firmware, context_addr as *mut u8);
    });

    0
}

#[no_mangle]
pub extern "C" fn release_firmware(fw: *mut Firmware) {
    if fw.is_null() {
        return;
    }
    unsafe { drop(Box::from_raw(fw)) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_ENV_LOCK: std::sync::LazyLock<Mutex<()>> =
        std::sync::LazyLock::new(|| Mutex::new(()));

    fn temp_root(prefix: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{stamp}"));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn request_firmware_direct_uses_override_root() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let root = temp_root("rbos-linux-kpi-fw");
        std::fs::write(root.join("iwlwifi-test.ucode"), [1u8, 2, 3]).unwrap();
        unsafe {
            std::env::set_var("REDBEAR_LINUX_KPI_FIRMWARE_ROOT", &root);
        }

        let mut fw: *mut Firmware = ptr::null_mut();
        let name = CString::new("iwlwifi-test.ucode").unwrap();
        let rc = request_firmware_direct(&mut fw, name.as_ptr().cast::<u8>(), ptr::null_mut());
        assert_eq!(rc, 0);
        assert!(!fw.is_null());
        assert_eq!(unsafe { (*fw).size }, 3);
        release_firmware(fw);
        unsafe {
            std::env::remove_var("REDBEAR_LINUX_KPI_FIRMWARE_ROOT");
        }
    }

    #[test]
    fn request_firmware_nowait_invokes_callback() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let root = temp_root("rbos-linux-kpi-fw-nowait");
        std::fs::write(root.join("iwlwifi-test-async.ucode"), [9u8, 8, 7]).unwrap();
        unsafe {
            std::env::set_var("REDBEAR_LINUX_KPI_FIRMWARE_ROOT", &root);
        }

        static CALLED: AtomicBool = AtomicBool::new(false);

        extern "C" fn callback(fw: *const Firmware, _context: *mut u8) {
            assert!(!fw.is_null());
            CALLED.store(true, Ordering::Release);
            release_firmware(fw as *mut Firmware);
        }

        let name = CString::new("iwlwifi-test-async.ucode").unwrap();
        let rc = request_firmware_nowait(
            ptr::null_mut(),
            0,
            name.as_ptr().cast::<u8>(),
            ptr::null_mut(),
            Some(callback),
        );
        assert_eq!(rc, 0);

        for _ in 0..100 {
            if CALLED.load(Ordering::Acquire) {
                unsafe {
                    std::env::remove_var("REDBEAR_LINUX_KPI_FIRMWARE_ROOT");
                }
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        unsafe {
            std::env::remove_var("REDBEAR_LINUX_KPI_FIRMWARE_ROOT");
        }
        panic!("request_firmware_nowait callback was not invoked");
    }
}
