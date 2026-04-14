use std::fs;
use std::process;

use redbear_hwutils::{parse_args, parse_pci_location, PciLocation};

const USAGE: &str = "Usage: lspci\nList PCI devices exposed by /scheme/pci.";

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PciDeviceSummary {
    location: PciLocation,
    vendor_id: u16,
    device_id: u16,
    class_code: u8,
    subclass: u8,
    prog_if: u8,
    revision: u8,
}

fn main() {
    match run() {
        Ok(()) => {}
        Err(err) if err.is_empty() => {}
        Err(err) => {
            eprintln!("lspci: {err}");
            process::exit(1);
        }
    }
}

fn run() -> Result<(), String> {
    parse_args("lspci", USAGE, std::env::args())?;

    let mut devices = collect_devices()?;
    devices.sort();

    for device in devices {
        println!(
            "{} class {:02x}:{:02x}.{:02x} vendor {:04x} device {:04x} rev {:02x}",
            device.location,
            device.class_code,
            device.subclass,
            device.prog_if,
            device.vendor_id,
            device.device_id,
            device.revision,
        );
    }

    Ok(())
}

fn collect_devices() -> Result<Vec<PciDeviceSummary>, String> {
    let entries =
        fs::read_dir("/scheme/pci").map_err(|err| format!("failed to read /scheme/pci: {err}"))?;

    let mut devices = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };

        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        let Some(location) = parse_pci_location(file_name) else {
            continue;
        };

        let config_path = format!("{}/config", location.scheme_path());
        let config = match fs::read(&config_path) {
            Ok(config) => config,
            Err(_) => continue,
        };

        if config.len() < 16 {
            continue;
        }

        devices.push(PciDeviceSummary {
            location,
            vendor_id: u16::from_le_bytes([config[0x00], config[0x01]]),
            device_id: u16::from_le_bytes([config[0x02], config[0x03]]),
            revision: config[0x08],
            prog_if: config[0x09],
            subclass: config[0x0A],
            class_code: config[0x0B],
        });
    }

    Ok(devices)
}
