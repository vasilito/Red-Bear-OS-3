use std::collections::HashMap;
use std::mem;
use std::os::raw::{c_int, c_ulong};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;

#[repr(C)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

unsafe extern "C" {
    fn clock_gettime(clock_id: c_int, tp: *mut Timespec) -> c_int;
}

const CLOCK_MONOTONIC: c_int = 1;

struct TimerEntry {
    generation: AtomicU64,
    active: AtomicBool,
    function: AtomicPtr<()>,
    data: AtomicPtr<u8>,
    handles: Mutex<Vec<JoinHandle<()>>>,
}

#[repr(C)]
pub struct TimerList {
    expires: AtomicU64,
    function: AtomicPtr<()>,
    data: AtomicPtr<u8>,
    active: AtomicBool,
}

fn timer_entries() -> &'static Mutex<HashMap<usize, Arc<TimerEntry>>> {
    static TIMER_ENTRIES: OnceLock<Mutex<HashMap<usize, Arc<TimerEntry>>>> = OnceLock::new();
    TIMER_ENTRIES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn current_jiffies() -> u64 {
    let mut ts = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let result = unsafe { clock_gettime(CLOCK_MONOTONIC, &mut ts) };
    if result != 0 || ts.tv_sec < 0 || ts.tv_nsec < 0 {
        return 0;
    }

    (ts.tv_sec as u64)
        .saturating_mul(1_000)
        .saturating_add((ts.tv_nsec as u64) / 1_000_000)
}

fn lock_timer_entries() -> std::sync::MutexGuard<'static, HashMap<usize, Arc<TimerEntry>>> {
    match timer_entries().lock() {
        Ok(entries) => entries,
        Err(e) => e.into_inner(),
    }
}

fn lock_timer_handles(entry: &TimerEntry) -> std::sync::MutexGuard<'_, Vec<JoinHandle<()>>> {
    match entry.handles.lock() {
        Ok(handles) => handles,
        Err(e) => e.into_inner(),
    }
}

fn timer_entry(timer: *mut TimerList) -> Arc<TimerEntry> {
    let mut entries = lock_timer_entries();
    entries
        .entry(timer as usize)
        .or_insert_with(|| {
            Arc::new(TimerEntry {
                generation: AtomicU64::new(0),
                active: AtomicBool::new(false),
                function: AtomicPtr::new(ptr::null_mut()),
                data: AtomicPtr::new(ptr::null_mut()),
                handles: Mutex::new(Vec::new()),
            })
        })
        .clone()
}

fn reset_timer_entry(timer: *mut TimerList, function: *mut (), data: *mut u8) {
    let mut entries = lock_timer_entries();
    if let Some(entry) = entries.get(&(timer as usize)) {
        entry.active.store(false, Ordering::Release);
        entry.generation.fetch_add(1, Ordering::AcqRel);
    }
    entries.insert(
        timer as usize,
        Arc::new(TimerEntry {
            generation: AtomicU64::new(0),
            active: AtomicBool::new(false),
            function: AtomicPtr::new(function),
            data: AtomicPtr::new(data),
            handles: Mutex::new(Vec::new()),
        }),
    );
}

fn join_all_handles(entry: &TimerEntry) {
    let handles = {
        let mut guard = lock_timer_handles(entry);
        mem::take(&mut *guard)
    };

    for handle in handles {
        let _ = handle.join();
    }
}

#[no_mangle]
pub extern "C" fn setup_timer(
    timer: *mut TimerList,
    function: extern "C" fn(c_ulong),
    data: c_ulong,
) {
    if timer.is_null() {
        return;
    }

    let function_ptr = function as usize as *mut ();
    unsafe {
        ptr::write(
            timer,
            TimerList {
                expires: AtomicU64::new(0),
                function: AtomicPtr::new(function_ptr),
                data: AtomicPtr::new(data as usize as *mut u8),
                active: AtomicBool::new(false),
            },
        );
    }

    reset_timer_entry(timer, function_ptr, data as usize as *mut u8);
}

#[no_mangle]
pub extern "C" fn mod_timer(timer: *mut TimerList, expires: u64) -> i32 {
    if timer.is_null() {
        return 0;
    }

    let timer_ref = unsafe { &*timer };
    let entry = timer_entry(timer);
    entry.function.store(
        timer_ref.function.load(Ordering::Acquire),
        Ordering::Release,
    );
    entry
        .data
        .store(timer_ref.data.load(Ordering::Acquire), Ordering::Release);

    let was_active = entry.active.swap(true, Ordering::AcqRel);
    timer_ref.active.store(true, Ordering::Release);
    timer_ref.expires.store(expires, Ordering::Release);
    let generation = entry
        .generation
        .fetch_add(1, Ordering::AcqRel)
        .wrapping_add(1);

    let delay = expires.saturating_sub(current_jiffies());
    let function_addr = entry.function.load(Ordering::Acquire) as usize;
    let data_addr = entry.data.load(Ordering::Acquire) as usize;
    let entry_for_thread = entry.clone();
    let handle = std::thread::spawn(move || {
        let start = std::time::Instant::now();
        let total = Duration::from_millis(delay);
        loop {
            let elapsed = start.elapsed();
            if elapsed >= total {
                break;
            }
            if !entry_for_thread.active.load(Ordering::Acquire) {
                return;
            }
            if entry_for_thread.generation.load(Ordering::Acquire) != generation {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        if !entry_for_thread.active.load(Ordering::Acquire) {
            return;
        }

        if entry_for_thread.generation.load(Ordering::Acquire) != generation {
            return;
        }

        if function_addr == 0 {
            entry_for_thread.active.store(false, Ordering::Release);
            return;
        }

        let function =
            unsafe { std::mem::transmute::<usize, extern "C" fn(c_ulong)>(function_addr) };
        function(data_addr as c_ulong);

        if entry_for_thread.generation.load(Ordering::Acquire) == generation {
            entry_for_thread.active.store(false, Ordering::Release);
        }
    });

    lock_timer_handles(&entry).push(handle);

    if was_active {
        1
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn del_timer(timer: *mut TimerList) -> i32 {
    if timer.is_null() {
        return 0;
    }

    let timer_ref = unsafe { &*timer };
    let entry = timer_entry(timer);
    let was_active = entry.active.swap(false, Ordering::AcqRel);
    entry.generation.fetch_add(1, Ordering::AcqRel);
    timer_ref.active.store(false, Ordering::Release);

    if was_active {
        1
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn del_timer_sync(timer: *mut TimerList) -> i32 {
    if timer.is_null() {
        return 0;
    }

    let timer_ref = unsafe { &*timer };
    let entry = timer_entry(timer);
    let was_active = entry.active.swap(false, Ordering::AcqRel);
    entry.generation.fetch_add(1, Ordering::AcqRel);
    timer_ref.active.store(false, Ordering::Release);
    join_all_handles(&entry);

    if was_active {
        1
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn timer_pending(timer: *const TimerList) -> i32 {
    if timer.is_null() {
        return 0;
    }

    let entries = lock_timer_entries();
    match entries.get(&(timer as usize)) {
        Some(entry) if entry.active.load(Ordering::Acquire) => 1,
        Some(_) => 0,
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    extern "C" fn test_timer_callback(_data: c_ulong) {}

    #[test]
    fn setup_timer_initializes_struct() {
        let mut timer = std::mem::MaybeUninit::<TimerList>::uninit();
        setup_timer(timer.as_mut_ptr(), test_timer_callback, 42);

        let timer_ref = unsafe { &*timer.as_ptr() };
        assert_eq!(timer_ref.expires.load(Ordering::Acquire), 0);
        assert!(!timer_ref.active.load(Ordering::Acquire));
    }

    #[test]
    fn setup_timer_null_pointer_is_safe() {
        setup_timer(std::ptr::null_mut(), test_timer_callback, 0);
    }

    #[test]
    fn mod_timer_returns_was_active() {
        let mut timer = std::mem::MaybeUninit::<TimerList>::uninit();
        setup_timer(timer.as_mut_ptr(), test_timer_callback, 0);

        let was_active = mod_timer(timer.as_mut_ptr(), current_jiffies() + 30000);
        assert_eq!(
            was_active, 0,
            "first mod_timer should return 0 (was inactive)"
        );

        let was_active = mod_timer(timer.as_mut_ptr(), current_jiffies() + 30000);
        assert_eq!(
            was_active, 1,
            "second mod_timer should return 1 (was active)"
        );

        del_timer_sync(timer.as_mut_ptr());
    }

    #[test]
    fn mod_timer_null_pointer_returns_zero() {
        assert_eq!(mod_timer(std::ptr::null_mut(), 1000), 0);
    }

    #[test]
    fn del_timer_cancels_active_timer() {
        let mut timer = std::mem::MaybeUninit::<TimerList>::uninit();
        setup_timer(timer.as_mut_ptr(), test_timer_callback, 0);
        mod_timer(timer.as_mut_ptr(), current_jiffies() + 30000);

        let result = del_timer_sync(timer.as_mut_ptr());
        assert_eq!(result, 1, "del_timer should return 1 for active timer");
    }

    #[test]
    fn del_timer_null_pointer_returns_zero() {
        assert_eq!(del_timer(std::ptr::null_mut()), 0);
    }

    #[test]
    fn del_timer_sync_cancels_long_timer() {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        extern "C" fn counting_callback(_data: c_ulong) {
            COUNTER.fetch_add(1, Ordering::Relaxed);
        }

        let mut timer = std::mem::MaybeUninit::<TimerList>::uninit();
        setup_timer(timer.as_mut_ptr(), counting_callback, 0);

        COUNTER.store(0, Ordering::Relaxed);
        mod_timer(timer.as_mut_ptr(), current_jiffies() + 30000);

        let result = del_timer_sync(timer.as_mut_ptr());
        assert_eq!(result, 1, "del_timer_sync should return 1 for active timer");

        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(
            COUNTER.load(Ordering::Relaxed),
            0,
            "callback should not have fired after cancel"
        );
    }

    #[test]
    fn del_timer_returns_zero_for_inactive() {
        let mut timer = std::mem::MaybeUninit::<TimerList>::uninit();
        setup_timer(timer.as_mut_ptr(), test_timer_callback, 0);

        assert_eq!(
            del_timer(timer.as_mut_ptr()),
            0,
            "del_timer on inactive timer should return 0"
        );
    }

    #[test]
    fn timer_pending_null_returns_zero() {
        assert_eq!(timer_pending(std::ptr::null()), 0);
    }

    #[test]
    fn timer_pending_reflects_state() {
        let mut timer = std::mem::MaybeUninit::<TimerList>::uninit();
        setup_timer(timer.as_mut_ptr(), test_timer_callback, 0);

        assert_eq!(
            timer_pending(timer.as_ptr()),
            0,
            "new timer should not be pending"
        );

        mod_timer(timer.as_mut_ptr(), current_jiffies() + 30000);
        assert_eq!(
            timer_pending(timer.as_ptr()),
            1,
            "armed timer should be pending"
        );

        del_timer_sync(timer.as_mut_ptr());
        assert_eq!(
            timer_pending(timer.as_ptr()),
            0,
            "cancelled timer should not be pending"
        );
    }

    #[test]
    fn mod_timer_fires_callback() {
        static FIRE_COUNT: AtomicUsize = AtomicUsize::new(0);

        extern "C" fn fire_callback(_data: c_ulong) {
            FIRE_COUNT.fetch_add(1, Ordering::Relaxed);
        }

        let mut timer = std::mem::MaybeUninit::<TimerList>::uninit();
        setup_timer(timer.as_mut_ptr(), fire_callback, 0);

        FIRE_COUNT.store(0, Ordering::Relaxed);
        mod_timer(timer.as_mut_ptr(), current_jiffies());

        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(
            FIRE_COUNT.load(Ordering::Relaxed) >= 1,
            "callback should have fired"
        );

        del_timer_sync(timer.as_mut_ptr());
    }
}
