use std::fs;
use std::process;

use redbear_hwutils::{describe_usb_device, parse_args};
use xhcid_interface::{PortId, PortState, XhciClientHandle};

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
        println!(
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

            let handle = match XhciClientHandle::new(controller.to_string(), port) {
                Ok(handle) => handle,
                Err(_) => continue,
            };

            let state = handle.port_state().ok();

            match handle.get_standard_descs() {
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
