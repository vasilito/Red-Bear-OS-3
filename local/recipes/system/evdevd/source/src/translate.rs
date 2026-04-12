use crate::types::*;

fn orb_key_to_evdev(orb_key: u8) -> Option<u16> {
    let mapped = match orb_key {
        b'1'..=b'9' => KEY_1 + (orb_key - b'1') as u16,
        b'0' => KEY_0,
        b'a'..=b'z' => KEY_A + (orb_key - b'a') as u16,
        b'\n' | b'\r' => KEY_ENTER,
        b'\t' => KEY_TAB,
        b' ' => KEY_SPACE,
        b'\x08' => KEY_BACKSPACE,
        b'\x1b' => KEY_ESC,
        b'-' => KEY_MINUS,
        b'=' => KEY_EQUAL,
        b'[' => KEY_LEFTBRACE,
        b']' => KEY_RIGHTBRACE,
        b'\\' => KEY_BACKSLASH,
        b';' => KEY_SEMICOLON,
        b'\'' => KEY_APOSTROPHE,
        b'`' => KEY_GRAVE,
        b',' => KEY_COMMA,
        b'.' => KEY_DOT,
        b'/' => KEY_SLASH,
        _ => return None,
    };
    Some(mapped)
}

pub fn translate_keyboard(orb_key: u8, pressed: bool) -> Vec<InputEvent> {
    let value = if pressed { 1 } else { 0 };
    match orb_key_to_evdev(orb_key) {
        Some(code) => vec![
            InputEvent::new(EV_KEY, code, value),
            InputEvent::syn_report(),
        ],
        None => vec![],
    }
}

pub fn translate_mouse_dx(dx: i32) -> Vec<InputEvent> {
    vec![InputEvent::new(EV_REL, REL_X, dx), InputEvent::syn_report()]
}

pub fn translate_mouse_dy(dy: i32) -> Vec<InputEvent> {
    vec![InputEvent::new(EV_REL, REL_Y, dy), InputEvent::syn_report()]
}

pub fn translate_mouse_scroll(y: i32) -> Vec<InputEvent> {
    vec![
        InputEvent::new(EV_REL, REL_WHEEL, y),
        InputEvent::syn_report(),
    ]
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

pub fn translate_touch(x: i32, y: i32, touching: bool) -> Vec<InputEvent> {
    let btn = InputEvent::new(EV_KEY, BTN_TOUCH, if touching { 1 } else { 0 });
    let abs_x = InputEvent::new(EV_ABS, ABS_X, x);
    let abs_y = InputEvent::new(EV_ABS, ABS_Y, y);
    let syn = InputEvent::syn_report();
    vec![btn, abs_x, abs_y, syn]
}
