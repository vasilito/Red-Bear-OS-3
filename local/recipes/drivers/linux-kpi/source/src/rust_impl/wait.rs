use std::ptr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

struct WaitState {
    generation: AtomicU64,
}

#[repr(C)]
pub struct WaitQueueHead {
    condvar: Condvar,
    mutex: Mutex<bool>,
}

fn wait_states() -> &'static Mutex<HashMap<usize, Arc<WaitState>>> {
    static WAIT_STATES: OnceLock<Mutex<HashMap<usize, Arc<WaitState>>>> = OnceLock::new();
    WAIT_STATES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_wait_states() -> std::sync::MutexGuard<'static, HashMap<usize, Arc<WaitState>>> {
    match wait_states().lock() {
        Ok(states) => states,
        Err(e) => e.into_inner(),
    }
}

fn reset_wait_state(wq: *mut WaitQueueHead) {
    lock_wait_states().insert(
        wq as usize,
        Arc::new(WaitState {
            generation: AtomicU64::new(0),
        }),
    );
}

fn wait_state(wq: *mut WaitQueueHead) -> Arc<WaitState> {
    let mut states = lock_wait_states();
    states
        .entry(wq as usize)
        .or_insert_with(|| {
            Arc::new(WaitState {
                generation: AtomicU64::new(0),
            })
        })
        .clone()
}

fn wait_event_impl<F>(wq: *mut WaitQueueHead, condition: F)
where
    F: Fn() -> bool,
{
    if wq.is_null() {
        return;
    }

    let wq_ref = unsafe { &*wq };
    let state = wait_state(wq);
    loop {
        if condition() {
            return;
        }

        let mut notified = match wq_ref.mutex.lock() {
            Ok(guard) => guard,
            Err(e) => e.into_inner(),
        };
        let generation = state.generation.load(Ordering::Acquire);

        while state.generation.load(Ordering::Acquire) == generation && !condition() {
            notified = match wq_ref.condvar.wait(notified) {
                Ok(guard) => guard,
                Err(e) => e.into_inner(),
            };
        }

        *notified = false;
    }
}

fn wait_event_timeout_impl<F>(wq: *mut WaitQueueHead, condition: F, timeout_ms: u64) -> i32
where
    F: Fn() -> bool,
{
    if wq.is_null() {
        return 0;
    }

    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let wq_ref = unsafe { &*wq };
    let state = wait_state(wq);

    loop {
        if condition() {
            return 1;
        }

        let now = Instant::now();
        if now >= deadline {
            return 0;
        }

        let remaining = deadline.saturating_duration_since(now);
        let notified = match wq_ref.mutex.lock() {
            Ok(guard) => guard,
            Err(e) => e.into_inner(),
        };
        let generation = state.generation.load(Ordering::Acquire);

        let (mut notified, wait_result) = match wq_ref.condvar.wait_timeout(notified, remaining) {
            Ok(result) => result,
            Err(e) => e.into_inner(),
        };

        if *notified {
            *notified = false;
        }

        if condition() {
            return 1;
        }

        if state.generation.load(Ordering::Acquire) != generation {
            continue;
        }

        if wait_result.timed_out() && !condition() {
            return 0;
        }
    }
}

#[no_mangle]
pub extern "C" fn init_waitqueue_head(wq: *mut WaitQueueHead) {
    if wq.is_null() {
        return;
    }

    unsafe {
        ptr::write(
            wq,
            WaitQueueHead {
                condvar: Condvar::new(),
                mutex: Mutex::new(false),
            },
        );
    }

    reset_wait_state(wq);
}

#[no_mangle]
pub extern "C" fn wait_event(wq: *mut WaitQueueHead, condition: extern "C" fn() -> bool) {
    wait_event_impl(wq, || condition());
}

#[no_mangle]
pub extern "C" fn wake_up(wq: *mut WaitQueueHead) {
    if wq.is_null() {
        return;
    }

    let wq_ref = unsafe { &*wq };
    let state = wait_state(wq);
    {
        let mut notified = match wq_ref.mutex.lock() {
            Ok(guard) => guard,
            Err(e) => e.into_inner(),
        };
        *notified = true;
        state.generation.fetch_add(1, Ordering::AcqRel);
    }
    wq_ref.condvar.notify_all();
}

#[no_mangle]
pub extern "C" fn wait_event_timeout(
    wq: *mut WaitQueueHead,
    condition: extern "C" fn() -> bool,
    timeout_ms: u64,
) -> i32 {
    wait_event_timeout_impl(wq, || condition(), timeout_ms)
}
