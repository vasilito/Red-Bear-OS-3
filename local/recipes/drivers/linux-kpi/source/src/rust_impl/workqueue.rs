use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

struct SendWorkPtr(*mut WorkStruct);

impl SendWorkPtr {
    fn as_ptr(&self) -> *mut WorkStruct {
        self.0
    }
}

unsafe impl Send for SendWorkPtr {}

#[repr(C)]
pub struct WorkStruct {
    pub func: Option<extern "C" fn(*mut WorkStruct)>,
    pub __opaque: [u8; 64],
}

#[repr(C)]
pub struct DelayedWork {
    pub work: WorkStruct,
    pub __timer_opaque: [u8; 64],
}

struct WorkqueueInner {
    queue: Mutex<VecDeque<SendWorkPtr>>,
    pending_count: AtomicUsize,
    done_condvar: Condvar,
    shutdown: AtomicBool,
    thread_count: usize,
}

pub struct WorkqueueStruct {
    inner: Arc<WorkqueueInner>,
    _name: String,
    handles: Vec<std::thread::JoinHandle<()>>,
}

lazy_static::lazy_static! {
    static ref DEFAULT_WQ: Arc<WorkqueueInner> = {
        let inner = Arc::new(WorkqueueInner {
            queue: Mutex::new(VecDeque::new()),
            pending_count: AtomicUsize::new(0),
            done_condvar: Condvar::new(),
            shutdown: AtomicBool::new(false),
            thread_count: 4,
        });

        let inner_clone = inner.clone();
        for _ in 0..inner.thread_count {
            let ic = inner_clone.clone();
            std::thread::spawn(move || worker_loop(ic));
        }
        inner
    };
}

fn worker_loop(inner: Arc<WorkqueueInner>) {
    loop {
        if inner.shutdown.load(Ordering::Acquire) {
            break;
        }

        let work = {
            let mut queue = match inner.queue.lock() {
                Ok(q) => q,
                Err(e) => {
                    log::error!("workqueue: lock poisoned, recovering: {}", e);
                    e.into_inner()
                }
            };
            queue.pop_front()
        };

        if let Some(send_work_ptr) = work {
            let work_ptr = send_work_ptr.as_ptr();
            if let Some(func) = unsafe { (*work_ptr).func } {
                func(work_ptr);
            }
            let prev = inner.pending_count.fetch_sub(1, Ordering::Release);
            if prev == 1 {
                let queue = match inner.queue.lock() {
                    Ok(q) => q,
                    Err(e) => {
                        log::error!("workqueue: lock poisoned, recovering: {}", e);
                        e.into_inner()
                    }
                };
                drop(queue);
                inner.done_condvar.notify_all();
            }
        } else {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}

fn dispatch_work(inner: &Arc<WorkqueueInner>, work: *mut WorkStruct) -> i32 {
    if work.is_null() {
        return 0;
    }
    {
        let mut queue = match inner.queue.lock() {
            Ok(q) => q,
            Err(e) => {
                log::error!("workqueue: lock poisoned, recovering: {}", e);
                e.into_inner()
            }
        };
        queue.push_back(SendWorkPtr(work));
    }
    inner.pending_count.fetch_add(1, Ordering::Release);
    1
}

#[no_mangle]
pub extern "C" fn alloc_workqueue(
    name: *const u8,
    _flags: u32,
    max_active: i32,
) -> *mut WorkqueueStruct {
    let name_str = if name.is_null() {
        String::from("unknown")
    } else {
        unsafe {
            let mut len = 0;
            while *name.add(len) != 0 {
                len += 1;
            }
            match std::str::from_utf8(std::slice::from_raw_parts(name, len)) {
                Ok(s) => s.to_string(),
                Err(_) => String::from("unknown"),
            }
        }
    };

    let thread_count = if max_active > 0 {
        max_active as usize
    } else {
        4
    };

    let inner = Arc::new(WorkqueueInner {
        queue: Mutex::new(VecDeque::new()),
        pending_count: AtomicUsize::new(0),
        done_condvar: Condvar::new(),
        shutdown: AtomicBool::new(false),
        thread_count,
    });

    let mut handles = Vec::with_capacity(inner.thread_count);
    for _ in 0..inner.thread_count {
        let ic = inner.clone();
        handles.push(std::thread::spawn(move || worker_loop(ic)));
    }

    let wq = Box::new(WorkqueueStruct {
        inner,
        _name: name_str,
        handles,
    });
    Box::into_raw(wq)
}

#[no_mangle]
pub extern "C" fn destroy_workqueue(wq: *mut WorkqueueStruct) {
    if wq.is_null() {
        return;
    }

    let mut wq = unsafe { Box::from_raw(wq) };

    {
        let mut queue = match wq.inner.queue.lock() {
            Ok(q) => q,
            Err(e) => {
                log::error!("workqueue: lock poisoned, recovering: {}", e);
                e.into_inner()
            }
        };
        while wq.inner.pending_count.load(Ordering::Acquire) > 0 {
            queue = match wq.inner.done_condvar.wait(queue) {
                Ok(q) => q,
                Err(e) => {
                    log::error!("workqueue: condvar wait failed, recovering: {}", e);
                    e.into_inner()
                }
            };
        }
    }

    wq.inner.shutdown.store(true, Ordering::Release);
    wq.inner.done_condvar.notify_all();

    for handle in wq.handles.drain(..) {
        let _ = handle.join();
    }
}

#[no_mangle]
pub extern "C" fn queue_work(wq: *mut WorkqueueStruct, work: *mut WorkStruct) -> i32 {
    if wq.is_null() {
        return 0;
    }
    let inner = unsafe { &(*wq).inner };
    dispatch_work(inner, work)
}

#[no_mangle]
pub extern "C" fn flush_workqueue(wq: *mut WorkqueueStruct) {
    if wq.is_null() {
        return;
    }
    let inner = unsafe { &(*wq).inner };
    let mut queue = match inner.queue.lock() {
        Ok(q) => q,
        Err(e) => {
            log::error!("workqueue: lock poisoned, recovering: {}", e);
            e.into_inner()
        }
    };
    while inner.pending_count.load(Ordering::Acquire) > 0 {
        queue = match inner.done_condvar.wait(queue) {
            Ok(q) => q,
            Err(e) => {
                log::error!("workqueue: condvar wait failed, recovering: {}", e);
                e.into_inner()
            }
        };
    }
}

#[no_mangle]
pub extern "C" fn schedule_work(work: *mut WorkStruct) -> i32 {
    dispatch_work(&DEFAULT_WQ, work)
}

#[no_mangle]
pub extern "C" fn schedule_delayed_work(dwork: *mut DelayedWork, delay: u64) -> i32 {
    if dwork.is_null() {
        return 0;
    }
    let work_ptr = SendWorkPtr(dwork as *mut WorkStruct);

    let inner = DEFAULT_WQ.clone();
    inner.pending_count.fetch_add(1, Ordering::Release);

    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(delay));
        let ptr = work_ptr.as_ptr();
        if let Some(func) = unsafe { (*ptr).func } {
            func(ptr);
        }
        let prev = inner.pending_count.fetch_sub(1, Ordering::Release);
        if prev == 1 {
            let queue = match inner.queue.lock() {
                Ok(q) => q,
                Err(e) => {
                    log::error!("workqueue: lock poisoned, recovering: {}", e);
                    e.into_inner()
                }
            };
            drop(queue);
            inner.done_condvar.notify_all();
        }
    });
    1
}

#[no_mangle]
pub extern "C" fn flush_scheduled_work() {
    let mut queue = match DEFAULT_WQ.queue.lock() {
        Ok(q) => q,
        Err(e) => {
            log::error!("workqueue: lock poisoned, recovering: {}", e);
            e.into_inner()
        }
    };
    while DEFAULT_WQ.pending_count.load(Ordering::Acquire) > 0 {
        queue = match DEFAULT_WQ.done_condvar.wait(queue) {
            Ok(q) => q,
            Err(e) => {
                log::error!("workqueue: condvar wait failed, recovering: {}", e);
                e.into_inner()
            }
        };
    }
}
