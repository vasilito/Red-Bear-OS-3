#![allow(dead_code)]
pub const USBCMD: u16 = 0x00;
pub const USBSTS: u16 = 0x02;
pub const USBINTR: u16 = 0x04;
pub const FRNUM: u16 = 0x06;
pub const FRBASEADD: u16 = 0x08;
pub const SOFMOD: u16 = 0x0C;
pub const PORTSC1: u16 = 0x10;
pub const PORTSC2: u16 = 0x12;

pub const CMD_RUN_STOP: u16 = 1 << 0;
pub const CMD_HOST_RESET: u16 = 1 << 1;
pub const CMD_GLOBAL_RESET: u16 = 1 << 2;
pub const CMD_CONFIGURE: u16 = 1 << 6;
pub const CMD_MAX_PACKET_64: u16 = 1 << 7;

pub const STS_INTERRUPT: u16 = 1 << 0;
pub const STS_ERROR: u16 = 1 << 1;
pub const STS_RESUME: u16 = 1 << 2;
pub const STS_HOST_ERROR: u16 = 1 << 3;
pub const STS_HALTED: u16 = 1 << 5;

pub const PORT_CONNECT: u16 = 1 << 0;
pub const PORT_ENABLE: u16 = 1 << 1;
pub const PORT_SUSPEND: u16 = 1 << 2;
pub const PORT_OVER_CURRENT: u16 = 1 << 3;
pub const PORT_RESET: u16 = 1 << 4;
pub const PORT_LOW_SPEED: u16 = 1 << 8;

pub const FRAME_COUNT: usize = 1024;
pub const FRAME_LIST_ALIGN: usize = 4096;
