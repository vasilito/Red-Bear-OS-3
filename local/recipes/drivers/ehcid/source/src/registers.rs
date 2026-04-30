#![allow(dead_code)]

use core::mem::size_of;

// EHCI (USB 2.0) MMIO Register Layout
// References: Intel EHCI 1.0 Specification (March 2002), sections 2.1-2.2
// USB 2.0 Specification, sections 5.2-5.3

pub const CAPLENGTH: usize = 0x00;
pub const HCSPARAMS: usize = 0x04;
pub const HCCPARAMS: usize = 0x08;
pub const HCSP_PORT_ROUTE: usize = 0x0C;

pub const HCCPARAMS_64BIT: u32 = 1 << 0;

pub fn op_base(caplength: u8) -> usize {
    caplength as usize
}

pub const USBCMD: usize = 0x00;
pub const USBSTS: usize = 0x04;
pub const USBINTR: usize = 0x08;
pub const FRINDEX: usize = 0x0C;
pub const CTRLDSSEGMENT: usize = 0x10;
pub const PERIODICLISTBASE: usize = 0x14;
pub const ASYNCLISTADDR: usize = 0x18;
pub const CONFIGFLAG: usize = 0x40;
pub const PORTSC_BASE: usize = 0x44;

pub const CMD_RUN_STOP: u32 = 1 << 0;
pub const CMD_HCRESET: u32 = 1 << 1;
pub const CMD_FRAME_LIST_SIZE_MASK: u32 = 0x3 << 2;
pub const CMD_FRAME_LIST_SIZE_1024: u32 = 0x0 << 2;
pub const CMD_FRAME_LIST_SIZE_512: u32 = 0x1 << 2;
pub const CMD_FRAME_LIST_SIZE_256: u32 = 0x2 << 2;
pub const CMD_PERIODIC_SCHEDULE_ENABLE: u32 = 1 << 4;
pub const CMD_ASYNC_SCHEDULE_ENABLE: u32 = 1 << 5;
pub const CMD_INTERRUPT_ON_ASYNC_ADVANCE: u32 = 1 << 6;
pub const CMD_LIGHT_HOST_CONTROLLER_RESET: u32 = 1 << 7;
pub const CMD_ASYNC_SCHEDULE_PARK_MODE_COUNT: u32 = 0x3 << 8;
pub const CMD_ASYNC_SCHEDULE_PARK_MODE_ENABLE: u32 = 1 << 11;
pub const CMD_INTERRUPT_THRESHOLD_CONTROL: u32 = 0xFF << 16;

pub const STS_USB_INTERRUPT: u32 = 1 << 0;
pub const STS_USB_ERROR_INTERRUPT: u32 = 1 << 1;
pub const STS_PORT_CHANGE_DETECT: u32 = 1 << 2;
pub const STS_FRAME_LIST_ROLLOVER: u32 = 1 << 3;
pub const STS_HOST_SYSTEM_ERROR: u32 = 1 << 4;
pub const STS_INTERRUPT_ON_ASYNC_ADVANCE: u32 = 1 << 5;
pub const STS_HC_HALTED: u32 = 1 << 12;
pub const STS_RECLAMATION: u32 = 1 << 13;
pub const STS_PERIODIC_SCHEDULE_STATUS: u32 = 1 << 14;
pub const STS_ASYNC_SCHEDULE_STATUS: u32 = 1 << 15;

pub const INTR_USB_INTERRUPT_ENABLE: u32 = 1 << 0;
pub const INTR_USB_ERROR_INTERRUPT_ENABLE: u32 = 1 << 1;
pub const INTR_PORT_CHANGE_ENABLE: u32 = 1 << 2;
pub const INTR_FRAME_LIST_ROLLOVER_ENABLE: u32 = 1 << 3;
pub const INTR_HOST_SYSTEM_ERROR_ENABLE: u32 = 1 << 4;
pub const INTR_ASYNC_ADVANCE_ENABLE: u32 = 1 << 5;

pub const CF_FLAG: u32 = 1 << 0;

pub const PORT_CONNECT: u32 = 1 << 0;
pub const PORT_CONNECT_CHANGE: u32 = 1 << 1;
pub const PORT_ENABLE: u32 = 1 << 2;
pub const PORT_ENABLE_CHANGE: u32 = 1 << 3;
pub const PORT_OVER_CURRENT_ACTIVE: u32 = 1 << 4;
pub const PORT_OVER_CURRENT_CHANGE: u32 = 1 << 5;
pub const PORT_FORCE_PORT_RESUME: u32 = 1 << 6;
pub const PORT_SUSPEND: u32 = 1 << 7;
pub const PORT_RESET: u32 = 1 << 8;
pub const PORT_LINE_STATUS: u32 = 0x3 << 10;
pub const PORT_LINE_STATUS_K: u32 = 0x1 << 10;
pub const PORT_LINE_STATUS_J: u32 = 0x2 << 10;
pub const PORT_POWER: u32 = 1 << 12;
pub const PORT_OWNER: u32 = 1 << 13;
pub const PORT_INDICATOR: u32 = 0x3 << 14;
pub const PORT_TEST_CONTROL: u32 = 0xF << 16;
pub const PORT_WAKE_CONNECT: u32 = 1 << 20;
pub const PORT_WAKE_DISCONNECT: u32 = 1 << 21;
pub const PORT_WAKE_OVER_CURRENT: u32 = 1 << 22;
pub const PORTSC_CHANGE_BITS: u32 =
    PORT_CONNECT_CHANGE | PORT_ENABLE_CHANGE | PORT_OVER_CURRENT_CHANGE;
pub const PORTSC_WRITE_MASK: u32 = PORT_ENABLE
    | PORT_FORCE_PORT_RESUME
    | PORT_SUSPEND
    | PORT_RESET
    | PORT_POWER
    | PORT_OWNER
    | PORT_INDICATOR
    | PORT_TEST_CONTROL
    | PORT_WAKE_CONNECT
    | PORT_WAKE_DISCONNECT
    | PORT_WAKE_OVER_CURRENT;

pub fn portsc_offset(port: usize) -> usize {
    PORTSC_BASE + (port * 4)
}

#[derive(Debug)]
pub struct HcCapParams {
    pub n_ports: u8,
    pub port_routing_rules: bool,
    pub n_cc: u8,
    pub n_pcc: u8,
    pub port_route: u32,
}

impl HcCapParams {
    pub fn from_hcsparams(hcsparams: u32) -> Self {
        HcCapParams {
            n_ports: (hcsparams & 0xF) as u8,
            port_routing_rules: (hcsparams >> 4) & 1 != 0,
            n_cc: ((hcsparams >> 8) & 0xF) as u8,
            n_pcc: ((hcsparams >> 12) & 0xF) as u8,
            port_route: 0,
        }
    }
}

pub struct EhciRegisters {
    pub mmio_base: usize,
    pub mmio_size: usize,
    pub op_base: usize,
    pub n_ports: u8,
    pub frame_list_size: u32,
    pub has_64bit: bool,
}

impl EhciRegisters {
    pub fn read32(&self, offset: usize) -> u32 {
        let addr = self.mmio_base + offset;
        unsafe { (addr as *const u32).read_volatile() }
    }

    pub fn write32(&self, offset: usize, value: u32) {
        let addr = self.mmio_base + offset;
        unsafe { (addr as *mut u32).write_volatile(value) }
    }

    pub fn read_op32(&self, offset: usize) -> u32 {
        self.read32(self.op_base + offset)
    }

    pub fn write_op32(&self, offset: usize, value: u32) {
        self.write32(self.op_base + offset, value)
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(C, align(64))]
pub struct QueueHead {
    pub horiz_link: u32,
    pub caps: [u32; 2],
    pub current_qtd: u32,
    pub overlay: [u32; 8],
}

pub const QH_TERMINATE: u32 = 1;
pub const QH_HEAD_MASK: u32 = !0x1F;
pub const QH_LINK_TYPE_QH: u32 = 0x2;
pub const QH_ENDPOINT_DTC: u32 = 1 << 14;
pub const QH_ENDPOINT_HEAD: u32 = 1 << 15;
pub const QH_ENDPOINT_SPEED_HIGH: u32 = 0x2 << 12;
pub const QH_NAK_RELOAD_4: u32 = 0x4 << 28;
pub const QH_CAP_MULT_ONE: u32 = 0x1 << 30;

impl QueueHead {
    pub fn new() -> Self {
        QueueHead {
            horiz_link: QH_TERMINATE,
            caps: [0; 2],
            current_qtd: 0,
            overlay: [TD_TERMINATE, TD_TERMINATE, 0, 0, 0, 0, 0, 0],
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(C, align(32))]
pub struct TransferDescriptor {
    pub next_qtd: u32,
    pub alt_qtd: u32,
    pub token: u32,
    pub buffers: [u32; 5],
}

pub const TD_TERMINATE: u32 = 1;
pub const TD_ACTIVE: u32 = 1 << 7;
pub const TD_HALTED: u32 = 1 << 6;
pub const TD_BUFERR: u32 = 1 << 5;
pub const TD_BABBLE: u32 = 1 << 4;
pub const TD_XACTERR: u32 = 1 << 3;
pub const TD_MISSED: u32 = 1 << 2;
pub const TD_C_PAGE_MASK: u32 = 0x7 << 12;
pub const TD_IOC: u32 = 1 << 15;
pub const TD_ERROR_COUNTER_3: u32 = 0x3 << 10;
pub const TD_TOTAL_BYTES_SHIFT: u32 = 16;
pub const TD_TOTAL_BYTES_MASK: u32 = 0x7FFF << TD_TOTAL_BYTES_SHIFT;
pub const TD_PID_IN: u32 = 1 << 8;
pub const TD_PID_OUT: u32 = 0 << 8;
pub const TD_PID_SETUP: u32 = 2 << 8;
pub const TD_PID_MASK: u32 = 3 << 8;

pub fn qh_link_pointer(phys_addr: u64) -> u32 {
    ((phys_addr as u32) & QH_HEAD_MASK) | QH_LINK_TYPE_QH
}

pub fn qh_endpoint_characteristics(
    device_address: u8,
    endpoint: u8,
    max_packet_size: u16,
    head_of_reclamation: bool,
) -> u32 {
    let mut value = u32::from(device_address & 0x7F)
        | (u32::from(endpoint & 0x0F) << 8)
        | QH_ENDPOINT_SPEED_HIGH
        | QH_ENDPOINT_DTC
        | (u32::from(max_packet_size & 0x07FF) << 16)
        | QH_NAK_RELOAD_4;

    if head_of_reclamation {
        value |= QH_ENDPOINT_HEAD;
    }

    value
}

pub fn qh_endpoint_capabilities() -> u32 {
    QH_CAP_MULT_ONE
}

fn fill_qtd_buffers(td: &mut TransferDescriptor, phys_addr: u64) {
    let addr = phys_addr as u32;
    td.buffers[0] = addr;

    let page_base = addr & !0xFFF;
    for (index, slot) in td.buffers.iter_mut().enumerate().skip(1) {
        *slot = page_base.wrapping_add((index as u32) * 0x1000);
    }
}

pub fn build_setup_td(phys_addr: u64, _setup_data: &[u8; 8], toggle: bool) -> TransferDescriptor {
    let mut td = TransferDescriptor {
        next_qtd: TD_TERMINATE,
        alt_qtd: TD_TERMINATE,
        token: TD_ACTIVE | TD_PID_SETUP | TD_ERROR_COUNTER_3 | (8 << TD_TOTAL_BYTES_SHIFT),
        buffers: [0; 5],
    };
    fill_qtd_buffers(&mut td, phys_addr);
    if toggle {
        td.token |= 1 << 31;
    }
    td
}

pub fn build_data_td(phys_addr: u64, len: usize, dir_in: bool, toggle: bool) -> TransferDescriptor {
    let pid = if dir_in { TD_PID_IN } else { TD_PID_OUT };
    let mut td = TransferDescriptor {
        next_qtd: TD_TERMINATE,
        alt_qtd: TD_TERMINATE,
        token: TD_ACTIVE
            | pid
            | TD_ERROR_COUNTER_3
            | (((len as u32) & 0x7FFF) << TD_TOTAL_BYTES_SHIFT),
        buffers: [0; 5],
    };
    fill_qtd_buffers(&mut td, phys_addr);
    if toggle {
        td.token |= 1 << 31;
    }
    td
}

pub fn build_status_td(dir_in: bool) -> TransferDescriptor {
    let pid = if dir_in { TD_PID_OUT } else { TD_PID_IN };
    TransferDescriptor {
        next_qtd: TD_TERMINATE,
        alt_qtd: TD_TERMINATE,
        token: TD_ACTIVE | pid | TD_ERROR_COUNTER_3 | TD_IOC | (1 << 31),
        buffers: [0; 5],
    }
}

fn td_phys(td_pool_phys: u64, index: usize) -> Option<u32> {
    let offset = index.checked_mul(size_of::<TransferDescriptor>())?;
    let phys = td_pool_phys.checked_add(offset as u64)?;
    Some((phys as u32) & !0x1F)
}

pub fn build_control_transfer(
    setup_phys: u64,
    setup_data: &[u8; 8],
    data_phys: u64,
    data_len: usize,
    dir_in: bool,
    td_pool: &mut [TransferDescriptor],
    td_pool_phys: u64,
) -> Option<u32> {
    if td_pool.len() < 2 {
        return None;
    }

    let td_count = if data_len > 0 { 3 } else { 2 };
    if td_pool.len() < td_count {
        return None;
    }

    let setup_td_phys = td_phys(td_pool_phys, 0)?;
    let status_td_phys = td_phys(td_pool_phys, td_count - 1)?;

    td_pool[0] = build_setup_td(setup_phys, setup_data, false);

    if data_len > 0 {
        let data_td_phys = td_phys(td_pool_phys, 1)?;
        td_pool[0].next_qtd = data_td_phys;

        td_pool[1] = build_data_td(data_phys, data_len, dir_in, true);
        td_pool[1].next_qtd = status_td_phys;
        td_pool[1].alt_qtd = status_td_phys;

        td_pool[2] = build_status_td(dir_in);
    } else {
        td_pool[0].next_qtd = status_td_phys;
        td_pool[1] = build_status_td(dir_in);
    }

    Some(setup_td_phys)
}
