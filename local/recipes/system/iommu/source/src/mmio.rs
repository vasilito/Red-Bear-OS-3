use core::mem::{offset_of, size_of};
use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};

pub const AMD_VI_MMIO_BYTES: usize = 0x2038;

pub mod offsets {
    pub const DEV_TABLE_BAR: usize = 0x0000;
    pub const CMD_BUF_BAR: usize = 0x0008;
    pub const EVT_LOG_BAR: usize = 0x0010;
    pub const CONTROL: usize = 0x0018;
    pub const EXCLUSION_BASE: usize = 0x0020;
    pub const EXCLUSION_LIMIT: usize = 0x0028;
    pub const EXTENDED_FEATURE: usize = 0x0030;
    pub const PPR_LOG_BAR: usize = 0x0038;
    pub const CMD_BUF_HEAD: usize = 0x2000;
    pub const CMD_BUF_TAIL: usize = 0x2008;
    pub const EVT_LOG_HEAD: usize = 0x2010;
    pub const EVT_LOG_TAIL: usize = 0x2018;
    pub const STATUS: usize = 0x2020;
    pub const PPR_LOG_HEAD: usize = 0x2028;
    pub const PPR_LOG_TAIL: usize = 0x2030;
}

pub mod control {
    pub const IOMMU_ENABLE: u32 = 1 << 0;
    pub const HT_TUN_EN: u32 = 1 << 1;
    pub const EVENT_LOG_EN: u32 = 1 << 2;
    pub const EVENT_INT_EN: u32 = 1 << 3;
    pub const COM_WAIT_INT_EN: u32 = 1 << 4;
    pub const CMD_BUF_EN: u32 = 1 << 12;
    pub const PPR_LOG_EN: u32 = 1 << 6;
    pub const PPR_INT_EN: u32 = 1 << 7;
    pub const PPR_EN: u32 = 1 << 8;
    pub const GT_EN: u32 = 1 << 9;
    pub const GA_EN: u32 = 1 << 10;
    pub const CRW: u32 = 1 << 12;
    pub const SMIF_EN: u32 = 1 << 13;
    pub const SLFW_EN: u32 = 1 << 14;
    pub const SMIF_LOG_EN: u32 = 1 << 15;
    pub const GAM_EN_0: u32 = 1 << 16;
    pub const GAM_EN_1: u32 = 1 << 17;
    pub const GAM_EN_2: u32 = 1 << 18;
    pub const XT_EN: u32 = 1 << 22;
    pub const NX_EN: u32 = 1 << 23;
    pub const IRQ_TABLE_LEN_EN: u32 = 1 << 24;
}

pub mod status {
    pub const EVENT_OVERFLOW: u32 = 1 << 0;
    pub const EVENT_LOG_INT: u32 = 1 << 1;
    pub const COM_WAIT_INT: u32 = 1 << 2;
    pub const EVT_RUN: u32 = 1 << 3;
    pub const CMDBUF_RUN: u32 = 1 << 4;
    pub const PPR_LOG_OVERFLOW: u32 = 1 << 5;
    pub const PPR_LOG_INT: u32 = 1 << 6;
    pub const PPR_RUN: u32 = 1 << 7;
}

pub mod ext_feature {
    pub const PREF_SUP: u64 = 1 << 0;
    pub const PPR_SUP: u64 = 1 << 1;
    pub const XT_SUP: u64 = 1 << 2;
    pub const NX_SUP: u64 = 1 << 3;
    pub const GT_SUP: u64 = 1 << 4;
    pub const IA_SUP: u64 = 1 << 6;
    pub const GA_SUP: u64 = 1 << 7;
    pub const HE_SUP: u64 = 1 << 8;
    pub const PC_SUP: u64 = 1 << 9;
    pub const GI_SUP: u64 = 1 << 57;
    pub const HA_SUP: u64 = 1 << 58;
}

#[repr(C)]
pub struct AmdViMmio {
    pub dev_table_bar: u64,
    pub cmd_buf_bar: u64,
    pub evt_log_bar: u64,
    pub control: u32,
    _reserved0: u32,
    pub exclusion_base: u64,
    pub exclusion_limit: u64,
    pub extended_feature: u64,
    pub ppr_log_bar: u64,
    _reserved1: [u8; 0x2000 - 0x40],
    pub cmd_buf_head: u64,
    pub cmd_buf_tail: u64,
    pub evt_log_head: u64,
    pub evt_log_tail: u64,
    pub status: u32,
    _reserved2: u32,
    pub ppr_log_head: u64,
    pub ppr_log_tail: u64,
}

const _: () = assert!(size_of::<AmdViMmio>() == AMD_VI_MMIO_BYTES);
const _: () = assert!(offset_of!(AmdViMmio, dev_table_bar) == offsets::DEV_TABLE_BAR);
const _: () = assert!(offset_of!(AmdViMmio, cmd_buf_bar) == offsets::CMD_BUF_BAR);
const _: () = assert!(offset_of!(AmdViMmio, evt_log_bar) == offsets::EVT_LOG_BAR);
const _: () = assert!(offset_of!(AmdViMmio, control) == offsets::CONTROL);
const _: () = assert!(offset_of!(AmdViMmio, extended_feature) == offsets::EXTENDED_FEATURE);
const _: () = assert!(offset_of!(AmdViMmio, cmd_buf_head) == offsets::CMD_BUF_HEAD);
const _: () = assert!(offset_of!(AmdViMmio, cmd_buf_tail) == offsets::CMD_BUF_TAIL);
const _: () = assert!(offset_of!(AmdViMmio, evt_log_head) == offsets::EVT_LOG_HEAD);
const _: () = assert!(offset_of!(AmdViMmio, evt_log_tail) == offsets::EVT_LOG_TAIL);
const _: () = assert!(offset_of!(AmdViMmio, status) == offsets::STATUS);
const _: () = assert!(offset_of!(AmdViMmio, ppr_log_head) == offsets::PPR_LOG_HEAD);
const _: () = assert!(offset_of!(AmdViMmio, ppr_log_tail) == offsets::PPR_LOG_TAIL);

impl AmdViMmio {
    pub unsafe fn read_dev_table_bar(base: *mut Self) -> u64 {
        read_volatile(addr_of!((*base).dev_table_bar))
    }

    pub unsafe fn write_dev_table_bar(base: *mut Self, value: u64) {
        write_volatile(addr_of_mut!((*base).dev_table_bar), value);
    }

    pub unsafe fn read_cmd_buf_bar(base: *mut Self) -> u64 {
        read_volatile(addr_of!((*base).cmd_buf_bar))
    }

    pub unsafe fn write_cmd_buf_bar(base: *mut Self, value: u64) {
        write_volatile(addr_of_mut!((*base).cmd_buf_bar), value);
    }

    pub unsafe fn read_evt_log_bar(base: *mut Self) -> u64 {
        read_volatile(addr_of!((*base).evt_log_bar))
    }

    pub unsafe fn write_evt_log_bar(base: *mut Self, value: u64) {
        write_volatile(addr_of_mut!((*base).evt_log_bar), value);
    }

    pub unsafe fn read_control(base: *mut Self) -> u32 {
        read_volatile(addr_of!((*base).control))
    }

    pub unsafe fn write_control(base: *mut Self, value: u32) {
        write_volatile(addr_of_mut!((*base).control), value);
    }

    pub unsafe fn read_exclusion_base(base: *mut Self) -> u64 {
        read_volatile(addr_of!((*base).exclusion_base))
    }

    pub unsafe fn write_exclusion_base(base: *mut Self, value: u64) {
        write_volatile(addr_of_mut!((*base).exclusion_base), value);
    }

    pub unsafe fn read_exclusion_limit(base: *mut Self) -> u64 {
        read_volatile(addr_of!((*base).exclusion_limit))
    }

    pub unsafe fn write_exclusion_limit(base: *mut Self, value: u64) {
        write_volatile(addr_of_mut!((*base).exclusion_limit), value);
    }

    pub unsafe fn read_extended_feature(base: *mut Self) -> u64 {
        read_volatile(addr_of!((*base).extended_feature))
    }

    pub unsafe fn read_ppr_log_bar(base: *mut Self) -> u64 {
        read_volatile(addr_of!((*base).ppr_log_bar))
    }

    pub unsafe fn write_ppr_log_bar(base: *mut Self, value: u64) {
        write_volatile(addr_of_mut!((*base).ppr_log_bar), value);
    }

    pub unsafe fn read_cmd_buf_head(base: *mut Self) -> u64 {
        read_volatile(addr_of!((*base).cmd_buf_head))
    }

    pub unsafe fn write_cmd_buf_head(base: *mut Self, value: u64) {
        write_volatile(addr_of_mut!((*base).cmd_buf_head), value);
    }

    pub unsafe fn read_cmd_buf_tail(base: *mut Self) -> u64 {
        read_volatile(addr_of!((*base).cmd_buf_tail))
    }

    pub unsafe fn write_cmd_buf_tail(base: *mut Self, value: u64) {
        write_volatile(addr_of_mut!((*base).cmd_buf_tail), value);
    }

    pub unsafe fn read_evt_log_head(base: *mut Self) -> u64 {
        read_volatile(addr_of!((*base).evt_log_head))
    }

    pub unsafe fn write_evt_log_head(base: *mut Self, value: u64) {
        write_volatile(addr_of_mut!((*base).evt_log_head), value);
    }

    pub unsafe fn read_evt_log_tail(base: *mut Self) -> u64 {
        read_volatile(addr_of!((*base).evt_log_tail))
    }

    pub unsafe fn read_status(base: *mut Self) -> u32 {
        read_volatile(addr_of!((*base).status))
    }

    pub unsafe fn read_ppr_log_head(base: *mut Self) -> u64 {
        read_volatile(addr_of!((*base).ppr_log_head))
    }

    pub unsafe fn write_ppr_log_head(base: *mut Self, value: u64) {
        write_volatile(addr_of_mut!((*base).ppr_log_head), value);
    }

    pub unsafe fn read_ppr_log_tail(base: *mut Self) -> u64 {
        read_volatile(addr_of!((*base).ppr_log_tail))
    }
}

#[cfg(test)]
mod tests {
    use core::mem::MaybeUninit;

    use super::{offsets, AmdViMmio};

    #[test]
    fn register_accessors_use_expected_offsets() {
        let mut mmio = MaybeUninit::<AmdViMmio>::zeroed();
        let base = mmio.as_mut_ptr();

        unsafe {
            AmdViMmio::write_control(base, 0xdead_beef);
            AmdViMmio::write_cmd_buf_head(base, 0x1122_3344_5566_7788);
            AmdViMmio::write_dev_table_bar(base, 0x2000);

            assert_eq!(AmdViMmio::read_control(base), 0xdead_beef);
            assert_eq!(AmdViMmio::read_cmd_buf_head(base), 0x1122_3344_5566_7788);
            assert_eq!(AmdViMmio::read_dev_table_bar(base), 0x2000);

            let byte_base = base.cast::<u8>();
            let control_ptr = byte_base.add(offsets::CONTROL).cast::<u32>();
            let head_ptr = byte_base.add(offsets::CMD_BUF_HEAD).cast::<u64>();

            assert_eq!(core::ptr::read_volatile(control_ptr), 0xdead_beef);
            assert_eq!(core::ptr::read_volatile(head_ptr), 0x1122_3344_5566_7788);
        }
    }
}
