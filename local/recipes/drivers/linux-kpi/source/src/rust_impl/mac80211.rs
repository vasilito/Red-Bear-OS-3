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
    static ref RX_QUEUE: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());
    static ref HW_WDEV_MAP: Mutex<HashMap<usize, usize>> = Mutex::new(HashMap::new());
    static ref RX_CALLBACKS: Mutex<HashMap<usize, usize>> = Mutex::new(HashMap::new());
    static ref TX_STATS: Mutex<HashMap<usize, TxStats>> = Mutex::new(HashMap::new());
}

#[derive(Clone, Copy)]
struct StaRegistryEntry {
    hw: usize,
    _vif: usize,
    state: u32,
}

#[derive(Clone, Copy, Default)]
pub struct TxStats {
    pub total: u64,
    pub acked: u64,
    pub nacked: u64,
}

pub type RxCallback = extern "C" fn(*mut Ieee80211Hw, *mut SkBuff);

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
    let hw_key = hw as usize;

    if let Ok(mut map) = HW_WDEV_MAP.lock() {
        map.remove(&hw_key);
    }

    if let Ok(mut map) = RX_CALLBACKS.lock() {
        map.remove(&hw_key);
    }

    if let Ok(mut stats_map) = TX_STATS.lock() {
        stats_map.remove(&hw_key);
    }

    if let Ok(mut queue) = RX_QUEUE.lock() {
        let mut i = 0;
        while i < queue.len() {
            if queue[i].0 == hw_key {
                let (_, skb_key) = queue.swap_remove(i);
                let skb = skb_key as *mut SkBuff;
                if !skb.is_null() {
                    super::net::kfree_skb(skb);
                }
            } else {
                i += 1;
            }
        }
    }

    if let Ok(mut registry) = STA_REGISTRY.lock() {
        registry.retain(|_, entry| entry.hw != hw_key);
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

/// Register the WirelessDev associated with an Ieee80211Hw.
/// Must be called by the driver after ieee80211_alloc_hw_nm and before scan/connect.
/// Required for ieee80211_scan_completed to find the correct wdev.
#[no_mangle]
pub extern "C" fn ieee80211_link_hw_wdev(
    hw: *mut Ieee80211Hw,
    wdev: *mut super::wireless::WirelessDev,
) {
    if hw.is_null() || wdev.is_null() {
        return;
    }
    if let Ok(mut map) = HW_WDEV_MAP.lock() {
        map.insert(hw as usize, wdev as usize);
    }
}

/// Register a per-hw callback that receives drained RX frames.
/// When `ieee80211_rx_drain` processes a queued frame and a callback
/// is registered for the hw instance, the frame is delivered to the
/// callback instead of being logged and freed.
#[no_mangle]
pub extern "C" fn ieee80211_register_rx_handler(
    hw: *mut Ieee80211Hw,
    callback: Option<RxCallback>,
) {
    if hw.is_null() {
        return;
    }
    if let Ok(mut map) = RX_CALLBACKS.lock() {
        match callback {
            Some(cb) => {
                map.insert(hw as usize, cb as usize);
            }
            None => {
                map.remove(&(hw as usize));
            }
        }
    }
}

/// Retrieve accumulated TX statistics for a given hw instance.
#[no_mangle]
pub extern "C" fn ieee80211_get_tx_stats(hw: *mut Ieee80211Hw) -> TxStats {
    if hw.is_null() {
        return TxStats::default();
    }
    if let Ok(stats_map) = TX_STATS.lock() {
        stats_map.get(&(hw as usize)).copied().unwrap_or_default()
    } else {
        TxStats::default()
    }
}

#[no_mangle]
pub extern "C" fn ieee80211_scan_completed(hw: *mut Ieee80211Hw, aborted: bool) {
    if hw.is_null() {
        return;
    }

    let wiphy = unsafe { (*hw).wiphy };
    if wiphy.is_null() {
        return;
    }

    let wdev_ptr = match HW_WDEV_MAP.lock() {
        Ok(map) => match map.get(&(hw as usize)) {
            Some(&ptr) => ptr as *mut super::wireless::WirelessDev,
            None => {
                log::warn!(
                    "ieee80211_scan_completed: no wdev registered for hw={:#x}",
                    hw as usize
                );
                return;
            }
        },
        Err(_) => return,
    };

    let scan_info = super::wireless::Cfg80211ScanInfo { aborted };
    let mut scan_request = super::wireless::Cfg80211ScanRequest {
        wiphy,
        wdev: wdev_ptr,
        n_ssids: 0,
        n_channels: 0,
    };
    super::wireless::cfg80211_scan_done(&mut scan_request, &scan_info);
}

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

    let hw_key = hw as usize;
    let skb_key = skb as usize;
    if let Ok(mut queue) = RX_QUEUE.lock() {
        queue.push((hw_key, skb_key));
        log::trace!(
            "ieee80211_rx_irqsafe: queued frame hw={:#x} skb={:#x} queue_len={}",
            hw_key,
            skb_key,
            queue.len()
        );
    } else {
        log::warn!("ieee80211_rx_irqsafe: failed to lock RX queue, dropping frame");
        super::net::kfree_skb(skb);
    }
}

/// Drain and consume all queued RX frames for a specific hw instance.
/// If an RX handler has been registered via ieee80211_register_rx_handler,
/// frames are delivered to that handler. Otherwise frames are classified,
/// logged, and freed.
#[no_mangle]
pub extern "C" fn ieee80211_rx_drain(hw: *mut Ieee80211Hw) -> usize {
    if hw.is_null() {
        return 0;
    }

    let hw_key = hw as usize;

    let rx_callback = if let Ok(map) = RX_CALLBACKS.lock() {
        map.get(&hw_key).copied()
    } else {
        None
    };

    let Ok(mut queue) = RX_QUEUE.lock() else {
        return 0;
    };

    let mut drained = 0usize;
    let mut i = 0;
    while i < queue.len() {
        if queue[i].0 == hw_key {
            let (_, skb_key) = queue.swap_remove(i);
            let skb = skb_key as *mut SkBuff;
            if !skb.is_null() {
                if let Some(cb) = rx_callback {
                    let callback: RxCallback = unsafe { std::mem::transmute(cb) };
                    callback(hw, skb);
                } else {
                    let skb_ref = unsafe { &mut *skb };
                    let frame_type = extract_frame_type(skb_ref);
                    let frame_len = skb_ref.len;

                    match frame_type {
                        FrameType::Management(subtype) => {
                            log::debug!(
                                "rx_drain: mgmt subtype={} len={} hw={:#x}",
                                subtype,
                                frame_len,
                                hw_key
                            );
                        }
                        FrameType::Data => {
                            log::debug!("rx_drain: data frame len={} hw={:#x}", frame_len, hw_key);
                        }
                        FrameType::Control(subtype) => {
                            log::trace!(
                                "rx_drain: ctrl subtype={} len={} hw={:#x}",
                                subtype,
                                frame_len,
                                hw_key
                            );
                        }
                        FrameType::Unknown => {
                            log::trace!(
                                "rx_drain: unknown frame len={} hw={:#x}",
                                frame_len,
                                hw_key
                            );
                        }
                    }

                    super::net::kfree_skb(skb);
                }
            }
            drained += 1;
        } else {
            i += 1;
        }
    }
    if drained > 0 {
        log::debug!(
            "ieee80211_rx_drain: hw={:#x} drained {} frames",
            hw_key,
            drained
        );
    }
    drained
}

enum FrameType {
    Management(u8),
    Data,
    Control(u8),
    Unknown,
}

fn extract_frame_type(skb: &SkBuff) -> FrameType {
    let len = (skb.len as usize).min(2);
    if len < 2 {
        return FrameType::Unknown;
    }
    let data = unsafe { std::slice::from_raw_parts(skb.data, len) };
    let frame_ctl = u16::from_le_bytes([data[0], data[1]]);
    let type_val = ((frame_ctl >> 2) & 0x3) as u8;
    let subtype = ((frame_ctl >> 4) & 0xF) as u8;
    match type_val {
        0 => FrameType::Management(subtype),
        1 => FrameType::Control(subtype),
        2 => FrameType::Data,
        _ => FrameType::Unknown,
    }
}

#[no_mangle]
pub extern "C" fn ieee80211_tx_status(hw: *mut Ieee80211Hw, skb: *mut SkBuff) {
    if hw.is_null() || skb.is_null() {
        return;
    }
    let hw_key = hw as usize;

    if let Ok(mut stats_map) = TX_STATS.lock() {
        let stats = stats_map.entry(hw_key).or_default();
        stats.total += 1;
        stats.acked += 1;
    }

    log::trace!(
        "ieee80211_tx_status: hw={:#x} skb={:#x}",
        hw_key,
        skb as usize
    );
    super::net::kfree_skb(skb);
}

#[no_mangle]
pub extern "C" fn ieee80211_get_tid(skb: *const SkBuff) -> u8 {
    if skb.is_null() {
        return 0;
    }
    unsafe {
        let s = &*skb;
        if s.data.is_null() || s.len < 26 {
            return 0;
        }
        let frame_control = u16::from_le_bytes([*s.data, *s.data.add(1)]);
        let subtype = (frame_control >> 4) & 0xF;
        if subtype != 0x8 {
            return 0;
        }
        let qos = (frame_control >> 7) & 0x1;
        if qos == 0 {
            return 0;
        }
        let qos_offset: usize = 24;
        if (s.len as usize) < qos_offset + 2 {
            return 0;
        }
        let tid = (*s.data.add(qos_offset)) & 0xF;
        tid
    }
}

#[repr(C)]
struct ChanDef {
    center_freq: u32,
    band: u16,
    channel: *mut c_void,
}

#[no_mangle]
pub extern "C" fn ieee80211_chandef_create(
    chandef: *mut c_void,
    channel: *const super::wireless::Ieee80211Channel,
    chan_type: u32,
) {
    if chandef.is_null() || channel.is_null() {
        return;
    }
    unsafe {
        let cd = chandef.cast::<ChanDef>();
        let ch = &*channel;
        (*cd).center_freq = ch.center_freq as u32;
        (*cd).band = ch.band as u16;
        (*cd).channel = channel as *mut c_void;
        let _ = chan_type;
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

    static RX_RECEIVED: AtomicI32 = AtomicI32::new(0);

    extern "C" fn test_rx_callback(_hw: *mut Ieee80211Hw, skb: *mut SkBuff) {
        RX_RECEIVED.fetch_add(1, Ordering::Release);
        super::super::net::kfree_skb(skb);
    }

    #[test]
    fn rx_callback_receives_drained_frames() {
        let hw = ieee80211_alloc_hw_nm(0, ptr::null(), ptr::null());
        assert!(!hw.is_null());

        RX_RECEIVED.store(0, Ordering::Release);
        ieee80211_register_rx_handler(hw, Some(test_rx_callback));

        let data: [u8; 24] = [
            0x88u8, 0x01, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x06, 0x05, 0x04, 0x03,
            0x02, 0x01, 0xAA, 0xAA, 0x03, 0x00, 0x00, 0x00, 0x08, 0x06,
        ];
        let skb = super::super::net::alloc_skb(128, 0);
        assert!(!skb.is_null());
        unsafe {
            ptr::copy_nonoverlapping(data.as_ptr(), (*skb).data, data.len());
            (*skb).len = data.len() as u32;
        }
        ieee80211_rx_irqsafe(hw, skb);
        assert_eq!(ieee80211_rx_drain(hw), 1);
        assert_eq!(RX_RECEIVED.load(Ordering::Acquire), 1);

        ieee80211_register_rx_handler(hw, None);
        let skb2 = super::super::net::alloc_skb(128, 0);
        assert!(!skb2.is_null());
        unsafe {
            ptr::copy_nonoverlapping(data.as_ptr(), (*skb2).data, data.len());
            (*skb2).len = data.len() as u32;
        }
        ieee80211_rx_irqsafe(hw, skb2);
        assert_eq!(ieee80211_rx_drain(hw), 1);
        assert_eq!(RX_RECEIVED.load(Ordering::Acquire), 1);

        ieee80211_free_hw(hw);
    }

    #[test]
    fn tx_status_tracks_statistics() {
        let hw = ieee80211_alloc_hw_nm(0, ptr::null(), ptr::null());
        assert!(!hw.is_null());

        let stats = ieee80211_get_tx_stats(hw);
        assert_eq!(stats.total, 0);
        assert_eq!(stats.acked, 0);

        for _ in 0..3 {
            let skb = super::super::net::alloc_skb(64, 0);
            assert!(!skb.is_null());
            unsafe {
                (*skb).len = 10;
            }
            ieee80211_tx_status(hw, skb);
        }

        let stats = ieee80211_get_tx_stats(hw);
        assert_eq!(stats.total, 3);
        assert_eq!(stats.acked, 3);

        ieee80211_free_hw(hw);
        let stats_after_free = ieee80211_get_tx_stats(hw);
        assert_eq!(stats_after_free.total, 0);
    }
}
