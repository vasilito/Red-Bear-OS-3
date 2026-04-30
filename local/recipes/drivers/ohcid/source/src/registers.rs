#![allow(dead_code)]
pub const HCREVISION: usize = 0x00;
pub const HCCONTROL: usize = 0x04;
pub const HCCOMMANDSTATUS: usize = 0x08;
pub const HCINTERRUPTSTATUS: usize = 0x0C;
pub const HCINTERRUPTENABLE: usize = 0x10;
pub const HCHCCA: usize = 0x18;
pub const HCCONTROLHEADED: usize = 0x20;
pub const HCBULKHEADED: usize = 0x28;
pub const HCDONEHEAD: usize = 0x30;
pub const HCFMINTERVAL: usize = 0x34;
pub const HCFMREMAINING: usize = 0x38;
pub const HCFMNUMBER: usize = 0x3C;
pub const HCRHDESCRIPTORA: usize = 0x48;
pub const HCRHSTATUS: usize = 0x50;
pub const HCRHPORTSTATUS1: usize = 0x54;

pub const CONTROL_BULK_ENABLE: u32 = 1 << 3;
pub const PERIODIC_ENABLE: u32 = 1 << 4;
pub const CONTROL_ENABLE: u32 = 1 << 6;
pub const BULK_ENABLE: u32 = 1 << 7;
pub const HC_FUNCTIONAL_STATE_MASK: u32 = 0x3 << 6;
pub const HC_RESET: u32 = 0;
pub const HC_RESUME: u32 = 1 << 6;
pub const HC_OPERATIONAL: u32 = 2 << 6;
pub const HC_SUSPEND: u32 = 3 << 6;

pub const PORT_CURRENT_CONNECT: u32 = 1 << 0;
pub const PORT_ENABLE: u32 = 1 << 1;
pub const PORT_SUSPEND: u32 = 1 << 2;
pub const PORT_OVER_CURRENT: u32 = 1 << 3;
pub const PORT_RESET: u32 = 1 << 4;
pub const PORT_POWER: u32 = 1 << 8;
pub const PORT_LOW_SPEED: u32 = 1 << 9;
pub const PORT_CONNECT_CHANGE: u32 = 1 << 16;
pub const PORT_ENABLE_CHANGE: u32 = 1 << 17;

pub const WRITE_BACK_DONE_HEAD: u32 = 1 << 1;
pub const START_OF_FRAME: u32 = 1 << 2;
pub const RESUME_DETECTED: u32 = 1 << 3;
pub const ROOT_HUB_STATUS_CHANGE: u32 = 1 << 6;

pub const HCCA_SIZE: usize = 256;
pub const HCCA_ALIGN: usize = 256;
