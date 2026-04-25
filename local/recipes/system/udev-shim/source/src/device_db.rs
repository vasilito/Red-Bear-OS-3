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
    use super::{DeviceInfo, InputKind, Subsystem, classify_pci_device, device_properties, format_device_info, format_uevent_info};

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

    #[test]
    fn new_platform_input_has_correct_defaults() {
        let dev = DeviceInfo::new_platform_input(
            "test-kbd",
            "/devices/platform/keyboard0",
            InputKind::Keyboard,
            "",
            "",
        );
        assert!(!dev.is_pci);
        assert_eq!(dev.subsystem, Subsystem::Input);
        assert_eq!(dev.input_kind, Some(InputKind::Keyboard));
        assert!(dev.devnode.is_empty());
        assert!(dev.scheme_target.is_empty());
        assert!(dev.symlinks.is_empty());
        assert_eq!(dev.bus, 0);
        assert_eq!(dev.dev, 0);
        assert_eq!(dev.func, 0);
        assert_eq!(dev.vendor_id, 0);
        assert_eq!(dev.device_id, 0);
    }

    #[test]
    fn set_node_metadata_sets_fields_correctly() {
        let mut dev = DeviceInfo::new_platform_input(
            "test-mouse",
            "/devices/platform/mouse0",
            InputKind::Mouse,
            "",
            "",
        );
        dev.set_node_metadata(
            "/dev/input/mouse0",
            "input:mouse0",
            vec!["/dev/input/by-path/platform-mouse0".to_string()],
        );
        assert_eq!(dev.devnode, "/dev/input/mouse0");
        assert_eq!(dev.scheme_target, "input:mouse0");
        assert_eq!(dev.symlinks.len(), 1);
        assert_eq!(dev.symlinks[0], "/dev/input/by-path/platform-mouse0");
    }

    #[test]
    fn subsystem_name_maps_all_variants() {
        let cases: Vec<(Subsystem, &'static str)> = vec![
            (Subsystem::Gpu, "drm"),
            (Subsystem::Network, "net"),
            (Subsystem::Storage, "block"),
            (Subsystem::Audio, "sound"),
            (Subsystem::Usb, "usb"),
            (Subsystem::Input, "input"),
            (Subsystem::Unknown, "unknown"),
        ];
        for (subsys, expected) in cases {
            let mut dev =
                DeviceInfo::new_platform_input("x", "/devices/x", InputKind::Generic, "", "");
            dev.subsystem = subsys;
            assert_eq!(dev.subsystem_name(), expected, "failed for {:?}", subsys);
        }
    }

    #[test]
    fn id_path_pci_device() {
        let dev = DeviceInfo {
            is_pci: true,
            bus: 0x02,
            dev: 0x00,
            func: 0x0,
            vendor_id: 0x1002,
            device_id: 0x67df,
            class_code: 0x03,
            subclass: 0x00,
            subsystem: Subsystem::Gpu,
            input_kind: None,
            name: "Test GPU".to_string(),
            devpath: "/devices/pci/0000:02:00.0".to_string(),
            devnode: String::new(),
            scheme_target: String::new(),
            symlinks: vec![],
        };
        assert_eq!(dev.id_path(), "pci-0000:02:00.0");
    }

    #[test]
    fn id_path_platform_device() {
        let dev = DeviceInfo::new_platform_input(
            "keyboard0",
            "/devices/platform/keyboard0",
            InputKind::Keyboard,
            "",
            "",
        );
        assert_eq!(dev.id_path(), "platform-keyboard0");
    }

    #[test]
    fn is_input_keyboard_true_only_for_keyboard() {
        let kb = DeviceInfo::new_platform_input("kb", "/devices/x", InputKind::Keyboard, "", "");
        assert!(kb.is_input_keyboard());
        assert!(!kb.is_input_mouse());

        let mouse = DeviceInfo::new_platform_input("ms", "/devices/x", InputKind::Mouse, "", "");
        assert!(!mouse.is_input_keyboard());

        let generic =
            DeviceInfo::new_platform_input("gen", "/devices/x", InputKind::Generic, "", "");
        assert!(!generic.is_input_keyboard());
    }

    #[test]
    fn is_input_mouse_true_only_for_mouse() {
        let mouse = DeviceInfo::new_platform_input("ms", "/devices/x", InputKind::Mouse, "", "");
        assert!(mouse.is_input_mouse());
        assert!(!mouse.is_input_keyboard());

        let kb = DeviceInfo::new_platform_input("kb", "/devices/x", InputKind::Keyboard, "", "");
        assert!(!kb.is_input_mouse());

        let generic =
            DeviceInfo::new_platform_input("gen", "/devices/x", InputKind::Generic, "", "");
        assert!(!generic.is_input_mouse());
    }

    #[test]
    fn device_properties_gpu_pci_contains_key_fields() {
        let dev = DeviceInfo {
            is_pci: true,
            bus: 0x02,
            dev: 0x00,
            func: 0x0,
            vendor_id: 0x1002,
            device_id: 0x67df,
            class_code: 0x03,
            subclass: 0x00,
            subsystem: Subsystem::Gpu,
            input_kind: None,
            name: "AMD RX 580 [1002:67df]".to_string(),
            devpath: "/devices/pci/0000:02:00.0".to_string(),
            devnode: "/dev/dri/card0".to_string(),
            scheme_target: "display:display".to_string(),
            symlinks: vec![],
        };
        let props = device_properties(&dev);
        let prop_map: std::collections::HashMap<&str, &str> = props
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        assert_eq!(prop_map.get("SUBSYSTEM").copied(), Some("drm"));
        assert_eq!(prop_map.get("PCI_VENDOR_ID").copied(), Some("0x1002"));
        assert_eq!(prop_map.get("PCI_DEVICE_ID").copied(), Some("0x67df"));
        assert_eq!(prop_map.get("PCI_CLASS").copied(), Some("0x0300"));
        assert_eq!(prop_map.get("DEVNAME").copied(), Some("/dev/dri/card0"));
    }

    #[test]
    fn device_properties_input_keyboard_has_input_flags() {
        let dev = DeviceInfo::new_platform_input(
            "keyboard0",
            "/devices/platform/keyboard0",
            InputKind::Keyboard,
            "/dev/input/event0",
            "input:keyboard0",
        );
        let props = device_properties(&dev);
        let prop_map: std::collections::HashMap<&str, &str> = props
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        assert_eq!(prop_map.get("ID_INPUT").copied(), Some("1"));
        assert_eq!(prop_map.get("ID_INPUT_KEYBOARD").copied(), Some("1"));
        assert!(!prop_map.contains_key("ID_INPUT_MOUSE"));
    }

    #[test]
    fn device_properties_input_mouse_has_input_flags() {
        let dev = DeviceInfo::new_platform_input(
            "mouse0",
            "/devices/platform/mouse0",
            InputKind::Mouse,
            "/dev/input/mouse0",
            "input:mouse0",
        );
        let props = device_properties(&dev);
        let prop_map: std::collections::HashMap<&str, &str> = props
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        assert_eq!(prop_map.get("ID_INPUT").copied(), Some("1"));
        assert_eq!(prop_map.get("ID_INPUT_MOUSE").copied(), Some("1"));
        assert!(!prop_map.contains_key("ID_INPUT_KEYBOARD"));
    }

    #[test]
    fn format_device_info_structure() {
        let dev = DeviceInfo {
            is_pci: true,
            bus: 0x02,
            dev: 0x00,
            func: 0x0,
            vendor_id: 0x8086,
            device_id: 0x1234,
            class_code: 0x03,
            subclass: 0x00,
            subsystem: Subsystem::Gpu,
            input_kind: None,
            name: "Intel GPU".to_string(),
            devpath: "/devices/pci/0000:02:00.0".to_string(),
            devnode: "/dev/dri/card0".to_string(),
            scheme_target: "display:display".to_string(),
            symlinks: vec!["/dev/dri/by-path/pci-0000:02:00.0-card".to_string()],
        };
        let info = format_device_info(&dev);
        assert!(info.starts_with("P=/devices/pci/0000:02:00.0\n"));
        assert!(info.contains("E=SUBSYSTEM=drm\n"));
        assert!(info.contains("S=dev/dri/by-path/pci-0000:02:00.0-card\n"));
    }

    #[test]
    fn format_uevent_info_starts_with_action_and_has_props() {
        let dev = DeviceInfo::new_platform_input(
            "keyboard0",
            "/devices/platform/keyboard0",
            InputKind::Keyboard,
            "/dev/input/event0",
            "input:keyboard0",
        );
        let uevent = format_uevent_info(&dev);
        assert!(uevent.starts_with("ACTION=add\n"));
        assert!(uevent.contains("SUBSYSTEM=input\n"));
        assert!(uevent.contains("DEVPATH=/devices/platform/keyboard0\n"));
    }

    #[test]
    fn classify_pci_device_with_no_pci_config_still_produces_pci_device() {
        let dev = classify_pci_device(0x00, 0x1f, 0x2);
        assert!(dev.is_pci);
        assert_eq!(dev.bus, 0x00);
        assert_eq!(dev.dev, 0x1f);
        assert_eq!(dev.func, 0x2);
        // Without real PCI config, read_pci_config returns 0xFFFF
        assert_eq!(dev.vendor_id, 0xFFFF);
        assert_eq!(dev.device_id, 0xFFFF);
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
