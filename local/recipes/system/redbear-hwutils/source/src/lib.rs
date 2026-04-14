use std::fmt;

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
