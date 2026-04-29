use std::collections::HashMap;
use std::fs;
use std::process;

use redbear_hwutils::{
    PciLocation, lookup_pci_device_name, lookup_pci_vendor_name, parse_args, parse_pci_location,
};
use redox_driver_sys::pci::{InterruptSupport, PciDeviceInfo, parse_device_info_from_config_space};
use redox_driver_sys::quirks::{PciQuirkFlags, lookup_pci_quirks};

const USAGE: &str = "Usage: lspci\nList PCI devices exposed by /scheme/pci.";

#[derive(Clone, Debug, Eq, PartialEq)]
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
    irq: Option<u32>,
    interrupt_support: InterruptSupport,
    irq_reason: Option<String>,
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
    for (flag, name) in [
        (PciQuirkFlags::NO_MSI, "no_msi"),
        (PciQuirkFlags::NO_MSIX, "no_msix"),
        (PciQuirkFlags::FORCE_LEGACY_IRQ, "force_legacy_irq"),
        (PciQuirkFlags::NO_PM, "no_pm"),
        (PciQuirkFlags::NO_D3COLD, "no_d3cold"),
        (PciQuirkFlags::NO_ASPM, "no_aspm"),
        (PciQuirkFlags::NEED_IOMMU, "need_iommu"),
        (PciQuirkFlags::NO_IOMMU, "no_iommu"),
        (PciQuirkFlags::DMA_32BIT_ONLY, "dma_32bit_only"),
        (PciQuirkFlags::RESIZE_BAR, "resize_bar"),
        (PciQuirkFlags::DISABLE_BAR_SIZING, "disable_bar_sizing"),
        (PciQuirkFlags::NEED_FIRMWARE, "need_firmware"),
        (PciQuirkFlags::DISABLE_ACCEL, "disable_accel"),
        (PciQuirkFlags::FORCE_VRAM_ONLY, "force_vram_only"),
        (PciQuirkFlags::NO_USB3, "no_usb3"),
        (PciQuirkFlags::RESET_DELAY_MS, "reset_delay_ms"),
        (PciQuirkFlags::NO_STRING_FETCH, "no_string_fetch"),
        (PciQuirkFlags::BAD_EEPROM, "bad_eeprom"),
        (PciQuirkFlags::BUS_MASTER_DELAY, "bus_master_delay"),
        (PciQuirkFlags::WRONG_CLASS, "wrong_class"),
        (PciQuirkFlags::BROKEN_BRIDGE, "broken_bridge"),
        (PciQuirkFlags::NO_RESOURCE_RELOC, "no_resource_reloc"),
    ] {
        if flags.contains(flag) {
            names.push(name);
        }
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

fn collect_runtime_irq_modes() -> HashMap<String, String> {
    let mut modes = HashMap::new();
    for dir in [
        "/tmp/redbear-irq-report",
        "/tmp/run/redbear-irq-report",
        "/run/redbear-irq-report",
    ] {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let name = match entry.file_name().into_string() {
                Ok(n) => n,
                Err(_) => continue,
            };
            if !name.ends_with(".env") {
                continue;
            }
            let content = match fs::read_to_string(entry.path()) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let mut device = None;
            let mut mode = None;
            let mut pid = None;
            for line in content.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    match k.trim() {
                        "device" => device = Some(v.trim().to_string()),
                        "mode" => mode = Some(v.trim().to_string()),
                        "pid" => pid = v.trim().parse::<u32>().ok(),
                        _ => {}
                    }
                }
            }
            if let (Some(device), Some(mode), Some(pid)) = (device, mode, pid) {
                if std::path::Path::new(&format!("/proc/{pid}")).exists() {
                    modes.insert(device, mode);
                }
            }
        }
    }
    modes
}

fn run() -> Result<(), String> {
    parse_args("lspci", USAGE, std::env::args())?;

    let runtime_modes = collect_runtime_irq_modes();

    let mut devices = collect_devices()?;
    devices.sort_by_key(|d| d.location);

    for device in &devices {
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
        print!(" irq-support: {}", device.interrupt_support.as_str());
        if let Some(line) = device.irq {
            print!(" line={line}");
        }
        if let Some(reason) = &device.irq_reason {
            print!(" reason={reason}");
        }
        let loc_key = device.location.to_string().replace(':', "--");
        if let Some(mode) = runtime_modes.get(&loc_key) {
            print!(" runtime-mode: {mode}");
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

        if config.len() < 64 {
            continue;
        }

        let info = match parse_device_info_from_config_space(
            redox_driver_sys::pci::PciLocation {
                segment: location.segment,
                bus: location.bus,
                device: location.device,
                function: location.function,
            },
            &config,
        ) {
            Some(info) => info,
            None => continue,
        };

        let quirk_flags = lookup_quirks(
            info.vendor_id,
            info.device_id,
            info.revision,
            info.class_code,
            info.subclass,
            info.prog_if,
            info.subsystem_vendor_id,
            info.subsystem_device_id,
        );
        let irq_reason = if quirk_flags.contains(PciQuirkFlags::FORCE_LEGACY_IRQ) {
            Some("quirk_force_legacy_irq".to_string())
        } else if quirk_flags.contains(PciQuirkFlags::NO_MSIX) {
            Some("quirk_disable_msix".to_string())
        } else if quirk_flags.contains(PciQuirkFlags::NO_MSI) {
            Some("quirk_disable_msi".to_string())
        } else {
            None
        };

        devices.push(PciDeviceSummary {
            location,
            vendor_id: info.vendor_id,
            device_id: info.device_id,
            revision: info.revision,
            prog_if: info.prog_if,
            subclass: info.subclass,
            class_code: info.class_code,
            subvendor_id: info.subsystem_vendor_id,
            subdevice_id: info.subsystem_device_id,
            irq: info.irq,
            interrupt_support: info.interrupt_support(),
            irq_reason,
            quirk_flags,
        });
    }

    Ok(devices)
}
