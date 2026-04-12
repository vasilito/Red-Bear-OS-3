#[derive(Clone, Debug)]
pub enum Subsystem {
    Gpu,
    Network,
    Storage,
    Audio,
    Usb,
    Input,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: u8,
    pub subclass: u8,
    pub subsystem: Subsystem,
    pub name: String,
    pub path: String,
}

pub fn classify_pci_device(bus: u8, dev: u8, func: u8) -> DeviceInfo {
    let path = format!("/devices/pci/{:04x}:{:02x}:{:02x}.{}", bus, 0, dev, func);

    let config_path = format!("/scheme/pci/{}.{}.{}", bus, dev, func);
    let (vendor_id, device_id, class_code, subclass) = read_pci_config(&config_path);

    let subsystem = match class_code {
        0x03 => Subsystem::Gpu,
        0x02 => Subsystem::Network,
        0x01 => Subsystem::Storage,
        0x04 => Subsystem::Audio,
        0x0C => Subsystem::Usb,
        0x09 => Subsystem::Input,
        _ => Subsystem::Unknown,
    };

    let name = format_device_name(vendor_id, device_id, class_code);

    DeviceInfo {
        bus,
        dev,
        func,
        vendor_id,
        device_id,
        class_code,
        subclass,
        subsystem,
        name,
        path,
    }
}

fn read_pci_config(path: &str) -> (u16, u16, u8, u8) {
    match std::fs::read(path) {
        Ok(data) if data.len() >= 16 => {
            let vendor_id = u16::from_le_bytes([data[0], data[1]]);
            let device_id = u16::from_le_bytes([data[2], data[3]]);
            let class_code = data[11];
            let subclass = data[10];
            (vendor_id, device_id, class_code, subclass)
        }
        _ => (0xFFFF, 0xFFFF, 0xFF, 0xFF),
    }
}

fn format_device_name(vendor_id: u16, device_id: u16, class_code: u8) -> String {
    let vendor_name = match vendor_id {
        0x8086 => "Intel",
        0x1002 => "AMD",
        0x10DE => "NVIDIA",
        0x10EC => "Realtek",
        0x8087 => "Intel",
        0x14E4 => "Broadcom",
        _ => "Unknown",
    };

    let class_name = match class_code {
        0x03 => "GPU",
        0x02 => "Network Controller",
        0x01 => "Storage Controller",
        0x04 => "Multimedia Device",
        0x0C => "USB Controller",
        0x09 => "Input Device",
        _ => "PCI Device",
    };

    format!(
        "{} {} [{:04x}:{:04x}]",
        vendor_name, class_name, vendor_id, device_id
    )
}

pub fn format_device_info(dev: &DeviceInfo) -> String {
    let subsystem = match dev.subsystem {
        Subsystem::Gpu => "gpu",
        Subsystem::Network => "net",
        Subsystem::Storage => "block",
        Subsystem::Audio => "sound",
        Subsystem::Usb => "usb",
        Subsystem::Input => "input",
        Subsystem::Unknown => "unknown",
    };

    format!(
        "P={}\nE=SUBSYSTEM={}\nE=PCI_VENDOR_ID={:#06x}\nE=PCI_DEVICE_ID={:#06x}\nE=PCI_CLASS={:#04x}{:02x}\nE=DEVNAME={}\n",
        dev.path, subsystem, dev.vendor_id, dev.device_id, dev.class_code, dev.subclass, dev.name
    )
}
