use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicI32, Ordering};

use super::wireless::{wiphy_free, wiphy_new_nm, wiphy_register, wiphy_unregister, Wiphy};
use super::workqueue::{schedule_work, WorkStruct};

#[repr(C)]
pub struct Ieee80211Hw {
    pub wiphy: *mut Wiphy,
    pub priv_data: *mut c_void,
    pub registered: AtomicI32,
    pub extra_tx_headroom: u32,
    pub queues: u16,
    priv_alloc_size: usize,
    priv_alloc_align: usize,
}

#[repr(C)]
pub struct Ieee80211Vif {
    pub addr: [u8; 6],
    pub drv_priv: *mut c_void,
    pub type_: u32,
    pub cfg_assoc: bool,
}

#[repr(C)]
pub struct Ieee80211Sta {
    pub addr: [u8; 6],
    pub drv_priv: *mut c_void,
    pub aid: u16,
}

#[repr(C)]
pub struct Ieee80211BssConf {
    pub assoc: bool,
    pub aid: u16,
    pub beacon_int: u16,
}

#[no_mangle]
pub extern "C" fn ieee80211_alloc_hw_nm(
    priv_data_len: usize,
    ops: *const c_void,
    requested_name: *const u8,
) -> *mut Ieee80211Hw {
    let wiphy = wiphy_new_nm(ops, 0, requested_name);
    if wiphy.is_null() {
        return ptr::null_mut();
    }

    let mut hw = Box::new(Ieee80211Hw {
        wiphy,
        priv_data: ptr::null_mut(),
        registered: AtomicI32::new(0),
        extra_tx_headroom: 0,
        queues: 1,
        priv_alloc_size: 0,
        priv_alloc_align: 0,
    });

    if priv_data_len != 0 {
        let layout = match Layout::from_size_align(priv_data_len, 16) {
            Ok(layout) => layout,
            Err(_) => {
                wiphy_free(wiphy);
                return ptr::null_mut();
            }
        };
        let ptr = unsafe { alloc_zeroed(layout) } as *mut c_void;
        if ptr.is_null() {
            wiphy_free(wiphy);
            return ptr::null_mut();
        }
        hw.priv_data = ptr;
        hw.priv_alloc_size = priv_data_len;
        hw.priv_alloc_align = 16;
    }

    Box::into_raw(hw)
}

#[no_mangle]
pub extern "C" fn ieee80211_free_hw(hw: *mut Ieee80211Hw) {
    if hw.is_null() {
        return;
    }
    unsafe {
        let hw_box = Box::from_raw(hw);
        if !hw_box.priv_data.is_null() {
            if let Ok(layout) = Layout::from_size_align(
                hw_box.priv_alloc_size.max(1),
                hw_box.priv_alloc_align.max(1),
            ) {
                dealloc(hw_box.priv_data.cast::<u8>(), layout);
            }
        }
        wiphy_free(hw_box.wiphy);
    }
}

#[no_mangle]
pub extern "C" fn ieee80211_register_hw(hw: *mut Ieee80211Hw) -> i32 {
    if hw.is_null() {
        return -22;
    }
    if unsafe { &*hw }.registered.load(Ordering::Acquire) != 0 {
        return -16;
    }
    let rc = wiphy_register(unsafe { (*hw).wiphy });
    if rc != 0 {
        return rc;
    }
    unsafe { &*hw }.registered.store(1, Ordering::Release);
    0
}

#[no_mangle]
pub extern "C" fn ieee80211_unregister_hw(hw: *mut Ieee80211Hw) {
    if hw.is_null() {
        return;
    }
    if unsafe { &*hw }.registered.load(Ordering::Acquire) == 0 {
        return;
    }
    wiphy_unregister(unsafe { (*hw).wiphy });
    unsafe { &*hw }.registered.store(0, Ordering::Release);
}

#[no_mangle]
pub extern "C" fn ieee80211_queue_work(_hw: *mut Ieee80211Hw, work: *mut c_void) {
    if work.is_null() {
        return;
    }
    let _ = schedule_work(work.cast::<WorkStruct>());
}

#[no_mangle]
pub extern "C" fn ieee80211_scan_completed(_hw: *mut Ieee80211Hw, _aborted: bool) {}

#[no_mangle]
pub extern "C" fn ieee80211_connection_loss(vif: *mut Ieee80211Vif) {
    if vif.is_null() {
        return;
    }
    unsafe { (*vif).cfg_assoc = false };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rust_impl::workqueue::{flush_scheduled_work, WorkStruct};
    use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

    static WORK_RAN: AtomicBool = AtomicBool::new(false);

    extern "C" fn test_work(_work: *mut WorkStruct) {
        WORK_RAN.store(true, AtomicOrdering::Release);
    }

    #[test]
    fn ieee80211_hw_registration_round_trip_works() {
        let hw = ieee80211_alloc_hw_nm(0, ptr::null(), ptr::null());
        assert!(!hw.is_null());
        assert_eq!(ieee80211_register_hw(hw), 0);
        assert_eq!(ieee80211_register_hw(hw), -16);
        assert_eq!(unsafe { (*hw).registered.load(Ordering::Acquire) }, 1);
        ieee80211_unregister_hw(hw);
        assert_eq!(unsafe { (*hw).registered.load(Ordering::Acquire) }, 0);
        ieee80211_free_hw(hw);
    }

    #[test]
    fn ieee80211_queue_work_dispatches_work() {
        let hw = ieee80211_alloc_hw_nm(0, ptr::null(), ptr::null());
        assert!(!hw.is_null());
        let mut work = WorkStruct {
            func: Some(test_work),
            __opaque: [0; 64],
        };
        WORK_RAN.store(false, AtomicOrdering::Release);
        ieee80211_queue_work(hw, (&mut work as *mut WorkStruct).cast::<c_void>());
        flush_scheduled_work();
        assert!(WORK_RAN.load(AtomicOrdering::Acquire));
        ieee80211_free_hw(hw);
    }

    #[test]
    fn connection_loss_clears_assoc_state() {
        let mut vif = Ieee80211Vif {
            addr: [0; 6],
            drv_priv: ptr::null_mut(),
            type_: 0,
            cfg_assoc: true,
        };
        ieee80211_connection_loss(&mut vif);
        assert!(!vif.cfg_assoc);
    }
}
