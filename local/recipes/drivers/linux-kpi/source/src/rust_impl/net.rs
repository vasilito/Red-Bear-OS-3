use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicI32, AtomicU32, AtomicUsize, Ordering};

const NAPI_STATE_IDLE: i32 = 0;
const NAPI_STATE_SCHEDULED: i32 = 1;

#[repr(C)]
struct SkbSharedInfo {
    refcount: AtomicUsize,
    capacity: usize,
    align: usize,
}

#[repr(C)]
pub struct SkBuff {
    pub head: *mut u8,
    pub data: *mut u8,
    pub len: u32,
    pub tail: u32,
    pub end: u32,
    pub next: *mut SkBuff,
    pub prev: *mut SkBuff,
    pub network_header: i32,
    pub mac_header: i32,
    shared: *mut SkbSharedInfo,
}

#[repr(C)]
pub struct SkBuffHead {
    pub next: *mut SkBuff,
    pub prev: *mut SkBuff,
    pub qlen: u32,
    pub lock: u8,
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
    pub tx_queue_state: AtomicU32,
    pub device_attached: AtomicI32,
    priv_alloc_size: usize,
    priv_alloc_align: usize,
}

#[repr(C)]
pub struct NapiStruct {
    pub poll: Option<extern "C" fn(*mut NapiStruct, budget: i32) -> i32>,
    pub dev: *mut NetDevice,
    pub state: AtomicI32,
    pub weight: i32,
}

unsafe fn release_skb_buffer(skb: *mut SkBuff) {
    if skb.is_null() {
        return;
    }

    let shared = (*skb).shared;
    if shared.is_null() {
        return;
    }

    if (*shared).refcount.fetch_sub(1, Ordering::AcqRel) == 1 {
        let capacity = (*shared).capacity.max(1);
        let align = (*shared).align.max(1);
        if !(*skb).head.is_null() {
            if let Ok(layout) = Layout::from_size_align(capacity, align) {
                dealloc((*skb).head, layout);
            }
        }
        drop(Box::from_raw(shared));
    }
}

fn skb_headroom_inner(skb: &SkBuff) -> u32 {
    let headroom = unsafe { skb.data.offset_from(skb.head) };
    u32::try_from(headroom).unwrap_or_default()
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

    let shared = Box::into_raw(Box::new(SkbSharedInfo {
        refcount: AtomicUsize::new(1),
        capacity: capacity.max(1),
        align: 16,
    }));

    Box::into_raw(Box::new(SkBuff {
        head,
        data: head,
        len: 0,
        tail: 0,
        end: capacity as u32,
        next: ptr::null_mut(),
        prev: ptr::null_mut(),
        network_header: 0,
        mac_header: 0,
        shared,
    }))
}

#[no_mangle]
pub extern "C" fn kfree_skb(skb: *mut SkBuff) {
    if skb.is_null() {
        return;
    }
    unsafe {
        release_skb_buffer(skb);
        drop(Box::from_raw(skb));
    }
}

#[no_mangle]
pub extern "C" fn skb_reserve(skb: *mut SkBuff, len: u32) {
    if skb.is_null() {
        return;
    }

    let skb_ref = unsafe { &mut *skb };
    let headroom = skb_headroom_inner(skb_ref);
    let new_headroom = headroom.saturating_add(len);
    if new_headroom > skb_ref.end || skb_ref.tail != 0 || skb_ref.len != 0 {
        return;
    }

    skb_ref.data = unsafe { skb_ref.head.add(new_headroom as usize) };
    skb_ref.mac_header = new_headroom as i32;
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
    skb_ref.tail = skb_ref.tail.saturating_sub(len);
    skb_ref.len = skb_ref.len.saturating_sub(len);
    skb_ref.data
}

#[no_mangle]
pub extern "C" fn skb_headroom(skb: *const SkBuff) -> u32 {
    if skb.is_null() {
        return 0;
    }

    skb_headroom_inner(unsafe { &*skb })
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
pub extern "C" fn skb_queue_head_init(list: *mut SkBuffHead) {
    if list.is_null() {
        return;
    }

    unsafe {
        (*list).next = ptr::null_mut();
        (*list).prev = ptr::null_mut();
        (*list).qlen = 0;
        (*list).lock = 0;
    }
}

#[no_mangle]
pub extern "C" fn skb_queue_tail(list: *mut SkBuffHead, newsk: *mut SkBuff) {
    if list.is_null() || newsk.is_null() {
        return;
    }

    unsafe {
        (*newsk).next = ptr::null_mut();
        (*newsk).prev = (*list).prev;
        if (*list).prev.is_null() {
            (*list).next = newsk;
        } else {
            (*(*list).prev).next = newsk;
        }
        (*list).prev = newsk;
        if (*list).next.is_null() {
            (*list).next = newsk;
        }
        (*list).qlen = (*list).qlen.saturating_add(1);
    }
}

#[no_mangle]
pub extern "C" fn skb_dequeue(list: *mut SkBuffHead) -> *mut SkBuff {
    if list.is_null() || unsafe { (*list).qlen } == 0 {
        return ptr::null_mut();
    }

    unsafe {
        let skb = (*list).next;
        if skb.is_null() {
            return ptr::null_mut();
        }

        (*list).next = (*skb).next;
        if (*list).next.is_null() {
            (*list).prev = ptr::null_mut();
        } else {
            (*(*list).next).prev = ptr::null_mut();
        }
        (*skb).next = ptr::null_mut();
        (*skb).prev = ptr::null_mut();
        (*list).qlen = (*list).qlen.saturating_sub(1);
        skb
    }
}

#[no_mangle]
pub extern "C" fn skb_queue_purge(list: *mut SkBuffHead) {
    if list.is_null() {
        return;
    }

    loop {
        let skb = skb_dequeue(list);
        if skb.is_null() {
            break;
        }
        kfree_skb(skb);
    }
}

#[no_mangle]
pub extern "C" fn skb_peek(list: *const SkBuffHead) -> *mut SkBuff {
    if list.is_null() || unsafe { (*list).qlen } == 0 {
        ptr::null_mut()
    } else {
        unsafe { (*list).next }
    }
}

#[no_mangle]
pub extern "C" fn skb_queue_len(list: *const SkBuffHead) -> u32 {
    if list.is_null() {
        0
    } else {
        unsafe { (*list).qlen }
    }
}

#[no_mangle]
pub extern "C" fn skb_queue_empty(list: *const SkBuffHead) -> i32 {
    if skb_queue_len(list) == 0 {
        1
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn __netdev_alloc_skb(
    _dev: *mut NetDevice,
    length: u32,
    gfp_mask: u32,
) -> *mut SkBuff {
    alloc_skb(length, gfp_mask)
}

#[no_mangle]
pub extern "C" fn skb_copy(src: *const SkBuff, gfp: u32) -> *mut SkBuff {
    if src.is_null() {
        return ptr::null_mut();
    }

    let src_ref = unsafe { &*src };
    let dst = alloc_skb(src_ref.end, gfp);
    if dst.is_null() {
        return ptr::null_mut();
    }

    let headroom = skb_headroom(src);
    skb_reserve(dst, headroom);
    let dst_data = skb_put(dst, src_ref.len);
    if dst_data.is_null() {
        kfree_skb(dst);
        return ptr::null_mut();
    }

    if src_ref.len != 0 {
        unsafe { ptr::copy_nonoverlapping(src_ref.data, dst_data, src_ref.len as usize) };
    }
    unsafe {
        (*dst).network_header = src_ref.network_header;
        (*dst).mac_header = src_ref.mac_header;
    }
    dst
}

#[no_mangle]
pub extern "C" fn skb_clone(skb: *const SkBuff, _gfp: u32) -> *mut SkBuff {
    if skb.is_null() {
        return ptr::null_mut();
    }

    let skb_ref = unsafe { &*skb };
    if skb_ref.shared.is_null() {
        return ptr::null_mut();
    }

    unsafe { &*skb_ref.shared }
        .refcount
        .fetch_add(1, Ordering::AcqRel);
    Box::into_raw(Box::new(SkBuff {
        head: skb_ref.head,
        data: skb_ref.data,
        len: skb_ref.len,
        tail: skb_ref.tail,
        end: skb_ref.end,
        next: ptr::null_mut(),
        prev: ptr::null_mut(),
        network_header: skb_ref.network_header,
        mac_header: skb_ref.mac_header,
        shared: skb_ref.shared,
    }))
}

#[no_mangle]
pub extern "C" fn skb_set_network_header(skb: *mut SkBuff, offset: i32) {
    if skb.is_null() {
        return;
    }
    unsafe { (*skb).network_header = offset };
}

#[no_mangle]
pub extern "C" fn skb_reset_mac_header(skb: *mut SkBuff) {
    if skb.is_null() {
        return;
    }
    unsafe {
        (*skb).mac_header = skb_headroom_inner(&*skb) as i32;
    }
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
        tx_queue_state: AtomicU32::new(0),
        device_attached: AtomicI32::new(1),
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

#[no_mangle]
pub extern "C" fn netif_napi_add(
    dev: *mut NetDevice,
    napi: *mut NapiStruct,
    poll: Option<extern "C" fn(*mut NapiStruct, i32) -> i32>,
    weight: i32,
) {
    if napi.is_null() {
        return;
    }

    unsafe {
        (*napi).dev = dev;
        (*napi).poll = poll;
        (*napi).weight = weight;
        (*napi).state.store(NAPI_STATE_IDLE, Ordering::Release);
    }
}

#[no_mangle]
pub extern "C" fn napi_schedule(napi: *mut NapiStruct) {
    if napi.is_null() {
        return;
    }

    let napi_ref = unsafe { &*napi };
    if napi_ref
        .state
        .compare_exchange(
            NAPI_STATE_IDLE,
            NAPI_STATE_SCHEDULED,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_ok()
    {
        if let Some(poll) = napi_ref.poll {
            let _ = poll(napi, napi_ref.weight);
        }
    }
}

#[no_mangle]
pub extern "C" fn napi_complete_done(napi: *mut NapiStruct, work_done: i32) -> i32 {
    if napi.is_null() || work_done < 0 {
        return 0;
    }

    unsafe { &*napi }
        .state
        .store(NAPI_STATE_IDLE, Ordering::Release);
    if work_done < unsafe { (*napi).weight } {
        1
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn netif_tx_wake_queue(dev: *mut NetDevice, queue_idx: u16) {
    if dev.is_null() || queue_idx >= 32 {
        return;
    }
    let mask = !(1u32 << queue_idx);
    let _ = unsafe { &*dev }
        .tx_queue_state
        .fetch_and(mask, Ordering::AcqRel);
}

#[no_mangle]
pub extern "C" fn netif_tx_stop_queue(dev: *mut NetDevice, queue_idx: u16) {
    if dev.is_null() || queue_idx >= 32 {
        return;
    }
    let mask = 1u32 << queue_idx;
    let _ = unsafe { &*dev }
        .tx_queue_state
        .fetch_or(mask, Ordering::AcqRel);
}

#[no_mangle]
pub extern "C" fn netif_device_attach(dev: *mut NetDevice) {
    if dev.is_null() {
        return;
    }
    unsafe { &*dev }.device_attached.store(1, Ordering::Release);
}

#[no_mangle]
pub extern "C" fn netif_device_detach(dev: *mut NetDevice) {
    if dev.is_null() {
        return;
    }
    unsafe { &*dev }.device_attached.store(0, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::sync::atomic::AtomicUsize;

    static SETUP_CALLS: AtomicUsize = AtomicUsize::new(0);
    static NAPI_POLLS: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn test_setup(_dev: *mut NetDevice) {
        SETUP_CALLS.fetch_add(1, Ordering::AcqRel);
    }

    extern "C" fn test_napi_poll(_napi: *mut NapiStruct, budget: i32) -> i32 {
        NAPI_POLLS.fetch_add(1, Ordering::AcqRel);
        budget - 1
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
    fn skb_queue_copy_and_clone_work() {
        let skb = alloc_skb(32, 0);
        assert!(!skb.is_null());
        skb_reserve(skb, 4);
        let data = skb_put(skb, 6);
        assert!(!data.is_null());
        unsafe { ptr::copy_nonoverlapping([1u8, 2, 3, 4, 5, 6].as_ptr(), data, 6) };
        skb_set_network_header(skb, 2);
        skb_reset_mac_header(skb);

        let copy = skb_copy(skb, 0);
        assert!(!copy.is_null());
        assert_eq!(unsafe { (*copy).len }, 6);
        assert_eq!(unsafe { (*copy).network_header }, 2);

        let clone = skb_clone(skb, 0);
        assert!(!clone.is_null());
        assert_eq!(unsafe { (*clone).data }, unsafe { (*skb).data });

        let mut queue = SkBuffHead {
            next: ptr::null_mut(),
            prev: ptr::null_mut(),
            qlen: 123,
            lock: 1,
        };
        skb_queue_head_init(&mut queue);
        skb_queue_tail(&mut queue, skb);
        skb_queue_tail(&mut queue, copy);
        assert_eq!(skb_queue_len(&queue), 2);
        assert_eq!(skb_queue_empty(&queue), 0);
        assert_eq!(skb_peek(&queue), skb);
        assert_eq!(skb_dequeue(&mut queue), skb);
        assert_eq!(skb_queue_len(&queue), 1);
        kfree_skb(skb);
        skb_queue_purge(&mut queue);
        assert_eq!(skb_queue_empty(&queue), 1);
        kfree_skb(clone);
    }

    #[test]
    fn net_device_carrier_tracking_works() {
        let name = CString::new("wlan%d").expect("valid test CString");
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
    fn net_device_setup_registration_and_queue_state_work() {
        SETUP_CALLS.store(0, Ordering::Release);
        let name = CString::new("wlan%d").expect("valid test CString");
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

        netif_tx_stop_queue(dev, 2);
        assert_ne!(
            unsafe { (*dev).tx_queue_state.load(Ordering::Acquire) } & (1 << 2),
            0
        );
        netif_tx_wake_queue(dev, 2);
        assert_eq!(
            unsafe { (*dev).tx_queue_state.load(Ordering::Acquire) } & (1 << 2),
            0
        );

        netif_device_detach(dev);
        assert_eq!(unsafe { (*dev).device_attached.load(Ordering::Acquire) }, 0);
        netif_device_attach(dev);
        assert_eq!(unsafe { (*dev).device_attached.load(Ordering::Acquire) }, 1);

        unregister_netdev(dev);
        assert_eq!(unsafe { (*dev).registered.load(Ordering::Acquire) }, 0);
        free_netdev(dev);
    }

    #[test]
    fn napi_schedule_and_complete_work() {
        let mut napi = NapiStruct {
            poll: None,
            dev: ptr::null_mut(),
            state: AtomicI32::new(99),
            weight: 0,
        };

        NAPI_POLLS.store(0, Ordering::Release);
        netif_napi_add(ptr::null_mut(), &mut napi, Some(test_napi_poll), 8);
        assert_eq!(napi.weight, 8);
        napi_schedule(&mut napi);
        assert_eq!(NAPI_POLLS.load(Ordering::Acquire), 1);
        assert_eq!(napi.state.load(Ordering::Acquire), NAPI_STATE_SCHEDULED);
        assert_eq!(napi_complete_done(&mut napi, 4), 1);
        assert_eq!(napi.state.load(Ordering::Acquire), NAPI_STATE_IDLE);
    }
}
