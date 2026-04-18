use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::sync::OnceLock;

const PCI_IDS_PATH: &str = "/usr/share/misc/pci.ids";

#[derive(Default)]
struct PciIdDatabase {
    vendor_names: HashMap<u16, String>,
    device_names: HashMap<(u16, u16), String>,
}

static PCI_ID_DATABASE: OnceLock<Option<PciIdDatabase>> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PciLocation {
    pub segment: u16,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

impl PciLocation {
    pub fn scheme_path(&self) -> String {
        format!(
            "/scheme/pci/{:04x}--{:02x}--{:02x}.{}",
            self.segment, self.bus, self.device, self.function
        )
    }
}

impl fmt::Display for PciLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04x}:{:02x}:{:02x}.{}",
            self.segment, self.bus, self.device, self.function
        )
    }
}

pub fn parse_pci_location(name: &str) -> Option<PciLocation> {
    let (segment, rest) = name.split_once("--")?;
    let (bus, rest) = rest.split_once("--")?;
    let (device, function) = rest.split_once('.')?;

    Some(PciLocation {
        segment: u16::from_str_radix(segment, 16).ok()?,
        bus: u8::from_str_radix(bus, 16).ok()?,
        device: u8::from_str_radix(device, 16).ok()?,
        function: function.parse().ok()?,
    })
}

pub fn parse_args(
    program: &str,
    usage: &str,
    args: impl IntoIterator<Item = String>,
) -> Result<(), String> {
    let extras: Vec<String> = args.into_iter().skip(1).collect();

    if extras.is_empty() {
        return Ok(());
    }

    if extras.len() == 1 && matches!(extras[0].as_str(), "-h" | "--help") {
        println!("{usage}");
        return Err(String::new());
    }

    Err(format!(
        "{program}: unsupported arguments: {}",
        extras.join(" ")
    ))
}

pub fn describe_usb_device(manufacturer: Option<&str>, product: Option<&str>) -> String {
    let mut parts = Vec::new();

    if let Some(manufacturer) = manufacturer.filter(|value| !value.is_empty()) {
        parts.push(manufacturer);
    }
    if let Some(product) = product.filter(|value| !value.is_empty()) {
        parts.push(product);
    }

    if parts.is_empty() {
        "USB device".to_string()
    } else {
        parts.join(" ")
    }
}

fn load_pci_id_database() -> Option<PciIdDatabase> {
    let text = fs::read_to_string(PCI_IDS_PATH).ok()?;
    Some(parse_pci_id_database(&text))
}

fn parse_pci_id_database(text: &str) -> PciIdDatabase {
    let mut database = PciIdDatabase::default();
    let mut current_vendor = None;

    for line in text.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(rest) = line.strip_prefix("\t\t") {
            let _ = rest;
            continue;
        }

        if let Some(rest) = line.strip_prefix('\t') {
            let Some(vendor_id) = current_vendor else {
                continue;
            };
            let mut parts = rest.splitn(2, char::is_whitespace).filter(|part| !part.is_empty());
            let Some(device_hex) = parts.next() else {
                continue;
            };
            let Some(name) = parts.next() else {
                continue;
            };
            let Ok(device_id) = u16::from_str_radix(device_hex, 16) else {
                continue;
            };
            database
                .device_names
                .insert((vendor_id, device_id), name.trim().to_string());
            continue;
        }

        let mut parts = line.splitn(2, char::is_whitespace).filter(|part| !part.is_empty());
        let Some(vendor_hex) = parts.next() else {
            continue;
        };
        let Some(name) = parts.next() else {
            continue;
        };
        let Ok(vendor_id) = u16::from_str_radix(vendor_hex, 16) else {
            continue;
        };
        current_vendor = Some(vendor_id);
        database.vendor_names.insert(vendor_id, name.trim().to_string());
    }

    database
}

fn pci_id_database() -> Option<&'static PciIdDatabase> {
    PCI_ID_DATABASE.get_or_init(load_pci_id_database).as_ref()
}

pub fn lookup_pci_vendor_name(vendor_id: u16) -> Option<String> {
    pci_id_database()?.vendor_names.get(&vendor_id).cloned()
}

pub fn lookup_pci_device_name(vendor_id: u16, device_id: u16) -> Option<String> {
    pci_id_database()?
        .device_names
        .get(&(vendor_id, device_id))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::parse_pci_id_database;

    #[test]
    fn parses_vendor_and_device_entries_from_pci_ids() {
        let db = parse_pci_id_database(
            "8086  Intel Corporation\n\t46A6  Alder Lake-P Integrated Graphics Controller\n1002  Advanced Micro Devices, Inc. [AMD/ATI]\n\t7480  Navi 32 [Radeon RX 7800 XT / 7700 XT]\n",
        );

        assert_eq!(
            db.vendor_names.get(&0x8086).map(String::as_str),
            Some("Intel Corporation")
        );
        assert_eq!(
            db.device_names.get(&(0x8086, 0x46A6)).map(String::as_str),
            Some("Alder Lake-P Integrated Graphics Controller")
        );
        assert_eq!(
            db.device_names.get(&(0x1002, 0x7480)).map(String::as_str),
            Some("Navi 32 [Radeon RX 7800 XT / 7700 XT]")
        );
    }

    #[test]
    fn ignores_subsystem_lines_and_comments() {
        let db = parse_pci_id_database(
            "# comment\n8086  Intel Corporation\n\t46A6  Alder Lake-P Integrated Graphics Controller\n\t\t17AA  3C6A  Lenovo variant\n",
        );

        assert_eq!(db.vendor_names.len(), 1);
        assert_eq!(db.device_names.len(), 1);
        assert!(db.device_names.get(&(0x17AA, 0x3C6A)).is_none());
    }
}
