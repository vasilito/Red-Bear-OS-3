use crate::types::*;

pub const KEYBOARD_KEY_CODES: &[u16] = &[
    KEY_ESC,
    KEY_1,
    KEY_2,
    KEY_3,
    KEY_4,
    KEY_5,
    KEY_6,
    KEY_7,
    KEY_8,
    KEY_9,
    KEY_0,
    KEY_MINUS,
    KEY_EQUAL,
    KEY_BACKSPACE,
    KEY_TAB,
    KEY_Q,
    KEY_W,
    KEY_E,
    KEY_R,
    KEY_T,
    KEY_Y,
    KEY_U,
    KEY_I,
    KEY_O,
    KEY_P,
    KEY_LEFTBRACE,
    KEY_RIGHTBRACE,
    KEY_ENTER,
    KEY_LEFTCTRL,
    KEY_A,
    KEY_S,
    KEY_D,
    KEY_F,
    KEY_G,
    KEY_H,
    KEY_J,
    KEY_K,
    KEY_L,
    KEY_SEMICOLON,
    KEY_APOSTROPHE,
    KEY_GRAVE,
    KEY_LEFTSHIFT,
    KEY_BACKSLASH,
    KEY_Z,
    KEY_X,
    KEY_C,
    KEY_V,
    KEY_B,
    KEY_N,
    KEY_M,
    KEY_COMMA,
    KEY_DOT,
    KEY_SLASH,
    KEY_RIGHTSHIFT,
    KEY_KPASTERISK,
    KEY_LEFTALT,
    KEY_SPACE,
    KEY_CAPSLOCK,
    KEY_F1,
    KEY_F2,
    KEY_F3,
    KEY_F4,
    KEY_F5,
    KEY_F6,
    KEY_F7,
    KEY_F8,
    KEY_F9,
    KEY_F10,
    KEY_NUMLOCK,
    KEY_SCROLLLOCK,
    KEY_KP7,
    KEY_KP8,
    KEY_KP9,
    KEY_KPMINUS,
    KEY_KP4,
    KEY_KP5,
    KEY_KP6,
    KEY_KPPLUS,
    KEY_KP1,
    KEY_KP2,
    KEY_KP3,
    KEY_KP0,
    KEY_KPDOT,
    KEY_F11,
    KEY_F12,
    KEY_KPENTER,
    KEY_RIGHTCTRL,
    KEY_KPSLASH,
    KEY_RIGHTALT,
    KEY_HOME,
    KEY_UP,
    KEY_PAGEUP,
    KEY_LEFT,
    KEY_RIGHT,
    KEY_END,
    KEY_DOWN,
    KEY_PAGEDOWN,
    KEY_INSERT,
    KEY_DELETE,
    KEY_LEFTMETA,
    KEY_RIGHTMETA,
    KEY_MENU,
];

pub const MOUSE_BUTTON_CODES: &[u16] = &[BTN_LEFT, BTN_RIGHT, BTN_MIDDLE];
pub const TOUCHPAD_KEY_CODES: &[u16] = &[BTN_TOUCH, BTN_TOOL_FINGER];

fn orb_key_to_evdev(scancode: u8) -> Option<u16> {
    Some(match scancode {
        0x01 => KEY_ESC,
        0x02 => KEY_1,
        0x03 => KEY_2,
        0x04 => KEY_3,
        0x05 => KEY_4,
        0x06 => KEY_5,
        0x07 => KEY_6,
        0x08 => KEY_7,
        0x09 => KEY_8,
        0x0A => KEY_9,
        0x0B => KEY_0,
        0x0C => KEY_MINUS,
        0x0D => KEY_EQUAL,
        0x0E => KEY_BACKSPACE,
        0x0F => KEY_TAB,
        0x10 => KEY_Q,
        0x11 => KEY_W,
        0x12 => KEY_E,
        0x13 => KEY_R,
        0x14 => KEY_T,
        0x15 => KEY_Y,
        0x16 => KEY_U,
        0x17 => KEY_I,
        0x18 => KEY_O,
        0x19 => KEY_P,
        0x1A => KEY_LEFTBRACE,
        0x1B => KEY_RIGHTBRACE,
        0x1C => KEY_ENTER,
        0x1D => KEY_LEFTCTRL,
        0x1E => KEY_A,
        0x1F => KEY_S,
        0x20 => KEY_D,
        0x21 => KEY_F,
        0x22 => KEY_G,
        0x23 => KEY_H,
        0x24 => KEY_J,
        0x25 => KEY_K,
        0x26 => KEY_L,
        0x27 => KEY_SEMICOLON,
        0x28 => KEY_APOSTROPHE,
        0x29 => KEY_GRAVE,
        0x2A => KEY_LEFTSHIFT,
        0x2B => KEY_BACKSLASH,
        0x2C => KEY_Z,
        0x2D => KEY_X,
        0x2E => KEY_C,
        0x2F => KEY_V,
        0x30 => KEY_B,
        0x31 => KEY_N,
        0x32 => KEY_M,
        0x33 => KEY_COMMA,
        0x34 => KEY_DOT,
        0x35 => KEY_SLASH,
        0x36 => KEY_RIGHTSHIFT,
        0x37 => KEY_KPASTERISK,
        0x38 => KEY_LEFTALT,
        0x39 => KEY_SPACE,
        0x3A => KEY_CAPSLOCK,
        0x3B => KEY_F1,
        0x3C => KEY_F2,
        0x3D => KEY_F3,
        0x3E => KEY_F4,
        0x3F => KEY_F5,
        0x40 => KEY_F6,
        0x41 => KEY_F7,
        0x42 => KEY_F8,
        0x43 => KEY_F9,
        0x44 => KEY_F10,
        0x45 => KEY_NUMLOCK,
        0x46 => KEY_SCROLLLOCK,
        0x47 => KEY_HOME,
        0x48 => KEY_UP,
        0x49 => KEY_PAGEUP,
        0x4B => KEY_LEFT,
        0x4D => KEY_RIGHT,
        0x4F => KEY_END,
        0x50 => KEY_DOWN,
        0x51 => KEY_PAGEDOWN,
        0x52 => KEY_INSERT,
        0x53 => KEY_DELETE,
        0x57 => KEY_F11,
        0x58 => KEY_F12,
        0x5B => KEY_LEFTMETA,
        0x5C => KEY_RIGHTMETA,
        0x5D => KEY_MENU,
        0x64 => KEY_RIGHTCTRL,
        0x70 => KEY_KP0,
        0x71 => KEY_KP1,
        0x72 => KEY_KP2,
        0x73 => KEY_KP3,
        0x74 => KEY_KP4,
        0x75 => KEY_KP5,
        0x76 => KEY_KP6,
        0x77 => KEY_KP7,
        0x78 => KEY_KP8,
        0x79 => KEY_KP9,
        0x7A => KEY_KPDOT,
        0x7B => KEY_KPMINUS,
        0x7C => KEY_KPPLUS,
        0x7D => KEY_KPASTERISK,
        0x7E => KEY_KPSLASH,
        0x7F => KEY_KPENTER,
        _ => return None,
    })
}

pub fn translate_keyboard(scancode: u8, pressed: bool) -> Vec<InputEvent> {
    let value = if pressed { 1 } else { 0 };
    match orb_key_to_evdev(scancode) {
        Some(code) => vec![
            InputEvent::new(EV_MSC, MSC_SCAN, i32::from(scancode)),
            InputEvent::new(EV_KEY, code, value),
            InputEvent::syn_report(),
        ],
        None => vec![],
    }
}

pub fn translate_mouse_motion(dx: i32, dy: i32) -> Vec<InputEvent> {
    let mut events = Vec::new();
    if dx != 0 {
        events.push(InputEvent::new(EV_REL, REL_X, dx));
    }
    if dy != 0 {
        events.push(InputEvent::new(EV_REL, REL_Y, dy));
    }
    if !events.is_empty() {
        events.push(InputEvent::syn_report());
    }
    events
}

pub fn translate_mouse_scroll(x: i32, y: i32) -> Vec<InputEvent> {
    let mut events = Vec::new();
    if x != 0 {
        events.push(InputEvent::new(EV_REL, REL_HWHEEL, x));
    }
    if y != 0 {
        events.push(InputEvent::new(EV_REL, REL_WHEEL, y));
    }
    if !events.is_empty() {
        events.push(InputEvent::syn_report());
    }
    events
}

pub fn translate_mouse_button(button: usize, pressed: bool) -> Vec<InputEvent> {
    let code = match button {
        0 => BTN_LEFT,
        1 => BTN_MIDDLE,
        2 => BTN_RIGHT,
        3 => BTN_SIDE,
        4 => BTN_EXTRA,
        _ => return vec![],
    };
    let value = if pressed { 1 } else { 0 };
    vec![
        InputEvent::new(EV_KEY, code, value),
        InputEvent::syn_report(),
    ]
}

pub fn translate_touchpad_motion(
    x: i32,
    y: i32,
    touching: bool,
    tracking_id: i32,
) -> Vec<InputEvent> {
    let mut events = vec![
        InputEvent::new(EV_ABS, ABS_X, x),
        InputEvent::new(EV_ABS, ABS_Y, y),
    ];

    if touching {
        events.extend_from_slice(&[
            InputEvent::new(EV_ABS, ABS_MT_SLOT, 0),
            InputEvent::new(EV_ABS, ABS_MT_TRACKING_ID, tracking_id),
            InputEvent::new(EV_ABS, ABS_MT_POSITION_X, x),
            InputEvent::new(EV_ABS, ABS_MT_POSITION_Y, y),
            InputEvent::new(EV_ABS, ABS_PRESSURE, 255),
            InputEvent::new(EV_ABS, ABS_MT_TOUCH_MAJOR, 1),
        ]);
    }

    events.push(InputEvent::syn_report());
    events
}

pub fn translate_touchpad_contact(
    x: i32,
    y: i32,
    touching: bool,
    tracking_id: i32,
) -> Vec<InputEvent> {
    let mut events = vec![
        InputEvent::new(EV_ABS, ABS_X, x),
        InputEvent::new(EV_ABS, ABS_Y, y),
        InputEvent::new(EV_ABS, ABS_MT_SLOT, 0),
    ];

    if touching {
        events.extend_from_slice(&[
            InputEvent::new(EV_KEY, BTN_TOUCH, 1),
            InputEvent::new(EV_KEY, BTN_TOOL_FINGER, 1),
            InputEvent::new(EV_ABS, ABS_MT_TRACKING_ID, tracking_id),
            InputEvent::new(EV_ABS, ABS_MT_POSITION_X, x),
            InputEvent::new(EV_ABS, ABS_MT_POSITION_Y, y),
            InputEvent::new(EV_ABS, ABS_PRESSURE, 255),
            InputEvent::new(EV_ABS, ABS_MT_TOUCH_MAJOR, 1),
        ]);
    } else {
        events.extend_from_slice(&[
            InputEvent::new(EV_ABS, ABS_MT_TRACKING_ID, -1),
            InputEvent::new(EV_ABS, ABS_PRESSURE, 0),
            InputEvent::new(EV_ABS, ABS_MT_TOUCH_MAJOR, 0),
            InputEvent::new(EV_KEY, BTN_TOUCH, 0),
            InputEvent::new(EV_KEY, BTN_TOOL_FINGER, 0),
        ]);
    }

    events.push(InputEvent::syn_report());
    events
}

#[cfg(test)]
mod tests {
    use super::{
        translate_keyboard, translate_mouse_button, translate_mouse_motion, translate_mouse_scroll,
        translate_touchpad_motion,
    };
    use crate::types::*;

    fn has_event(events: &[InputEvent], event_type: u16, code: u16, value: i32) -> bool {
        events.iter().any(|event| {
            event.event_type == event_type && event.code == code && event.value == value
        })
    }

    fn has_event_code(events: &[InputEvent], event_type: u16, code: u16) -> bool {
        events
            .iter()
            .any(|event| event.event_type == event_type && event.code == code)
    }

    fn event_index(events: &[InputEvent], event_type: u16, code: u16, value: i32) -> Option<usize> {
        events.iter().position(|event| {
            event.event_type == event_type && event.code == code && event.value == value
        })
    }

    #[test]
    fn keyboard_press_translates_to_key_a_down() {
        let events = translate_keyboard(0x1E, true);

        assert!(has_event(&events, EV_KEY, KEY_A, 1));
    }

    #[test]
    fn keyboard_release_translates_to_key_a_up() {
        let events = translate_keyboard(0x1E, false);

        assert!(has_event(&events, EV_KEY, KEY_A, 0));
    }

    #[test]
    fn keyboard_events_include_scan_before_key() {
        let events = translate_keyboard(0x1E, true);
        let scan_index = event_index(&events, EV_MSC, MSC_SCAN, 0x1E).unwrap();
        let key_index = event_index(&events, EV_KEY, KEY_A, 1).unwrap();

        assert!(scan_index < key_index);
    }

    #[test]
    fn keyboard_events_end_with_syn_report() {
        let events = translate_keyboard(0x1E, true);

        let last = events
            .last()
            .expect("keyboard translation should emit events");
        assert_eq!(last.event_type, EV_SYN);
        assert_eq!(last.code, SYN_REPORT);
        assert_eq!(last.value, 0);
    }

    #[test]
    fn unknown_keyboard_scancode_returns_empty_events() {
        let events = translate_keyboard(0xFF, true);

        assert!(events.is_empty());
    }

    #[test]
    fn mouse_motion_x_only_emits_x_and_syn() {
        let events = translate_mouse_motion(10, 0);

        assert!(has_event(&events, EV_REL, REL_X, 10));
        assert!(!has_event_code(&events, EV_REL, REL_Y));
        assert_eq!(
            events
                .last()
                .map(|event| (event.event_type, event.code, event.value)),
            Some((EV_SYN, SYN_REPORT, 0))
        );
    }

    #[test]
    fn mouse_motion_x_and_y_emits_both_axes() {
        let events = translate_mouse_motion(5, -3);

        assert!(has_event(&events, EV_REL, REL_X, 5));
        assert!(has_event(&events, EV_REL, REL_Y, -3));
    }

    #[test]
    fn mouse_motion_zero_returns_empty_events() {
        let events = translate_mouse_motion(0, 0);

        assert!(events.is_empty());
    }

    #[test]
    fn mouse_scroll_up_emits_vertical_wheel() {
        let events = translate_mouse_scroll(0, 1);

        assert!(has_event(&events, EV_REL, REL_WHEEL, 1));
    }

    #[test]
    fn mouse_scroll_horizontal_emits_horizontal_wheel() {
        let events = translate_mouse_scroll(2, 0);

        assert!(has_event(&events, EV_REL, REL_HWHEEL, 2));
    }

    #[test]
    fn mouse_button_left_press_emits_btn_left_down() {
        let events = translate_mouse_button(0, true);

        assert!(has_event(&events, EV_KEY, BTN_LEFT, 1));
    }

    #[test]
    fn mouse_button_right_release_emits_btn_right_up() {
        let events = translate_mouse_button(2, false);

        assert!(has_event(&events, EV_KEY, BTN_RIGHT, 0));
    }

    #[test]
    fn unknown_mouse_button_returns_empty_events() {
        let events = translate_mouse_button(10, true);

        assert!(events.is_empty());
    }

    #[test]
    fn touchpad_motion_emits_absolute_contact_details() {
        let events = translate_touchpad_motion(100, 200, true, 1);

        assert!(has_event(&events, EV_ABS, ABS_X, 100));
        assert!(has_event(&events, EV_ABS, ABS_Y, 200));
        assert!(has_event(&events, EV_ABS, ABS_MT_TRACKING_ID, 1));
    }
}
