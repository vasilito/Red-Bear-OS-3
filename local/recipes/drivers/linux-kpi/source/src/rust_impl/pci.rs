use std::os::raw::c_ulong;
use std::ptr;
use std::sync::Mutex;

use redox_driver_sys::pci::{
    enumerate_pci_class, PciDevice, PciDeviceInfo, PciLocation, PCI_CLASS_DISPLAY,
};

const EINVAL: i32 = 22;
const ENODEV: i32 = 19;
const EIO: i32 = 5;
const PCI_ANY_ID: u32 = !0;

#[repr(C)]
#[derive(Default)]
pub struct Device {
    driver: *mut u8,
    driver_data: *mut u8,
    platform_data: *mut u8,
    of_node: *mut u8,
    dma_mask: u64,
}

#[repr(C)]
pub struct PciDev {
    pub vendor: u16,
    pub device: u16,
    bus: u8,
    dev: u8,
    func: u8,
    revision: u8,
    irq: u32,
    bars: [u64; 6],
    bar_sizes: [u64; 6],
    driver_data: *mut u8,
    device_obj: Device,
    pub enabled: bool,
}

#[repr(C)]
pub struct PciDeviceId {
    vendor: u32,
    device: u32,
    subvendor: u32,
    subdevice: u32,
    class: u32,
    class_mask: u32,
    driver_data: c_ulong,
}

impl Default for PciDev {
    fn default() -> Self {
        PciDev {
            vendor: 0,
            device: 0,
            bus: 0,
            dev: 0,
            func: 0,
            revision: 0,
            irq: 0,
            bars: [0; 6],
            bar_sizes: [0; 6],
            driver_data: ptr::null_mut(),
            device_obj: Device::default(),
            enabled: false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct CurrentDevice {
    location: PciLocation,
    ptr: usize,
}

lazy_static::lazy_static! {
    static ref CURRENT_DEVICE: Mutex<Option<CurrentDevice>> = Mutex::new(None);
    static ref REGISTERED_PROBE: Mutex<Option<PciDriverProbe>> = Mutex::new(None);
}

pub const PCI_VENDOR_ID_AMD: u16 = 0x1002;
pub const PCI_VENDOR_ID_INTEL: u16 = 0x8086;

fn current_location_from_state(dev: *mut PciDev) -> Result<PciLocation, i32> {
    if let Ok(state) = CURRENT_DEVICE.lock() {
        if let Some(current) = *state {
            return Ok(current.location);
        }
    }

    if dev.is_null() {
        return Err(-EINVAL);
    }

    Ok(PciLocation {
        segment: 0,
        bus: unsafe { (*dev).bus },
        device: unsafe { (*dev).dev },
        function: unsafe { (*dev).func },
    })
}

fn open_current_device(dev: *mut PciDev) -> Result<PciDevice, i32> {
    let location = current_location_from_state(dev)?;
    PciDevice::open_location(&location).map_err(|error| {
        log::warn!("pci: failed to open PCI device {}: {}", location, error);
        -ENODEV
    })
}

fn matches_id(info: &PciDeviceInfo, id: &PciDeviceId) -> bool {
    let class =
        ((info.class_code as u32) << 16) | ((info.subclass as u32) << 8) | info.prog_if as u32;

    let vendor_matches = id.vendor == PCI_ANY_ID || id.vendor == info.vendor_id as u32;
    let device_matches = id.device == PCI_ANY_ID || id.device == info.device_id as u32;
    let subvendor_matches = id.subvendor == PCI_ANY_ID;
    let subdevice_matches = id.subdevice == PCI_ANY_ID;
    let class_matches = id.class_mask == 0 || (class & id.class_mask) == (id.class & id.class_mask);

    vendor_matches && device_matches && subvendor_matches && subdevice_matches && class_matches
}

fn matching_id_entry(
    info: &PciDeviceInfo,
    mut id: *const PciDeviceId,
) -> Option<*const PciDeviceId> {
    if id.is_null() {
        return None;
    }

    loop {
        let current = unsafe { &*id };
        if current.vendor == 0
            && current.device == 0
            && current.subvendor == 0
            && current.subdevice == 0
            && current.class == 0
            && current.class_mask == 0
            && current.driver_data == 0
        {
            return None;
        }

        if matches_id(info, current) {
            return Some(id);
        }

        id = unsafe { id.add(1) };
    }
}

fn build_pci_dev(info: &PciDeviceInfo, id: &PciDeviceId) -> PciDev {
    let mut dev = PciDev {
        vendor: info.vendor_id,
        device: info.device_id,
        bus: info.location.bus,
        dev: info.location.device,
        func: info.location.function,
        revision: info.revision,
        irq: info.irq.unwrap_or(0),
        bars: [0; 6],
        bar_sizes: [0; 6],
        driver_data: id.driver_data as usize as *mut u8,
        device_obj: Device::default(),
        enabled: false,
    };

    for bar in &info.bars {
        if bar.index < dev.bars.len() {
            dev.bars[bar.index] = bar.addr;
            dev.bar_sizes[bar.index] = bar.size;
        }
    }

    dev
}

fn replace_current_device(location: PciLocation, dev_ptr: *mut PciDev) {
    if let Ok(mut state) = CURRENT_DEVICE.lock() {
        if let Some(previous) = state.replace(CurrentDevice {
            location,
            ptr: dev_ptr as usize,
        }) {
            unsafe { drop(Box::from_raw(previous.ptr as *mut PciDev)) };
        }
    }
}

fn clear_current_device() {
    if let Ok(mut state) = CURRENT_DEVICE.lock() {
        if let Some(previous) = state.take() {
            unsafe { drop(Box::from_raw(previous.ptr as *mut PciDev)) };
        }
    }
}

#[no_mangle]
pub extern "C" fn pci_enable_device(dev: *mut PciDev) -> i32 {
    if dev.is_null() {
        return -EINVAL;
    }
    log::info!(
        "pci_enable_device: vendor=0x{:04x} device=0x{:04x}",
        unsafe { (*dev).vendor },
        unsafe { (*dev).device }
    );
    unsafe { (*dev).enabled = true };
    0
}

#[no_mangle]
pub extern "C" fn pci_disable_device(dev: *mut PciDev) {
    if dev.is_null() {
        return;
    }
    log::info!("pci_disable_device");
    unsafe { (*dev).enabled = false };
}

#[no_mangle]
pub extern "C" fn pci_iomap(dev: *mut PciDev, bar: u32, max_len: usize) -> *mut u8 {
    if dev.is_null() || bar >= 6 {
        return ptr::null_mut();
    }
    let len = if max_len > 0 {
        max_len
    } else {
        unsafe { (*dev).bar_sizes[bar as usize] as usize }
    };
    if len == 0 {
        return ptr::null_mut();
    }
    log::warn!("pci_iomap: bar={} len={} — using heap fallback", bar, len);
    super::io::ioremap(unsafe { (*dev).bars[bar as usize] }, len)
}

#[no_mangle]
pub extern "C" fn pci_iounmap(_dev: *mut PciDev, addr: *mut u8, size: usize) {
    super::io::iounmap(addr, size);
}

#[no_mangle]
pub extern "C" fn pci_read_config_dword(dev: *mut PciDev, offset: u32, val: *mut u32) -> i32 {
    if dev.is_null() || val.is_null() {
        return -EINVAL;
    }

    let mut pci = match open_current_device(dev) {
        Ok(pci) => pci,
        Err(error) => return error,
    };

    match pci.read_config_dword(offset as u64) {
        Ok(read) => {
            unsafe { *val = read };
            log::info!(
                "pci_read_config_dword: offset=0x{:x} -> 0x{:08x}",
                offset,
                read
            );
            0
        }
        Err(error) => {
            log::warn!(
                "pci_read_config_dword: failed at offset=0x{:x}: {}",
                offset,
                error
            );
            -EIO
        }
    }
}

#[no_mangle]
pub extern "C" fn pci_write_config_dword(dev: *mut PciDev, offset: u32, val: u32) -> i32 {
    if dev.is_null() {
        return -EINVAL;
    }

    let mut pci = match open_current_device(dev) {
        Ok(pci) => pci,
        Err(error) => return error,
    };

    match pci.write_config_dword(offset as u64, val) {
        Ok(()) => {
            log::info!(
                "pci_write_config_dword: offset=0x{:x} val=0x{:08x}",
                offset,
                val
            );
            0
        }
        Err(error) => {
            log::warn!(
                "pci_write_config_dword: failed at offset=0x{:x} val=0x{:08x}: {}",
                offset,
                val,
                error
            );
            -EIO
        }
    }
}

#[no_mangle]
pub extern "C" fn pci_set_master(dev: *mut PciDev) {
    if dev.is_null() {
        return;
    }
    log::info!("pci_set_master");
}

#[no_mangle]
pub extern "C" fn pci_resource_start(dev: *const PciDev, bar: u32) -> u64 {
    if dev.is_null() || bar >= 6 {
        return 0;
    }
    unsafe { (*dev).bars[bar as usize] }
}

#[no_mangle]
pub extern "C" fn pci_resource_len(dev: *const PciDev, bar: u32) -> u64 {
    if dev.is_null() || bar >= 6 {
        return 0;
    }
    unsafe { (*dev).bar_sizes[bar as usize] }
}

pub type PciDriverProbe = extern "C" fn(*mut PciDev, *const PciDeviceId) -> i32;
pub type PciDriverRemove = extern "C" fn(*mut PciDev);

#[repr(C)]
pub struct PciDriver {
    name: *const u8,
    id_table: *const PciDeviceId,
    probe: Option<PciDriverProbe>,
    remove: Option<PciDriverRemove>,
}

#[no_mangle]
pub extern "C" fn pci_register_driver(drv: *mut PciDriver) -> i32 {
    if drv.is_null() {
        return -EINVAL;
    }

    let driver = unsafe { &*drv };
    let probe = match driver.probe {
        Some(probe) => probe,
        None => {
            log::warn!("pci_register_driver: missing probe callback");
            return -EINVAL;
        }
    };

    let devices = match enumerate_pci_class(PCI_CLASS_DISPLAY) {
        Ok(devices) => devices,
        Err(error) => {
            log::warn!("pci_register_driver: PCI enumeration failed: {}", error);
            return -ENODEV;
        }
    };

    let Some((info, id_ptr)) = devices.into_iter().find_map(|candidate| {
        matching_id_entry(&candidate, driver.id_table).map(|id_ptr| (candidate, id_ptr))
    }) else {
        log::info!("pci_register_driver: no matching PCI display device found");
        return -ENODEV;
    };

    let mut pci = match PciDevice::from_info(&info) {
        Ok(pci) => pci,
        Err(error) => {
            log::warn!(
                "pci_register_driver: failed to open {}: {}",
                info.location,
                error
            );
            return -ENODEV;
        }
    };

    let full_info = match pci.full_info() {
        Ok(full_info) => full_info,
        Err(error) => {
            log::warn!(
                "pci_register_driver: failed to read PCI info for {}: {}",
                info.location,
                error
            );
            return -EIO;
        }
    };

    let id = unsafe { &*id_ptr };
    let dev_ptr = Box::into_raw(Box::new(build_pci_dev(&full_info, id)));
    replace_current_device(full_info.location, dev_ptr);

    if let Ok(mut registered_probe) = REGISTERED_PROBE.lock() {
        *registered_probe = Some(probe);
    }

    log::info!(
        "pci_register_driver: probing {:04x}:{:04x} at {}",
        full_info.vendor_id,
        full_info.device_id,
        full_info.location
    );

    let status = probe(dev_ptr, id_ptr);
    if status != 0 {
        log::warn!("pci_register_driver: probe failed with status {}", status);
        clear_current_device();
        if let Ok(mut registered_probe) = REGISTERED_PROBE.lock() {
            *registered_probe = None;
        }
    }

    status
}

#[no_mangle]
pub extern "C" fn pci_unregister_driver(drv: *mut PciDriver) {
    if !drv.is_null() {
        let driver = unsafe { &*drv };
        if let Some(remove) = driver.remove {
            let current_ptr = CURRENT_DEVICE
                .lock()
                .ok()
                .and_then(|state| state.as_ref().map(|current| current.ptr as *mut PciDev));
            if let Some(dev_ptr) = current_ptr {
                remove(dev_ptr);
            }
        }
    }

    clear_current_device();
    if let Ok(mut registered_probe) = REGISTERED_PROBE.lock() {
        *registered_probe = None;
    }
    log::info!("pci_unregister_driver: cleared registered PCI driver state");
}
