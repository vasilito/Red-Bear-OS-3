use super::{toml_loader, PciQuirkFlags, PCI_QUIRK_ANY_ID};
use crate::pci::PciDeviceInfo;
use std::borrow::Cow;

/// DMI/SMBIOS field identifiers for system matching.
#[derive(Clone, Debug, Default)]
pub struct DmiMatchRule {
    pub sys_vendor: Option<Cow<'static, str>>,
    pub board_vendor: Option<Cow<'static, str>>,
    pub board_name: Option<Cow<'static, str>>,
    pub board_version: Option<Cow<'static, str>>,
    pub product_name: Option<Cow<'static, str>>,
    pub product_version: Option<Cow<'static, str>>,
    pub bios_version: Option<Cow<'static, str>>,
}

impl DmiMatchRule {
    pub fn is_empty(&self) -> bool {
        self.sys_vendor.is_none()
            && self.board_vendor.is_none()
            && self.board_name.is_none()
            && self.board_version.is_none()
            && self.product_name.is_none()
            && self.product_version.is_none()
            && self.bios_version.is_none()
    }

    pub fn matches(&self, info: &DmiInfo) -> bool {
        if let Some(ref val) = self.sys_vendor {
            if info
                .sys_vendor
                .as_deref()
                .map_or(true, |v| v != val.as_ref())
            {
                return false;
            }
        }
        if let Some(ref val) = self.board_vendor {
            if info
                .board_vendor
                .as_deref()
                .map_or(true, |v| v != val.as_ref())
            {
                return false;
            }
        }
        if let Some(ref val) = self.board_name {
            if info
                .board_name
                .as_deref()
                .map_or(true, |v| v != val.as_ref())
            {
                return false;
            }
        }
        if let Some(ref val) = self.board_version {
            if info
                .board_version
                .as_deref()
                .map_or(true, |v| v != val.as_ref())
            {
                return false;
            }
        }
        if let Some(ref val) = self.product_name {
            if info
                .product_name
                .as_deref()
                .map_or(true, |v| v != val.as_ref())
            {
                return false;
            }
        }
        if let Some(ref val) = self.product_version {
            if info
                .product_version
                .as_deref()
                .map_or(true, |v| v != val.as_ref())
            {
                return false;
            }
        }
        if let Some(ref val) = self.bios_version {
            if info
                .bios_version
                .as_deref()
                .map_or(true, |v| v != val.as_ref())
            {
                return false;
            }
        }
        true
    }
}

#[derive(Clone, Debug, Default)]
pub struct DmiInfo {
    pub sys_vendor: Option<String>,
    pub board_vendor: Option<String>,
    pub board_name: Option<String>,
    pub board_version: Option<String>,
    pub product_name: Option<String>,
    pub product_version: Option<String>,
    pub bios_version: Option<String>,
}

/// A DMI-based quirk rule: if the system matches the DMI rule, apply PCI
/// quirk flags to matching devices.
#[derive(Clone, Debug)]
pub struct DmiPciQuirkRule {
    pub dmi_match: DmiMatchRule,
    pub vendor: u16,
    pub device: u16,
    pub flags: PciQuirkFlags,
}

/// Read DMI/SMBIOS data from the ACPI scheme.
///
/// Returns `Err(())` if DMI data is not available (e.g., early boot,
/// no SMBIOS table, or acpid not running).
pub fn read_dmi_info() -> Result<DmiInfo, ()> {
    let dmi_path = "/scheme/acpi/dmi";
    match std::fs::read_to_string(dmi_path) {
        Ok(data) => parse_dmi_data(&data),
        Err(_) => Err(()),
    }
}

fn parse_dmi_data(data: &str) -> Result<DmiInfo, ()> {
    let mut info = DmiInfo::default();
    for line in data.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().to_string();
        match key {
            "sys_vendor" => info.sys_vendor = Some(value),
            "board_vendor" => info.board_vendor = Some(value),
            "board_name" => info.board_name = Some(value),
            "board_version" => info.board_version = Some(value),
            "product_name" => info.product_name = Some(value),
            "product_version" => info.product_version = Some(value),
            "bios_version" => info.bios_version = Some(value),
            _ => {}
        }
    }
    Ok(info)
}

/// Compiled-in DMI-based PCI quirk rules.
const F_NO_MSIX_NO_ASPM: PciQuirkFlags = PciQuirkFlags::from_bits_truncate(
    PciQuirkFlags::NO_MSIX.bits() | PciQuirkFlags::NO_ASPM.bits(),
);
const F_NO_ASPM_NEED_FW: PciQuirkFlags = PciQuirkFlags::from_bits_truncate(
    PciQuirkFlags::NO_ASPM.bits() | PciQuirkFlags::NEED_FIRMWARE.bits(),
);
const F_NEED_IOMMU_NO_ASPM: PciQuirkFlags = PciQuirkFlags::from_bits_truncate(
    PciQuirkFlags::NEED_IOMMU.bits() | PciQuirkFlags::NO_ASPM.bits(),
);

pub const DMI_PCI_QUIRK_RULES: &[DmiPciQuirkRule] = &[
    DmiPciQuirkRule {
        dmi_match: DmiMatchRule {
            sys_vendor: Some(Cow::Borrowed("LENOVO")),
            product_name: Some(Cow::Borrowed("ThinkPad X1 Carbon")),
            board_vendor: None,
            board_name: None,
            board_version: None,
            product_version: None,
            bios_version: None,
        },
        vendor: 0x8086,
        device: PCI_QUIRK_ANY_ID,
        flags: PciQuirkFlags::NO_ASPM,
    },
    DmiPciQuirkRule {
        dmi_match: DmiMatchRule {
            sys_vendor: Some(Cow::Borrowed("Dell Inc.")),
            product_name: Some(Cow::Borrowed("OptiPlex 7090")),
            board_vendor: None,
            board_name: None,
            board_version: None,
            product_version: None,
            bios_version: None,
        },
        vendor: PCI_QUIRK_ANY_ID,
        device: PCI_QUIRK_ANY_ID,
        flags: PciQuirkFlags::NO_MSI,
    },
    DmiPciQuirkRule {
        dmi_match: DmiMatchRule {
            sys_vendor: Some(Cow::Borrowed("Dell Inc.")),
            product_name: Some(Cow::Borrowed("PowerEdge R740")),
            board_vendor: None,
            board_name: None,
            board_version: None,
            product_version: None,
            bios_version: None,
        },
        vendor: 0x14E4,
        device: PCI_QUIRK_ANY_ID,
        flags: F_NO_MSIX_NO_ASPM,
    },
    DmiPciQuirkRule {
        dmi_match: DmiMatchRule {
            sys_vendor: Some(Cow::Borrowed("HP")),
            product_name: Some(Cow::Borrowed("HP ProBook")),
            board_vendor: None,
            board_name: None,
            board_version: None,
            product_version: None,
            bios_version: None,
        },
        vendor: 0x8086,
        device: PCI_QUIRK_ANY_ID,
        flags: PciQuirkFlags::NO_D3COLD,
    },
    DmiPciQuirkRule {
        dmi_match: DmiMatchRule {
            sys_vendor: Some(Cow::Borrowed("ASUSTeK COMPUTER INC.")),
            board_name: Some(Cow::Borrowed("PRIME X570-PRO")),
            board_vendor: None,
            board_version: None,
            product_name: None,
            product_version: None,
            bios_version: None,
        },
        vendor: 0x1002,
        device: PCI_QUIRK_ANY_ID,
        flags: F_NO_ASPM_NEED_FW,
    },
    DmiPciQuirkRule {
        dmi_match: DmiMatchRule {
            sys_vendor: Some(Cow::Borrowed("Framework")),
            product_name: Some(Cow::Borrowed("Laptop 16")),
            board_vendor: None,
            board_name: None,
            board_version: None,
            product_version: None,
            bios_version: None,
        },
        vendor: 0x1002,
        device: PCI_QUIRK_ANY_ID,
        flags: F_NEED_IOMMU_NO_ASPM,
    },
    DmiPciQuirkRule {
        dmi_match: DmiMatchRule {
            sys_vendor: Some(Cow::Borrowed("Microsoft Corporation")),
            product_name: Some(Cow::Borrowed("Surface Pro")),
            board_vendor: None,
            board_name: None,
            board_version: None,
            product_version: None,
            bios_version: None,
        },
        vendor: PCI_QUIRK_ANY_ID,
        device: PCI_QUIRK_ANY_ID,
        flags: PciQuirkFlags::NO_USB3,
    },
    DmiPciQuirkRule {
        dmi_match: DmiMatchRule {
            sys_vendor: Some(Cow::Borrowed("Gigabyte Technology Co., Ltd.")),
            board_name: Some(Cow::Borrowed("X570 AORUS MASTER")),
            board_vendor: None,
            board_version: None,
            product_name: None,
            product_version: None,
            bios_version: None,
        },
        vendor: 0x1022,
        device: PCI_QUIRK_ANY_ID,
        flags: PciQuirkFlags::RESET_DELAY_MS,
    },
];

pub(crate) fn apply_dmi_pci_quirk_rules(
    info: &PciDeviceInfo,
    dmi_info: Option<&DmiInfo>,
    rules: &[DmiPciQuirkRule],
) -> PciQuirkFlags {
    let Some(dmi_info) = dmi_info else {
        return PciQuirkFlags::empty();
    };

    let mut flags = PciQuirkFlags::empty();
    for rule in rules {
        if !rule.dmi_match.matches(dmi_info) {
            continue;
        }
        if rule.vendor != super::PCI_QUIRK_ANY_ID && rule.vendor != info.vendor_id {
            continue;
        }
        if rule.device != super::PCI_QUIRK_ANY_ID && rule.device != info.device_id {
            continue;
        }
        flags |= rule.flags;
    }

    flags
}

/// Look up DMI-based PCI quirks for the given device.
///
/// Checks if the current system matches any DMI rules and if so, applies
/// PCI quirk flags to matching devices.
pub fn load_dmi_pci_quirks(info: &PciDeviceInfo) -> Result<PciQuirkFlags, ()> {
    let dmi_info = read_dmi_info()?;

    let mut flags = apply_dmi_pci_quirk_rules(info, Some(&dmi_info), DMI_PCI_QUIRK_RULES);

    if let Ok(toml_flags) = toml_loader::load_dmi_pci_quirks(info, &dmi_info) {
        flags |= toml_flags;
    }

    Ok(flags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dmi_match_all_fields() {
        let rule = DmiMatchRule {
            sys_vendor: Some(Cow::Borrowed("Framework")),
            product_name: Some(Cow::Borrowed("Laptop 16")),
            board_vendor: None,
            board_name: None,
            board_version: None,
            product_version: None,
            bios_version: None,
        };
        let info = DmiInfo {
            sys_vendor: Some("Framework".to_string()),
            product_name: Some("Laptop 16".to_string()),
            board_name: Some("FRANMECP01".to_string()),
            board_vendor: None,
            board_version: None,
            product_version: None,
            bios_version: None,
        };
        assert!(rule.matches(&info));
    }

    #[test]
    fn dmi_no_match_wrong_vendor() {
        let rule = DmiMatchRule {
            sys_vendor: Some(Cow::Borrowed("Framework")),
            board_vendor: None,
            board_name: None,
            board_version: None,
            product_name: None,
            product_version: None,
            bios_version: None,
        };
        let info = DmiInfo {
            sys_vendor: Some("Lenovo".to_string()),
            board_vendor: None,
            board_name: None,
            board_version: None,
            product_name: None,
            product_version: None,
            bios_version: None,
        };
        assert!(!rule.matches(&info));
    }

    #[test]
    fn dmi_match_missing_field_fails() {
        let rule = DmiMatchRule {
            sys_vendor: Some(Cow::Borrowed("Framework")),
            board_vendor: None,
            board_name: None,
            board_version: None,
            product_name: None,
            product_version: None,
            bios_version: None,
        };
        let info = DmiInfo {
            sys_vendor: None,
            board_vendor: None,
            board_name: None,
            board_version: None,
            product_name: None,
            product_version: None,
            bios_version: None,
        };
        assert!(!rule.matches(&info));
    }

    #[test]
    fn apply_dmi_rules_requires_live_dmi_info() {
        let info = PciDeviceInfo {
            location: crate::pci::PciLocation {
                segment: 0,
                bus: 0,
                device: 0,
                function: 0,
            },
            vendor_id: 0x1002,
            device_id: 0x73BF,
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
        };
        let rules = [DmiPciQuirkRule {
            dmi_match: DmiMatchRule {
                sys_vendor: Some(Cow::Borrowed("Framework")),
                product_name: Some(Cow::Borrowed("Laptop 16")),
                board_vendor: None,
                board_name: None,
                board_version: None,
                product_version: None,
                bios_version: None,
            },
            vendor: 0x1002,
            device: PCI_QUIRK_ANY_ID,
            flags: PciQuirkFlags::DISABLE_ACCEL,
        }];

        let flags = apply_dmi_pci_quirk_rules(&info, None, &rules);
        assert!(flags.is_empty());
    }
}
