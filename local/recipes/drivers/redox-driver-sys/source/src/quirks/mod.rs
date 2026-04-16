//! Hardware quirks system for Red Bear OS.
//!
//! Provides data-driven device quirk tables inspired by Linux's PCI/USB/DMI
//! quirk infrastructure, adapted for Redox's userspace driver architecture.
//!
//! # Design
//!
//! Quirks are loaded in layers:
//! 1. **Compiled-in** (`pci_table`, `usb_table`): Critical quirks always available.
//! 2. **TOML files** (`toml_loader`): Extensible quirks from `/etc/quirks.d/*.toml`.
//! 3. **DMI rules** (`dmi`): System-level overrides matched by SMBIOS data.
//!
//! Each layer accumulates flags via bitwise OR, so broader rules can set
//! baseline flags and narrower rules add more.
//!
//! # Usage
//!
//! ```no_run
//! use redox_driver_sys::pci::PciDeviceInfo;
//! use redox_driver_sys::quirks::PciQuirkFlags;
//!
//! fn probe(info: &PciDeviceInfo) {
//!     let quirks = info.quirks();
//!     if quirks.contains(PciQuirkFlags::NO_MSIX) {
//!         // fall back to MSI or legacy IRQ
//!     }
//! }
//! ```

pub mod dmi;
pub mod pci_table;
pub mod toml_loader;
pub mod usb_table;

use crate::pci::PciDeviceInfo;

bitflags::bitflags! {
    /// Flags for PCI device quirks.
    ///
    /// Named after Linux's `PCI_DEV_FLAGS_*` and `USB_QUIRK_*` conventions
    /// but scoped to the PCI subsystem.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct PciQuirkFlags: u64 {
        const NO_MSI = 1 << 0;
        const NO_MSIX = 1 << 1;
        const FORCE_LEGACY_IRQ = 1 << 2;
        const NO_PM = 1 << 3;
        const NO_D3COLD = 1 << 4;
        const NO_ASPM = 1 << 5;
        const NEED_IOMMU = 1 << 6;
        const NO_IOMMU = 1 << 7;
        const DMA_32BIT_ONLY = 1 << 8;
        const RESIZE_BAR = 1 << 9;
        const DISABLE_BAR_SIZING = 1 << 10;
        const NEED_FIRMWARE = 1 << 11;
        const DISABLE_ACCEL = 1 << 12;
        const FORCE_VRAM_ONLY = 1 << 13;
        const NO_USB3 = 1 << 14;
        const RESET_DELAY_MS = 1 << 15;
        const NO_STRING_FETCH = 1 << 16;
        const BAD_EEPROM = 1 << 17;
        const BUS_MASTER_DELAY = 1 << 18;
        const WRONG_CLASS = 1 << 19;
        const BROKEN_BRIDGE = 1 << 20;
        const NO_RESOURCE_RELOC = 1 << 21;
    }
}

bitflags::bitflags! {
    /// Flags for USB device quirks.
    ///
    /// Mirrors Linux's `USB_QUIRK_*` defines.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct UsbQuirkFlags: u64 {
        const NO_STRING_FETCH = 1 << 0;
        const RESET_DELAY = 1 << 1;
        const NO_USB3 = 1 << 2;
        const NO_SET_CONFIG = 1 << 3;
        const NO_SUSPEND = 1 << 4;
        const NEED_RESET = 1 << 5;
        const BAD_DESCRIPTOR = 1 << 6;
        const NO_LPM = 1 << 7;
        const NO_U1U2 = 1 << 8;
    }
}

/// Wildcard value for PCI ID matching.
pub const PCI_QUIRK_ANY_ID: u16 = 0xFFFF;

/// Compiled-in PCI quirk entry. All matching entries' flags accumulate via OR.
#[derive(Clone, Copy, Debug)]
pub struct PciQuirkEntry {
    /// PCI vendor ID to match, or `PCI_QUIRK_ANY_ID` for any.
    pub vendor: u16,
    /// PCI device ID to match, or `PCI_QUIRK_ANY_ID` for any.
    pub device: u16,
    /// PCI subsystem vendor ID to match, or `PCI_QUIRK_ANY_ID` for any.
    pub subvendor: u16,
    /// PCI subsystem device ID to match, or `PCI_QUIRK_ANY_ID` for any.
    pub subdevice: u16,
    /// Bitmask applied to the 24-bit class code before comparison.
    pub class_mask: u32,
    /// 24-bit class code value to match after masking. Ignored when `class_mask` is 0.
    pub class_match: u32,
    /// Lower bound of revision ID (inclusive).
    pub revision_lo: u8,
    /// Upper bound of revision ID (inclusive).
    pub revision_hi: u8,
    /// Quirk flags to apply when this entry matches.
    pub flags: PciQuirkFlags,
}

impl PciQuirkEntry {
    /// Convenience constant for an all-wildcard entry (matches everything).
    ///
    /// Useful as a base when constructing entries with `..PciQuirkEntry::WILDCARD`.
    pub const WILDCARD: Self = Self {
        vendor: PCI_QUIRK_ANY_ID,
        device: PCI_QUIRK_ANY_ID,
        subvendor: PCI_QUIRK_ANY_ID,
        subdevice: PCI_QUIRK_ANY_ID,
        class_mask: 0,
        class_match: 0,
        revision_lo: 0x00,
        revision_hi: 0xFF,
        flags: PciQuirkFlags::empty(),
    };

    fn matches_with_subsystem(&self, info: &PciDeviceInfo, match_subsystem: bool) -> bool {
        if self.vendor != PCI_QUIRK_ANY_ID && self.vendor != info.vendor_id {
            return false;
        }
        if self.device != PCI_QUIRK_ANY_ID && self.device != info.device_id {
            return false;
        }
        if info.revision < self.revision_lo || info.revision > self.revision_hi {
            return false;
        }
        if self.class_mask != 0 {
            let class24 = ((info.class_code as u32) << 16)
                | ((info.subclass as u32) << 8)
                | (info.prog_if as u32);
            if (class24 & self.class_mask) != (self.class_match & self.class_mask) {
                return false;
            }
        }
        if match_subsystem {
            if self.subvendor != PCI_QUIRK_ANY_ID && self.subvendor != info.subsystem_vendor_id {
                return false;
            }
            if self.subdevice != PCI_QUIRK_ANY_ID && self.subdevice != info.subsystem_device_id {
                return false;
            }
        }

        true
    }

    /// Check whether this quirk entry matches the given PCI device info.
    pub fn matches(&self, info: &PciDeviceInfo) -> bool {
        self.matches_with_subsystem(info, true)
    }

    pub(crate) fn matches_toml(&self, info: &PciDeviceInfo) -> bool {
        self.matches_with_subsystem(info, true)
    }
}

impl Default for PciQuirkEntry {
    fn default() -> Self {
        Self::WILDCARD
    }
}

/// A single compiled-in USB quirk entry.
#[derive(Clone, Copy, Debug)]
pub struct UsbQuirkEntry {
    /// USB vendor ID to match, or `PCI_QUIRK_ANY_ID` for any.
    pub vendor: u16,
    /// USB product ID to match, or `PCI_QUIRK_ANY_ID` for any.
    pub product: u16,
    /// Quirk flags to apply when this entry matches.
    pub flags: UsbQuirkFlags,
}

impl UsbQuirkEntry {
    /// Convenience constant for an all-wildcard entry.
    pub const WILDCARD: Self = Self {
        vendor: PCI_QUIRK_ANY_ID,
        product: PCI_QUIRK_ANY_ID,
        flags: UsbQuirkFlags::empty(),
    };

    pub fn matches(&self, vendor: u16, product: u16) -> bool {
        (self.vendor == PCI_QUIRK_ANY_ID || self.vendor == vendor)
            && (self.product == PCI_QUIRK_ANY_ID || self.product == product)
    }
}

impl Default for UsbQuirkEntry {
    fn default() -> Self {
        Self::WILDCARD
    }
}

/// Look up accumulated PCI quirk flags for the given device.
///
/// Checks all available quirk sources in order:
/// 1. Compiled-in table (always available)
/// 2. TOML quirk files from `/etc/quirks.d/` (if filesystem is mounted)
/// 3. DMI-based system quirk overrides (if SMBIOS data is available)
///
/// All matching entries' flags are ORed together.
pub fn lookup_pci_quirks(info: &PciDeviceInfo) -> PciQuirkFlags {
    let mut flags = PciQuirkFlags::empty();

    // Layer 1: Compiled-in table
    for entry in pci_table::PCI_QUIRK_TABLE {
        if entry.matches(info) {
            flags |= entry.flags;
        }
    }

    // Layer 2: TOML quirk files (best-effort; may not be available early in boot)
    if let Ok(toml_flags) = toml_loader::load_pci_quirks(info) {
        flags |= toml_flags;
    }

    // Layer 3: DMI-based system quirks (best-effort)
    if let Ok(dmi_flags) = dmi::load_dmi_pci_quirks(info) {
        flags |= dmi_flags;
    }

    flags
}

/// Look up accumulated USB quirk flags for the given vendor/product pair.
pub fn lookup_usb_quirks(vendor: u16, product: u16) -> UsbQuirkFlags {
    let mut flags = UsbQuirkFlags::empty();

    for entry in usb_table::USB_QUIRK_TABLE {
        if entry.matches(vendor, product) {
            flags |= entry.flags;
        }
    }

    if let Ok(toml_flags) = toml_loader::load_usb_quirks(vendor, product) {
        flags |= toml_flags;
    }

    flags
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pci::{PciDeviceInfo, PciLocation};

    fn make_info(vendor: u16, device: u16, class: u8, subclass: u8, revision: u8) -> PciDeviceInfo {
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
            revision,
            class_code: class,
            subclass,
            prog_if: 0,
            header_type: 0,
            irq: None,
            bars: Vec::new(),
            capabilities: Vec::new(),
        }
    }

    #[test]
    fn wildcard_entry_matches_everything() {
        let entry = PciQuirkEntry {
            flags: PciQuirkFlags::NO_PM,
            ..PciQuirkEntry::WILDCARD
        };
        let info = make_info(0x8086, 0x1234, 0x03, 0x00, 0x01);
        assert!(entry.matches(&info));
    }

    #[test]
    fn vendor_only_match() {
        let entry = PciQuirkEntry {
            vendor: 0x1002,
            flags: PciQuirkFlags::NEED_FIRMWARE,
            ..PciQuirkEntry::WILDCARD
        };
        assert!(entry.matches(&make_info(0x1002, 0x73BF, 0x03, 0x00, 0x00)));
        assert!(!entry.matches(&make_info(0x8086, 0x46A6, 0x03, 0x00, 0x00)));
    }

    #[test]
    fn revision_range_match() {
        let entry = PciQuirkEntry {
            vendor: 0x8086,
            device: 0x46A6,
            revision_lo: 0x00,
            revision_hi: 0x04,
            flags: PciQuirkFlags::DISABLE_ACCEL,
            ..PciQuirkEntry::WILDCARD
        };
        assert!(entry.matches(&make_info(0x8086, 0x46A6, 0x03, 0x00, 0x03)));
        assert!(!entry.matches(&make_info(0x8086, 0x46A6, 0x03, 0x00, 0x05)));
    }

    #[test]
    fn class_mask_match() {
        let entry = PciQuirkEntry {
            vendor: 0x1002,
            class_mask: 0xFF0000,
            class_match: 0x030000,
            flags: PciQuirkFlags::NO_D3COLD,
            ..PciQuirkEntry::WILDCARD
        };
        assert!(entry.matches(&make_info(0x1002, 0x7310, 0x03, 0x00, 0x00)));
        assert!(!entry.matches(&make_info(0x1002, 0x7310, 0x02, 0x00, 0x00)));
    }

    #[test]
    fn flags_accumulate() {
        let table = &[
            PciQuirkEntry {
                vendor: 0x1002,
                flags: PciQuirkFlags::NEED_FIRMWARE,
                ..PciQuirkEntry::WILDCARD
            },
            PciQuirkEntry {
                vendor: 0x1002,
                device: 0x7310,
                flags: PciQuirkFlags::NO_MSIX,
                ..PciQuirkEntry::WILDCARD
            },
        ];

        let info = make_info(0x1002, 0x7310, 0x03, 0x00, 0x00);
        let mut flags = PciQuirkFlags::empty();
        for entry in table {
            if entry.matches(&info) {
                flags |= entry.flags;
            }
        }
        assert!(flags.contains(PciQuirkFlags::NEED_FIRMWARE));
        assert!(flags.contains(PciQuirkFlags::NO_MSIX));
    }

    #[test]
    fn usb_quirk_lookup_works() {
        let flags = lookup_usb_quirks(0x0000, 0x0000);
        assert!(!flags.contains(UsbQuirkFlags::NO_STRING_FETCH));
    }
}
