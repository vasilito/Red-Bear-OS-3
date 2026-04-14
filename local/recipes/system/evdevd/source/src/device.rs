use std::collections::BTreeMap;

use crate::translate::{KEYBOARD_KEY_CODES, MOUSE_BUTTON_CODES, TOUCHPAD_KEY_CODES};
use crate::types::{
    AbsInfo, InputId, ABS_MT_POSITION_X, ABS_MT_POSITION_Y, ABS_MT_SLOT, ABS_MT_TOUCH_MAJOR,
    ABS_MT_TRACKING_ID, ABS_PRESSURE, ABS_X, ABS_Y, BUS_VIRTUAL, EV_ABS, EV_KEY, EV_LED, EV_MSC,
    EV_REL, EV_REP, EV_SYN, INPUT_PROP_POINTER, KEY_MAX, LED_CAPSL, LED_MAX, LED_NUML, LED_SCROLLL,
    MSC_SCAN, REL_HWHEEL, REL_WHEEL, REL_X, REL_Y, REP_DELAY, REP_PERIOD,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DeviceKind {
    Keyboard,
    Mouse,
    Touchpad,
}

pub struct InputDevice {
    pub kind: DeviceKind,
    pub name: String,
    pub input_id: InputId,
    pub key_state: [u8; KEY_MAX / 8 + 1],
    pub led_state: [u8; LED_MAX / 8 + 1],
    pub custom_abs: BTreeMap<u16, AbsInfo>,
}

impl InputDevice {
    pub fn new_keyboard(id: usize) -> Self {
        InputDevice {
            kind: DeviceKind::Keyboard,
            name: format!("Redox Keyboard {}", id),
            input_id: InputId {
                bustype: BUS_VIRTUAL,
                vendor: 0,
                product: id as u16,
                version: 1,
            },
            key_state: [0u8; KEY_MAX / 8 + 1],
            led_state: [0u8; LED_MAX / 8 + 1],
            custom_abs: BTreeMap::new(),
        }
    }

    pub fn new_mouse(id: usize) -> Self {
        InputDevice {
            kind: DeviceKind::Mouse,
            name: format!("Redox Mouse {}", id),
            input_id: InputId {
                bustype: BUS_VIRTUAL,
                vendor: 0,
                product: (id + 0x10) as u16,
                version: 1,
            },
            key_state: [0u8; KEY_MAX / 8 + 1],
            led_state: [0u8; LED_MAX / 8 + 1],
            custom_abs: BTreeMap::new(),
        }
    }

    pub fn new_touchpad(id: usize) -> Self {
        InputDevice {
            kind: DeviceKind::Touchpad,
            name: format!("Redox Touchpad {}", id),
            input_id: InputId {
                bustype: BUS_VIRTUAL,
                vendor: 0,
                product: (id + 0x20) as u16,
                version: 1,
            },
            key_state: [0u8; KEY_MAX / 8 + 1],
            led_state: [0u8; LED_MAX / 8 + 1],
            custom_abs: BTreeMap::new(),
        }
    }

    pub fn supported_event_types(&self) -> Vec<u8> {
        match self.kind {
            DeviceKind::Keyboard => bitmap_from_codes(&[EV_SYN, EV_KEY, EV_MSC, EV_LED, EV_REP]),
            DeviceKind::Mouse => bitmap_from_codes(&[EV_SYN, EV_KEY, EV_REL]),
            DeviceKind::Touchpad => bitmap_from_codes(&[EV_SYN, EV_KEY, EV_ABS]),
        }
    }

    pub fn supported_keys(&self) -> Vec<u8> {
        match self.kind {
            DeviceKind::Keyboard => bitmap_from_codes(KEYBOARD_KEY_CODES),
            DeviceKind::Mouse => bitmap_from_codes(MOUSE_BUTTON_CODES),
            DeviceKind::Touchpad => bitmap_from_codes(TOUCHPAD_KEY_CODES),
        }
    }

    pub fn supported_rel(&self) -> Vec<u8> {
        match self.kind {
            DeviceKind::Mouse => bitmap_from_codes(&[REL_X, REL_Y, REL_WHEEL, REL_HWHEEL]),
            _ => Vec::new(),
        }
    }

    pub fn supported_abs(&self) -> Vec<u8> {
        match self.kind {
            DeviceKind::Touchpad => bitmap_from_codes(&[
                ABS_X,
                ABS_Y,
                ABS_PRESSURE,
                ABS_MT_SLOT,
                ABS_MT_TOUCH_MAJOR,
                ABS_MT_POSITION_X,
                ABS_MT_POSITION_Y,
                ABS_MT_TRACKING_ID,
            ]),
            _ => Vec::new(),
        }
    }

    pub fn supported_msc(&self) -> Vec<u8> {
        match self.kind {
            DeviceKind::Keyboard => bitmap_from_codes(&[MSC_SCAN]),
            _ => Vec::new(),
        }
    }

    pub fn supported_leds(&self) -> Vec<u8> {
        match self.kind {
            DeviceKind::Keyboard => bitmap_from_codes(&[LED_NUML, LED_CAPSL, LED_SCROLLL]),
            _ => Vec::new(),
        }
    }

    pub fn supported_rep(&self) -> Vec<u8> {
        match self.kind {
            DeviceKind::Keyboard => bitmap_from_codes(&[REP_DELAY, REP_PERIOD]),
            _ => Vec::new(),
        }
    }

    pub fn supported_props(&self) -> Vec<u8> {
        match self.kind {
            DeviceKind::Mouse | DeviceKind::Touchpad => bitmap_from_codes(&[INPUT_PROP_POINTER]),
            DeviceKind::Keyboard => Vec::new(),
        }
    }

    pub fn abs_info(&self, axis: u16) -> AbsInfo {
        if let Some(abs_info) = self.custom_abs.get(&axis) {
            return *abs_info;
        }

        if self.kind != DeviceKind::Touchpad {
            return AbsInfo::default();
        }

        match axis {
            ABS_X | ABS_MT_POSITION_X => AbsInfo {
                minimum: 0,
                maximum: 65_535,
                resolution: 1,
                ..AbsInfo::default()
            },
            ABS_Y | ABS_MT_POSITION_Y => AbsInfo {
                minimum: 0,
                maximum: 65_535,
                resolution: 1,
                ..AbsInfo::default()
            },
            ABS_PRESSURE => AbsInfo {
                minimum: 0,
                maximum: 255,
                resolution: 1,
                ..AbsInfo::default()
            },
            ABS_MT_TOUCH_MAJOR => AbsInfo {
                minimum: 0,
                maximum: 255,
                resolution: 1,
                ..AbsInfo::default()
            },
            ABS_MT_SLOT => AbsInfo {
                minimum: 0,
                maximum: 9,
                ..AbsInfo::default()
            },
            ABS_MT_TRACKING_ID => AbsInfo {
                minimum: 0,
                maximum: i32::MAX,
                ..AbsInfo::default()
            },
            _ => AbsInfo::default(),
        }
    }

    pub fn set_abs_info(&mut self, axis: u16, abs_info: AbsInfo) {
        self.custom_abs.insert(axis, abs_info);
    }

    pub fn update_key_state(&mut self, code: u16, pressed: bool) {
        let byte = (code / 8) as usize;
        let bit = code % 8;
        if byte < self.key_state.len() {
            if pressed {
                self.key_state[byte] |= 1 << bit;
            } else {
                self.key_state[byte] &= !(1 << bit);
            }
        }
    }

    pub fn update_led_state(&mut self, code: u16, lit: bool) {
        let byte = (code / 8) as usize;
        let bit = code % 8;
        if byte < self.led_state.len() {
            if lit {
                self.led_state[byte] |= 1 << bit;
            } else {
                self.led_state[byte] &= !(1 << bit);
            }
        }
    }
}

fn bitmap_from_codes(codes: &[u16]) -> Vec<u8> {
    let Some(max) = codes.iter().copied().max() else {
        return Vec::new();
    };

    let mut bitmap = vec![0u8; (usize::from(max) / 8) + 1];
    for &code in codes {
        let index = usize::from(code / 8);
        let bit = 1u8 << (code % 8);
        bitmap[index] |= bit;
    }
    bitmap
}
