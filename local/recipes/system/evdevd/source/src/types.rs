#![allow(dead_code)]

/// Linux-compatible evdev event types and constants.
///
/// These mirror the Linux kernel's `include/uapi/linux/input.h` definitions
/// so that clients expecting evdev semantics can work on Redox.
use std::mem::size_of;

// Event types
pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_REL: u16 = 0x02;
pub const EV_ABS: u16 = 0x03;
pub const EV_MSC: u16 = 0x04;
pub const EV_SW: u16 = 0x05;
pub const EV_LED: u16 = 0x11;
pub const EV_SND: u16 = 0x12;
pub const EV_REP: u16 = 0x14;
pub const EV_FF: u16 = 0x15;

// Synchronization events
pub const SYN_REPORT: u16 = 0;
pub const SYN_CONFIG: u16 = 1;

// Misc events
pub const MSC_SCAN: u16 = 0x04;

// Switch events
pub const SW_LID: u16 = 0x00;
pub const SW_TABLET_MODE: u16 = 0x01;
pub const SW_HEADPHONE_INSERT: u16 = 0x02;
pub const SW_RFKILL_ALL: u16 = 0x03;
pub const SW_MICROPHONE_INSERT: u16 = 0x04;
pub const SW_DOCK: u16 = 0x05;
pub const SW_LINEOUT_INSERT: u16 = 0x06;
pub const SW_JACK_PHYSICAL_INSERT: u16 = 0x07;
pub const SW_VIDEOOUT_INSERT: u16 = 0x08;
pub const SW_CAMERA_LENS_COVER: u16 = 0x09;
pub const SW_KEYPAD_SLIDE: u16 = 0x0a;
pub const SW_FRONT_PROXIMITY: u16 = 0x0b;
pub const SW_ROTATE_LOCK: u16 = 0x0c;
pub const SW_LINEIN_INSERT: u16 = 0x0d;
pub const SW_MUTE_DEVICE: u16 = 0x0e;
pub const SW_PEN_INSERTED: u16 = 0x0f;
pub const SW_MACHINE_COVER: u16 = 0x10;

// Input properties
pub const INPUT_PROP_POINTER: u16 = 0x00;
pub const INPUT_PROP_DIRECT: u16 = 0x01;

// LEDs
pub const LED_NUML: u16 = 0x00;
pub const LED_CAPSL: u16 = 0x01;
pub const LED_SCROLLL: u16 = 0x02;

// Repeat settings
pub const REP_DELAY: u16 = 0x00;
pub const REP_PERIOD: u16 = 0x01;

// Relative axes
pub const REL_X: u16 = 0x00;
pub const REL_Y: u16 = 0x01;
pub const REL_Z: u16 = 0x02;
pub const REL_WHEEL: u16 = 0x08;
pub const REL_HWHEEL: u16 = 0x06;

// Absolute axes
pub const ABS_X: u16 = 0x00;
pub const ABS_Y: u16 = 0x01;
pub const ABS_PRESSURE: u16 = 0x18;
pub const ABS_DISTANCE: u16 = 0x19;
pub const ABS_MT_SLOT: u16 = 0x2f;
pub const ABS_MT_TOUCH_MAJOR: u16 = 0x30;
pub const ABS_MT_POSITION_X: u16 = 0x35;
pub const ABS_MT_POSITION_Y: u16 = 0x36;
pub const ABS_MT_TRACKING_ID: u16 = 0x39;

// Keys and buttons
pub const KEY_RESERVED: u16 = 0;
pub const KEY_ESC: u16 = 1;
pub const KEY_1: u16 = 2;
pub const KEY_2: u16 = 3;
pub const KEY_3: u16 = 4;
pub const KEY_4: u16 = 5;
pub const KEY_5: u16 = 6;
pub const KEY_6: u16 = 7;
pub const KEY_7: u16 = 8;
pub const KEY_8: u16 = 9;
pub const KEY_9: u16 = 10;
pub const KEY_0: u16 = 11;
pub const KEY_MINUS: u16 = 12;
pub const KEY_EQUAL: u16 = 13;
pub const KEY_BACKSPACE: u16 = 14;
pub const KEY_TAB: u16 = 15;
pub const KEY_Q: u16 = 16;
pub const KEY_W: u16 = 17;
pub const KEY_E: u16 = 18;
pub const KEY_R: u16 = 19;
pub const KEY_T: u16 = 20;
pub const KEY_Y: u16 = 21;
pub const KEY_U: u16 = 22;
pub const KEY_I: u16 = 23;
pub const KEY_O: u16 = 24;
pub const KEY_P: u16 = 25;
pub const KEY_LEFTBRACE: u16 = 26;
pub const KEY_RIGHTBRACE: u16 = 27;
pub const KEY_ENTER: u16 = 28;
pub const KEY_LEFTCTRL: u16 = 29;
pub const KEY_A: u16 = 30;
pub const KEY_S: u16 = 31;
pub const KEY_D: u16 = 32;
pub const KEY_F: u16 = 33;
pub const KEY_G: u16 = 34;
pub const KEY_H: u16 = 35;
pub const KEY_J: u16 = 36;
pub const KEY_K: u16 = 37;
pub const KEY_L: u16 = 38;
pub const KEY_SEMICOLON: u16 = 39;
pub const KEY_APOSTROPHE: u16 = 40;
pub const KEY_GRAVE: u16 = 41;
pub const KEY_LEFTSHIFT: u16 = 42;
pub const KEY_BACKSLASH: u16 = 43;
pub const KEY_Z: u16 = 44;
pub const KEY_X: u16 = 45;
pub const KEY_C: u16 = 46;
pub const KEY_V: u16 = 47;
pub const KEY_B: u16 = 48;
pub const KEY_N: u16 = 49;
pub const KEY_M: u16 = 50;
pub const KEY_COMMA: u16 = 51;
pub const KEY_DOT: u16 = 52;
pub const KEY_SLASH: u16 = 53;
pub const KEY_RIGHTSHIFT: u16 = 54;
pub const KEY_KPASTERISK: u16 = 55;
pub const KEY_LEFTALT: u16 = 56;
pub const KEY_SPACE: u16 = 57;
pub const KEY_CAPSLOCK: u16 = 58;
pub const KEY_F1: u16 = 59;
pub const KEY_F2: u16 = 60;
pub const KEY_F3: u16 = 61;
pub const KEY_F4: u16 = 62;
pub const KEY_F5: u16 = 63;
pub const KEY_F6: u16 = 64;
pub const KEY_F7: u16 = 65;
pub const KEY_F8: u16 = 66;
pub const KEY_F9: u16 = 67;
pub const KEY_F10: u16 = 68;
pub const KEY_NUMLOCK: u16 = 69;
pub const KEY_SCROLLLOCK: u16 = 70;
pub const KEY_KP7: u16 = 71;
pub const KEY_KP8: u16 = 72;
pub const KEY_KP9: u16 = 73;
pub const KEY_KPMINUS: u16 = 74;
pub const KEY_KP4: u16 = 75;
pub const KEY_KP5: u16 = 76;
pub const KEY_KP6: u16 = 77;
pub const KEY_KPPLUS: u16 = 78;
pub const KEY_KP1: u16 = 79;
pub const KEY_KP2: u16 = 80;
pub const KEY_KP3: u16 = 81;
pub const KEY_KP0: u16 = 82;
pub const KEY_KPDOT: u16 = 83;
pub const KEY_F11: u16 = 87;
pub const KEY_F12: u16 = 88;
pub const KEY_KPENTER: u16 = 96;

pub const KEY_HOME: u16 = 102;
pub const KEY_UP: u16 = 103;
pub const KEY_PAGEUP: u16 = 104;
pub const KEY_LEFT: u16 = 105;
pub const KEY_RIGHT: u16 = 106;
pub const KEY_END: u16 = 107;
pub const KEY_DOWN: u16 = 108;
pub const KEY_PAGEDOWN: u16 = 109;
pub const KEY_INSERT: u16 = 110;
pub const KEY_DELETE: u16 = 111;
pub const KEY_KPSLASH: u16 = 98;
pub const KEY_MENU: u16 = 139;

pub const KEY_LEFTMETA: u16 = 125;
pub const KEY_RIGHTMETA: u16 = 126;
pub const KEY_RIGHTCTRL: u16 = 97;
pub const KEY_RIGHTALT: u16 = 100;

// Mouse buttons
pub const BTN_LEFT: u16 = 0x110;
pub const BTN_RIGHT: u16 = 0x111;
pub const BTN_MIDDLE: u16 = 0x112;
pub const BTN_SIDE: u16 = 0x113;
pub const BTN_EXTRA: u16 = 0x114;

// Touch
pub const BTN_TOUCH: u16 = 0x14a;
pub const BTN_TOOL_FINGER: u16 = 0x145;

// Bus types
pub const BUS_PCI: u16 = 0x01;
pub const BUS_USB: u16 = 0x03;
pub const BUS_VIRTUAL: u16 = 0x06;

// Evdev version
pub const EV_VERSION: i32 = 0x010001;

// ioctl constants
pub const EVIOCGVERSION: u64 = 0x80044501;
pub const EVIOCGID: u64 = 0x80084502;
pub const EVIOCGNAME: u64 = 0x80000000 | 0x45 << 8;
pub const EVIOCGBIT: u64 = 0x80000000 | 0x45 << 8;
pub const EVIOCGABS: u64 = 0x80184540;
pub const EVIOCSABS: u64 = 0x401845c0;
pub const EVIOCGRAB: u64 = 0x40044590;
pub const EVIOCSCLOCKID: u64 = 0x400445a0;
pub const EVIOCGPROP: u64 = 0x804045a0;
pub const EVIOCSFF: u64 = 0x402c4580;
pub const EVIOCRMFF: u64 = 0x40044581;
pub const EVIOCGEFFECTS: u64 = 0x80044584;

// EVIOCGKEY returns the current state of all keys (bitmask of pressed keys)
pub const EVIOCGKEY: u64 = (IOC_READ << IOC_DIRSHIFT)
    | ((KEY_MAX / 8 + 1) as u64) << IOC_SIZESHIFT
    | (EVDEV_IOCTL_TYPE << IOC_TYPESHIFT)
    | 0x18;

// EVIOCGLED returns the current state of all LEDs (bitmask of lit LEDs)
pub const EVIOCGLED: u64 = (IOC_READ << IOC_DIRSHIFT)
    | ((LED_MAX / 8 + 1) as u64) << IOC_SIZESHIFT
    | (EVDEV_IOCTL_TYPE << IOC_TYPESHIFT)
    | 0x19;

// Key and LED state bitmaps
pub const KEY_MAX: usize = 0x2FF; // 767 bits = 96 bytes
pub const LED_MAX: usize = 0x0F; // 15 bits = 2 bytes

pub const IOC_NRBITS: u64 = 8;
pub const IOC_TYPEBITS: u64 = 8;
pub const IOC_SIZEBITS: u64 = 14;

pub const IOC_NRMASK: u64 = (1 << IOC_NRBITS) - 1;
pub const IOC_TYPEMASK: u64 = (1 << IOC_TYPEBITS) - 1;
pub const IOC_SIZEMASK: u64 = (1 << IOC_SIZEBITS) - 1;

pub const IOC_NRSHIFT: u64 = 0;
pub const IOC_TYPESHIFT: u64 = IOC_NRSHIFT + IOC_NRBITS;
pub const IOC_SIZESHIFT: u64 = IOC_TYPESHIFT + IOC_TYPEBITS;
pub const IOC_DIRSHIFT: u64 = IOC_SIZESHIFT + IOC_SIZEBITS;

pub const IOC_NONE: u64 = 0;
pub const IOC_WRITE: u64 = 1;
pub const IOC_READ: u64 = 2;
pub const EVDEV_IOCTL_TYPE: u64 = 0x45;

pub const fn ioc_dir(cmd: u64) -> u64 {
    cmd >> IOC_DIRSHIFT
}

pub const fn ioc_type(cmd: u64) -> u64 {
    (cmd >> IOC_TYPESHIFT) & IOC_TYPEMASK
}

pub const fn ioc_nr(cmd: u64) -> u64 {
    (cmd >> IOC_NRSHIFT) & IOC_NRMASK
}

pub const fn ioc_size(cmd: u64) -> usize {
    ((cmd >> IOC_SIZESHIFT) & IOC_SIZEMASK) as usize
}

pub const fn is_evdev_ioctl(cmd: u64) -> bool {
    ioc_type(cmd) == EVDEV_IOCTL_TYPE
}

pub const fn eviocgname(len: usize) -> u64 {
    (IOC_READ << IOC_DIRSHIFT)
        | ((len as u64) << IOC_SIZESHIFT)
        | (EVDEV_IOCTL_TYPE << IOC_TYPESHIFT)
        | 0x06
}

pub const fn eviocgbit(ev: u8, len: usize) -> u64 {
    (IOC_READ << IOC_DIRSHIFT)
        | ((len as u64) << IOC_SIZESHIFT)
        | (EVDEV_IOCTL_TYPE << IOC_TYPESHIFT)
        | ((0x20 + ev as u64) << IOC_NRSHIFT)
}

pub const fn eviocgabs(axis: u8) -> u64 {
    (IOC_READ << IOC_DIRSHIFT)
        | ((size_of::<AbsInfo>() as u64) << IOC_SIZESHIFT)
        | (EVDEV_IOCTL_TYPE << IOC_TYPESHIFT)
        | ((0x40 + axis as u64) << IOC_NRSHIFT)
}

/// Linux `struct input_event` layout (24 bytes).
///
/// Matches the kernel binary layout:
///   struct input_event {
///       struct timeval time;  // 8 + 8 bytes (sec + usec, 64-bit each on x86_64)
///       __u16 type;
///       __u16 code;
///       __s32 value;
///   };
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct InputEvent {
    pub time_sec: u64,
    pub time_usec: u64,
    pub event_type: u16,
    pub code: u16,
    pub value: i32,
}

impl InputEvent {
    pub const SIZE: usize = 24;

    pub fn new(event_type: u16, code: u16, value: i32) -> Self {
        let (sec, usec) = now_timestamp();
        InputEvent {
            time_sec: sec,
            time_usec: usec,
            event_type,
            code,
            value,
        }
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0..8].copy_from_slice(&self.time_sec.to_le_bytes());
        buf[8..16].copy_from_slice(&self.time_usec.to_le_bytes());
        buf[16..18].copy_from_slice(&self.event_type.to_le_bytes());
        buf[18..20].copy_from_slice(&self.code.to_le_bytes());
        buf[20..24].copy_from_slice(&self.value.to_le_bytes());
        buf
    }

    pub fn syn_report() -> Self {
        Self::new(EV_SYN, SYN_REPORT, 0)
    }
}

/// Linux `struct input_id` layout (8 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct InputId {
    pub bustype: u16,
    pub vendor: u16,
    pub product: u16,
    pub version: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct AbsInfo {
    pub value: i32,
    pub minimum: i32,
    pub maximum: i32,
    pub fuzz: i32,
    pub flat: i32,
    pub resolution: i32,
}

fn now_timestamp() -> (u64, u64) {
    use std::time::SystemTime;
    let dur = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    (dur.as_secs(), dur.subsec_micros() as u64)
}
