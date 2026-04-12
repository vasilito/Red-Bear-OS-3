use std::collections::VecDeque;

use crate::types::{InputEvent, InputId, BUS_VIRTUAL};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DeviceKind {
    Keyboard,
    Mouse,
    Touchpad,
}

pub struct InputDevice {
    pub id: usize,
    pub kind: DeviceKind,
    pub name: String,
    pub input_id: InputId,
    pub event_buf: VecDeque<InputEvent>,
}

impl InputDevice {
    pub fn new_keyboard(id: usize) -> Self {
        InputDevice {
            id,
            kind: DeviceKind::Keyboard,
            name: format!("Redox Keyboard {}", id),
            input_id: InputId {
                bustype: BUS_VIRTUAL,
                vendor: 0,
                product: id as u16,
                version: 1,
            },
            event_buf: VecDeque::new(),
        }
    }

    pub fn new_mouse(id: usize) -> Self {
        InputDevice {
            id,
            kind: DeviceKind::Mouse,
            name: format!("Redox Mouse {}", id),
            input_id: InputId {
                bustype: BUS_VIRTUAL,
                vendor: 0,
                product: (id + 0x10) as u16,
                version: 1,
            },
            event_buf: VecDeque::new(),
        }
    }

    pub fn new_touchpad(id: usize) -> Self {
        InputDevice {
            id,
            kind: DeviceKind::Touchpad,
            name: format!("Redox Touchpad {}", id),
            input_id: InputId {
                bustype: BUS_VIRTUAL,
                vendor: 0,
                product: (id + 0x20) as u16,
                version: 1,
            },
            event_buf: VecDeque::new(),
        }
    }

    pub fn push_event(&mut self, event: InputEvent) {
        self.event_buf.push_back(event);
    }

    pub fn push_events(&mut self, events: &[InputEvent]) {
        for &ev in events {
            self.event_buf.push_back(ev);
        }
    }

    pub fn pop_bytes(&mut self, buf: &mut [u8]) -> usize {
        let event_count = buf.len() / InputEvent::SIZE;
        let mut written = 0;
        for _ in 0..event_count {
            match self.event_buf.pop_front() {
                Some(ev) => {
                    let bytes = ev.to_bytes();
                    buf[written..written + InputEvent::SIZE].copy_from_slice(&bytes);
                    written += InputEvent::SIZE;
                }
                None => break,
            }
        }
        written
    }

    pub fn has_events(&self) -> bool {
        !self.event_buf.is_empty()
    }
}
