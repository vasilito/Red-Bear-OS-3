use std::fmt;
use std::fs;
use std::process;
use std::str::FromStr;

use redbear_hwutils::{describe_usb_device, parse_args};
use redox_driver_sys::quirks::{UsbQuirkFlags, lookup_usb_quirks};
use serde::Deserialize;

const USAGE: &str = "Usage: lsusb\nList USB devices exposed by native usb.* schemes.";

#[derive(Clone, Debug, Eq, PartialEq)]
struct UsbDeviceSummary {
    controller: String,
    port: PortId,
    vendor_id: u16,
    product_id: u16,
    class: u8,
    subclass: u8,
    protocol: u8,
    usb_major: u8,
    usb_minor: u8,
    description: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct UsbPortStateSummary {
    controller: String,
    port: PortId,
    state: PortState,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct PortId {
    root_hub_port_num: u8,
    route_string: u32,
}

impl fmt::Display for PortId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.root_hub_port_num)?;
        let mut route_string = self.route_string;
        while route_string != 0 {
            write!(f, ".{}", route_string & 0xF)?;
            route_string >>= 4;
        }
        Ok(())
    }
}

impl FromStr for PortId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut root_hub_port_num = 0;
        let mut route_string = 0;

        for (i, part) in s.split('.').enumerate() {
            let value: u8 = part
                .parse()
                .map_err(|err| format!("failed to parse {:?}: {}", part, err))?;

            if value == 0 {
                return Err("zero is not a valid port ID component".to_string());
            }

            if i == 0 {
                root_hub_port_num = value;
                continue;
            }

            let depth = i - 1;
            if depth >= 5 {
                return Err("too many route string components".to_string());
            }
            if value & 0xF0 != 0 {
                return Err(format!(
                    "value {:?} is too large for route string component",
                    value
                ));
            }
            route_string |= u32::from(value) << (depth * 4);
        }

        if root_hub_port_num == 0 {
            return Err("missing root hub port number".to_string());
        }

        Ok(Self {
            root_hub_port_num,
            route_string,
        })
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PortState {
    EnabledOrDisabled,
    Default,
    Addressed,
    Configured,
}

impl PortState {
    fn as_str(&self) -> &'static str {
        match self {
            Self::EnabledOrDisabled => "enabled_or_disabled",
            Self::Default => "default",
            Self::Addressed => "addressed",
            Self::Configured => "configured",
        }
    }
}

impl FromStr for PortState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "enabled_or_disabled" | "enabled/disabled" => Ok(Self::EnabledOrDisabled),
            "default" => Ok(Self::Default),
            "addressed" => Ok(Self::Addressed),
            "configured" => Ok(Self::Configured),
            _ => Err("read reserved port state".to_string()),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct DevDesc {
    usb: u16,
    class: u8,
    sub_class: u8,
    protocol: u8,
    vendor: u16,
    product: u16,
    manufacturer_str: Option<String>,
    product_str: Option<String>,
}

impl DevDesc {
    fn major_version(&self) -> u8 {
        ((self.usb & 0xFF00) >> 8) as u8
    }

    fn minor_version(&self) -> u8 {
        self.usb as u8
    }
}

fn main() {
    match run() {
        Ok(()) => {}
        Err(err) if err.is_empty() => {}
        Err(err) => {
            eprintln!("lsusb: {err}");
            process::exit(1);
        }
    }
}

fn format_usb_quirk_flags(flags: UsbQuirkFlags) -> String {
    let all_flags: &[(UsbQuirkFlags, &str)] = &[
        (UsbQuirkFlags::NO_STRING_FETCH, "no_string_fetch"),
        (UsbQuirkFlags::RESET_DELAY, "reset_delay"),
        (UsbQuirkFlags::NO_USB3, "no_usb3"),
        (UsbQuirkFlags::NO_SET_CONFIG, "no_set_config"),
        (UsbQuirkFlags::NO_SUSPEND, "no_suspend"),
        (UsbQuirkFlags::NEED_RESET, "need_reset"),
        (UsbQuirkFlags::BAD_DESCRIPTOR, "bad_descriptor"),
        (UsbQuirkFlags::NO_LPM, "no_lpm"),
        (UsbQuirkFlags::NO_U1U2, "no_u1u2"),
        (UsbQuirkFlags::NO_SET_INTF, "no_set_intf"),
        (UsbQuirkFlags::CONFIG_INTF_STRINGS, "config_intf_strings"),
        (UsbQuirkFlags::NO_RESET, "no_reset"),
        (UsbQuirkFlags::HONOR_BNUMINTERFACES, "honor_bnuminterfaces"),
        (UsbQuirkFlags::DEVICE_QUALIFIER, "device_qualifier"),
        (UsbQuirkFlags::IGNORE_REMOTE_WAKEUP, "ignore_remote_wakeup"),
        (UsbQuirkFlags::DELAY_CTRL_MSG, "delay_ctrl_msg"),
        (UsbQuirkFlags::HUB_SLOW_RESET, "hub_slow_reset"),
        (UsbQuirkFlags::NO_BOS, "no_bos"),
        (
            UsbQuirkFlags::SHORT_SET_ADDR_TIMEOUT,
            "short_set_addr_timeout",
        ),
        (UsbQuirkFlags::FORCE_ONE_CONFIG, "force_one_config"),
        (UsbQuirkFlags::ENDPOINT_IGNORE, "endpoint_ignore"),
        (
            UsbQuirkFlags::LINEAR_FRAME_BINTERVAL,
            "linear_frame_binterval",
        ),
    ];
    all_flags
        .iter()
        .filter(|(flag, _)| flags.contains(*flag))
        .map(|(_, name)| *name)
        .collect::<Vec<_>>()
        .join(",")
}

fn run() -> Result<(), String> {
    parse_args("lsusb", USAGE, std::env::args())?;

    let (mut devices, mut fallback_ports) = collect_usb_state()?;
    devices.sort_by(|left, right| {
        left.controller
            .cmp(&right.controller)
            .then(left.port.cmp(&right.port))
    });
    fallback_ports.sort_by(|left, right| {
        left.controller
            .cmp(&right.controller)
            .then(left.port.cmp(&right.port))
    });

    for device in devices {
        print!(
            "{} {} ID {:04x}:{:04x} class {:02x}/{:02x}/{:02x} usb {}.{:02x} {}",
            device.controller,
            device.port,
            device.vendor_id,
            device.product_id,
            device.class,
            device.subclass,
            device.protocol,
            device.usb_major,
            device.usb_minor,
            device.description,
        );
        let quirk_flags = lookup_usb_quirks(device.vendor_id, device.product_id);
        if !quirk_flags.is_empty() {
            print!(" quirks: {}", format_usb_quirk_flags(quirk_flags));
        }
        println!();
    }

    for fallback in fallback_ports {
        println!(
            "{} {} state {}",
            fallback.controller,
            fallback.port,
            fallback.state.as_str(),
        );
    }

    Ok(())
}

fn collect_usb_state() -> Result<(Vec<UsbDeviceSummary>, Vec<UsbPortStateSummary>), String> {
    let entries =
        fs::read_dir("/scheme").map_err(|err| format!("failed to read /scheme: {err}"))?;

    let mut devices = Vec::new();
    let mut fallback_ports = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };

        let file_name = entry.file_name();
        let Some(controller) = file_name.to_str() else {
            continue;
        };
        if !controller.starts_with("usb.") {
            continue;
        }

        let controller_dir = format!("/scheme/{controller}");
        let ports = match fs::read_dir(&controller_dir) {
            Ok(ports) => ports,
            Err(_) => continue,
        };

        for port_entry in ports {
            let port_entry = match port_entry {
                Ok(port_entry) => port_entry,
                Err(_) => continue,
            };

            let port_name = port_entry.file_name();
            let Some(port_name) = port_name.to_str() else {
                continue;
            };
            let Some(raw_port_id) = port_name.strip_prefix("port") else {
                continue;
            };
            let Ok(port) = raw_port_id.parse::<PortId>() else {
                continue;
            };

            let state = read_port_state(controller, port).ok();

            match read_standard_descs(controller, port) {
                Ok(descriptors) => {
                    devices.push(UsbDeviceSummary {
                        controller: controller.to_string(),
                        port,
                        vendor_id: descriptors.vendor,
                        product_id: descriptors.product,
                        class: descriptors.class,
                        subclass: descriptors.sub_class,
                        protocol: descriptors.protocol,
                        usb_major: descriptors.major_version(),
                        usb_minor: descriptors.minor_version(),
                        description: describe_usb_device(
                            descriptors.manufacturer_str.as_deref(),
                            descriptors.product_str.as_deref(),
                        ),
                    });
                }
                Err(_) => {
                    if let Some(state) =
                        state.filter(|state| *state != PortState::EnabledOrDisabled)
                    {
                        fallback_ports.push(UsbPortStateSummary {
                            controller: controller.to_string(),
                            port,
                            state,
                        });
                    }
                }
            }
        }
    }

    Ok((devices, fallback_ports))
}

fn read_port_state(controller: &str, port: PortId) -> Result<PortState, String> {
    let state_path = format!("/scheme/{controller}/port{port}/state");
    let raw = fs::read_to_string(&state_path)
        .map_err(|err| format!("failed to read {state_path}: {err}"))?;
    raw.parse()
}

fn read_standard_descs(controller: &str, port: PortId) -> Result<DevDesc, String> {
    let descriptor_path = format!("/scheme/{controller}/port{port}/descriptors");
    let raw = fs::read(&descriptor_path)
        .map_err(|err| format!("failed to read {descriptor_path}: {err}"))?;
    serde_json::from_slice(&raw).map_err(|err| format!("failed to parse {descriptor_path}: {err}"))
}
