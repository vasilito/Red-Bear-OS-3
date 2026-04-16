use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicI32, Ordering};

use super::net::{netif_carrier_off, netif_carrier_on, NetDevice};

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
    _wdev: *mut WirelessDev,
    _cookie: u64,
    _chan: *mut c_void,
    _chan_type: u32,
    _duration: u32,
    _gfp: u32,
) {
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
        let name = CString::new("wlan%d").unwrap();
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
}
