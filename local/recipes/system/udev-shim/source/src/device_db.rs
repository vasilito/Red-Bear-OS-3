#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Subsystem {
    Gpu,
    Network,
    Storage,
    Audio,
    Usb,
    Input,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputKind {
    Keyboard,
    Mouse,
    Generic,
}

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub is_pci: bool,
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: u8,
    pub subclass: u8,
    pub subsystem: Subsystem,
    pub input_kind: Option<InputKind>,
    pub name: String,
    pub devpath: String,
    pub devnode: String,
    pub scheme_target: String,
    pub symlinks: Vec<String>,
}

impl DeviceInfo {
    pub fn new_platform_input(
        name: &str,
        devpath: &str,
        input_kind: InputKind,
        devnode: &str,
        scheme_target: &str,
    ) -> Self {
        Self {
            is_pci: false,
            bus: 0,
            dev: 0,
            func: 0,
            vendor_id: 0,
            device_id: 0,
            class_code: 0,
            subclass: 0,
            subsystem: Subsystem::Input,
            input_kind: Some(input_kind),
            name: name.to_string(),
            devpath: devpath.to_string(),
            devnode: devnode.to_string(),
            scheme_target: scheme_target.to_string(),
            symlinks: Vec::new(),
        }
    }

    pub fn set_node_metadata(
        &mut self,
        devnode: impl Into<String>,
        scheme_target: impl Into<String>,
        symlinks: Vec<String>,
    ) {
        self.devnode = devnode.into();
        self.scheme_target = scheme_target.into();
        self.symlinks = symlinks;
    }

    pub fn subsystem_name(&self) -> &'static str {
        match self.subsystem {
            Subsystem::Gpu => "drm",
            Subsystem::Network => "net",
            Subsystem::Storage => "block",
            Subsystem::Audio => "sound",
            Subsystem::Usb => "usb",
            Subsystem::Input => "input",
            Subsystem::Unknown => "unknown",
        }
    }

    pub fn id_path(&self) -> String {
        if let Some(slot) = self.devpath.strip_prefix("/devices/pci/") {
            return format!("pci-{slot}");
        }

        self.devpath
            .trim_start_matches("/devices/")
            .replace('/', "-")
    }

    pub fn is_input_keyboard(&self) -> bool {
        self.input_kind == Some(InputKind::Keyboard)
    }

    pub fn is_input_mouse(&self) -> bool {
        self.input_kind == Some(InputKind::Mouse)
    }
}

pub fn classify_pci_device(bus: u8, dev: u8, func: u8) -> DeviceInfo {
    let location = PciLocation {
        segment: 0,
        bus,
        device: dev,
        function: func,
    };
    let devpath = format!("/devices/pci/{}", location);
    let config_path = format!("{}/config", location.scheme_path());
    let (vendor_id, device_id, class_code, subclass) = read_pci_config(&config_path);
    let input_kind = detect_input_kind(class_code, subclass);

    let subsystem = match class_code {
        0x03 => Subsystem::Gpu,
        0x02 => Subsystem::Network,
        0x01 => Subsystem::Storage,
        0x04 => Subsystem::Audio,
        0x0C => Subsystem::Usb,
        0x09 => Subsystem::Input,
        _ => Subsystem::Unknown,
    };

    let name = format_device_name(vendor_id, device_id, class_code, subclass, input_kind);

    DeviceInfo {
        is_pci: true,
        bus,
        dev,
        func,
        vendor_id,
        device_id,
        class_code,
        subclass,
        subsystem,
        input_kind,
        name,
        devpath,
        devnode: String::new(),
        scheme_target: String::new(),
        symlinks: Vec::new(),
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

fn detect_input_kind(class_code: u8, subclass: u8) -> Option<InputKind> {
    if class_code != 0x09 {
        return None;
    }

    match subclass {
        0x00 => Some(InputKind::Keyboard),
        0x04 => Some(InputKind::Generic),
        _ => Some(InputKind::Generic),
    }
}

fn format_device_name(
    vendor_id: u16,
    device_id: u16,
    class_code: u8,
    subclass: u8,
    input_kind: Option<InputKind>,
) -> String {
    if let Some(name) = lookup_pci_device_name(vendor_id, device_id) {
        return format!("{name} [{vendor_id:04x}:{device_id:04x}]");
    }

    if class_code == 0x09 {
        let name = match (subclass, input_kind) {
            (0x00, Some(InputKind::Keyboard)) => "PS/2 Keyboard Controller",
            (0x04, _) => "USB HID Controller",
            _ => "Input Device",
        };
        return format!("{name} [{vendor_id:04x}:{device_id:04x}]");
    }

    let vendor_name = lookup_pci_vendor_name(vendor_id).unwrap_or_else(|| "Unknown".to_string());

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

#[cfg(test)]
mod tests {
    use super::classify_pci_device;

    #[test]
    fn classify_pci_device_uses_shared_location_format() {
        let device = classify_pci_device(0x02, 0x00, 0x0);

        assert_eq!(device.devpath, "/devices/pci/0000:02:00.0");
    }

    #[test]
    fn id_path_tracks_shared_pci_devpath_shape() {
        let device = classify_pci_device(0x02, 0x00, 0x0);

        assert_eq!(device.id_path(), "pci-0000:02:00.0");
    }
}

pub fn device_properties(dev: &DeviceInfo) -> Vec<(String, String)> {
    let mut props = Vec::new();
    props.push(("DEVPATH".to_string(), dev.devpath.clone()));
    props.push(("SUBSYSTEM".to_string(), dev.subsystem_name().to_string()));
    props.push(("ID_MODEL_FROM_DATABASE".to_string(), dev.name.clone()));

    if !dev.devnode.is_empty() {
        props.push(("DEVNAME".to_string(), dev.devnode.clone()));
    }

    let id_path = dev.id_path();
    if !id_path.is_empty() {
        props.push(("ID_PATH".to_string(), id_path));
    }

    if dev.is_pci {
        props.push((
            "PCI_VENDOR_ID".to_string(),
            format!("0x{:04x}", dev.vendor_id),
        ));
        props.push((
            "PCI_DEVICE_ID".to_string(),
            format!("0x{:04x}", dev.device_id),
        ));
        props.push((
            "PCI_CLASS".to_string(),
            format!("0x{:02x}{:02x}", dev.class_code, dev.subclass),
        ));
    }

    if dev.subsystem == Subsystem::Input {
        props.push(("ID_INPUT".to_string(), "1".to_string()));
        match dev.input_kind {
            Some(InputKind::Keyboard) => {
                props.push(("ID_INPUT_KEYBOARD".to_string(), "1".to_string()));
            }
            Some(InputKind::Mouse) => {
                props.push(("ID_INPUT_MOUSE".to_string(), "1".to_string()));
            }
            _ => {}
        }
    }

    props
}

pub fn format_device_info(dev: &DeviceInfo) -> String {
    let mut info = format!("P={}\n", dev.devpath);
    for (key, value) in device_properties(dev) {
        info.push_str(&format!("E={key}={value}\n"));
    }
    for link in &dev.symlinks {
        info.push_str(&format!("S={}\n", link.trim_start_matches('/')));
    }
    info
}

pub fn format_uevent_info(dev: &DeviceInfo) -> String {
    let mut info = String::from("ACTION=add\n");
    for (key, value) in device_properties(dev) {
        info.push_str(&format!("{key}={value}\n"));
    }
    info
}
use redbear_hwutils::{lookup_pci_device_name, lookup_pci_vendor_name, PciLocation};
