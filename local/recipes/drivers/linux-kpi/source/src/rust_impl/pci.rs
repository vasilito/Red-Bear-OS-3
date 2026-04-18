use std::collections::HashMap;
use std::os::raw::c_ulong;
use std::ptr;
use std::sync::Mutex;

use redox_driver_sys::pci::{enumerate_pci_all, PciDevice, PciDeviceInfo, PciLocation};

const EINVAL: i32 = 22;
const ENODEV: i32 = 19;
const EIO: i32 = 5;
const EBUSY: i32 = 16;
const PCI_ANY_ID: u32 = !0;

pub const PCI_IRQ_MSI: u32 = 1;
pub const PCI_IRQ_MSIX: u32 = 2;
pub const PCI_IRQ_LEGACY: u32 = 4;
pub const PCI_IRQ_NOLEGACY: u32 = 8;

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

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct MsixEntry {
    pub vector: u32,
    pub entry: u16,
    pub _pad: u16,
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

#[derive(Clone)]
struct AllocatedVectors {
    _flags: u32,
    vectors: Vec<i32>,
}

fn describe_irq_flags(flags: u32) -> String {
    let mut parts = Vec::new();
    if flags & PCI_IRQ_MSI != 0 {
        parts.push("msi");
    }
    if flags & PCI_IRQ_MSIX != 0 {
        parts.push("msix");
    }
    if flags & PCI_IRQ_LEGACY != 0 {
        parts.push("legacy");
    }
    if flags & PCI_IRQ_NOLEGACY != 0 {
        parts.push("nolegacy");
    }
    if parts.is_empty() {
        "none".to_string()
    } else {
        parts.join("|")
    }
}

lazy_static::lazy_static! {
    static ref CURRENT_DEVICE: Mutex<Option<CurrentDevice>> = Mutex::new(None);
    static ref REGISTERED_PROBE: Mutex<Option<PciDriverProbe>> = Mutex::new(None);
    static ref IRQ_VECTORS: Mutex<HashMap<usize, AllocatedVectors>> = Mutex::new(HashMap::new());
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

fn quirk_location_from_dev(dev: &PciDev) -> PciLocation {
    PciLocation {
        segment: 0,
        bus: dev.bus,
        device: dev.dev,
        function: dev.func,
    }
}

fn quirk_info_from_dev(dev: &PciDev) -> PciDeviceInfo {
    let location = quirk_location_from_dev(dev);
    let mut info = PciDeviceInfo {
        location,
        vendor_id: dev.vendor,
        device_id: dev.device,
        subsystem_vendor_id: redox_driver_sys::quirks::PCI_QUIRK_ANY_ID,
        subsystem_device_id: redox_driver_sys::quirks::PCI_QUIRK_ANY_ID,
        revision: dev.revision,
        class_code: 0,
        subclass: 0,
        prog_if: 0,
        header_type: 0,
        irq: if dev.irq != 0 && dev.irq != u32::from(u8::MAX) {
            Some(dev.irq)
        } else {
            None
        },
        bars: Vec::new(),
        capabilities: Vec::new(),
    };

    if let Ok(mut pci) = PciDevice::open_location(&location) {
        if let Ok(full_info) = pci.full_info() {
            info = full_info;
        }
    }

    info
}

fn clear_irq_vectors_for_ptr(dev_ptr: usize) {
    if let Ok(mut vectors) = IRQ_VECTORS.lock() {
        vectors.remove(&dev_ptr);
    }
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
            clear_irq_vectors_for_ptr(previous.ptr);
            unsafe { drop(Box::from_raw(previous.ptr as *mut PciDev)) };
        }
    }
}

fn clear_current_device() {
    if let Ok(mut state) = CURRENT_DEVICE.lock() {
        if let Some(previous) = state.take() {
            clear_irq_vectors_for_ptr(previous.ptr);
            unsafe { drop(Box::from_raw(previous.ptr as *mut PciDev)) };
        }
    }
}

fn allocate_vectors(dev: *mut PciDev, min_vecs: i32, max_vecs: i32, flags: u32) -> i32 {
    if dev.is_null() || min_vecs <= 0 || max_vecs <= 0 || min_vecs > max_vecs {
        return -EINVAL;
    }
    if flags & (PCI_IRQ_MSI | PCI_IRQ_MSIX | PCI_IRQ_LEGACY) == 0 {
        return -EINVAL;
    }

    let base_irq = unsafe { (*dev).irq as i32 };
    if base_irq <= 0 {
        return -ENODEV;
    }

    let dev_key = dev as usize;
    let Ok(mut vectors) = IRQ_VECTORS.lock() else {
        return -EINVAL;
    };
    if vectors.contains_key(&dev_key) {
        return -EBUSY;
    }

    let count = if flags & PCI_IRQ_MSIX != 0 {
        max_vecs
    } else {
        1
    };
    if count < min_vecs {
        return -EINVAL;
    }

    let allocated = (0..count).map(|index| base_irq + index).collect::<Vec<_>>();
    log::info!(
        "pci_alloc_irq_vectors: base_irq={} count={} flags={} vectors={:?}",
        base_irq,
        count,
        describe_irq_flags(flags),
        allocated
    );
    vectors.insert(
        dev_key,
        AllocatedVectors {
            _flags: flags,
            vectors: allocated,
        },
    );
    count
}

#[no_mangle]
pub extern "C" fn pci_enable_device(dev: *mut PciDev) -> i32 {
    if dev.is_null() {
        return -EINVAL;
    }

    #[cfg(target_os = "redox")]
    {
        let mut pci = match open_current_device(dev) {
            Ok(pci) => pci,
            Err(error) => return error,
        };

        if let Err(error) = pci.enable_device() {
            log::warn!(
                "pci_enable_device: failed to enable {:04x}:{:04x}: {}",
                unsafe { (*dev).vendor },
                unsafe { (*dev).device },
                error
            );
            return -EIO;
        }
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
    log::debug!("pci_iomap: bar={} len={} — mapping via ioremap", bar, len);
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

    let mut cmd: u32 = 0;
    let rc = pci_read_config_dword(dev, 0x04, &mut cmd);
    if rc != 0 {
        log::warn!("pci_set_master: failed to read command register");
        return;
    }

    if cmd & 0x04 != 0 {
        return;
    }

    cmd = (cmd & 0x0000_FFFF) | 0x04;
    let rc = pci_write_config_dword(dev, 0x04, cmd);
    if rc != 0 {
        log::warn!("pci_set_master: failed to write command register");
        return;
    }
    log::info!(
        "pci_set_master: enabled bus mastering (cmd={:#06x})",
        cmd & 0xFFFF
    );
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

#[no_mangle]
pub extern "C" fn pci_get_quirk_flags(dev: *mut PciDev) -> u64 {
    if dev.is_null() {
        return 0;
    }

    let info = quirk_info_from_dev(unsafe { &*dev });
    redox_driver_sys::quirks::lookup_pci_quirks(&info).bits()
}

#[no_mangle]
pub extern "C" fn pci_has_quirk(dev: *mut PciDev, flag: u64) -> bool {
    (pci_get_quirk_flags(dev) & flag) != 0
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
pub extern "C" fn pci_alloc_irq_vectors(
    dev: *mut PciDev,
    min_vecs: i32,
    max_vecs: i32,
    flags: u32,
) -> i32 {
    allocate_vectors(dev, min_vecs, max_vecs, flags)
}

#[no_mangle]
pub extern "C" fn pci_free_irq_vectors(dev: *mut PciDev) {
    if dev.is_null() {
        return;
    }
    if let Ok(vectors) = IRQ_VECTORS.lock() {
        if let Some(allocated) = vectors.get(&(dev as usize)) {
            log::info!(
                "pci_free_irq_vectors: releasing {} vectors {:?}",
                allocated.vectors.len(),
                allocated.vectors
            );
        }
    }
    clear_irq_vectors_for_ptr(dev as usize);
}

#[no_mangle]
pub extern "C" fn pci_irq_vector(dev: *mut PciDev, vector_idx: i32) -> i32 {
    if dev.is_null() || vector_idx < 0 {
        return -EINVAL;
    }

    let Ok(vectors) = IRQ_VECTORS.lock() else {
        return -EINVAL;
    };
    let Some(allocated) = vectors.get(&(dev as usize)) else {
        return -EINVAL;
    };
    allocated
        .vectors
        .get(vector_idx as usize)
        .copied()
        .unwrap_or(-EINVAL)
}

#[no_mangle]
pub extern "C" fn pci_enable_msi(dev: *mut PciDev) -> i32 {
    pci_alloc_irq_vectors(dev, 1, 1, PCI_IRQ_MSI)
}

#[no_mangle]
pub extern "C" fn pci_disable_msi(dev: *mut PciDev) {
    pci_free_irq_vectors(dev);
}

#[no_mangle]
pub extern "C" fn pci_enable_msix_range(
    dev: *mut PciDev,
    entries: *mut MsixEntry,
    minvec: i32,
    maxvec: i32,
) -> i32 {
    if entries.is_null() {
        return -EINVAL;
    }

    let count = pci_alloc_irq_vectors(dev, minvec, maxvec, PCI_IRQ_MSIX);
    if count < 0 {
        return count;
    }

    for index in 0..count {
        unsafe {
            (*entries.add(index as usize)).vector = pci_irq_vector(dev, index) as u32;
        }
    }
    count
}

#[no_mangle]
pub extern "C" fn pci_disable_msix(dev: *mut PciDev) {
    pci_free_irq_vectors(dev);
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

    let devices = match enumerate_pci_all() {
        Ok(devices) => devices,
        Err(error) => {
            log::warn!("pci_register_driver: PCI enumeration failed: {}", error);
            return -ENODEV;
        }
    };

    let Some((info, id_ptr)) = devices.into_iter().find_map(|candidate| {
        matching_id_entry(&candidate, driver.id_table).map(|id_ptr| (candidate, id_ptr))
    }) else {
        log::info!("pci_register_driver: no matching PCI device found");
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dev(irq: u32) -> PciDev {
        PciDev {
            irq,
            ..PciDev::default()
        }
    }

    #[test]
    fn pci_irq_vector_lifecycle_works() {
        let mut dev = test_dev(32);
        assert_eq!(pci_alloc_irq_vectors(&mut dev, 1, 1, PCI_IRQ_MSI), 1);
        assert_eq!(pci_irq_vector(&mut dev, 0), 32);
        assert_eq!(pci_alloc_irq_vectors(&mut dev, 1, 1, PCI_IRQ_MSI), -16);
        pci_free_irq_vectors(&mut dev);
        assert_eq!(pci_irq_vector(&mut dev, 0), -22);
    }

    #[test]
    fn pci_msix_range_populates_entries() {
        let mut dev = test_dev(40);
        let mut entries = [MsixEntry::default(); 3];
        assert_eq!(
            pci_enable_msix_range(&mut dev, entries.as_mut_ptr(), 2, 3),
            3
        );
        assert_eq!(entries[0].vector, 40);
        assert_eq!(entries[1].vector, 41);
        assert_eq!(entries[2].vector, 42);
        pci_disable_msix(&mut dev);
    }

    #[test]
    fn pci_rejects_invalid_irq_vector_requests() {
        let mut dev = test_dev(0);
        assert_eq!(pci_enable_msi(&mut dev), -19);
        assert_eq!(
            pci_alloc_irq_vectors(ptr::null_mut(), 1, 1, PCI_IRQ_MSI),
            -22
        );
    }

    #[test]
    fn describe_irq_flags_formats_requested_modes() {
        assert_eq!(describe_irq_flags(0), "none");
        assert_eq!(describe_irq_flags(PCI_IRQ_MSI), "msi");
        assert_eq!(describe_irq_flags(PCI_IRQ_MSIX | PCI_IRQ_NOLEGACY), "msix|nolegacy");
        assert_eq!(
            describe_irq_flags(PCI_IRQ_MSI | PCI_IRQ_MSIX | PCI_IRQ_LEGACY),
            "msi|msix|legacy"
        );
    }
}
