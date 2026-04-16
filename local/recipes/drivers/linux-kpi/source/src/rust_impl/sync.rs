use std::ptr;
use std::sync::atomic::{AtomicI32, AtomicU8, Ordering};
use std::time::{Duration, Instant};

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
pub extern "C" fn mutex_trylock(m: *mut LinuxMutex) -> i32 {
    if m.is_null() {
        return 0;
    }
    if unsafe { &*m }
        .state
        .compare_exchange(UNLOCKED, LOCKED, Ordering::Acquire, Ordering::Relaxed)
        .is_ok()
    {
        1
    } else {
        0
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
pub extern "C" fn local_irq_disable() {
    IRQ_DEPTH.fetch_add(1, Ordering::Acquire);
}

#[no_mangle]
pub extern "C" fn local_irq_enable() {
    let _ = IRQ_DEPTH.fetch_update(Ordering::AcqRel, Ordering::Relaxed, |depth| {
        Some(depth.saturating_sub(1))
    });
}

#[no_mangle]
pub extern "C" fn irqs_disabled() -> bool {
    IRQ_DEPTH.load(Ordering::Acquire) > 0
}

#[repr(C)]
pub struct Completion {
    done: AtomicU8,
    _padding: [u8; 63],
}

#[repr(C)]
pub struct AtomicT {
    value: AtomicI32,
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
pub extern "C" fn complete_all(c: *mut Completion) {
    if c.is_null() {
        return;
    }
    unsafe { &*c }.done.store(u8::MAX, Ordering::Release);
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
pub extern "C" fn wait_for_completion_timeout(c: *mut Completion, timeout_ms: u64) -> i32 {
    if c.is_null() {
        return 0;
    }

    if unsafe { &*c }.done.load(Ordering::Acquire) != 0 {
        return 1;
    }

    let deadline = Instant::now()
        .checked_add(Duration::from_millis(timeout_ms))
        .unwrap_or_else(Instant::now);

    loop {
        if unsafe { &*c }.done.load(Ordering::Acquire) != 0 {
            return 1;
        }
        if Instant::now() >= deadline {
            return 0;
        }
        std::thread::yield_now();
    }
}

#[no_mangle]
pub extern "C" fn reinit_completion(c: *mut Completion) {
    if c.is_null() {
        return;
    }
    unsafe { &*c }.done.store(0, Ordering::Release);
}

#[no_mangle]
pub extern "C" fn atomic_set(v: *mut AtomicT, i: i32) {
    if v.is_null() {
        return;
    }
    unsafe { &*v }.value.store(i, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn atomic_read(v: *const AtomicT) -> i32 {
    if v.is_null() {
        return 0;
    }
    unsafe { &*v }.value.load(Ordering::SeqCst)
}

#[no_mangle]
pub extern "C" fn atomic_add(i: i32, v: *mut AtomicT) {
    if v.is_null() {
        return;
    }
    unsafe { &*v }.value.fetch_add(i, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn atomic_sub(i: i32, v: *mut AtomicT) {
    if v.is_null() {
        return;
    }
    unsafe { &*v }.value.fetch_sub(i, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn atomic_inc(v: *mut AtomicT) {
    atomic_add(1, v);
}

#[no_mangle]
pub extern "C" fn atomic_dec(v: *mut AtomicT) {
    atomic_sub(1, v);
}

#[no_mangle]
pub extern "C" fn atomic_inc_and_test(v: *mut AtomicT) -> i32 {
    if v.is_null() {
        return 0;
    }
    if unsafe { &*v }.value.fetch_add(1, Ordering::SeqCst) + 1 == 0 {
        1
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn atomic_dec_and_test(v: *mut AtomicT) -> i32 {
    if v.is_null() {
        return 0;
    }
    if unsafe { &*v }.value.fetch_sub(1, Ordering::SeqCst) - 1 == 0 {
        1
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn atomic_add_return(i: i32, v: *mut AtomicT) -> i32 {
    if v.is_null() {
        return 0;
    }
    unsafe { &*v }.value.fetch_add(i, Ordering::SeqCst) + i
}

#[no_mangle]
pub extern "C" fn atomic_sub_return(i: i32, v: *mut AtomicT) -> i32 {
    if v.is_null() {
        return 0;
    }
    unsafe { &*v }.value.fetch_sub(i, Ordering::SeqCst) - i
}

#[no_mangle]
pub extern "C" fn atomic_xchg(v: *mut AtomicT, new: i32) -> i32 {
    if v.is_null() {
        return 0;
    }
    unsafe { &*v }.value.swap(new, Ordering::SeqCst)
}

#[no_mangle]
pub extern "C" fn atomic_cmpxchg(v: *mut AtomicT, old: i32, new: i32) -> i32 {
    if v.is_null() {
        return 0;
    }
    match unsafe { &*v }
        .value
        .compare_exchange(old, new, Ordering::SeqCst, Ordering::SeqCst)
    {
        Ok(previous) | Err(previous) => previous,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutex_trylock_reflects_lock_state() {
        let mut lock = LinuxMutex {
            state: AtomicU8::new(UNLOCKED),
        };

        assert_eq!(mutex_trylock(&mut lock), 1);
        assert_eq!(mutex_trylock(&mut lock), 0);
        mutex_unlock(&mut lock);
        assert_eq!(mutex_trylock(&mut lock), 1);
    }

    #[test]
    fn local_irq_disable_enable_tracks_depth() {
        IRQ_DEPTH.store(0, Ordering::Release);
        local_irq_disable();
        assert!(irqs_disabled());
        local_irq_enable();
        assert!(!irqs_disabled());
    }

    #[test]
    fn atomic_operations_cover_all_paths() {
        let mut value = AtomicT {
            value: AtomicI32::new(0),
        };

        atomic_set(&mut value, 3);
        assert_eq!(atomic_read(&value), 3);
        atomic_add(4, &mut value);
        assert_eq!(atomic_read(&value), 7);
        atomic_sub(2, &mut value);
        assert_eq!(atomic_read(&value), 5);
        atomic_inc(&mut value);
        atomic_dec(&mut value);
        assert_eq!(atomic_add_return(5, &mut value), 10);
        assert_eq!(atomic_sub_return(3, &mut value), 7);
        assert_eq!(atomic_xchg(&mut value, 11), 7);
        assert_eq!(atomic_cmpxchg(&mut value, 10, 12), 11);
        assert_eq!(atomic_cmpxchg(&mut value, 11, 13), 11);
        assert_eq!(atomic_read(&value), 13);

        atomic_set(&mut value, -1);
        assert_eq!(atomic_inc_and_test(&mut value), 1);
        atomic_set(&mut value, 1);
        assert_eq!(atomic_dec_and_test(&mut value), 1);
    }

    #[test]
    fn completion_timeout_and_complete_all_work() {
        let mut completion = Completion {
            done: AtomicU8::new(0),
            _padding: [0; 63],
        };

        assert_eq!(wait_for_completion_timeout(&mut completion, 1), 0);
        complete_all(&mut completion);
        assert_eq!(wait_for_completion_timeout(&mut completion, 1), 1);
        reinit_completion(&mut completion);
        assert_eq!(wait_for_completion_timeout(&mut completion, 1), 0);
        complete(&mut completion);
        wait_for_completion(&mut completion);
        assert_eq!(wait_for_completion_timeout(&mut completion, 1), 1);
    }
}
