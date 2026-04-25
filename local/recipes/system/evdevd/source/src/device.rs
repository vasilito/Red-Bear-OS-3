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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        ABS_MT_POSITION_X, ABS_MT_POSITION_Y, ABS_MT_SLOT, ABS_MT_TOUCH_MAJOR, ABS_MT_TRACKING_ID,
        ABS_PRESSURE, ABS_X, ABS_Y, BTN_LEFT, BTN_TOUCH, BUS_VIRTUAL, EV_ABS, EV_KEY, EV_LED,
        EV_MSC, EV_REL, EV_REP, EV_SYN, INPUT_PROP_POINTER, KEY_A, LED_CAPSL, LED_NUML,
        LED_SCROLLL, MSC_SCAN, REL_HWHEEL, REL_WHEEL, REL_X, REL_Y,
    };

    // Helper: check that a bitmap has the expected bit set for a given code.
    fn assert_bit_set(bitmap: &[u8], code: u16) {
        let byte = (code / 8) as usize;
        let bit = code % 8;
        assert!(
            byte < bitmap.len(),
            "code {} (byte index {}) out of bitmap range (len {})",
            code,
            byte,
            bitmap.len()
        );
        assert_eq!(
            bitmap[byte] & (1 << bit),
            1 << bit,
            "bit {} in byte {} not set for code {}",
            bit,
            byte,
            code
        );
    }

    fn assert_bit_clear(bitmap: &[u8], code: u16) {
        let byte = (code / 8) as usize;
        let bit = code % 8;
        if byte < bitmap.len() {
            assert_eq!(
                bitmap[byte] & (1 << bit),
                0,
                "bit {} in byte {} unexpectedly set for code {}",
                bit,
                byte,
                code
            );
        }
    }

    // ---------------------------------------------------------------
    // 1. InputDevice::new_keyboard
    // ---------------------------------------------------------------
    #[test]
    fn new_keyboard_has_correct_kind() {
        let dev = InputDevice::new_keyboard(3);
        assert_eq!(dev.kind, DeviceKind::Keyboard);
    }

    #[test]
    fn new_keyboard_name_format() {
        let dev = InputDevice::new_keyboard(7);
        assert_eq!(dev.name, "Redox Keyboard 7");
    }

    #[test]
    fn new_keyboard_input_id_bus_virtual() {
        let dev = InputDevice::new_keyboard(1);
        assert_eq!(dev.input_id.bustype, BUS_VIRTUAL);
        assert_eq!(dev.input_id.vendor, 0);
        assert_eq!(dev.input_id.product, 1);
        assert_eq!(dev.input_id.version, 1);
    }

    #[test]
    fn new_keyboard_key_and_led_state_zeroed() {
        let dev = InputDevice::new_keyboard(0);
        assert!(dev.key_state.iter().all(|&b| b == 0));
        assert!(dev.led_state.iter().all(|&b| b == 0));
    }

    // ---------------------------------------------------------------
    // 2. InputDevice::new_mouse
    // ---------------------------------------------------------------
    #[test]
    fn new_mouse_has_correct_kind() {
        let dev = InputDevice::new_mouse(2);
        assert_eq!(dev.kind, DeviceKind::Mouse);
    }

    #[test]
    fn new_mouse_name_format() {
        let dev = InputDevice::new_mouse(5);
        assert_eq!(dev.name, "Redox Mouse 5");
    }

    #[test]
    fn new_mouse_product_id_offset() {
        let dev = InputDevice::new_mouse(3);
        assert_eq!(dev.input_id.product, 3 + 0x10);
        assert_eq!(dev.input_id.bustype, BUS_VIRTUAL);
        assert_eq!(dev.input_id.version, 1);
    }

    #[test]
    fn new_mouse_key_and_led_state_zeroed() {
        let dev = InputDevice::new_mouse(0);
        assert!(dev.key_state.iter().all(|&b| b == 0));
        assert!(dev.led_state.iter().all(|&b| b == 0));
    }

    // ---------------------------------------------------------------
    // 3. InputDevice::new_touchpad
    // ---------------------------------------------------------------
    #[test]
    fn new_touchpad_has_correct_kind() {
        let dev = InputDevice::new_touchpad(1);
        assert_eq!(dev.kind, DeviceKind::Touchpad);
    }

    #[test]
    fn new_touchpad_name_format() {
        let dev = InputDevice::new_touchpad(4);
        assert_eq!(dev.name, "Redox Touchpad 4");
    }

    #[test]
    fn new_touchpad_product_id_offset() {
        let dev = InputDevice::new_touchpad(2);
        assert_eq!(dev.input_id.product, 2 + 0x20);
        assert_eq!(dev.input_id.bustype, BUS_VIRTUAL);
        assert_eq!(dev.input_id.version, 1);
    }

    // ---------------------------------------------------------------
    // 4. supported_event_types
    // ---------------------------------------------------------------
    #[test]
    fn keyboard_event_types() {
        let dev = InputDevice::new_keyboard(0);
        let bm = dev.supported_event_types();
        assert_bit_set(&bm, EV_SYN);
        assert_bit_set(&bm, EV_KEY);
        assert_bit_set(&bm, EV_MSC);
        assert_bit_set(&bm, EV_LED);
        assert_bit_set(&bm, EV_REP);
    }

    #[test]
    fn mouse_event_types() {
        let dev = InputDevice::new_mouse(0);
        let bm = dev.supported_event_types();
        assert_bit_set(&bm, EV_SYN);
        assert_bit_set(&bm, EV_KEY);
        assert_bit_set(&bm, EV_REL);
        // No EV_ABS for mouse
        assert_bit_clear(&bm, EV_ABS);
    }

    #[test]
    fn touchpad_event_types() {
        let dev = InputDevice::new_touchpad(0);
        let bm = dev.supported_event_types();
        assert_bit_set(&bm, EV_SYN);
        assert_bit_set(&bm, EV_KEY);
        assert_bit_set(&bm, EV_ABS);
        // No EV_REL for touchpad
        assert_bit_clear(&bm, EV_REL);
    }

    // ---------------------------------------------------------------
    // 5. supported_keys
    // ---------------------------------------------------------------
    #[test]
    fn keyboard_has_key_a() {
        let dev = InputDevice::new_keyboard(0);
        let bm = dev.supported_keys();
        assert_bit_set(&bm, KEY_A);
    }

    #[test]
    fn mouse_has_btn_left() {
        let dev = InputDevice::new_mouse(0);
        let bm = dev.supported_keys();
        assert_bit_set(&bm, BTN_LEFT);
    }

    #[test]
    fn touchpad_has_btn_touch() {
        let dev = InputDevice::new_touchpad(0);
        let bm = dev.supported_keys();
        assert_bit_set(&bm, BTN_TOUCH);
    }

    // ---------------------------------------------------------------
    // 6. supported_rel
    // ---------------------------------------------------------------
    #[test]
    fn mouse_rel_axes() {
        let dev = InputDevice::new_mouse(0);
        let bm = dev.supported_rel();
        assert_bit_set(&bm, REL_X);
        assert_bit_set(&bm, REL_Y);
        assert_bit_set(&bm, REL_WHEEL);
        assert_bit_set(&bm, REL_HWHEEL);
    }

    #[test]
    fn keyboard_rel_empty() {
        let dev = InputDevice::new_keyboard(0);
        assert!(dev.supported_rel().is_empty());
    }

    #[test]
    fn touchpad_rel_empty() {
        let dev = InputDevice::new_touchpad(0);
        assert!(dev.supported_rel().is_empty());
    }

    // ---------------------------------------------------------------
    // 7. supported_abs
    // ---------------------------------------------------------------
    #[test]
    fn touchpad_abs_axes() {
        let dev = InputDevice::new_touchpad(0);
        let bm = dev.supported_abs();
        assert_bit_set(&bm, ABS_X);
        assert_bit_set(&bm, ABS_Y);
        assert_bit_set(&bm, ABS_PRESSURE);
        assert_bit_set(&bm, ABS_MT_SLOT);
        assert_bit_set(&bm, ABS_MT_TOUCH_MAJOR);
        assert_bit_set(&bm, ABS_MT_POSITION_X);
        assert_bit_set(&bm, ABS_MT_POSITION_Y);
        assert_bit_set(&bm, ABS_MT_TRACKING_ID);
    }

    #[test]
    fn keyboard_abs_empty() {
        let dev = InputDevice::new_keyboard(0);
        assert!(dev.supported_abs().is_empty());
    }

    #[test]
    fn mouse_abs_empty() {
        let dev = InputDevice::new_mouse(0);
        assert!(dev.supported_abs().is_empty());
    }

    // ---------------------------------------------------------------
    // 8. supported_msc
    // ---------------------------------------------------------------
    #[test]
    fn keyboard_msc_has_scan() {
        let dev = InputDevice::new_keyboard(0);
        let bm = dev.supported_msc();
        assert_bit_set(&bm, MSC_SCAN);
    }

    #[test]
    fn mouse_msc_empty() {
        let dev = InputDevice::new_mouse(0);
        assert!(dev.supported_msc().is_empty());
    }

    #[test]
    fn touchpad_msc_empty() {
        let dev = InputDevice::new_touchpad(0);
        assert!(dev.supported_msc().is_empty());
    }

    // ---------------------------------------------------------------
    // 9. supported_leds
    // ---------------------------------------------------------------
    #[test]
    fn keyboard_leds() {
        let dev = InputDevice::new_keyboard(0);
        let bm = dev.supported_leds();
        assert_bit_set(&bm, LED_NUML);
        assert_bit_set(&bm, LED_CAPSL);
        assert_bit_set(&bm, LED_SCROLLL);
    }

    #[test]
    fn mouse_leds_empty() {
        let dev = InputDevice::new_mouse(0);
        assert!(dev.supported_leds().is_empty());
    }

    #[test]
    fn touchpad_leds_empty() {
        let dev = InputDevice::new_touchpad(0);
        assert!(dev.supported_leds().is_empty());
    }

    // ---------------------------------------------------------------
    // 10. supported_props
    // ---------------------------------------------------------------
    #[test]
    fn mouse_has_pointer_prop() {
        let dev = InputDevice::new_mouse(0);
        let bm = dev.supported_props();
        assert_bit_set(&bm, INPUT_PROP_POINTER);
    }

    #[test]
    fn touchpad_has_pointer_prop() {
        let dev = InputDevice::new_touchpad(0);
        let bm = dev.supported_props();
        assert_bit_set(&bm, INPUT_PROP_POINTER);
    }

    #[test]
    fn keyboard_props_empty() {
        let dev = InputDevice::new_keyboard(0);
        assert!(dev.supported_props().is_empty());
    }

    // ---------------------------------------------------------------
    // 11. update_key_state
    // ---------------------------------------------------------------
    #[test]
    fn update_key_state_press_sets_bit() {
        let mut dev = InputDevice::new_keyboard(0);
        // KEY_A = 30 → byte 3, bit 6
        dev.update_key_state(KEY_A, true);
        let byte = (KEY_A / 8) as usize;
        let bit = KEY_A % 8;
        assert_eq!(dev.key_state[byte] & (1 << bit), 1 << bit);
    }

    #[test]
    fn update_key_state_release_clears_bit() {
        let mut dev = InputDevice::new_keyboard(0);
        dev.update_key_state(KEY_A, true);
        assert_bit_set(&dev.key_state, KEY_A);
        dev.update_key_state(KEY_A, false);
        assert_bit_clear(&dev.key_state, KEY_A);
    }

    // ---------------------------------------------------------------
    // 12. update_led_state
    // ---------------------------------------------------------------
    #[test]
    fn update_led_state_set_capsl() {
        let mut dev = InputDevice::new_keyboard(0);
        // LED_CAPSL = 1 → byte 0, bit 1
        dev.update_led_state(LED_CAPSL, true);
        let byte = (LED_CAPSL / 8) as usize;
        let bit = LED_CAPSL % 8;
        assert_eq!(dev.led_state[byte] & (1 << bit), 1 << bit);
    }

    #[test]
    fn update_led_state_clear_capsl() {
        let mut dev = InputDevice::new_keyboard(0);
        dev.update_led_state(LED_CAPSL, true);
        assert_bit_set(&dev.led_state, LED_CAPSL);
        dev.update_led_state(LED_CAPSL, false);
        assert_bit_clear(&dev.led_state, LED_CAPSL);
    }

    // ---------------------------------------------------------------
    // 13. abs_info default touchpad
    // ---------------------------------------------------------------
    #[test]
    fn touchpad_abs_x_range() {
        let dev = InputDevice::new_touchpad(0);
        let info = dev.abs_info(ABS_X);
        assert_eq!(info.minimum, 0);
        assert_eq!(info.maximum, 65_535);
    }

    #[test]
    fn touchpad_abs_pressure_max() {
        let dev = InputDevice::new_touchpad(0);
        let info = dev.abs_info(ABS_PRESSURE);
        assert_eq!(info.maximum, 255);
    }

    #[test]
    fn touchpad_abs_mt_slot_range() {
        let dev = InputDevice::new_touchpad(0);
        let info = dev.abs_info(ABS_MT_SLOT);
        assert_eq!(info.minimum, 0);
        assert_eq!(info.maximum, 9);
    }

    // ---------------------------------------------------------------
    // 14. set_abs_info overrides default
    // ---------------------------------------------------------------
    #[test]
    fn set_abs_info_override() {
        let mut dev = InputDevice::new_touchpad(0);
        let custom = AbsInfo {
            value: 42,
            minimum: -100,
            maximum: 100,
            fuzz: 1,
            flat: 2,
            resolution: 3,
        };
        dev.set_abs_info(ABS_X, custom);
        let info = dev.abs_info(ABS_X);
        assert_eq!(info.value, 42);
        assert_eq!(info.minimum, -100);
        assert_eq!(info.maximum, 100);
        assert_eq!(info.fuzz, 1);
        assert_eq!(info.flat, 2);
        assert_eq!(info.resolution, 3);
    }

    #[test]
    fn set_abs_info_does_not_affect_other_axes() {
        let mut dev = InputDevice::new_touchpad(0);
        let custom = AbsInfo {
            value: 0,
            minimum: -50,
            maximum: 50,
            ..AbsInfo::default()
        };
        dev.set_abs_info(ABS_X, custom);
        // ABS_Y should still return the default touchpad range
        let info_y = dev.abs_info(ABS_Y);
        assert_eq!(info_y.minimum, 0);
        assert_eq!(info_y.maximum, 65_535);
    }

    // ---------------------------------------------------------------
    // 15. bitmap_from_codes edge cases
    // ---------------------------------------------------------------
    #[test]
    fn bitmap_from_codes_empty_input() {
        let bm = bitmap_from_codes(&[]);
        assert!(bm.is_empty());
    }

    #[test]
    fn bitmap_from_codes_single_code() {
        let bm = bitmap_from_codes(&[5u16]);
        assert_eq!(bm.len(), 1); // (5/8)+1 = 1
        assert_eq!(bm[0], 1 << 5);
    }

    #[test]
    fn bitmap_from_codes_multiple_codes_same_byte() {
        // REL_X=0, REL_Y=1, REL_WHEEL=8, REL_HWHEEL=6
        // REL_X and REL_Y are in byte 0; REL_HWHEEL is in byte 0 too; REL_WHEEL is byte 1
        let bm = bitmap_from_codes(&[REL_X, REL_Y, REL_HWHEEL, REL_WHEEL]);
        assert_bit_set(&bm, REL_X);
        assert_bit_set(&bm, REL_Y);
        assert_bit_set(&bm, REL_WHEEL);
        assert_bit_set(&bm, REL_HWHEEL);
        // Byte 0 should have bits 0 (REL_X), 1 (REL_Y), 6 (REL_HWHEEL) set
        assert_eq!(bm[0], (1 << 0) | (1 << 1) | (1 << 6));
        // Byte 1 should have bit 0 (REL_WHEEL = 8 → byte 1, bit 0)
        assert_eq!(bm[1], 1 << 0);
    }
}
