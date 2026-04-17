use super::{UsbQuirkEntry, UsbQuirkFlags, PCI_QUIRK_ANY_ID};

const F_00: UsbQuirkFlags = UsbQuirkFlags::from_bits_truncate(
    UsbQuirkFlags::NEED_RESET.bits() | UsbQuirkFlags::NO_LPM.bits(),
);
const F_01: UsbQuirkFlags = UsbQuirkFlags::from_bits_truncate(
    UsbQuirkFlags::NO_LPM.bits() | UsbQuirkFlags::RESET_DELAY.bits(),
);
const F_02: UsbQuirkFlags = UsbQuirkFlags::from_bits_truncate(
    UsbQuirkFlags::DELAY_CTRL_MSG.bits() | UsbQuirkFlags::RESET_DELAY.bits(),
);
const F_03: UsbQuirkFlags = UsbQuirkFlags::from_bits_truncate(
    UsbQuirkFlags::NO_SUSPEND.bits() | UsbQuirkFlags::NEED_RESET.bits(),
);

pub const USB_QUIRK_TABLE: &[UsbQuirkEntry] = &[
    UsbQuirkEntry {
        vendor: 0x0204,
        product: 0x6025,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x0218,
        product: 0x0201,
        flags: UsbQuirkFlags::CONFIG_INTF_STRINGS,
    },
    UsbQuirkEntry {
        vendor: 0x0218,
        product: 0x0401,
        flags: UsbQuirkFlags::CONFIG_INTF_STRINGS,
    },
    UsbQuirkEntry {
        vendor: 0x03F0,
        product: 0x0701,
        flags: UsbQuirkFlags::NO_STRING_FETCH,
    },
    UsbQuirkEntry {
        vendor: 0x03F0,
        product: 0x3F40,
        flags: UsbQuirkFlags::RESET_DELAY,
    },
    UsbQuirkEntry {
        vendor: 0x03F0,
        product: 0xA31D,
        flags: UsbQuirkFlags::NO_SUSPEND,
    },
    UsbQuirkEntry {
        vendor: 0x041E,
        product: 0x3020,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x0424,
        product: 0x3503,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x045E,
        product: 0x00E1,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x045E,
        product: 0x0770,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x045E,
        product: 0x07C6,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x046A,
        product: 0x0023,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x046D,
        product: 0x0825,
        flags: F_00,
    },
    UsbQuirkEntry {
        vendor: 0x046D,
        product: 0x082D,
        flags: UsbQuirkFlags::RESET_DELAY,
    },
    UsbQuirkEntry {
        vendor: 0x046D,
        product: 0x0841,
        flags: UsbQuirkFlags::RESET_DELAY,
    },
    UsbQuirkEntry {
        vendor: 0x046D,
        product: 0x0843,
        flags: UsbQuirkFlags::RESET_DELAY,
    },
    UsbQuirkEntry {
        vendor: 0x046D,
        product: 0x085B,
        flags: UsbQuirkFlags::RESET_DELAY,
    },
    UsbQuirkEntry {
        vendor: 0x046D,
        product: 0x085C,
        flags: UsbQuirkFlags::RESET_DELAY,
    },
    UsbQuirkEntry {
        vendor: 0x046D,
        product: 0x0847,
        flags: UsbQuirkFlags::RESET_DELAY,
    },
    UsbQuirkEntry {
        vendor: 0x046D,
        product: 0x0848,
        flags: UsbQuirkFlags::RESET_DELAY,
    },
    UsbQuirkEntry {
        vendor: 0x046D,
        product: 0x0853,
        flags: UsbQuirkFlags::RESET_DELAY,
    },
    UsbQuirkEntry {
        vendor: 0x046D,
        product: 0x086C,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x046D,
        product: 0x08C1,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x046D,
        product: 0x08C2,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x046D,
        product: 0x08C3,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x046D,
        product: 0x08C5,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x046D,
        product: 0x08C6,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x046D,
        product: 0x08C7,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x046D,
        product: 0xC122,
        flags: UsbQuirkFlags::RESET_DELAY,
    },
    UsbQuirkEntry {
        vendor: 0x0471,
        product: 0x0155,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x047F,
        product: 0xC008,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x047F,
        product: 0xC013,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x04B4,
        product: 0x0526,
        flags: UsbQuirkFlags::CONFIG_INTF_STRINGS,
    },
    UsbQuirkEntry {
        vendor: 0x04D8,
        product: 0x000C,
        flags: UsbQuirkFlags::CONFIG_INTF_STRINGS,
    },
    UsbQuirkEntry {
        vendor: 0x04E7,
        product: 0x0009,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x04E7,
        product: 0x0030,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x04E8,
        product: 0x6601,
        flags: UsbQuirkFlags::CONFIG_INTF_STRINGS,
    },
    UsbQuirkEntry {
        vendor: 0x04F3,
        product: 0x0089,
        flags: UsbQuirkFlags::DEVICE_QUALIFIER,
    },
    UsbQuirkEntry {
        vendor: 0x04F3,
        product: 0x009B,
        flags: UsbQuirkFlags::DEVICE_QUALIFIER,
    },
    UsbQuirkEntry {
        vendor: 0x04F3,
        product: 0x010C,
        flags: UsbQuirkFlags::DEVICE_QUALIFIER,
    },
    UsbQuirkEntry {
        vendor: 0x04F3,
        product: 0x0125,
        flags: UsbQuirkFlags::DEVICE_QUALIFIER,
    },
    UsbQuirkEntry {
        vendor: 0x04F3,
        product: 0x016F,
        flags: UsbQuirkFlags::DEVICE_QUALIFIER,
    },
    UsbQuirkEntry {
        vendor: 0x04F3,
        product: 0x0381,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x04F3,
        product: 0x21B8,
        flags: UsbQuirkFlags::DEVICE_QUALIFIER,
    },
    UsbQuirkEntry {
        vendor: 0x0582,
        product: 0x0007,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x0582,
        product: 0x0027,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x058F,
        product: 0x9254,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x05AC,
        product: 0x021A,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x05E3,
        product: 0x0612,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x05CC,
        product: 0x2267,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x05E3,
        product: 0x0616,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x0638,
        product: 0x0A13,
        flags: UsbQuirkFlags::NO_STRING_FETCH,
    },
    UsbQuirkEntry {
        vendor: 0x067B,
        product: 0x2731,
        flags: F_01,
    },
    UsbQuirkEntry {
        vendor: 0x06A3,
        product: 0x0006,
        flags: UsbQuirkFlags::CONFIG_INTF_STRINGS,
    },
    UsbQuirkEntry {
        vendor: 0x06BD,
        product: 0x0001,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x06F8,
        product: 0x0804,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x06F8,
        product: 0x3005,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x06F8,
        product: 0xB000,
        flags: UsbQuirkFlags::ENDPOINT_IGNORE,
    },
    UsbQuirkEntry {
        vendor: 0x0763,
        product: 0x0192,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x0781,
        product: 0x5583,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x0781,
        product: 0x5591,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x0781,
        product: 0x5596,
        flags: UsbQuirkFlags::RESET_DELAY,
    },
    UsbQuirkEntry {
        vendor: 0x0781,
        product: 0x55A3,
        flags: UsbQuirkFlags::RESET_DELAY,
    },
    UsbQuirkEntry {
        vendor: 0x0781,
        product: 0x55AE,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x07CA,
        product: 0x2553,
        flags: UsbQuirkFlags::NO_BOS,
    },
    UsbQuirkEntry {
        vendor: 0x0853,
        product: 0x011B,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x08EC,
        product: 0x1000,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x0904,
        product: 0x6101,
        flags: UsbQuirkFlags::LINEAR_FRAME_BINTERVAL,
    },
    UsbQuirkEntry {
        vendor: 0x0904,
        product: 0x6102,
        flags: UsbQuirkFlags::LINEAR_FRAME_BINTERVAL,
    },
    UsbQuirkEntry {
        vendor: 0x0904,
        product: 0x6103,
        flags: UsbQuirkFlags::LINEAR_FRAME_BINTERVAL,
    },
    UsbQuirkEntry {
        vendor: 0x090C,
        product: 0x1000,
        flags: UsbQuirkFlags::RESET_DELAY,
    },
    UsbQuirkEntry {
        vendor: 0x090C,
        product: 0x2000,
        flags: UsbQuirkFlags::RESET_DELAY,
    },
    UsbQuirkEntry {
        vendor: 0x0926,
        product: 0x0202,
        flags: UsbQuirkFlags::ENDPOINT_IGNORE,
    },
    UsbQuirkEntry {
        vendor: 0x0926,
        product: 0x0208,
        flags: UsbQuirkFlags::ENDPOINT_IGNORE,
    },
    UsbQuirkEntry {
        vendor: 0x0926,
        product: 0x3333,
        flags: UsbQuirkFlags::CONFIG_INTF_STRINGS,
    },
    UsbQuirkEntry {
        vendor: 0x0951,
        product: 0x1666,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x0930,
        product: 0x1408,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x0955,
        product: 0x7018,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x0955,
        product: 0x7019,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x0955,
        product: 0x7418,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x0955,
        product: 0x7721,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x0955,
        product: 0x7C18,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x0955,
        product: 0x7E19,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x0955,
        product: 0x7F21,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x0971,
        product: 0x2000,
        flags: UsbQuirkFlags::NO_SET_INTF,
    },
    UsbQuirkEntry {
        vendor: 0x09A1,
        product: 0x0028,
        flags: UsbQuirkFlags::DELAY_CTRL_MSG,
    },
    UsbQuirkEntry {
        vendor: 0x0A5C,
        product: 0x2021,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x0A92,
        product: 0x0091,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x0B05,
        product: 0x17E0,
        flags: UsbQuirkFlags::IGNORE_REMOTE_WAKEUP,
    },
    UsbQuirkEntry {
        vendor: 0x0B05,
        product: 0x1AB9,
        flags: UsbQuirkFlags::NO_BOS,
    },
    UsbQuirkEntry {
        vendor: 0x0BDA,
        product: 0x0151,
        flags: UsbQuirkFlags::CONFIG_INTF_STRINGS,
    },
    UsbQuirkEntry {
        vendor: 0x0BDA,
        product: 0x0487,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x0BDA,
        product: 0x8153,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x0C45,
        product: 0x7056,
        flags: UsbQuirkFlags::IGNORE_REMOTE_WAKEUP,
    },
    UsbQuirkEntry {
        vendor: 0x0FD9,
        product: 0x009B,
        flags: UsbQuirkFlags::NO_BOS,
    },
    UsbQuirkEntry {
        vendor: 0x0FCE,
        product: 0x0DDE,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x10D6,
        product: 0x2200,
        flags: UsbQuirkFlags::NO_STRING_FETCH,
    },
    UsbQuirkEntry {
        vendor: 0x1235,
        product: 0x0061,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x1235,
        product: 0x8211,
        flags: UsbQuirkFlags::NO_SUSPEND,
    },
    UsbQuirkEntry {
        vendor: 0x12D1,
        product: 0x15BB,
        flags: UsbQuirkFlags::NO_SUSPEND,
    },
    UsbQuirkEntry {
        vendor: 0x12D1,
        product: 0x15C1,
        flags: UsbQuirkFlags::NO_SUSPEND,
    },
    UsbQuirkEntry {
        vendor: 0x12D1,
        product: 0x15C3,
        flags: UsbQuirkFlags::NO_SUSPEND,
    },
    UsbQuirkEntry {
        vendor: 0x1516,
        product: 0x8628,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x1532,
        product: 0x0116,
        flags: UsbQuirkFlags::BAD_DESCRIPTOR,
    },
    UsbQuirkEntry {
        vendor: 0x1532,
        product: 0x0E05,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x17EF,
        product: 0x1018,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x17EF,
        product: 0x1019,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x17EF,
        product: 0x720C,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x17EF,
        product: 0x721E,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x17EF,
        product: 0xA012,
        flags: UsbQuirkFlags::NO_SUSPEND,
    },
    UsbQuirkEntry {
        vendor: 0x17EF,
        product: 0xA387,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x1908,
        product: 0x1315,
        flags: UsbQuirkFlags::HONOR_BNUMINTERFACES,
    },
    UsbQuirkEntry {
        vendor: 0x1A0A,
        product: 0x0200,
        flags: UsbQuirkFlags::BAD_DESCRIPTOR,
    },
    UsbQuirkEntry {
        vendor: 0x1A40,
        product: 0x0101,
        flags: UsbQuirkFlags::HUB_SLOW_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x1B1C,
        product: 0x1B13,
        flags: F_02,
    },
    UsbQuirkEntry {
        vendor: 0x1B1C,
        product: 0x1B15,
        flags: F_02,
    },
    UsbQuirkEntry {
        vendor: 0x1B1C,
        product: 0x1B20,
        flags: F_02,
    },
    UsbQuirkEntry {
        vendor: 0x1B1C,
        product: 0x1B33,
        flags: UsbQuirkFlags::RESET_DELAY,
    },
    UsbQuirkEntry {
        vendor: 0x1B1C,
        product: 0x1B36,
        flags: UsbQuirkFlags::RESET_DELAY,
    },
    UsbQuirkEntry {
        vendor: 0x1B1C,
        product: 0x1B38,
        flags: F_02,
    },
    UsbQuirkEntry {
        vendor: 0x1BC3,
        product: 0x0003,
        flags: UsbQuirkFlags::NO_SET_INTF,
    },
    UsbQuirkEntry {
        vendor: 0x1C75,
        product: 0x0204,
        flags: UsbQuirkFlags::CONFIG_INTF_STRINGS,
    },
    UsbQuirkEntry {
        vendor: 0x1DE1,
        product: 0xC102,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x1EDB,
        product: 0xBD3B,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x1EDB,
        product: 0xBD4F,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x1F75,
        product: 0x0917,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x2040,
        product: 0x7200,
        flags: UsbQuirkFlags::CONFIG_INTF_STRINGS,
    },
    UsbQuirkEntry {
        vendor: 0x2109,
        product: 0x0711,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x2386,
        product: 0x3114,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x2386,
        product: 0x3119,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x2386,
        product: 0x350E,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x2B89,
        product: 0x5871,
        flags: UsbQuirkFlags::NO_BOS,
    },
    UsbQuirkEntry {
        vendor: 0x2C48,
        product: 0x0132,
        flags: UsbQuirkFlags::SHORT_SET_ADDR_TIMEOUT,
    },
    UsbQuirkEntry {
        vendor: 0x2CA3,
        product: 0x0031,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x2CE3,
        product: 0x9563,
        flags: UsbQuirkFlags::NO_LPM,
    },
    UsbQuirkEntry {
        vendor: 0x32ED,
        product: 0x0401,
        flags: UsbQuirkFlags::NO_BOS,
    },
    UsbQuirkEntry {
        vendor: 0x413C,
        product: 0xB062,
        flags: F_00,
    },
    UsbQuirkEntry {
        vendor: 0x4296,
        product: 0x7570,
        flags: UsbQuirkFlags::CONFIG_INTF_STRINGS,
    },
    UsbQuirkEntry {
        vendor: 0x5131,
        product: 0x2007,
        flags: UsbQuirkFlags::FORCE_ONE_CONFIG,
    },
    UsbQuirkEntry {
        vendor: 0x8086,
        product: 0xF1A5,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x17EF,
        product: 0x602E,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x093A,
        product: 0x2500,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x093A,
        product: 0x2510,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x093A,
        product: 0x2521,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x03F0,
        product: 0x2B4A,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x046D,
        product: 0xC05A,
        flags: UsbQuirkFlags::NEED_RESET,
    },
    UsbQuirkEntry {
        vendor: 0x8087,
        product: 0x0A2B,
        flags: F_03,
    },
];
