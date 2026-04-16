use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Mutex;

use super::net::{netif_carrier_off, netif_carrier_on, NetDevice};

#[derive(Clone, Default)]
struct WirelessEventState {
    new_sta: Option<[u8; 6]>,
    mgmt_rx_freq: u32,
    mgmt_rx_signal: i32,
    mgmt_rx_len: usize,
    mgmt_rx_data: Vec<u8>,
    mgmt_tx_cookie: u64,
    mgmt_tx_len: usize,
    mgmt_tx_ack: bool,
    mgmt_tx_data: Vec<u8>,
    sched_scan_reqid: u64,
    roc_cookie: u64,
    roc_chan_freq: u16,
    roc_band: u32,
    roc_duration: u32,
    roc_active: bool,
}

#[repr(C)]
pub struct WiphyBands {
    bands: [usize; 3],
}

unsafe impl Send for WiphyBands {}

lazy_static::lazy_static! {
    static ref WIRELESS_EVENTS: Mutex<HashMap<usize, WirelessEventState>> = Mutex::new(HashMap::new());
    static ref BSS_REGISTRY: Mutex<Vec<Box<Cfg80211Bss>>> = Mutex::new(Vec::new());
    static ref BSS_IES: Mutex<HashMap<usize, Vec<u8>>> = Mutex::new(HashMap::new());
    static ref WIPY_BANDS_MAP: Mutex<HashMap<usize, WiphyBands>> = Mutex::new(HashMap::new());
}

#[repr(C)]
pub struct Wiphy {
    pub priv_data: *mut c_void,
    pub registered: AtomicI32,
    pub interface_modes: u32,
    pub max_scan_ssids: i32,
    pub max_scan_ie_len: i32,
    priv_alloc_size: usize,
    priv_alloc_align: usize,
}

#[repr(C)]
pub struct WirelessDev {
    pub wiphy: *mut Wiphy,
    pub netdev: *mut c_void,
    pub iftype: u32,
    pub scan_in_flight: bool,
    pub scan_aborted: bool,
    pub connecting: bool,
    pub connected: bool,
    pub locally_generated: bool,
    pub last_status: u16,
    pub last_reason: u16,
    pub has_bssid: bool,
    pub last_bssid: [u8; 6],
}

#[repr(C)]
pub struct Cfg80211ScanInfo {
    pub aborted: bool,
}

#[repr(C)]
pub struct Cfg80211ScanRequest {
    pub wiphy: *mut Wiphy,
    pub wdev: *mut WirelessDev,
    pub n_ssids: u32,
    pub n_channels: u32,
}

#[repr(C)]
pub struct Cfg80211Ssid {
    pub ssid: [u8; 32],
    pub ssid_len: u8,
}

#[repr(C)]
pub struct KeyParams {
    pub key: *const u8,
    pub key_len: u8,
    pub cipher: u32,
    pub key_idx: u8,
}

#[repr(C)]
pub struct Cfg80211ConnectParams {
    pub ssid: *const u8,
    pub ssid_len: usize,
    pub bssid: *const u8,
    pub ie: *const u8,
    pub ie_len: usize,
    pub key: KeyParams,
}

#[repr(C)]
pub struct StationParameters {
    pub supported_rates: *const u8,
    pub supported_rates_len: usize,
    pub sta_flags_mask: u32,
    pub sta_flags_set: u32,
}

fn update_event_state<F>(key: usize, update: F)
where
    F: FnOnce(&mut WirelessEventState),
{
    if let Ok(mut events) = WIRELESS_EVENTS.lock() {
        update(events.entry(key).or_default());
    }
}

#[no_mangle]
pub extern "C" fn wiphy_new_nm(
    _ops: *const c_void,
    sizeof_priv: usize,
    _requested_name: *const u8,
) -> *mut Wiphy {
    let mut wiphy = Box::new(Wiphy {
        priv_data: ptr::null_mut(),
        registered: AtomicI32::new(0),
        interface_modes: 0,
        max_scan_ssids: 4,
        max_scan_ie_len: 512,
        priv_alloc_size: 0,
        priv_alloc_align: 0,
    });

    if sizeof_priv != 0 {
        let layout = match Layout::from_size_align(sizeof_priv, 16) {
            Ok(layout) => layout,
            Err(_) => return ptr::null_mut(),
        };
        let ptr = unsafe { alloc_zeroed(layout) } as *mut c_void;
        if ptr.is_null() {
            return ptr::null_mut();
        }
        wiphy.priv_data = ptr;
        wiphy.priv_alloc_size = sizeof_priv;
        wiphy.priv_alloc_align = 16;
    }

    Box::into_raw(wiphy)
}

#[no_mangle]
pub extern "C" fn wiphy_free(wiphy: *mut Wiphy) {
    if wiphy.is_null() {
        return;
    }
    let wiphy_key = wiphy as usize;
    if let Ok(mut events) = WIRELESS_EVENTS.lock() {
        events.remove(&wiphy_key);
    }
    if let Ok(mut registry) = BSS_REGISTRY.lock() {
        if let Ok(mut ies_map) = BSS_IES.lock() {
            for entry in registry.iter() {
                if entry.wiphy == wiphy_key {
                    let ptr = entry.as_ref() as *const Cfg80211Bss as usize;
                    ies_map.remove(&ptr);
                }
            }
        }
        registry.retain(|e| e.wiphy != wiphy_key);
    }
    if let Ok(mut bands_map) = WIPY_BANDS_MAP.lock() {
        bands_map.remove(&wiphy_key);
    }
    unsafe {
        let wiphy_box = Box::from_raw(wiphy);
        if !wiphy_box.priv_data.is_null() {
            if let Ok(layout) = Layout::from_size_align(
                wiphy_box.priv_alloc_size.max(1),
                wiphy_box.priv_alloc_align.max(1),
            ) {
                dealloc(wiphy_box.priv_data.cast::<u8>(), layout);
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn wiphy_register(wiphy: *mut Wiphy) -> i32 {
    if wiphy.is_null() {
        return -22;
    }
    if unsafe { &*wiphy }.registered.load(Ordering::Acquire) != 0 {
        return -16;
    }
    unsafe { &*wiphy }.registered.store(1, Ordering::Release);
    0
}

#[no_mangle]
pub extern "C" fn wiphy_unregister(wiphy: *mut Wiphy) {
    if wiphy.is_null() {
        return;
    }
    unsafe { &*wiphy }.registered.store(0, Ordering::Release);
}

#[no_mangle]
pub extern "C" fn cfg80211_scan_done(
    request: *mut Cfg80211ScanRequest,
    info: *const Cfg80211ScanInfo,
) {
    if request.is_null() {
        return;
    }

    let wdev = unsafe { (*request).wdev };
    if wdev.is_null() {
        return;
    }

    unsafe {
        (*wdev).scan_in_flight = false;
        (*wdev).scan_aborted = if info.is_null() {
            false
        } else {
            (*info).aborted
        };
    }
}

fn netdev_to_wireless_dev(dev: *mut c_void) -> *mut WirelessDev {
    if dev.is_null() {
        return ptr::null_mut();
    }
    let dev = dev.cast::<NetDevice>();
    unsafe { (*dev).ieee80211_ptr.cast::<WirelessDev>() }
}

fn copy_bssid(dst: &mut WirelessDev, bssid: *const u8) {
    if bssid.is_null() {
        dst.has_bssid = false;
        dst.last_bssid = [0; 6];
        return;
    }

    unsafe {
        ptr::copy_nonoverlapping(bssid, dst.last_bssid.as_mut_ptr(), dst.last_bssid.len());
    }
    dst.has_bssid = true;
}

#[no_mangle]
pub extern "C" fn cfg80211_connect_result(
    dev: *mut c_void,
    bssid: *const u8,
    _req_ie: *const u8,
    _req_ie_len: usize,
    _resp_ie: *const u8,
    _resp_ie_len: usize,
    status: u16,
    _gfp: u32,
) {
    let wdev = netdev_to_wireless_dev(dev);
    if wdev.is_null() {
        return;
    }

    unsafe {
        let wdev_ref = &mut *wdev;
        wdev_ref.connecting = false;
        wdev_ref.connected = status == 0;
        wdev_ref.last_status = status;
        wdev_ref.locally_generated = false;
        copy_bssid(wdev_ref, bssid);
    }

    if status == 0 {
        netif_carrier_on(dev.cast::<NetDevice>());
    } else {
        netif_carrier_off(dev.cast::<NetDevice>());
    }
}

#[no_mangle]
pub extern "C" fn cfg80211_disconnected(
    dev: *mut c_void,
    reason: u16,
    _ie: *const u8,
    _ie_len: usize,
    locally_generated: bool,
    _gfp: u32,
) {
    let wdev = netdev_to_wireless_dev(dev);
    if !wdev.is_null() {
        unsafe {
            let wdev_ref = &mut *wdev;
            wdev_ref.connecting = false;
            wdev_ref.connected = false;
            wdev_ref.last_reason = reason;
            wdev_ref.locally_generated = locally_generated;
            wdev_ref.has_bssid = false;
            wdev_ref.last_bssid = [0; 6];
        }
    }

    netif_carrier_off(dev.cast::<NetDevice>());
}

#[no_mangle]
pub extern "C" fn cfg80211_connect_bss(
    dev: *mut c_void,
    bssid: *const u8,
    req_ie: *const u8,
    req_ie_len: usize,
    resp_ie: *const u8,
    resp_ie_len: usize,
    status: u16,
    gfp: u32,
) {
    cfg80211_connect_result(
        dev,
        bssid,
        req_ie,
        req_ie_len,
        resp_ie,
        resp_ie_len,
        status,
        gfp,
    )
}

#[no_mangle]
pub extern "C" fn cfg80211_ready_on_channel(
    wdev: *mut WirelessDev,
    cookie: u64,
    chan: *mut c_void,
    _chan_type: u32,
    duration: u32,
    _gfp: u32,
) {
    if wdev.is_null() {
        return;
    }
    let key = wdev as usize;
    let (freq, band) = if chan.is_null() {
        (0u16, 0u32)
    } else {
        let ch = chan.cast::<Ieee80211Channel>();
        unsafe { ((*ch).center_freq, (*ch).band) }
    };
    update_event_state(key, |state| {
        state.roc_cookie = cookie;
        state.roc_chan_freq = freq;
        state.roc_band = band;
        state.roc_duration = duration;
        state.roc_active = true;
    });
    log::trace!(
        "cfg80211_ready_on_channel: wdev={:#x} cookie={} freq={} duration={}",
        key,
        cookie,
        freq,
        duration
    );
}

#[repr(C)]
pub struct Ieee80211Channel {
    pub band: u32,
    pub center_freq: u16,
    pub hw_value: u16,
    pub flags: u32,
    pub max_power: i8,
    pub max_reg_power: i8,
    pub max_antenna_gain: i8,
    pub beacon_found: bool,
}

pub const NL80211_BAND_2GHZ: u32 = 0;
pub const NL80211_BAND_5GHZ: u32 = 1;
pub const NL80211_BAND_6GHZ: u32 = 2;

pub const IEEE80211_CHAN_DISABLED: u32 = 1 << 0;
pub const IEEE80211_CHAN_NO_IR: u32 = 1 << 1;
pub const IEEE80211_CHAN_RADAR: u32 = 1 << 2;
pub const IEEE80211_CHAN_NO_HT40PLUS: u32 = 1 << 3;
pub const IEEE80211_CHAN_NO_HT40MINUS: u32 = 1 << 4;
pub const IEEE80211_CHAN_NO_OFDM: u32 = 1 << 5;
pub const IEEE80211_CHAN_NO_80MHZ: u32 = 1 << 6;
pub const IEEE80211_CHAN_NO_160MHZ: u32 = 1 << 7;

#[repr(C)]
pub struct Ieee80211Rate {
    pub flags: u32,
    pub bitrate: u16,
    pub hw_value: u16,
    pub hw_value_short: u16,
}

pub const IEEE80211_RATE_SHORT_PREAMBLE: u32 = 1 << 0;
pub const IEEE80211_RATE_MANDATORY: u32 = 1 << 1;
pub const IEEE80211_RATE_ERP_G: u32 = 1 << 2;

#[repr(C)]
pub struct Ieee80211SupportedBand {
    pub channels: *mut Ieee80211Channel,
    pub n_channels: usize,
    pub bitrates: *mut Ieee80211Rate,
    pub n_bitrates: usize,
    pub ht_cap: *mut c_void,
    pub vht_cap: *mut c_void,
}

#[no_mangle]
pub extern "C" fn wiphy_bands_append(
    wiphy: *mut Wiphy,
    band_idx: u32,
    band: *mut Ieee80211SupportedBand,
) -> i32 {
    if wiphy.is_null() || band.is_null() {
        return -22;
    }

    if band_idx > NL80211_BAND_6GHZ {
        return -22;
    }

    let band_ref = unsafe { &*band };
    if band_ref.n_channels == 0 || band_ref.channels.is_null() {
        return -22;
    }

    let key = wiphy as usize;
    if let Ok(mut map) = WIPY_BANDS_MAP.lock() {
        let entry = map
            .entry(key)
            .or_insert_with(|| WiphyBands { bands: [0; 3] });
        entry.bands[band_idx as usize] = band as usize;
    }

    0
}

#[repr(C)]
pub struct Cfg80211Bss {
    pub bssid: [u8; 6],
    pub channel: *mut Ieee80211Channel,
    pub signal: i16,
    pub capability: u16,
    pub beacon_interval: u16,
    pub ies: *const u8,
    pub ies_len: usize,
    wiphy: usize,
}

unsafe impl Send for Cfg80211Bss {}

#[no_mangle]
pub extern "C" fn cfg80211_inform_bss(
    wiphy: *mut Wiphy,
    wdev: *mut WirelessDev,
    _freq: u32,
    bssid: *const u8,
    _tsf: u64,
    capability: u16,
    beacon_interval: u16,
    ies: *const u8,
    ies_len: usize,
    signal: i32,
    _gfp: u32,
) -> *mut Cfg80211Bss {
    if wiphy.is_null() || wdev.is_null() || bssid.is_null() {
        return ptr::null_mut();
    }

    let mut bssid_bytes = [0; 6];
    unsafe {
        ptr::copy_nonoverlapping(bssid, bssid_bytes.as_mut_ptr(), bssid_bytes.len());
    }

    let Ok(mut registry) = BSS_REGISTRY.lock() else {
        log::warn!("cfg80211_inform_bss: registry lock failed");
        return ptr::null_mut();
    };

    let clamped_signal = signal.clamp(i16::MIN as i32, i16::MAX as i32) as i16;

    let ies_owned: Vec<u8> = if !ies.is_null() && ies_len > 0 {
        unsafe { std::slice::from_raw_parts(ies, ies_len) }.to_vec()
    } else {
        Vec::new()
    };

    for entry in registry.iter_mut() {
        if entry.bssid == bssid_bytes && entry.wiphy == wiphy as usize {
            entry.signal = clamped_signal;
            entry.capability = capability;
            entry.beacon_interval = beacon_interval;
            let entry_ptr = entry.as_ref() as *const Cfg80211Bss as usize;
            if let Ok(mut ies_map) = BSS_IES.lock() {
                entry.ies_len = ies_owned.len();
                if ies_owned.is_empty() {
                    entry.ies = ptr::null();
                    ies_map.remove(&entry_ptr);
                } else {
                    let stored = ies_map.entry(entry_ptr).or_default();
                    stored.clear();
                    stored.extend_from_slice(&ies_owned);
                    entry.ies = stored.as_ptr();
                }
            }
            return entry.as_ref() as *const Cfg80211Bss as *mut Cfg80211Bss;
        }
    }

    let bss = Box::new(Cfg80211Bss {
        bssid: bssid_bytes,
        channel: ptr::null_mut(),
        signal: clamped_signal,
        capability,
        beacon_interval,
        ies: ptr::null(),
        ies_len: 0,
        wiphy: wiphy as usize,
    });
    registry.push(bss);
    let entry = match registry.last_mut() {
        Some(e) => e,
        None => return ptr::null_mut(),
    };
    let entry_ptr = entry.as_ref() as *const Cfg80211Bss as usize;
    if let Ok(mut ies_map) = BSS_IES.lock() {
        if !ies_owned.is_empty() {
            ies_map.insert(entry_ptr, ies_owned);
            entry.ies = ies_map[&entry_ptr].as_ptr();
            entry.ies_len = ies_map[&entry_ptr].len();
        }
    }
    entry.as_ref() as *const Cfg80211Bss as *mut Cfg80211Bss
}

#[no_mangle]
pub extern "C" fn cfg80211_put_bss(bss: *mut Cfg80211Bss) {
    if bss.is_null() {
        return;
    }
    let bss_addr = bss as usize;
    let Ok(mut registry) = BSS_REGISTRY.lock() else {
        log::warn!("cfg80211_put_bss: registry lock failed");
        return;
    };
    let before = registry.len();
    registry.retain(|entry| entry.as_ref() as *const Cfg80211Bss as usize != bss_addr);
    let removed = before != registry.len();
    if removed {
        if let Ok(mut ies_map) = BSS_IES.lock() {
            ies_map.remove(&bss_addr);
        }
    }
    log::trace!(
        "cfg80211_put_bss: released reference bss={:#x} (removed={})",
        bss_addr,
        removed
    );
}

#[no_mangle]
pub extern "C" fn cfg80211_get_bss(
    wiphy: *mut Wiphy,
    _band: u32,
    bssid: *const u8,
    ssid: *const u8,
    ssid_len: usize,
    _bss_type: u32,
    _privacy: u32,
) -> *mut Cfg80211Bss {
    if wiphy.is_null() {
        return ptr::null_mut();
    }

    let Ok(registry) = BSS_REGISTRY.lock() else {
        return ptr::null_mut();
    };

    let want_bssid = if bssid.is_null() {
        None
    } else {
        let mut bytes = [0u8; 6];
        unsafe { ptr::copy_nonoverlapping(bssid, bytes.as_mut_ptr(), 6) };
        Some(bytes)
    };

    let want_ssid = if ssid.is_null() || ssid_len == 0 {
        None
    } else {
        let slice = unsafe { std::slice::from_raw_parts(ssid, ssid_len) };
        Some(slice.to_vec())
    };

    for entry in registry.iter() {
        if entry.wiphy != wiphy as usize {
            continue;
        }
        if let Some(ref wb) = want_bssid {
            if entry.bssid != *wb {
                continue;
            }
        }
        if let Some(ref ws) = want_ssid {
            if entry.ies.is_null() || entry.ies_len < ws.len() + 2 {
                continue;
            }
            unsafe {
                let ies_slice = std::slice::from_raw_parts(entry.ies, entry.ies_len);
                let mut offset = 0;
                let mut found = false;
                while offset + 2 <= ies_slice.len() {
                    let tag_id = ies_slice[offset];
                    let tag_len = ies_slice[offset + 1] as usize;
                    if offset + 2 + tag_len > ies_slice.len() {
                        break;
                    }
                    if tag_id == 0 && tag_len == ws.len() {
                        if &ies_slice[offset + 2..offset + 2 + tag_len] == ws.as_slice() {
                            found = true;
                            break;
                        }
                    }
                    offset += 2 + tag_len;
                }
                if !found {
                    continue;
                }
            }
        }
        return entry.as_ref() as *const Cfg80211Bss as *mut Cfg80211Bss;
    }

    ptr::null_mut()
}

#[no_mangle]
pub extern "C" fn cfg80211_new_sta(
    dev: *mut c_void,
    mac_addr: *const u8,
    _params: *const StationParameters,
    _gfp: u32,
) {
    if dev.is_null() || mac_addr.is_null() {
        return;
    }

    let wdev = netdev_to_wireless_dev(dev);
    if wdev.is_null() || unsafe { (*wdev).wiphy }.is_null() {
        return;
    }

    let mut addr = [0u8; 6];
    unsafe { ptr::copy_nonoverlapping(mac_addr, addr.as_mut_ptr(), addr.len()) };
    update_event_state(unsafe { (*wdev).wiphy as usize }, |state| {
        state.new_sta = Some(addr)
    });
}

#[no_mangle]
pub extern "C" fn cfg80211_rx_mgmt(
    wdev: *mut WirelessDev,
    freq: u32,
    sig_dbm: i32,
    buf: *const u8,
    len: usize,
    _gfp: u32,
) {
    if wdev.is_null() || (buf.is_null() && len != 0) {
        return;
    }

    let frame_data = if !buf.is_null() && len > 0 {
        unsafe { std::slice::from_raw_parts(buf, len) }.to_vec()
    } else {
        Vec::new()
    };

    update_event_state(wdev as usize, |state| {
        state.mgmt_rx_freq = freq;
        state.mgmt_rx_signal = sig_dbm;
        state.mgmt_rx_len = len;
        state.mgmt_rx_data = frame_data;
    });

    log::debug!(
        "cfg80211_rx_mgmt: wdev={:#x} freq={} sig={} len={}",
        wdev as usize,
        freq,
        sig_dbm,
        len
    );
}

#[no_mangle]
pub extern "C" fn cfg80211_mgmt_tx_status(
    wdev: *mut WirelessDev,
    cookie: u64,
    buf: *const u8,
    len: usize,
    ack: bool,
    _gfp: u32,
) {
    if wdev.is_null() || (buf.is_null() && len != 0) {
        return;
    }

    let frame_data = if !buf.is_null() && len > 0 {
        unsafe { std::slice::from_raw_parts(buf, len) }.to_vec()
    } else {
        Vec::new()
    };

    update_event_state(wdev as usize, |state| {
        state.mgmt_tx_cookie = cookie;
        state.mgmt_tx_len = len;
        state.mgmt_tx_ack = ack;
        state.mgmt_tx_data = frame_data;
    });

    log::debug!(
        "cfg80211_mgmt_tx_status: wdev={:#x} cookie={} len={} ack={}",
        wdev as usize,
        cookie,
        len,
        ack
    );
}

#[no_mangle]
pub extern "C" fn cfg80211_sched_scan_results(wiphy: *mut Wiphy, reqid: u64) {
    if wiphy.is_null() {
        return;
    }
    update_event_state(wiphy as usize, |state| state.sched_scan_reqid = reqid);
}

#[no_mangle]
pub extern "C" fn ieee80211_channel_to_frequency(chan: u32, band: u32) -> u32 {
    match band {
        NL80211_BAND_2GHZ => match chan {
            14 => 2484,
            1..=13 => 2407 + chan * 5,
            _ => 0,
        },
        NL80211_BAND_5GHZ => 5000 + chan * 5,
        NL80211_BAND_6GHZ => {
            if chan == 2 {
                5935
            } else if chan >= 1 {
                5950 + chan * 5
            } else {
                0
            }
        }
        _ => 0,
    }
}

#[no_mangle]
pub extern "C" fn ieee80211_frequency_to_channel(freq: u32) -> u32 {
    match freq {
        2484 => 14,
        2412..=2472 => (freq - 2407) / 5,
        5000..=5895 => (freq - 5000) / 5,
        5935 => 2,
        5955..=7115 => (freq - 5950) / 5,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rust_impl::net::{alloc_netdev_mqs, free_netdev, netif_carrier_ok};
    use std::ffi::CString;

    #[test]
    fn wiphy_registration_round_trip_works() {
        let wiphy = wiphy_new_nm(ptr::null(), 0, ptr::null());
        assert!(!wiphy.is_null());
        assert_eq!(wiphy_register(wiphy), 0);
        assert_eq!(wiphy_register(wiphy), -16);
        assert_eq!(unsafe { (*wiphy).registered.load(Ordering::Acquire) }, 1);
        wiphy_unregister(wiphy);
        assert_eq!(unsafe { (*wiphy).registered.load(Ordering::Acquire) }, 0);
        wiphy_free(wiphy);
    }

    #[test]
    fn scan_and_connect_lifecycle_updates_wireless_state() {
        let name = CString::new("wlan%d").expect("valid test CString");
        let dev = alloc_netdev_mqs(0, name.as_ptr().cast::<u8>(), 0, None, 1, 1);
        assert!(!dev.is_null());

        let wiphy = wiphy_new_nm(ptr::null(), 32, ptr::null());
        assert!(!wiphy.is_null());

        let mut wdev = WirelessDev {
            wiphy,
            netdev: dev.cast::<c_void>(),
            iftype: 0,
            scan_in_flight: true,
            scan_aborted: false,
            connecting: true,
            connected: false,
            locally_generated: false,
            last_status: u16::MAX,
            last_reason: 0,
            has_bssid: false,
            last_bssid: [0; 6],
        };

        unsafe {
            (*dev).ieee80211_ptr = (&mut wdev as *mut WirelessDev).cast::<c_void>();
        }

        let mut request = Cfg80211ScanRequest {
            wiphy,
            wdev: &mut wdev,
            n_ssids: 1,
            n_channels: 1,
        };
        let info = Cfg80211ScanInfo { aborted: true };
        cfg80211_scan_done(&mut request, &info);
        assert!(!wdev.scan_in_flight);
        assert!(wdev.scan_aborted);

        let bssid = [1, 2, 3, 4, 5, 6];
        cfg80211_connect_result(
            dev.cast::<c_void>(),
            bssid.as_ptr(),
            ptr::null(),
            0,
            ptr::null(),
            0,
            0,
            0,
        );
        assert!(wdev.connected);
        assert_eq!(wdev.last_status, 0);
        assert!(wdev.has_bssid);
        assert_eq!(wdev.last_bssid, bssid);
        assert_eq!(netif_carrier_ok(dev), 1);

        cfg80211_disconnected(dev.cast::<c_void>(), 7, ptr::null(), 0, true, 0);
        assert!(!wdev.connected);
        assert_eq!(wdev.last_reason, 7);
        assert!(wdev.locally_generated);
        assert_eq!(netif_carrier_ok(dev), 0);

        wiphy_free(wiphy);
        free_netdev(dev);
    }

    #[test]
    fn ieee80211_channel_creation_and_flags_work() {
        let channel = Ieee80211Channel {
            band: NL80211_BAND_5GHZ,
            center_freq: 5180,
            hw_value: 36,
            flags: IEEE80211_CHAN_NO_IR | IEEE80211_CHAN_RADAR | IEEE80211_CHAN_NO_80MHZ,
            max_power: 20,
            max_reg_power: 23,
            max_antenna_gain: 6,
            beacon_found: true,
        };

        assert_eq!(channel.band, NL80211_BAND_5GHZ);
        assert_eq!(channel.center_freq, 5180);
        assert_eq!(channel.hw_value, 36);
        assert_ne!(channel.flags & IEEE80211_CHAN_NO_IR, 0);
        assert_ne!(channel.flags & IEEE80211_CHAN_RADAR, 0);
        assert_ne!(channel.flags & IEEE80211_CHAN_NO_80MHZ, 0);
        assert_eq!(channel.flags & IEEE80211_CHAN_DISABLED, 0);
        assert!(channel.beacon_found);
    }

    #[test]
    fn cfg80211_events_and_channel_frequency_conversions_work() {
        let name = CString::new("wlan%d").expect("valid test CString");
        let dev = alloc_netdev_mqs(0, name.as_ptr().cast::<u8>(), 0, None, 1, 1);
        assert!(!dev.is_null());
        let wiphy = wiphy_new_nm(ptr::null(), 0, ptr::null());
        assert!(!wiphy.is_null());
        let mut wdev = WirelessDev {
            wiphy,
            netdev: dev.cast::<c_void>(),
            iftype: 0,
            scan_in_flight: false,
            scan_aborted: false,
            connecting: false,
            connected: false,
            locally_generated: false,
            last_status: 0,
            last_reason: 0,
            has_bssid: false,
            last_bssid: [0; 6],
        };
        unsafe { (*dev).ieee80211_ptr = (&mut wdev as *mut WirelessDev).cast::<c_void>() };

        let sta = [6u8, 5, 4, 3, 2, 1];
        cfg80211_new_sta(dev.cast::<c_void>(), sta.as_ptr(), ptr::null(), 0);
        cfg80211_rx_mgmt(&mut wdev, 2412, -42, sta.as_ptr(), sta.len(), 0);
        cfg80211_mgmt_tx_status(&mut wdev, 99, sta.as_ptr(), sta.len(), true, 0);
        cfg80211_sched_scan_results(wiphy, 1234);

        let events = WIRELESS_EVENTS.lock().expect("wireless events lock");
        let wiphy_state = events.get(&(wiphy as usize)).expect("wiphy event state");
        assert_eq!(wiphy_state.new_sta, Some(sta));
        assert_eq!(wiphy_state.sched_scan_reqid, 1234);
        let wdev_state = events
            .get(&((&mut wdev as *mut WirelessDev) as usize))
            .expect("wdev event state");
        assert_eq!(wdev_state.mgmt_rx_freq, 2412);
        assert_eq!(wdev_state.mgmt_rx_signal, -42);
        assert_eq!(wdev_state.mgmt_rx_data, sta.to_vec());
        assert_eq!(wdev_state.mgmt_tx_cookie, 99);
        assert!(wdev_state.mgmt_tx_ack);
        assert_eq!(wdev_state.mgmt_tx_data, sta.to_vec());
        drop(events);

        assert_eq!(ieee80211_channel_to_frequency(1, NL80211_BAND_2GHZ), 2412);
        assert_eq!(ieee80211_channel_to_frequency(36, NL80211_BAND_5GHZ), 5180);
        assert_eq!(ieee80211_frequency_to_channel(2484), 14);
        assert_eq!(ieee80211_frequency_to_channel(5955), 1);

        wiphy_free(wiphy);
        free_netdev(dev);
    }

    #[test]
    fn test_cfg80211_put_bss_removes_from_registry() {
        let name = CString::new("wlan%d").expect("valid test CString");
        let dev = alloc_netdev_mqs(0, name.as_ptr().cast::<u8>(), 0, None, 1, 1);
        assert!(!dev.is_null());
        let wiphy = wiphy_new_nm(ptr::null(), 32, ptr::null());
        assert!(!wiphy.is_null());

        let mut wdev = WirelessDev {
            wiphy,
            netdev: dev.cast::<c_void>(),
            iftype: 0,
            scan_in_flight: false,
            scan_aborted: false,
            connecting: false,
            connected: false,
            locally_generated: false,
            last_status: 0,
            last_reason: 0,
            has_bssid: false,
            last_bssid: [0; 6],
        };
        unsafe { (*dev).ieee80211_ptr = (&mut wdev as *mut WirelessDev).cast::<c_void>() };

        let bssid: [u8; 6] = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];
        let ies = [1u8, 2, 3, 4];

        let bss = cfg80211_inform_bss(
            wiphy,
            &mut wdev as *mut WirelessDev,
            2412,
            bssid.as_ptr(),
            0,
            0x0431,
            100,
            ies.as_ptr(),
            ies.len(),
            -50,
            0,
        );
        assert!(!bss.is_null());

        let found = cfg80211_get_bss(wiphy, 0, bssid.as_ptr(), ptr::null(), 0, 0, 0);
        assert!(
            !found.is_null(),
            "BSS should be in registry after inform_bss"
        );

        cfg80211_put_bss(bss);

        let after_put = cfg80211_get_bss(wiphy, 0, bssid.as_ptr(), ptr::null(), 0, 0, 0);
        assert!(
            after_put.is_null(),
            "BSS should be removed from registry after put_bss"
        );

        wiphy_free(wiphy);
        free_netdev(dev);
    }
}
