use inputd::ProducerHandle;
use log::{error, warn};
use orbclient::{ButtonEvent, KeyEvent, MouseEvent, MouseRelativeEvent, ScrollEvent};
use std::{
    convert::TryInto,
    fs::File,
    io::{Read, Write},
    time::Duration,
};
use syscall::TimeSpec;

use crate::controller::Ps2;
use crate::mouse::{MouseResult, MouseState};
use crate::vm;

bitflags! {
    pub struct MousePacketFlags: u8 {
        const LEFT_BUTTON = 1;
        const RIGHT_BUTTON = 1 << 1;
        const MIDDLE_BUTTON = 1 << 2;
        const ALWAYS_ON = 1 << 3;
        const X_SIGN = 1 << 4;
        const Y_SIGN = 1 << 5;
        const X_OVERFLOW = 1 << 6;
        const Y_OVERFLOW = 1 << 7;
    }
}

fn timespec_from_duration(duration: Duration) -> TimeSpec {
    TimeSpec {
        tv_sec: duration.as_secs().try_into().unwrap(),
        tv_nsec: duration.subsec_nanos().try_into().unwrap(),
    }
}

fn duration_from_timespec(timespec: TimeSpec) -> Duration {
    Duration::new(
        timespec.tv_sec.try_into().unwrap(),
        timespec.tv_nsec.try_into().unwrap(),
    )
}

pub struct Ps2d {
    ps2: Ps2,
    vmmouse: bool,
    vmmouse_relative: bool,
    input: ProducerHandle,
    time_file: File,
    extended: bool,
    mouse_x: i32,
    mouse_y: i32,
    mouse_left: bool,
    mouse_middle: bool,
    mouse_right: bool,
    mouse_state: MouseState,
    mouse_timeout: Option<TimeSpec>,
    packets: [u8; 4],
    packet_i: usize,
}

impl Ps2d {
    pub fn new(input: ProducerHandle, time_file: File) -> Self {
        let mut ps2 = Ps2::new();
        ps2.init().expect("failed to initialize");

        // FIXME add an option for orbital to disable this when an app captures the mouse.
        let vmmouse_relative = false;
        let vmmouse = vm::enable(vmmouse_relative);

        // TODO: QEMU hack, maybe do this when Init timed out?
        if vmmouse {
            // 3 = MouseId::Intellimouse1
            MouseState::Bat.handle(3, &mut ps2);
        }

        let mut this = Ps2d {
            ps2,
            vmmouse,
            vmmouse_relative,
            input,
            time_file,
            extended: false,
            mouse_x: 0,
            mouse_y: 0,
            mouse_left: false,
            mouse_middle: false,
            mouse_right: false,
            mouse_state: MouseState::Init,
            mouse_timeout: None,
            packets: [0; 4],
            packet_i: 0,
        };

        if !this.vmmouse {
            // This triggers initializing the mouse
            this.handle_mouse(None);
        }

        this
    }

    pub fn irq(&mut self) {
        while let Some((keyboard, data)) = self.ps2.next() {
            self.handle(keyboard, data);
        }
    }

    pub fn time_event(&mut self) {
        let mut time = TimeSpec::default();
        match self.time_file.read(&mut time) {
            Ok(_count) => {}
            Err(err) => {
                log::error!("failed to read time file: {}", err);
                return;
            }
        }
        if let Some(mouse_timeout) = self.mouse_timeout {
            if time.tv_sec > mouse_timeout.tv_sec
                || (time.tv_sec == mouse_timeout.tv_sec && time.tv_nsec >= mouse_timeout.tv_nsec)
            {
                self.handle_mouse(None);
            }
        }
    }

    pub fn handle(&mut self, keyboard: bool, data: u8) {
        if keyboard {
            if data == 0xE0 {
                self.extended = true;
            } else {
                let (ps2_scancode, pressed) = if data >= 0x80 {
                    (data - 0x80, false)
                } else {
                    (data, true)
                };

                let scancode = if self.extended {
                    self.extended = false;
                    match ps2_scancode {
                        0x1C => orbclient::K_NUM_ENTER,
                        0x1D => orbclient::K_RIGHT_CTRL,
                        0x20 => orbclient::K_VOLUME_TOGGLE,
                        0x22 => orbclient::K_MEDIA_PLAY_PAUSE,
                        0x24 => orbclient::K_MEDIA_STOP,
                        0x10 => orbclient::K_MEDIA_REWIND,
                        0x19 => orbclient::K_MEDIA_FAST_FORWARD,
                        0x2E => orbclient::K_VOLUME_DOWN,
                        0x30 => orbclient::K_VOLUME_UP,
                        0x35 => orbclient::K_NUM_SLASH,
                        0x38 => orbclient::K_ALT_GR,
                        0x47 => orbclient::K_HOME,
                        0x48 => orbclient::K_UP,
                        0x49 => orbclient::K_PGUP,
                        0x4B => orbclient::K_LEFT,
                        0x4D => orbclient::K_RIGHT,
                        0x4F => orbclient::K_END,
                        0x50 => orbclient::K_DOWN,
                        0x51 => orbclient::K_PGDN,
                        0x52 => orbclient::K_INS,
                        0x53 => orbclient::K_DEL,
                        0x5B => orbclient::K_LEFT_SUPER,
                        0x5C => orbclient::K_RIGHT_SUPER,
                        0x5D => orbclient::K_APP,
                        0x5E => orbclient::K_POWER,
                        0x5F => orbclient::K_SLEEP,
                        /* 0x80 to 0xFF used for press/release detection */
                        _ => {
                            if pressed {
                                warn!("unknown extended scancode {:02X}", ps2_scancode);
                            }
                            0
                        }
                    }
                } else {
                    match ps2_scancode {
                        /* 0x00 unused */
                        0x01 => orbclient::K_ESC,
                        0x02 => orbclient::K_1,
                        0x03 => orbclient::K_2,
                        0x04 => orbclient::K_3,
                        0x05 => orbclient::K_4,
                        0x06 => orbclient::K_5,
                        0x07 => orbclient::K_6,
                        0x08 => orbclient::K_7,
                        0x09 => orbclient::K_8,
                        0x0A => orbclient::K_9,
                        0x0B => orbclient::K_0,
                        0x0C => orbclient::K_MINUS,
                        0x0D => orbclient::K_EQUALS,
                        0x0E => orbclient::K_BKSP,
                        0x0F => orbclient::K_TAB,
                        0x10 => orbclient::K_Q,
                        0x11 => orbclient::K_W,
                        0x12 => orbclient::K_E,
                        0x13 => orbclient::K_R,
                        0x14 => orbclient::K_T,
                        0x15 => orbclient::K_Y,
                        0x16 => orbclient::K_U,
                        0x17 => orbclient::K_I,
                        0x18 => orbclient::K_O,
                        0x19 => orbclient::K_P,
                        0x1A => orbclient::K_BRACE_OPEN,
                        0x1B => orbclient::K_BRACE_CLOSE,
                        0x1C => orbclient::K_ENTER,
                        0x1D => orbclient::K_CTRL,
                        0x1E => orbclient::K_A,
                        0x1F => orbclient::K_S,
                        0x20 => orbclient::K_D,
                        0x21 => orbclient::K_F,
                        0x22 => orbclient::K_G,
                        0x23 => orbclient::K_H,
                        0x24 => orbclient::K_J,
                        0x25 => orbclient::K_K,
                        0x26 => orbclient::K_L,
                        0x27 => orbclient::K_SEMICOLON,
                        0x28 => orbclient::K_QUOTE,
                        0x29 => orbclient::K_TICK,
                        0x2A => orbclient::K_LEFT_SHIFT,
                        0x2B => orbclient::K_BACKSLASH,
                        0x2C => orbclient::K_Z,
                        0x2D => orbclient::K_X,
                        0x2E => orbclient::K_C,
                        0x2F => orbclient::K_V,
                        0x30 => orbclient::K_B,
                        0x31 => orbclient::K_N,
                        0x32 => orbclient::K_M,
                        0x33 => orbclient::K_COMMA,
                        0x34 => orbclient::K_PERIOD,
                        0x35 => orbclient::K_SLASH,
                        0x36 => orbclient::K_RIGHT_SHIFT,
                        0x37 => orbclient::K_NUM_ASTERISK,
                        0x38 => orbclient::K_ALT,
                        0x39 => orbclient::K_SPACE,
                        0x3A => orbclient::K_CAPS,
                        0x3B => orbclient::K_F1,
                        0x3C => orbclient::K_F2,
                        0x3D => orbclient::K_F3,
                        0x3E => orbclient::K_F4,
                        0x3F => orbclient::K_F5,
                        0x40 => orbclient::K_F6,
                        0x41 => orbclient::K_F7,
                        0x42 => orbclient::K_F8,
                        0x43 => orbclient::K_F9,
                        0x44 => orbclient::K_F10,
                        0x45 => orbclient::K_NUM,
                        0x46 => orbclient::K_SCROLL,
                        0x47 => orbclient::K_NUM_7,
                        0x48 => orbclient::K_NUM_8,
                        0x49 => orbclient::K_NUM_9,
                        0x4A => orbclient::K_NUM_MINUS,
                        0x4B => orbclient::K_NUM_4,
                        0x4C => orbclient::K_NUM_5,
                        0x4D => orbclient::K_NUM_6,
                        0x4E => orbclient::K_NUM_PLUS,
                        0x4F => orbclient::K_NUM_1,
                        0x50 => orbclient::K_NUM_2,
                        0x51 => orbclient::K_NUM_3,
                        0x52 => orbclient::K_NUM_0,
                        0x53 => orbclient::K_NUM_PERIOD,
                        /* 0x54 to 0x55 unused */
                        0x56 => 0x56, // UK Backslash
                        0x57 => orbclient::K_F11,
                        0x58 => orbclient::K_F12,
                        /* 0x59 to 0x7F unused */
                        /* 0x80 to 0xFF used for press/release detection */
                        _ => {
                            if pressed {
                                warn!("unknown scancode {:02X}", ps2_scancode);
                            }
                            0
                        }
                    }
                };

                if scancode != 0 {
                    self.input
                        .write_event(
                            KeyEvent {
                                character: '\0',
                                scancode,
                                pressed,
                            }
                            .to_event(),
                        )
                        .expect("failed to write key event");
                }
            }
        } else if self.vmmouse {
            for _i in 0..256 {
                let (status, _, _, _) = unsafe { vm::cmd(vm::ABSPOINTER_STATUS, 0) };
                //TODO if ((status & VMMOUSE_ERROR) == VMMOUSE_ERROR)

                let queue_length = status & 0xffff;
                if queue_length == 0 {
                    break;
                }

                if queue_length % 4 != 0 {
                    error!("queue length not a multiple of 4: {}", queue_length);
                    break;
                }

                let (status, dx, dy, dz) = unsafe { vm::cmd(vm::ABSPOINTER_DATA, 4) };

                if self.vmmouse_relative {
                    if dx != 0 || dy != 0 {
                        self.input
                            .write_event(
                                MouseRelativeEvent {
                                    dx: dx as i32,
                                    dy: dy as i32,
                                }
                                .to_event(),
                            )
                            .expect("ps2d: failed to write mouse event");
                    }
                } else {
                    let x = dx as i32;
                    let y = dy as i32;
                    if x != self.mouse_x || y != self.mouse_y {
                        self.mouse_x = x;
                        self.mouse_y = y;
                        self.input
                            .write_event(MouseEvent { x, y }.to_event())
                            .expect("ps2d: failed to write mouse event");
                    }
                };

                if dz != 0 {
                    self.input
                        .write_event(
                            ScrollEvent {
                                x: 0,
                                y: -(dz as i32),
                            }
                            .to_event(),
                        )
                        .expect("ps2d: failed to write scroll event");
                }

                let left = status & vm::LEFT_BUTTON == vm::LEFT_BUTTON;
                let middle = status & vm::MIDDLE_BUTTON == vm::MIDDLE_BUTTON;
                let right = status & vm::RIGHT_BUTTON == vm::RIGHT_BUTTON;
                if left != self.mouse_left
                    || middle != self.mouse_middle
                    || right != self.mouse_right
                {
                    self.mouse_left = left;
                    self.mouse_middle = middle;
                    self.mouse_right = right;
                    self.input
                        .write_event(
                            ButtonEvent {
                                left,
                                middle,
                                right,
                            }
                            .to_event(),
                        )
                        .expect("ps2d: failed to write button event");
                }
            }
        } else {
            self.handle_mouse(Some(data));
        }
    }

    pub fn handle_mouse(&mut self, data_opt: Option<u8>) {
        // log::trace!(
        //     "handle_mouse state {:?} data {:?}",
        //     self.mouse_state,
        //     data_opt
        // );
        let mouse_res = match data_opt {
            Some(data) => self.mouse_state.handle(data, &mut self.ps2),
            None => self.mouse_state.handle_timeout(&mut self.ps2),
        };
        self.mouse_timeout = None;
        let (packet_data, extra_packet) = match mouse_res {
            MouseResult::None => {
                return;
            }
            MouseResult::Packet(packet_data, extra_packet) => (packet_data, extra_packet),
            MouseResult::Timeout(duration) => {
                // Read current time
                let mut time = TimeSpec::default();
                match self.time_file.read(&mut time) {
                    Ok(_count) => {}
                    Err(err) => {
                        log::error!("failed to read time file: {}", err);
                        return;
                    }
                }

                // Add duration to time
                time = timespec_from_duration(duration_from_timespec(time) + duration);

                // Write next time
                match self.time_file.write(&time) {
                    Ok(_count) => {}
                    Err(err) => {
                        log::error!("failed to write time file: {}", err);
                    }
                }

                self.mouse_timeout = Some(time);
                return;
            }
        };

        self.packets[self.packet_i] = packet_data;
        self.packet_i += 1;

        let flags = MousePacketFlags::from_bits_truncate(self.packets[0]);
        if !flags.contains(MousePacketFlags::ALWAYS_ON) {
            error!("mouse misalign {:X}", self.packets[0]);

            self.packets = [0; 4];
            self.packet_i = 0;
        } else if self.packet_i >= self.packets.len() || (!extra_packet && self.packet_i >= 3) {
            if !flags.contains(MousePacketFlags::X_OVERFLOW)
                && !flags.contains(MousePacketFlags::Y_OVERFLOW)
            {
                let mut dx = self.packets[1] as i32;
                if flags.contains(MousePacketFlags::X_SIGN) {
                    dx -= 0x100;
                }

                let mut dy = -(self.packets[2] as i32);
                if flags.contains(MousePacketFlags::Y_SIGN) {
                    dy += 0x100;
                }

                let mut dz = 0;
                if extra_packet {
                    let mut scroll = (self.packets[3] & 0xF) as i8;
                    if scroll & (1 << 3) == 1 << 3 {
                        scroll -= 16;
                    }
                    dz = -scroll as i32;
                }

                if dx != 0 || dy != 0 {
                    self.input
                        .write_event(MouseRelativeEvent { dx, dy }.to_event())
                        .expect("ps2d: failed to write mouse event");
                }

                if dz != 0 {
                    self.input
                        .write_event(ScrollEvent { x: 0, y: dz }.to_event())
                        .expect("ps2d: failed to write scroll event");
                }

                let left = flags.contains(MousePacketFlags::LEFT_BUTTON);
                let middle = flags.contains(MousePacketFlags::MIDDLE_BUTTON);
                let right = flags.contains(MousePacketFlags::RIGHT_BUTTON);
                if left != self.mouse_left
                    || middle != self.mouse_middle
                    || right != self.mouse_right
                {
                    self.mouse_left = left;
                    self.mouse_middle = middle;
                    self.mouse_right = right;
                    self.input
                        .write_event(
                            ButtonEvent {
                                left,
                                middle,
                                right,
                            }
                            .to_event(),
                        )
                        .expect("ps2d: failed to write button event");
                }
            } else {
                warn!(
                    "overflow {:X} {:X} {:X} {:X}",
                    self.packets[0], self.packets[1], self.packets[2], self.packets[3]
                );
            }

            self.packets = [0; 4];
            self.packet_i = 0;
        }
    }
}
