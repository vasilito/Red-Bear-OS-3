use std::sync::atomic::{AtomicU8, Ordering};

const UNLOCKED: u8 = 0;
const LOCKED: u8 = 1;

#[repr(C)]
pub struct LinuxMutex {
    state: AtomicU8,
}

#[no_mangle]
pub extern "C" fn mutex_init(m: *mut LinuxMutex) {
    if m.is_null() {
        return;
    }
    unsafe {
        (*m).state = AtomicU8::new(UNLOCKED);
    }
}

#[no_mangle]
pub extern "C" fn mutex_lock(m: *mut LinuxMutex) {
    if m.is_null() {
        return;
    }
    while unsafe { &*m }
        .state
        .compare_exchange(UNLOCKED, LOCKED, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        std::hint::spin_loop();
    }
}

#[no_mangle]
pub extern "C" fn mutex_unlock(m: *mut LinuxMutex) {
    if m.is_null() {
        return;
    }
    unsafe { &*m }.state.store(UNLOCKED, Ordering::Release);
}

#[no_mangle]
pub extern "C" fn mutex_is_locked(m: *mut LinuxMutex) -> bool {
    if m.is_null() {
        return false;
    }
    unsafe { &*m }.state.load(Ordering::Acquire) == LOCKED
}

#[repr(C)]
#[derive(Default)]
pub struct Spinlock {
    locked: AtomicU8,
}

#[no_mangle]
pub extern "C" fn spin_lock_init(lock: *mut Spinlock) {
    if lock.is_null() {
        return;
    }
    unsafe {
        (*lock).locked.store(0, Ordering::SeqCst);
    }
}

#[no_mangle]
pub extern "C" fn spin_lock(lock: *mut Spinlock) {
    if lock.is_null() {
        return;
    }
    while unsafe {
        (*lock)
            .locked
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
    }
    .is_err()
    {
        std::hint::spin_loop();
    }
}

#[no_mangle]
pub extern "C" fn spin_unlock(lock: *mut Spinlock) {
    if lock.is_null() {
        return;
    }
    unsafe {
        (*lock).locked.store(0, Ordering::Release);
    }
}

static IRQ_DEPTH: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

#[no_mangle]
pub extern "C" fn spin_lock_irqsave(lock: *mut Spinlock, flags: *mut u64) -> u64 {
    let prev_depth = IRQ_DEPTH.fetch_add(1, Ordering::Acquire);
    spin_lock(lock);
    if !flags.is_null() {
        unsafe { *flags = prev_depth as u64 };
    }
    prev_depth as u64
}

#[no_mangle]
pub extern "C" fn spin_unlock_irqrestore(lock: *mut Spinlock, flags: u64) {
    spin_unlock(lock);
    IRQ_DEPTH.store(flags as u32, Ordering::Release);
}

#[no_mangle]
pub extern "C" fn local_irq_save(flags: *mut u64) {
    let prev_depth = IRQ_DEPTH.fetch_add(1, Ordering::Acquire);
    if !flags.is_null() {
        unsafe { *flags = prev_depth as u64 };
    }
}

#[no_mangle]
pub extern "C" fn local_irq_restore(flags: u64) {
    IRQ_DEPTH.store(flags as u32, Ordering::Release);
}

#[no_mangle]
pub extern "C" fn irqs_disabled() -> bool {
    IRQ_DEPTH.load(Ordering::Acquire) > 0
}

use std::ptr;

#[repr(C)]
pub struct Completion {
    done: AtomicU8,
    _padding: [u8; 63],
}

#[no_mangle]
pub extern "C" fn init_completion(c: *mut Completion) {
    if c.is_null() {
        return;
    }
    unsafe {
        ptr::write(
            c,
            Completion {
                done: AtomicU8::new(0),
                _padding: [0; 63],
            },
        );
    }
}

#[no_mangle]
pub extern "C" fn complete(c: *mut Completion) {
    if c.is_null() {
        return;
    }
    unsafe { &*c }.done.store(1, Ordering::Release);
}

#[no_mangle]
pub extern "C" fn wait_for_completion(c: *mut Completion) {
    if c.is_null() {
        return;
    }
    while unsafe { &*c }.done.load(Ordering::Acquire) == 0 {
        std::hint::spin_loop();
    }
}

#[no_mangle]
pub extern "C" fn reinit_completion(c: *mut Completion) {
    if c.is_null() {
        return;
    }
    unsafe { &*c }.done.store(0, Ordering::Release);
}
