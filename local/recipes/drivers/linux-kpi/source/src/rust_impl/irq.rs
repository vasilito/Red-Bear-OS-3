use std::collections::HashMap;
use std::fs::File;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

struct SendU8Ptr(*mut u8);

impl SendU8Ptr {
    fn as_ptr(&self) -> *mut u8 {
        self.0
    }
}

unsafe impl Send for SendU8Ptr {}

pub type IrqHandler = extern "C" fn(i32, *mut u8) -> u32;

struct IrqEntry {
    cancel: Arc<AtomicBool>,
    fd: Option<File>,
    handle: Option<std::thread::JoinHandle<()>>,
}

lazy_static::lazy_static! {
    static ref IRQ_TABLE: Mutex<HashMap<u32, IrqEntry>> = Mutex::new(HashMap::new());
}

#[no_mangle]
pub extern "C" fn request_irq(
    irq: u32,
    handler: IrqHandler,
    _flags: u32,
    _name: *const u8,
    dev_id: *mut u8,
) -> i32 {
    let path = format!("/scheme/irq/{}", irq);
    let fd = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(e) => {
            log::error!("request_irq: failed to open {} : {}", path, e);
            return -22;
        }
    };

    let thread_fd = match fd.try_clone() {
        Ok(f) => f,
        Err(e) => {
            log::error!("request_irq: failed to clone {} : {}", path, e);
            return -22;
        }
    };

    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_clone = Arc::clone(&cancel);
    let send_dev_id = SendU8Ptr(dev_id);

    let handle = std::thread::spawn(move || {
        use std::io::Read;
        let mut fd = thread_fd;
        let mut buf = [0u8; 8];
        loop {
            if cancel_clone.load(Ordering::Acquire) {
                break;
            }

            match fd.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if cancel_clone.load(Ordering::Acquire) {
                        break;
                    }
                    handler(irq as i32, send_dev_id.as_ptr());
                }
            }
        }
    });

    let entry = IrqEntry {
        cancel: Arc::clone(&cancel),
        fd: Some(fd),
        handle: Some(handle),
    };

    if let Ok(mut table) = IRQ_TABLE.lock() {
        table.insert(irq, entry);
    } else {
        cancel.store(true, Ordering::Release);
        let mut entry = entry;
        let _ = entry.fd.take();
        if let Some(handle) = entry.handle.take() {
            let _ = handle.join();
        }
        log::error!("request_irq: failed to record handler for IRQ {}", irq);
        return -22;
    }

    log::info!("request_irq: registered handler for IRQ {}", irq);
    0
}

#[no_mangle]
pub extern "C" fn free_irq(irq: u32, _dev_id: *mut u8) {
    let entry = if let Ok(mut table) = IRQ_TABLE.lock() {
        let mut entry = table.remove(&irq);
        if let Some(ref mut entry_ref) = entry {
            entry_ref.cancel.store(true, Ordering::Release);
            let _ = entry_ref.fd.take();
        }
        entry
    } else {
        None
    };

    if let Some(mut entry) = entry {
        if let Some(handle) = entry.handle.take() {
            let _ = handle.join();
        }
    }
    log::info!("free_irq: released IRQ {}", irq);
}

#[no_mangle]
pub extern "C" fn enable_irq(_irq: u32) {}

#[no_mangle]
pub extern "C" fn disable_irq(_irq: u32) {}
