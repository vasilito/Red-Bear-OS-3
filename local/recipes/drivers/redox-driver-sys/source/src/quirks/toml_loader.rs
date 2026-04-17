use super::{
    dmi::{self, DmiInfo, DmiMatchRule, DmiPciQuirkRule},
    PciQuirkEntry, PciQuirkFlags, UsbQuirkEntry, UsbQuirkFlags, PCI_QUIRK_ANY_ID,
};
use crate::pci::PciDeviceInfo;
use std::borrow::Cow;
use std::convert::TryFrom;

const QUIRKS_DIR: &str = "/etc/quirks.d";

pub fn load_pci_quirks(info: &PciDeviceInfo) -> Result<PciQuirkFlags, ()> {
    let mut flags = PciQuirkFlags::empty();
    let entries = read_toml_pci_entries().map_err(|_| ())?;
    for entry in &entries {
        if entry.matches_toml(info) {
            flags |= entry.flags;
        }
    }
    Ok(flags)
}

pub fn load_usb_quirks(vendor: u16, product: u16) -> Result<UsbQuirkFlags, ()> {
    let mut flags = UsbQuirkFlags::empty();
    let entries = read_toml_usb_entries().map_err(|_| ())?;
    for entry in &entries {
        if entry.matches(vendor, product) {
            flags |= entry.flags;
        }
    }
    Ok(flags)
}

pub(crate) fn load_dmi_pci_quirks(
    info: &PciDeviceInfo,
    dmi_info: &DmiInfo,
) -> Result<PciQuirkFlags, ()> {
    let entries = read_toml_dmi_entries().map_err(|_| ())?;
    Ok(dmi::apply_dmi_pci_quirk_rules(
        info,
        Some(dmi_info),
        &entries,
    ))
}

fn bounded_u16(val: &toml::Value, field: &str, path: &str) -> Option<u16> {
    match val.as_integer() {
        Some(v) => u16::try_from(v).ok().or_else(|| {
            log::warn!("quirks: {path}: {field}={v} out of u16 range, skipping entry");
            None
        }),
        None => {
            log::warn!("quirks: {path}: {field} is not an integer, skipping entry");
            None
        }
    }
}

fn bounded_u8(val: &toml::Value, field: &str, path: &str) -> Option<u8> {
    match val.as_integer() {
        Some(v) => u8::try_from(v).ok().or_else(|| {
            log::warn!("quirks: {path}: {field}={v} out of u8 range, skipping entry");
            None
        }),
        None => {
            log::warn!("quirks: {path}: {field} is not an integer, skipping entry");
            None
        }
    }
}

fn bounded_u32(val: &toml::Value, field: &str, path: &str) -> Option<u32> {
    match val.as_integer() {
        Some(v) => u32::try_from(v).ok().or_else(|| {
            log::warn!("quirks: {path}: {field}={v} out of u32 range, skipping entry");
            None
        }),
        None => {
            log::warn!("quirks: {path}: {field} is not an integer, skipping entry");
            None
        }
    }
}

fn sorted_toml_files(dir: &str) -> std::io::Result<Vec<std::path::PathBuf>> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |e| e == "toml"))
        .collect();
    paths.sort();
    Ok(paths)
}

const PCI_FLAG_NAMES: &[(&str, PciQuirkFlags)] = &[
    ("no_msi", PciQuirkFlags::NO_MSI),
    ("no_msix", PciQuirkFlags::NO_MSIX),
    ("force_legacy_irq", PciQuirkFlags::FORCE_LEGACY_IRQ),
    ("no_pm", PciQuirkFlags::NO_PM),
    ("no_d3cold", PciQuirkFlags::NO_D3COLD),
    ("no_aspm", PciQuirkFlags::NO_ASPM),
    ("need_iommu", PciQuirkFlags::NEED_IOMMU),
    ("no_iommu", PciQuirkFlags::NO_IOMMU),
    ("dma_32bit_only", PciQuirkFlags::DMA_32BIT_ONLY),
    ("resize_bar", PciQuirkFlags::RESIZE_BAR),
    ("disable_bar_sizing", PciQuirkFlags::DISABLE_BAR_SIZING),
    ("need_firmware", PciQuirkFlags::NEED_FIRMWARE),
    ("disable_accel", PciQuirkFlags::DISABLE_ACCEL),
    ("force_vram_only", PciQuirkFlags::FORCE_VRAM_ONLY),
    ("no_usb3", PciQuirkFlags::NO_USB3),
    ("reset_delay_ms", PciQuirkFlags::RESET_DELAY_MS),
    ("no_string_fetch", PciQuirkFlags::NO_STRING_FETCH),
    ("bad_eeprom", PciQuirkFlags::BAD_EEPROM),
    ("bus_master_delay", PciQuirkFlags::BUS_MASTER_DELAY),
    ("wrong_class", PciQuirkFlags::WRONG_CLASS),
    ("broken_bridge", PciQuirkFlags::BROKEN_BRIDGE),
    ("no_resource_reloc", PciQuirkFlags::NO_RESOURCE_RELOC),
];

const USB_FLAG_NAMES: &[(&str, UsbQuirkFlags)] = &[
    ("no_string_fetch", UsbQuirkFlags::NO_STRING_FETCH),
    ("reset_delay", UsbQuirkFlags::RESET_DELAY),
    ("no_usb3", UsbQuirkFlags::NO_USB3),
    ("no_set_config", UsbQuirkFlags::NO_SET_CONFIG),
    ("no_suspend", UsbQuirkFlags::NO_SUSPEND),
    ("need_reset", UsbQuirkFlags::NEED_RESET),
    ("bad_descriptor", UsbQuirkFlags::BAD_DESCRIPTOR),
    ("no_lpm", UsbQuirkFlags::NO_LPM),
    ("no_u1u2", UsbQuirkFlags::NO_U1U2),
    ("no_set_intf", UsbQuirkFlags::NO_SET_INTF),
    ("config_intf_strings", UsbQuirkFlags::CONFIG_INTF_STRINGS),
    ("no_reset", UsbQuirkFlags::NO_RESET),
    ("honor_bnuminterfaces", UsbQuirkFlags::HONOR_BNUMINTERFACES),
    ("device_qualifier", UsbQuirkFlags::DEVICE_QUALIFIER),
    ("ignore_remote_wakeup", UsbQuirkFlags::IGNORE_REMOTE_WAKEUP),
    ("delay_ctrl_msg", UsbQuirkFlags::DELAY_CTRL_MSG),
    ("hub_slow_reset", UsbQuirkFlags::HUB_SLOW_RESET),
    ("no_bos", UsbQuirkFlags::NO_BOS),
    (
        "short_set_addr_timeout",
        UsbQuirkFlags::SHORT_SET_ADDR_TIMEOUT,
    ),
    ("force_one_config", UsbQuirkFlags::FORCE_ONE_CONFIG),
    ("endpoint_ignore", UsbQuirkFlags::ENDPOINT_IGNORE),
    (
        "linear_frame_binterval",
        UsbQuirkFlags::LINEAR_FRAME_BINTERVAL,
    ),
];

fn flag_from_name<F: Copy>(name: &str, mapping: &[(&str, F)]) -> Option<F> {
    mapping
        .iter()
        .find_map(|(candidate, flag)| (*candidate == name).then_some(*flag))
}

fn parse_flags_from_names<F>(names: &[toml::Value], mapping: &[(&str, F)]) -> F
where
    F: bitflags::Flags + Copy + std::ops::BitOrAssign,
{
    let mut flags = F::empty();
    for name in names.iter().filter_map(toml::Value::as_str) {
        if let Some(flag) = flag_from_name(name, mapping) {
            flags |= flag;
        }
    }
    flags
}

fn parse_flags<F>(table: &toml::Table, path: &str, kind: &str, mapping: &[(&str, F)]) -> F
where
    F: bitflags::Flags + Copy + std::ops::BitOrAssign,
{
    let Some(names) = table.get("flags").and_then(|v| v.as_array()) else {
        return F::empty();
    };

    for flag in names {
        let Some(name) = flag.as_str() else {
            continue;
        };
        if flag_from_name(name, mapping).is_none() {
            log::warn!("quirks: {path}: unknown {kind} quirk flag '{name}'");
        }
    }

    parse_flags_from_names(names, mapping)
}

fn parse_string_field(
    table: &toml::Table,
    field: &str,
    path: &str,
    kind: &str,
) -> Result<Option<Cow<'static, str>>, ()> {
    let Some(value) = table.get(field) else {
        return Ok(None);
    };

    match value.as_str() {
        Some(value) => Ok(Some(Cow::Owned(value.to_string()))),
        None => {
            log::warn!("quirks: {path}: {kind}.{field} is not a string, skipping entry");
            Err(())
        }
    }
}

fn parse_dmi_match_rule(table: &toml::Table, path: &str) -> Option<DmiMatchRule> {
    let sys_vendor = match parse_string_field(table, "sys_vendor", path, "match") {
        Ok(value) => value,
        Err(()) => return None,
    };
    let board_vendor = match parse_string_field(table, "board_vendor", path, "match") {
        Ok(value) => value,
        Err(()) => return None,
    };
    let board_name = match parse_string_field(table, "board_name", path, "match") {
        Ok(value) => value,
        Err(()) => return None,
    };
    let board_version = match parse_string_field(table, "board_version", path, "match") {
        Ok(value) => value,
        Err(()) => return None,
    };
    let product_name = match parse_string_field(table, "product_name", path, "match") {
        Ok(value) => value,
        Err(()) => return None,
    };
    let product_version = match parse_string_field(table, "product_version", path, "match") {
        Ok(value) => value,
        Err(()) => return None,
    };
    let bios_version = match parse_string_field(table, "bios_version", path, "match") {
        Ok(value) => value,
        Err(()) => return None,
    };

    let rule = DmiMatchRule {
        sys_vendor,
        board_vendor,
        board_name,
        board_version,
        product_name,
        product_version,
        bios_version,
    };

    if rule.is_empty() {
        log::warn!("quirks: {path}: dmi_system_quirk.match has no fields, skipping entry");
        return None;
    }

    Some(rule)
}

fn read_toml_pci_entries() -> std::io::Result<Vec<PciQuirkEntry>> {
    let mut entries = Vec::new();
    for path in sorted_toml_files(QUIRKS_DIR)? {
        let path_str = path.display().to_string();
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("quirks: failed to read {path_str}: {e}");
                continue;
            }
        };
        let doc = match content.parse::<toml::Value>() {
            Ok(d) => d,
            Err(e) => {
                log::warn!("quirks: failed to parse {path_str}: {e}");
                continue;
            }
        };
        parse_pci_toml(&doc, &mut entries, &path_str);
    }
    Ok(entries)
}

fn parse_pci_toml(doc: &toml::Value, out: &mut Vec<PciQuirkEntry>, path: &str) {
    let Some(arr) = doc.get("pci_quirk").and_then(|v| v.as_array()) else {
        return;
    };
    for item in arr {
        let Some(table) = item.as_table() else {
            log::warn!("quirks: {path}: pci_quirk entry is not a table, skipping");
            continue;
        };
        let vendor = table
            .get("vendor")
            .and_then(|v| bounded_u16(v, "vendor", path))
            .unwrap_or(PCI_QUIRK_ANY_ID);
        let device = table
            .get("device")
            .and_then(|v| bounded_u16(v, "device", path))
            .unwrap_or(PCI_QUIRK_ANY_ID);
        let subvendor = table
            .get("subvendor")
            .and_then(|v| bounded_u16(v, "subvendor", path))
            .unwrap_or(PCI_QUIRK_ANY_ID);
        let subdevice = table
            .get("subdevice")
            .and_then(|v| bounded_u16(v, "subdevice", path))
            .unwrap_or(PCI_QUIRK_ANY_ID);
        let class_match = table
            .get("class")
            .and_then(|v| bounded_u32(v, "class", path))
            .unwrap_or(0);
        let explicit_mask = table
            .get("class_mask")
            .and_then(|v| bounded_u32(v, "class_mask", path));
        let class_mask = explicit_mask.unwrap_or(if class_match != 0 { 0xFF0000 } else { 0 });
        let revision_lo = table
            .get("revision_lo")
            .and_then(|v| bounded_u8(v, "revision_lo", path))
            .unwrap_or(0x00);
        let revision_hi = table
            .get("revision_hi")
            .and_then(|v| bounded_u8(v, "revision_hi", path))
            .unwrap_or(0xFF);
        let flags = parse_flags(table, path, "PCI", PCI_FLAG_NAMES);
        out.push(PciQuirkEntry {
            vendor,
            device,
            subvendor,
            subdevice,
            class_mask,
            class_match,
            revision_lo,
            revision_hi,
            flags,
        });
    }
}

fn read_toml_usb_entries() -> std::io::Result<Vec<UsbQuirkEntry>> {
    let mut entries = Vec::new();
    for path in sorted_toml_files(QUIRKS_DIR)? {
        let path_str = path.display().to_string();
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("quirks: failed to read {path_str}: {e}");
                continue;
            }
        };
        let doc = match content.parse::<toml::Value>() {
            Ok(d) => d,
            Err(e) => {
                log::warn!("quirks: failed to parse {path_str}: {e}");
                continue;
            }
        };
        parse_usb_toml(&doc, &mut entries, &path_str);
    }
    Ok(entries)
}

fn parse_usb_toml(doc: &toml::Value, out: &mut Vec<UsbQuirkEntry>, path: &str) {
    let Some(arr) = doc.get("usb_quirk").and_then(|v| v.as_array()) else {
        return;
    };
    for item in arr {
        let Some(table) = item.as_table() else {
            log::warn!("quirks: {path}: usb_quirk entry is not a table, skipping");
            continue;
        };
        let vendor = table
            .get("vendor")
            .and_then(|v| bounded_u16(v, "vendor", path))
            .unwrap_or(PCI_QUIRK_ANY_ID);
        let product = table
            .get("product")
            .and_then(|v| bounded_u16(v, "product", path))
            .unwrap_or(PCI_QUIRK_ANY_ID);
        let flags = parse_flags(table, path, "USB", USB_FLAG_NAMES);
        out.push(UsbQuirkEntry {
            vendor,
            product,
            flags,
        });
    }
}

fn read_toml_dmi_entries() -> std::io::Result<Vec<DmiPciQuirkRule>> {
    let mut entries = Vec::new();
    for path in sorted_toml_files(QUIRKS_DIR)? {
        let path_str = path.display().to_string();
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("quirks: failed to read {path_str}: {e}");
                continue;
            }
        };
        let doc = match content.parse::<toml::Value>() {
            Ok(d) => d,
            Err(e) => {
                log::warn!("quirks: failed to parse {path_str}: {e}");
                continue;
            }
        };
        parse_dmi_toml(&doc, &mut entries, &path_str);
    }
    Ok(entries)
}

fn parse_dmi_toml(doc: &toml::Value, out: &mut Vec<DmiPciQuirkRule>, path: &str) {
    let Some(arr) = doc.get("dmi_system_quirk").and_then(|v| v.as_array()) else {
        return;
    };
    for item in arr {
        let Some(table) = item.as_table() else {
            log::warn!("quirks: {path}: dmi_system_quirk entry is not a table, skipping");
            continue;
        };
        let Some(match_table) = table.get("match").and_then(|v| v.as_table()) else {
            log::warn!("quirks: {path}: dmi_system_quirk entry is missing match table, skipping");
            continue;
        };
        let Some(dmi_match) = parse_dmi_match_rule(match_table, path) else {
            continue;
        };
        let vendor = match table.get("pci_vendor") {
            Some(value) => match bounded_u16(value, "pci_vendor", path) {
                Some(value) => value,
                None => continue,
            },
            None => PCI_QUIRK_ANY_ID,
        };
        let device = match table.get("pci_device") {
            Some(value) => match bounded_u16(value, "pci_device", path) {
                Some(value) => value,
                None => continue,
            },
            None => PCI_QUIRK_ANY_ID,
        };
        let flags = parse_flags(table, path, "PCI", PCI_FLAG_NAMES);

        out.push(DmiPciQuirkRule {
            dmi_match,
            vendor,
            device,
            flags,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pci::{PciDeviceInfo, PciLocation};

    fn make_info(vendor: u16, device: u16) -> PciDeviceInfo {
        PciDeviceInfo {
            location: PciLocation {
                segment: 0,
                bus: 0,
                device: 0,
                function: 0,
            },
            vendor_id: vendor,
            device_id: device,
            subsystem_vendor_id: 0,
            subsystem_device_id: 0,
            revision: 0,
            class_code: 0,
            subclass: 0,
            prog_if: 0,
            header_type: 0,
            irq: None,
            bars: Vec::new(),
            capabilities: Vec::new(),
        }
    }

    #[test]
    fn dmi_toml_matches_sys_vendor_and_product_name() {
        let doc = r#"
            [[dmi_system_quirk]]
            pci_vendor = 0x1002
            pci_device = 0x73BF
            flags = ["disable_accel"]
            match.sys_vendor = "Framework"
            match.product_name = "Laptop 16"
        "#
        .parse::<toml::Value>()
        .unwrap();

        let mut rules = Vec::new();
        parse_dmi_toml(&doc, &mut rules, "test.toml");
        assert_eq!(rules.len(), 1);

        let dmi_info = DmiInfo {
            sys_vendor: Some("Framework".to_string()),
            product_name: Some("Laptop 16".to_string()),
            board_vendor: None,
            board_name: None,
            board_version: None,
            product_version: None,
            bios_version: None,
        };

        let flags =
            dmi::apply_dmi_pci_quirk_rules(&make_info(0x1002, 0x73BF), Some(&dmi_info), &rules);
        assert!(flags.contains(PciQuirkFlags::DISABLE_ACCEL));
    }

    #[test]
    fn dmi_toml_vendor_only_selector_matches_any_device_for_vendor() {
        let doc = r#"
            [[dmi_system_quirk]]
            pci_vendor = 0x8086
            flags = ["no_aspm"]
            match.sys_vendor = "LENOVO"
            match.product_name = "ThinkPad X1 Carbon"
        "#
        .parse::<toml::Value>()
        .unwrap();

        let mut rules = Vec::new();
        parse_dmi_toml(&doc, &mut rules, "test.toml");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].vendor, 0x8086);
        assert_eq!(rules[0].device, PCI_QUIRK_ANY_ID);

        let dmi_info = DmiInfo {
            sys_vendor: Some("LENOVO".to_string()),
            product_name: Some("ThinkPad X1 Carbon".to_string()),
            board_vendor: None,
            board_name: None,
            board_version: None,
            product_version: None,
            bios_version: None,
        };

        let flags =
            dmi::apply_dmi_pci_quirk_rules(&make_info(0x8086, 0x46A6), Some(&dmi_info), &rules);
        assert!(flags.contains(PciQuirkFlags::NO_ASPM));
    }

    #[test]
    fn dmi_toml_rules_do_not_apply_without_dmi_info() {
        let doc = r#"
            [[dmi_system_quirk]]
            pci_vendor = 0x1002
            flags = ["need_firmware"]
            match.sys_vendor = "Framework"
            match.product_name = "Laptop 16"
        "#
        .parse::<toml::Value>()
        .unwrap();

        let mut rules = Vec::new();
        parse_dmi_toml(&doc, &mut rules, "test.toml");

        let flags = dmi::apply_dmi_pci_quirk_rules(&make_info(0x1002, 0x73BF), None, &rules);
        assert!(flags.is_empty());
    }

    #[test]
    fn dmi_toml_invalid_pci_selector_skips_entry() {
        let doc = r#"
            [[dmi_system_quirk]]
            pci_vendor = 0x1_0000
            flags = ["disable_accel"]
            match.sys_vendor = "Framework"
            match.product_name = "Laptop 16"
        "#
        .parse::<toml::Value>()
        .unwrap();

        let mut rules = Vec::new();
        parse_dmi_toml(&doc, &mut rules, "test.toml");

        assert!(rules.is_empty());
    }
}
