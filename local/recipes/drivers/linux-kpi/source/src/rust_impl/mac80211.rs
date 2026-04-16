use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Mutex;

use super::net::SkBuff;
use super::wireless::{
    wiphy_free, wiphy_new_nm, wiphy_register, wiphy_unregister, KeyParams, Wiphy,
};
use super::workqueue::{schedule_work, WorkStruct};

const EINVAL: i32 = 22;
const EBUSY: i32 = 16;

lazy_static::lazy_static! {
    static ref STA_REGISTRY: Mutex<HashMap<usize, StaRegistryEntry>> = Mutex::new(HashMap::new());
    static ref BA_SESSIONS: Mutex<HashMap<usize, Vec<u16>>> = Mutex::new(HashMap::new());
}

#[derive(Clone, Copy)]
struct StaRegistryEntry {
    hw: usize,
    _vif: usize,
    state: u32,
}

#[repr(C)]
pub struct Ieee80211Ops {
    pub tx: Option<extern "C" fn(*mut Ieee80211Hw, *mut SkBuff)>,
    pub start: Option<extern "C" fn(*mut Ieee80211Hw) -> i32>,
    pub stop: Option<extern "C" fn(*mut Ieee80211Hw)>,
    pub add_interface: Option<extern "C" fn(*mut Ieee80211Hw, *mut Ieee80211Vif) -> i32>,
    pub remove_interface: Option<extern "C" fn(*mut Ieee80211Hw, *mut Ieee80211Vif)>,
    pub config: Option<extern "C" fn(*mut Ieee80211Hw, u32) -> i32>,
    pub bss_info_changed:
        Option<extern "C" fn(*mut Ieee80211Hw, *mut Ieee80211Vif, *mut Ieee80211BssConf, u32)>,
    pub sta_state:
        Option<extern "C" fn(*mut Ieee80211Hw, *mut Ieee80211Vif, *mut Ieee80211Sta, u32) -> i32>,
    pub set_key: Option<
        extern "C" fn(
            *mut Ieee80211Hw,
            *mut Ieee80211Vif,
            i32,
            *mut Ieee80211Sta,
            *mut KeyParams,
        ) -> i32,
    >,
    pub ampdu_action: Option<
        extern "C" fn(*mut Ieee80211Hw, *mut Ieee80211Vif, *mut Ieee80211Sta, u16, u16, u16) -> i32,
    >,
    pub sw_scan_start: Option<extern "C" fn(*mut Ieee80211Hw, *mut Ieee80211Vif, *const u8)>,
    pub sw_scan_complete: Option<extern "C" fn(*mut Ieee80211Hw, *mut Ieee80211Vif)>,
    pub prepare_multicast: Option<extern "C" fn(*mut Ieee80211Hw, *mut c_void) -> u64>,
    pub configure_filter: Option<extern "C" fn(*mut Ieee80211Hw, u32, *mut u32, u64)>,
    pub sched_scan_start:
        Option<extern "C" fn(*mut Ieee80211Hw, *mut Ieee80211Vif, *mut c_void) -> i32>,
    pub sched_scan_stop: Option<extern "C" fn(*mut Ieee80211Hw, *mut Ieee80211Vif)>,
}

#[repr(C)]
pub struct Ieee80211Hw {
    pub wiphy: *mut Wiphy,
    pub ops: *const Ieee80211Ops,
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
#[derive(Debug)]
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

pub const BSS_CHANGED_ASSOC: u32 = 1;
pub const BSS_CHANGED_BSSID: u32 = 2;
pub const BSS_CHANGED_ERP_CTS_PROT: u32 = 4;
pub const BSS_CHANGED_HT: u32 = 8;
pub const BSS_CHANGED_BASIC_RATES: u32 = 16;
pub const BSS_CHANGED_BEACON_INT: u32 = 32;
pub const BSS_CHANGED_BANDWIDTH: u32 = 64;

fn update_sta_registry(
    hw: *mut Ieee80211Hw,
    vif: *mut Ieee80211Vif,
    sta: *mut Ieee80211Sta,
    new_state: u32,
) {
    if let Ok(mut registry) = STA_REGISTRY.lock() {
        if new_state <= IEEE80211_STA_NONE {
            registry.remove(&(sta as usize));
        } else {
            registry.insert(
                sta as usize,
                StaRegistryEntry {
                    hw: hw as usize,
                    _vif: vif as usize,
                    state: new_state,
                },
            );
        }
    }
    if new_state <= IEEE80211_STA_NONE {
        if let Ok(mut sessions) = BA_SESSIONS.lock() {
            sessions.remove(&(sta as usize));
        }
    }
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
        ops: ops.cast::<Ieee80211Ops>(),
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
    if let Ok(mut registry) = STA_REGISTRY.lock() {
        registry.retain(|_, entry| entry.hw != hw as usize);
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
        return -EINVAL;
    }
    if unsafe { &*hw }.registered.load(Ordering::Acquire) != 0 {
        return -EBUSY;
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

#[repr(C)]
pub struct Ieee80211RxStatus {
    pub freq: u16,
    pub band: u32,
    pub signal: i8,
    pub noise: i8,
    pub rate_idx: u8,
    pub flag: u32,
    pub antenna: u8,
    pub rx_flags: u32,
}

impl Default for Ieee80211RxStatus {
    fn default() -> Self {
        Self {
            freq: 0,
            band: 0,
            signal: 0,
            noise: 0,
            rate_idx: 0,
            flag: 0,
            antenna: 0,
            rx_flags: 0,
        }
    }
}

pub const RX_FLAG_MMIC_ERROR: u32 = 1 << 0;
pub const RX_FLAG_DECRYPTED: u32 = 1 << 1;
pub const RX_FLAG_MMIC_STRIPPED: u32 = 1 << 2;
pub const RX_FLAG_IV_STRIPPED: u32 = 1 << 3;

#[repr(C)]
pub struct Ieee80211TxInfo {
    pub flags: u32,
    pub band: u32,
    pub hw_queue: u8,
    pub rate_driver_data: [u8; 16],
}

pub const IEEE80211_TX_CTL_REQ_TX_STATUS: u32 = 1 << 0;
pub const IEEE80211_TX_CTL_NO_ACK: u32 = 1 << 1;
pub const IEEE80211_TX_CTL_CLEAR_PS_FILT: u32 = 1 << 2;
pub const IEEE80211_TX_CTL_FIRST_FRAGMENT: u32 = 1 << 3;

#[no_mangle]
pub extern "C" fn ieee80211_rx_irqsafe(hw: *mut Ieee80211Hw, skb: *mut SkBuff) {
    if hw.is_null() || skb.is_null() {
        return;
    }
}

#[no_mangle]
pub extern "C" fn ieee80211_tx_status(hw: *mut Ieee80211Hw, skb: *mut SkBuff) {
    if hw.is_null() || skb.is_null() {
        return;
    }
}

#[no_mangle]
pub extern "C" fn ieee80211_get_tid(skb: *const SkBuff) -> u8 {
    if skb.is_null() {
        return 0;
    }

    0
}

#[no_mangle]
pub extern "C" fn ieee80211_chandef_create(
    chandef: *mut c_void,
    channel: *const super::wireless::Ieee80211Channel,
    _chan_type: u32,
) {
    if chandef.is_null() || channel.is_null() {
        return;
    }
}

pub const IEEE80211_STA_NOTEXIST: u32 = 0;
pub const IEEE80211_STA_NONE: u32 = 1;
pub const IEEE80211_STA_AUTH: u32 = 2;
pub const IEEE80211_STA_ASSOC: u32 = 3;
pub const IEEE80211_STA_AUTHORIZED: u32 = 4;

#[no_mangle]
pub extern "C" fn ieee80211_start_tx_ba_session(
    pub_sta: *mut Ieee80211Sta,
    tid: u16,
    _timeout: u16,
) -> i32 {
    if pub_sta.is_null() || tid >= 16 {
        return -EINVAL;
    }

    let Ok(mut sessions) = BA_SESSIONS.lock() else {
        return -EINVAL;
    };
    let entry = sessions.entry(pub_sta as usize).or_default();
    if entry.contains(&tid) {
        return -EBUSY;
    }
    entry.push(tid);
    0
}

#[no_mangle]
pub extern "C" fn ieee80211_stop_tx_ba_session(pub_sta: *mut Ieee80211Sta, tid: u16) -> i32 {
    if pub_sta.is_null() || tid >= 16 {
        return -EINVAL;
    }

    if let Ok(mut sessions) = BA_SESSIONS.lock() {
        if let Some(entry) = sessions.get_mut(&(pub_sta as usize)) {
            entry.retain(|existing| *existing != tid);
            if entry.is_empty() {
                sessions.remove(&(pub_sta as usize));
            }
        }
    }
    0
}

#[no_mangle]
pub extern "C" fn ieee80211_sta_state(
    hw: *mut Ieee80211Hw,
    vif: *mut Ieee80211Vif,
    sta: *mut Ieee80211Sta,
    _old_state: u32,
    new_state: u32,
) -> i32 {
    if hw.is_null() || vif.is_null() || sta.is_null() {
        return -EINVAL;
    }

    update_sta_registry(hw, vif, sta, new_state);
    let ops = unsafe { (*hw).ops };
    if ops.is_null() {
        return 0;
    }

    match unsafe { (*ops).sta_state } {
        Some(callback) => callback(hw, vif, sta, new_state),
        None => 0,
    }
}

#[no_mangle]
pub extern "C" fn ieee80211_find_sta(hw: *mut Ieee80211Hw, addr: *const u8) -> *mut Ieee80211Sta {
    if hw.is_null() || addr.is_null() {
        return ptr::null_mut();
    }

    let Ok(registry) = STA_REGISTRY.lock() else {
        return ptr::null_mut();
    };
    let wanted = unsafe { ptr::read(addr.cast::<[u8; 6]>()) };
    for (sta_ptr, entry) in registry.iter() {
        if entry.hw != hw as usize || entry.state <= IEEE80211_STA_NONE {
            continue;
        }
        let sta = *sta_ptr as *mut Ieee80211Sta;
        if sta.is_null() {
            continue;
        }
        if wanted == unsafe { (*sta).addr } {
            return sta;
        }
    }
    ptr::null_mut()
}

#[no_mangle]
pub extern "C" fn ieee80211_beacon_loss(vif: *mut Ieee80211Vif) {
    if vif.is_null() {
        return;
    }
    unsafe { (*vif).cfg_assoc = false };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rust_impl::workqueue::{flush_scheduled_work, WorkStruct};
    use std::sync::atomic::AtomicBool;

    static WORK_RAN: AtomicBool = AtomicBool::new(false);
    static STA_CALLBACKS: AtomicI32 = AtomicI32::new(0);

    extern "C" fn test_work(_work: *mut WorkStruct) {
        WORK_RAN.store(true, Ordering::Release);
    }

    extern "C" fn test_sta_state(
        _hw: *mut Ieee80211Hw,
        _vif: *mut Ieee80211Vif,
        _sta: *mut Ieee80211Sta,
        state: u32,
    ) -> i32 {
        STA_CALLBACKS.store(state as i32, Ordering::Release);
        0
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
        WORK_RAN.store(false, Ordering::Release);
        ieee80211_queue_work(hw, (&mut work as *mut WorkStruct).cast::<c_void>());
        flush_scheduled_work();
        assert!(WORK_RAN.load(Ordering::Acquire));
        ieee80211_free_hw(hw);
    }

    #[test]
    fn connection_loss_and_beacon_loss_clear_assoc_state() {
        let mut vif = Ieee80211Vif {
            addr: [0; 6],
            drv_priv: ptr::null_mut(),
            type_: 0,
            cfg_assoc: true,
        };
        ieee80211_connection_loss(&mut vif);
        assert!(!vif.cfg_assoc);
        vif.cfg_assoc = true;
        ieee80211_beacon_loss(&mut vif);
        assert!(!vif.cfg_assoc);
    }

    #[test]
    fn ieee80211_rx_status_default_and_flags_work() {
        let status = Ieee80211RxStatus::default();
        assert_eq!(status.freq, 0);
        assert_eq!(status.band, 0);
        assert_eq!(status.signal, 0);
        assert_eq!(status.noise, 0);
        assert_eq!(status.rate_idx, 0);
        assert_eq!(status.flag, 0);
        assert_eq!(status.antenna, 0);
        assert_eq!(status.rx_flags, 0);

        let combined = RX_FLAG_DECRYPTED | RX_FLAG_IV_STRIPPED | RX_FLAG_MMIC_STRIPPED;
        assert_ne!(combined & RX_FLAG_DECRYPTED, 0);
        assert_ne!(combined & RX_FLAG_IV_STRIPPED, 0);
        assert_ne!(combined & RX_FLAG_MMIC_STRIPPED, 0);
        assert_eq!(combined & RX_FLAG_MMIC_ERROR, 0);
    }

    #[test]
    fn ieee80211_sta_registry_and_ba_sessions_work() {
        let ops = Ieee80211Ops {
            tx: None,
            start: None,
            stop: None,
            add_interface: None,
            remove_interface: None,
            config: None,
            bss_info_changed: None,
            sta_state: Some(test_sta_state),
            set_key: None,
            ampdu_action: None,
            sw_scan_start: None,
            sw_scan_complete: None,
            prepare_multicast: None,
            configure_filter: None,
            sched_scan_start: None,
            sched_scan_stop: None,
        };
        let hw = ieee80211_alloc_hw_nm(
            0,
            (&ops as *const Ieee80211Ops).cast::<c_void>(),
            ptr::null(),
        );
        assert!(!hw.is_null());
        assert_eq!(unsafe { (*hw).ops }, &ops as *const Ieee80211Ops);

        let mut vif = Ieee80211Vif {
            addr: [0; 6],
            drv_priv: ptr::null_mut(),
            type_: 0,
            cfg_assoc: false,
        };
        let mut sta = Ieee80211Sta {
            addr: [1, 2, 3, 4, 5, 6],
            drv_priv: ptr::null_mut(),
            aid: 1,
        };

        STA_CALLBACKS.store(0, Ordering::Release);
        assert_eq!(
            ieee80211_sta_state(
                hw,
                &mut vif,
                &mut sta,
                IEEE80211_STA_NONE,
                IEEE80211_STA_ASSOC
            ),
            0
        );
        assert_eq!(
            STA_CALLBACKS.load(Ordering::Acquire),
            IEEE80211_STA_ASSOC as i32
        );
        assert!(std::ptr::eq(
            ieee80211_find_sta(hw, sta.addr.as_ptr()),
            &mut sta,
        ));

        assert_eq!(ieee80211_start_tx_ba_session(&mut sta, 3, 100), 0);
        assert_eq!(ieee80211_start_tx_ba_session(&mut sta, 3, 100), -16);
        assert_eq!(ieee80211_stop_tx_ba_session(&mut sta, 3), 0);

        assert_eq!(
            ieee80211_sta_state(
                hw,
                &mut vif,
                &mut sta,
                IEEE80211_STA_ASSOC,
                IEEE80211_STA_NONE
            ),
            0
        );
        assert!(ieee80211_find_sta(hw, sta.addr.as_ptr()).is_null());
        ieee80211_free_hw(hw);
    }

    #[test]
    fn ieee80211_get_tid_returns_zero_for_null() {
        assert_eq!(ieee80211_get_tid(ptr::null()), 0);
    }
}
