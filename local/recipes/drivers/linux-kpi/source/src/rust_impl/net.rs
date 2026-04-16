use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicI32, Ordering};

#[repr(C)]
pub struct SkBuff {
    pub head: *mut u8,
    pub data: *mut u8,
    pub len: u32,
    pub tail: u32,
    pub end: u32,
}

#[repr(C)]
pub struct NetDevice {
    pub name: [u8; 16],
    pub dev_addr: [u8; 6],
    pub addr_len: u8,
    pub mtu: u32,
    pub flags: u32,
    pub carrier: AtomicI32,
    pub ml_priv: *mut c_void,
    pub ieee80211_ptr: *mut c_void,
    pub priv_data: *mut c_void,
    pub registered: AtomicI32,
    priv_alloc_size: usize,
    priv_alloc_align: usize,
}

unsafe fn free_skb_buffer(skb: *mut SkBuff) {
    if skb.is_null() {
        return;
    }
    let head = (*skb).head;
    let end = (*skb).end as usize;
    if !head.is_null() && end != 0 {
        if let Ok(layout) = Layout::from_size_align(end, 16) {
            dealloc(head, layout);
        }
    }
}

#[no_mangle]
pub extern "C" fn alloc_skb(size: u32, _gfp_mask: u32) -> *mut SkBuff {
    let capacity = size as usize;
    let layout = match Layout::from_size_align(capacity.max(1), 16) {
        Ok(layout) => layout,
        Err(_) => return ptr::null_mut(),
    };
    let head = unsafe { alloc_zeroed(layout) };
    if head.is_null() {
        return ptr::null_mut();
    }

    Box::into_raw(Box::new(SkBuff {
        head,
        data: head,
        len: 0,
        tail: 0,
        end: capacity as u32,
    }))
}

#[no_mangle]
pub extern "C" fn kfree_skb(skb: *mut SkBuff) {
    if skb.is_null() {
        return;
    }
    unsafe {
        free_skb_buffer(skb);
        drop(Box::from_raw(skb));
    }
}

#[no_mangle]
pub extern "C" fn skb_reserve(skb: *mut SkBuff, len: u32) {
    if skb.is_null() {
        return;
    }

    let skb_ref = unsafe { &mut *skb };
    let headroom = unsafe { skb_ref.data.offset_from(skb_ref.head) };
    let Ok(headroom) = u32::try_from(headroom) else {
        return;
    };
    let new_headroom = headroom.saturating_add(len);
    if new_headroom > skb_ref.end || skb_ref.tail != 0 || skb_ref.len != 0 {
        return;
    }

    skb_ref.data = unsafe { skb_ref.head.add(new_headroom as usize) };
}

#[no_mangle]
pub extern "C" fn skb_put(skb: *mut SkBuff, len: u32) -> *mut u8 {
    if skb.is_null() {
        return ptr::null_mut();
    }

    let skb_ref = unsafe { &mut *skb };
    let new_tail = skb_ref.tail.saturating_add(len);
    if new_tail > skb_ref.end {
        return ptr::null_mut();
    }

    let ptr = unsafe { skb_ref.data.add(skb_ref.tail as usize) };
    skb_ref.tail = new_tail;
    skb_ref.len = skb_ref.len.saturating_add(len);
    ptr
}

#[no_mangle]
pub extern "C" fn skb_push(skb: *mut SkBuff, len: u32) -> *mut u8 {
    if skb.is_null() {
        return ptr::null_mut();
    }

    let skb_ref = unsafe { &mut *skb };
    let headroom = unsafe { skb_ref.data.offset_from(skb_ref.head) };
    if headroom < len as isize {
        return ptr::null_mut();
    }

    skb_ref.data = unsafe { skb_ref.data.sub(len as usize) };
    skb_ref.tail = skb_ref.tail.saturating_add(len);
    skb_ref.len = skb_ref.len.saturating_add(len);
    skb_ref.data
}

#[no_mangle]
pub extern "C" fn skb_pull(skb: *mut SkBuff, len: u32) -> *mut u8 {
    if skb.is_null() {
        return ptr::null_mut();
    }

    let skb_ref = unsafe { &mut *skb };
    if len > skb_ref.len {
        return ptr::null_mut();
    }

    skb_ref.data = unsafe { skb_ref.data.add(len as usize) };
    skb_ref.tail -= len;
    skb_ref.len -= len;
    skb_ref.data
}

#[no_mangle]
pub extern "C" fn skb_headroom(skb: *const SkBuff) -> u32 {
    if skb.is_null() {
        return 0;
    }

    let skb_ref = unsafe { &*skb };
    let headroom = unsafe { skb_ref.data.offset_from(skb_ref.head) };
    u32::try_from(headroom).unwrap_or_default()
}

#[no_mangle]
pub extern "C" fn skb_tailroom(skb: *const SkBuff) -> u32 {
    if skb.is_null() {
        return 0;
    }

    let skb_ref = unsafe { &*skb };
    let headroom = skb_headroom(skb);
    skb_ref
        .end
        .saturating_sub(headroom.saturating_add(skb_ref.tail))
}

#[no_mangle]
pub extern "C" fn skb_trim(skb: *mut SkBuff, len: u32) {
    if skb.is_null() {
        return;
    }
    let skb_ref = unsafe { &mut *skb };
    let new_len = len.min(skb_ref.len);
    skb_ref.len = new_len;
    skb_ref.tail = new_len;
}

#[no_mangle]
pub extern "C" fn alloc_netdev_mqs(
    sizeof_priv: usize,
    name: *const u8,
    _name_assign_type: u8,
    _setup: Option<extern "C" fn(*mut NetDevice)>,
    _txqs: u32,
    _rxqs: u32,
) -> *mut NetDevice {
    let mut dev = Box::new(NetDevice {
        name: [0; 16],
        dev_addr: [0; 6],
        addr_len: 6,
        mtu: 1500,
        flags: 0,
        carrier: AtomicI32::new(0),
        ml_priv: ptr::null_mut(),
        ieee80211_ptr: ptr::null_mut(),
        priv_data: ptr::null_mut(),
        registered: AtomicI32::new(0),
        priv_alloc_size: 0,
        priv_alloc_align: 0,
    });

    if !name.is_null() {
        for (idx, byte) in dev.name.iter_mut().enumerate() {
            let value = unsafe { *name.add(idx) };
            *byte = value;
            if value == 0 {
                break;
            }
        }
    }

    if sizeof_priv != 0 {
        let layout = match Layout::from_size_align(sizeof_priv, 16) {
            Ok(layout) => layout,
            Err(_) => return ptr::null_mut(),
        };
        let priv_ptr = unsafe { alloc_zeroed(layout) } as *mut c_void;
        if priv_ptr.is_null() {
            return ptr::null_mut();
        }
        dev.priv_data = priv_ptr;
        dev.priv_alloc_size = sizeof_priv;
        dev.priv_alloc_align = 16;
    }

    if let Some(setup) = _setup {
        setup(dev.as_mut());
    }

    Box::into_raw(dev)
}

#[no_mangle]
pub extern "C" fn free_netdev(dev: *mut NetDevice) {
    if dev.is_null() {
        return;
    }
    unsafe {
        let dev_box = Box::from_raw(dev);
        if !dev_box.priv_data.is_null() {
            let layout = Layout::from_size_align(
                dev_box.priv_alloc_size.max(1),
                dev_box.priv_alloc_align.max(1),
            )
            .ok();
            if let Some(layout) = layout {
                dealloc(dev_box.priv_data.cast::<u8>(), layout);
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn register_netdev(dev: *mut NetDevice) -> i32 {
    if dev.is_null() {
        return -22;
    }
    if unsafe { &*dev }.registered.load(Ordering::Acquire) != 0 {
        return -16;
    }
    unsafe { &*dev }.registered.store(1, Ordering::Release);
    0
}

#[no_mangle]
pub extern "C" fn unregister_netdev(dev: *mut NetDevice) {
    if dev.is_null() {
        return;
    }
    unsafe { &*dev }.registered.store(0, Ordering::Release);
    netif_carrier_off(dev);
}

#[no_mangle]
pub extern "C" fn netif_carrier_on(dev: *mut NetDevice) {
    if dev.is_null() {
        return;
    }
    unsafe { &*dev }.carrier.store(1, Ordering::Release);
}

#[no_mangle]
pub extern "C" fn netif_carrier_off(dev: *mut NetDevice) {
    if dev.is_null() {
        return;
    }
    unsafe { &*dev }.carrier.store(0, Ordering::Release);
}

#[no_mangle]
pub extern "C" fn netif_carrier_ok(dev: *const NetDevice) -> i32 {
    if dev.is_null() {
        return 0;
    }
    if unsafe { &*dev }.carrier.load(Ordering::Acquire) != 0 {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::sync::atomic::AtomicUsize;

    static SETUP_CALLS: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn test_setup(_dev: *mut NetDevice) {
        SETUP_CALLS.fetch_add(1, Ordering::AcqRel);
    }

    #[test]
    fn skb_allocation_and_growth_work() {
        let skb = alloc_skb(64, 0);
        assert!(!skb.is_null());
        let tail_ptr = skb_put(skb, 8);
        assert!(!tail_ptr.is_null());
        assert_eq!(unsafe { (*skb).len }, 8);
        skb_trim(skb, 4);
        assert_eq!(unsafe { (*skb).len }, 4);
        kfree_skb(skb);
    }

    #[test]
    fn skb_headroom_and_push_pull_work() {
        let skb = alloc_skb(64, 0);
        assert!(!skb.is_null());
        skb_reserve(skb, 8);
        assert_eq!(skb_headroom(skb), 8);
        assert_eq!(skb_tailroom(skb), 56);
        assert!(!skb_put(skb, 8).is_null());
        assert!(!skb_push(skb, 4).is_null());
        assert_eq!(unsafe { (*skb).len }, 12);
        assert!(!skb_pull(skb, 6).is_null());
        assert_eq!(unsafe { (*skb).len }, 6);
        kfree_skb(skb);
    }

    #[test]
    fn net_device_carrier_tracking_works() {
        let name = CString::new("wlan%d").unwrap();
        let dev = alloc_netdev_mqs(
            0usize,
            name.as_ptr().cast::<u8>(),
            0u8,
            None::<extern "C" fn(*mut NetDevice)>,
            1u32,
            1u32,
        );
        assert!(!dev.is_null());
        assert_eq!(netif_carrier_ok(dev), 0);
        netif_carrier_on(dev);
        assert_eq!(netif_carrier_ok(dev), 1);
        netif_carrier_off(dev);
        assert_eq!(netif_carrier_ok(dev), 0);
        free_netdev(dev);
    }

    #[test]
    fn net_device_setup_and_registration_work() {
        SETUP_CALLS.store(0, Ordering::Release);
        let name = CString::new("wlan%d").unwrap();
        let dev = alloc_netdev_mqs(
            32usize,
            name.as_ptr().cast::<u8>(),
            0u8,
            Some(test_setup),
            1u32,
            1u32,
        );
        assert!(!dev.is_null());
        assert_eq!(SETUP_CALLS.load(Ordering::Acquire), 1);
        assert_eq!(register_netdev(dev), 0);
        assert_eq!(unsafe { (*dev).registered.load(Ordering::Acquire) }, 1);
        assert_eq!(register_netdev(dev), -16);
        unregister_netdev(dev);
        assert_eq!(unsafe { (*dev).registered.load(Ordering::Acquire) }, 0);
        free_netdev(dev);
    }
}
