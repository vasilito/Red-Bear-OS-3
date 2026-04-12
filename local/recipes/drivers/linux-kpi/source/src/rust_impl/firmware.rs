use std::ptr;

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
            Ok(s) => s,
            Err(_) => return -22,
        }
    };

    let firmware_path = format!("/scheme/firmware/{}", name_str);
    log::info!(
        "request_firmware: loading '{}' via {}",
        name_str,
        firmware_path
    );

    let data = match std::fs::read(&firmware_path) {
        Ok(d) => d,
        Err(e) => {
            log::error!("request_firmware: failed to load '{}': {}", name_str, e);
            return -2;
        }
    };

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

    log::info!("request_firmware: loaded {} bytes for '{}'", size, name_str);
    0
}

#[no_mangle]
pub extern "C" fn release_firmware(fw: *mut Firmware) {
    if fw.is_null() {
        return;
    }
    unsafe { drop(Box::from_raw(fw)) };
}
