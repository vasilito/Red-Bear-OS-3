use super::{PciQuirkEntry, PciQuirkFlags};

const F_NEED_FW_NO_D3: PciQuirkFlags = PciQuirkFlags::from_bits_truncate(
    PciQuirkFlags::NEED_FIRMWARE.bits() | PciQuirkFlags::NO_D3COLD.bits(),
);
const F_NEED_FW_NO_PM: PciQuirkFlags = PciQuirkFlags::from_bits_truncate(
    PciQuirkFlags::NEED_FIRMWARE.bits() | PciQuirkFlags::NO_PM.bits(),
);
const F_FULL_AMD_GPU: PciQuirkFlags = PciQuirkFlags::from_bits_truncate(
    PciQuirkFlags::NEED_FIRMWARE.bits()
        | PciQuirkFlags::NO_D3COLD.bits()
        | PciQuirkFlags::NO_ASPM.bits(),
);

pub const PCI_QUIRK_TABLE: &[PciQuirkEntry] = &[
    PciQuirkEntry {
        vendor: 0x1002,
        class_mask: 0xFF0000,
        class_match: 0x030000,
        flags: F_NEED_FW_NO_D3,
        ..PciQuirkEntry::WILDCARD
    },
    PciQuirkEntry {
        vendor: 0x1002,
        device: 0x7310,
        revision_lo: 0x00,
        revision_hi: 0x01,
        flags: PciQuirkFlags::NO_ASPM,
        ..PciQuirkEntry::WILDCARD
    },
    PciQuirkEntry {
        vendor: 0x1002,
        device: 0x73BF,
        flags: F_NEED_FW_NO_D3,
        ..PciQuirkEntry::WILDCARD
    },
    PciQuirkEntry {
        vendor: 0x1002,
        device: 0x7480,
        flags: F_FULL_AMD_GPU,
        ..PciQuirkEntry::WILDCARD
    },
    PciQuirkEntry {
        vendor: 0x8086,
        class_mask: 0xFF0000,
        class_match: 0x030000,
        revision_lo: 0x00,
        revision_hi: 0x04,
        flags: PciQuirkFlags::NO_MSI,
        ..PciQuirkEntry::WILDCARD
    },
    PciQuirkEntry {
        vendor: 0x8086,
        device: 0x46A6,
        flags: PciQuirkFlags::NO_MSI,
        ..PciQuirkEntry::WILDCARD
    },
    PciQuirkEntry {
        vendor: 0x8086,
        device: 0x2725,
        class_mask: 0xFF0000,
        class_match: 0x028000,
        flags: F_NEED_FW_NO_PM,
        ..PciQuirkEntry::WILDCARD
    },
    PciQuirkEntry {
        vendor: 0x8086,
        device: 0x51F0,
        class_mask: 0xFF0000,
        class_match: 0x028000,
        flags: F_NEED_FW_NO_PM,
        ..PciQuirkEntry::WILDCARD
    },
    PciQuirkEntry {
        vendor: 0x1022,
        device: 0x145C,
        flags: PciQuirkFlags::NO_MSIX,
        ..PciQuirkEntry::WILDCARD
    },
    PciQuirkEntry {
        vendor: 0x1022,
        device: 0x1639,
        flags: PciQuirkFlags::NO_ASPM,
        ..PciQuirkEntry::WILDCARD
    },
    PciQuirkEntry {
        vendor: 0x1022,
        device: 0x1483,
        flags: PciQuirkFlags::NO_RESOURCE_RELOC,
        ..PciQuirkEntry::WILDCARD
    },
];
