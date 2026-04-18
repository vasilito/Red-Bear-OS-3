use std::fs;
use std::process;

use redbear_hwutils::{
    lookup_pci_device_name, lookup_pci_vendor_name, parse_args, parse_pci_location, PciLocation,
};
use redox_driver_sys::pci::PciDeviceInfo;
use redox_driver_sys::quirks::{lookup_pci_quirks, PciQuirkFlags};

const USAGE: &str = "Usage: lspci\nList PCI devices exposed by /scheme/pci.";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PciDeviceSummary {
    location: PciLocation,
    vendor_id: u16,
    device_id: u16,
    class_code: u8,
    subclass: u8,
    prog_if: u8,
    revision: u8,
    subvendor_id: u16,
    subdevice_id: u16,
    quirk_flags: PciQuirkFlags,
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

fn format_quirk_flags(flags: PciQuirkFlags) -> String {
    let mut names = Vec::new();
    if flags.contains(PciQuirkFlags::NO_MSI) {
        names.push("no_msi");
    }
    if flags.contains(PciQuirkFlags::NO_MSIX) {
        names.push("no_msix");
    }
    if flags.contains(PciQuirkFlags::FORCE_LEGACY_IRQ) {
        names.push("force_legacy_irq");
    }
    if flags.contains(PciQuirkFlags::NO_PM) {
        names.push("no_pm");
    }
    if flags.contains(PciQuirkFlags::NO_D3COLD) {
        names.push("no_d3cold");
    }
    if flags.contains(PciQuirkFlags::NO_ASPM) {
        names.push("no_aspm");
    }
    if flags.contains(PciQuirkFlags::NEED_IOMMU) {
        names.push("need_iommu");
    }
    if flags.contains(PciQuirkFlags::NO_IOMMU) {
        names.push("no_iommu");
    }
    if flags.contains(PciQuirkFlags::DMA_32BIT_ONLY) {
        names.push("dma_32bit_only");
    }
    if flags.contains(PciQuirkFlags::RESIZE_BAR) {
        names.push("resize_bar");
    }
    if flags.contains(PciQuirkFlags::DISABLE_BAR_SIZING) {
        names.push("disable_bar_sizing");
    }
    if flags.contains(PciQuirkFlags::NEED_FIRMWARE) {
        names.push("need_firmware");
    }
    if flags.contains(PciQuirkFlags::DISABLE_ACCEL) {
        names.push("disable_accel");
    }
    if flags.contains(PciQuirkFlags::FORCE_VRAM_ONLY) {
        names.push("force_vram_only");
    }
    if flags.contains(PciQuirkFlags::NO_USB3) {
        names.push("no_usb3");
    }
    if flags.contains(PciQuirkFlags::RESET_DELAY_MS) {
        names.push("reset_delay_ms");
    }
    if flags.contains(PciQuirkFlags::NO_STRING_FETCH) {
        names.push("no_string_fetch");
    }
    if flags.contains(PciQuirkFlags::BAD_EEPROM) {
        names.push("bad_eeprom");
    }
    if flags.contains(PciQuirkFlags::BUS_MASTER_DELAY) {
        names.push("bus_master_delay");
    }
    if flags.contains(PciQuirkFlags::WRONG_CLASS) {
        names.push("wrong_class");
    }
    if flags.contains(PciQuirkFlags::BROKEN_BRIDGE) {
        names.push("broken_bridge");
    }
    if flags.contains(PciQuirkFlags::NO_RESOURCE_RELOC) {
        names.push("no_resource_reloc");
    }
    names.join(",")
}

fn lookup_quirks(
    vendor_id: u16,
    device_id: u16,
    revision: u8,
    class_code: u8,
    subclass: u8,
    prog_if: u8,
    subvendor_id: u16,
    subdevice_id: u16,
) -> PciQuirkFlags {
    let info = PciDeviceInfo {
        location: redox_driver_sys::pci::PciLocation {
            segment: 0,
            bus: 0,
            device: 0,
            function: 0,
        },
        vendor_id,
        device_id,
        subsystem_vendor_id: subvendor_id,
        subsystem_device_id: subdevice_id,
        revision,
        class_code,
        subclass,
        prog_if,
        header_type: 0,
        irq: None,
        bars: Vec::new(),
        capabilities: Vec::new(),
    };
    lookup_pci_quirks(&info)
}

fn run() -> Result<(), String> {
    parse_args("lspci", USAGE, std::env::args())?;

    let mut devices = collect_devices()?;
    devices.sort_by_key(|d| d.location);

    for device in devices {
        print!(
            "{} class {:02x}:{:02x}.{:02x} vendor {:04x} device {:04x} rev {:02x}",
            device.location,
            device.class_code,
            device.subclass,
            device.prog_if,
            device.vendor_id,
            device.device_id,
            device.revision,
        );
        if let Some(device_name) = lookup_pci_device_name(device.vendor_id, device.device_id) {
            print!(" ({device_name})");
        } else if let Some(vendor_name) = lookup_pci_vendor_name(device.vendor_id) {
            print!(" ({vendor_name})");
        }
        if !device.quirk_flags.is_empty() {
            print!(" quirks: {}", format_quirk_flags(device.quirk_flags));
        }
        println!();
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

        let vendor_id = u16::from_le_bytes([config[0x00], config[0x01]]);
        let device_id = u16::from_le_bytes([config[0x02], config[0x03]]);
        let revision = config[0x08];
        let prog_if = config[0x09];
        let subclass = config[0x0A];
        let class_code = config[0x0B];

        let (subvendor_id, subdevice_id) = if config.len() >= 0x30 {
            (
                u16::from_le_bytes([config[0x2C], config[0x2D]]),
                u16::from_le_bytes([config[0x2E], config[0x2F]]),
            )
        } else {
            (0xFFFF, 0xFFFF)
        };

        let quirk_flags = lookup_quirks(
            vendor_id,
            device_id,
            revision,
            class_code,
            subclass,
            prog_if,
            subvendor_id,
            subdevice_id,
        );

        devices.push(PciDeviceSummary {
            location,
            vendor_id,
            device_id,
            revision,
            prog_if,
            subclass,
            class_code,
            subvendor_id,
            subdevice_id,
            quirk_flags,
        });
    }

    Ok(devices)
}
