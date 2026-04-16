use super::{UsbQuirkEntry, UsbQuirkFlags, PCI_QUIRK_ANY_ID};

const F_NO_SUSP_RESET: UsbQuirkFlags = UsbQuirkFlags::from_bits_truncate(
    UsbQuirkFlags::NO_SUSPEND.bits() | UsbQuirkFlags::NEED_RESET.bits(),
);
const F_BAD_DESC_NO_CFG: UsbQuirkFlags = UsbQuirkFlags::from_bits_truncate(
    UsbQuirkFlags::BAD_DESCRIPTOR.bits() | UsbQuirkFlags::NO_SET_CONFIG.bits(),
);

pub const USB_QUIRK_TABLE: &[UsbQuirkEntry] = &[
    UsbQuirkEntry {
        vendor: 0x0BDA,
        product: 0x8153,
        flags: UsbQuirkFlags::NO_STRING_FETCH,
    },
    UsbQuirkEntry {
        vendor: 0x0BDA,
        product: 0x8156,
        flags: UsbQuirkFlags::NO_STRING_FETCH,
    },
    UsbQuirkEntry {
        vendor: 0x1A40,
        product: 0x0101,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x2109,
        product: 0x2813,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x2109,
        product: 0x0815,
        flags: UsbQuirkFlags::NO_U1U2,
    },
    UsbQuirkEntry {
        vendor: 0x8087,
        product: 0x0025,
        flags: UsbQuirkFlags::NO_SUSPEND,
    },
    UsbQuirkEntry {
        vendor: 0x8087,
        product: 0x0A2B,
        flags: F_NO_SUSP_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x0A12,
        product: PCI_QUIRK_ANY_ID,
        flags: F_BAD_DESC_NO_CFG,
    },
];
