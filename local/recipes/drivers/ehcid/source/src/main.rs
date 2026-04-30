mod registers;

use std::collections::BTreeMap;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::io::{self, Write};
use std::mem::size_of;
use std::process;
use std::sync::atomic::{Ordering, fence};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use log::{LevelFilter, Metadata, Record, error, info, warn};
use redox_driver_sys::dma::DmaBuffer;
use redox_driver_sys::memory::{CacheType, MmioProt, MmioRegion};
use redox_driver_sys::pcid_client::PcidClient;
use redox_scheme::scheme::{SchemeState, SchemeSync, register_sync_scheme};
use redox_scheme::{CallerCtx, OpenResult, SignalBehavior, Socket};
use syscall::Stat;
use syscall::error::{
    EACCES, EBADF, EINVAL, ENOENT, EROFS, Error as SysError, Result as SysResult,
};
use syscall::flag::{EventFlags, MODE_DIR, MODE_FILE};
use syscall::schemev2::NewFdFlags;
use usb_core::{
    PortStatus, SetupPacket, TransferDirection, UsbError, UsbHostController,
    parse_config_descriptor, parse_device_descriptor,
};

use registers::*;

const SCHEME_NAME: &str = "usb";
const SCHEME_ROOT_ID: usize = 1;
const MMIO_MAP_SIZE: usize = 0x1000;
const FRAME_LIST_LEN: usize = 1024;
const CONTROL_TD_COUNT: usize = 3;
const DEFAULT_CONTROL_MPS: u16 = 64;
const PORT_POLL_INTERVAL: Duration = Duration::from_millis(100);
const PORT_RESET_HOLD: Duration = Duration::from_millis(50);
const PORT_RESET_SETTLE: Duration = Duration::from_millis(10);
const WAIT_STEP: Duration = Duration::from_millis(1);
const CONTROL_TRANSFER_TIMEOUT_POLLS: usize = 1000;
const MAX_CONFIG_DESCRIPTOR_LEN: usize = 512;
const MAX_SCHEME_CONTROL_BYTES: usize = 4096;
const STATUS_CLEAR_BITS: u32 = STS_USB_INTERRUPT
    | STS_USB_ERROR_INTERRUPT
    | STS_PORT_CHANGE_DETECT
    | STS_FRAME_LIST_ROLLOVER
    | STS_HOST_SYSTEM_ERROR
    | STS_INTERRUPT_ON_ASYNC_ADVANCE;

static LOGGER: StderrLogger = StderrLogger;

struct StderrLogger;

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= LevelFilter::Info
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            let _ = writeln!(
                io::stderr().lock(),
                "[{}] ehcid: {}",
                record.level(),
                record.args()
            );
        }
    }

    fn flush(&self) {}
}

#[derive(Clone, Debug, Default)]
struct PortDevice {
    address: u8,
    max_packet_size0: u16,
    device_descriptor: Vec<u8>,
    config_descriptor: Vec<u8>,
    vendor_id: u16,
    product_id: u16,
    device_class: u8,
    device_subclass: u8,
    device_protocol: u8,
}

#[derive(Clone, Debug, Default)]
struct PortRecord {
    last_portsc: u32,
    last_status: Option<PortStatus>,
    companion_owned: bool,
    last_error: Option<String>,
    device: Option<PortDevice>,
}

#[derive(Clone, Debug)]
struct ControlRequest {
    request_type: u8,
    request: u8,
    value: u16,
    index: u16,
    length: u16,
    data: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum HandleKind {
    PortDir { port: usize },
    Status { port: usize },
    Descriptor { port: usize },
    Control { port: usize },
}

#[derive(Clone, Debug)]
struct HandleState {
    kind: HandleKind,
    response: Vec<u8>,
}

struct EhciScheme {
    controller: Arc<Mutex<EhciController>>,
    handles: BTreeMap<usize, HandleState>,
    next_id: usize,
}

struct EhciController {
    controller_name: String,
    mmio: MmioRegion,
    op_base: usize,
    n_ports: u8,
    frame_list: DmaBuffer,
    async_qh: DmaBuffer,
    dma_segment: u32,
    has_64bit: bool,
    next_address: u8,
    ports: Vec<PortRecord>,
}

impl EhciController {
    fn new(device_path: &str, channel_fd: usize) -> Result<Self, String> {
        info!("EHCI USB 2.0 at {} (fd={})", device_path, channel_fd);

        let mut pcid = PcidClient::connect_default()
            .ok_or_else(|| "failed to connect to PCID client channel".to_string())?;
        pcid.enable_device()
            .map_err(|err| format!("failed to enable PCI device: {err}"))?;

        let config_path = format!("{device_path}/config");
        let config = match fs::read(&config_path) {
            Ok(data) => data,
            Err(err) => return Err(format!("cannot read PCI config at {config_path}: {err}")),
        };

        let mmio_base = parse_mmio_bar(&config)?;
        info!("MMIO base: 0x{mmio_base:016X}");

        let mmio = MmioRegion::map(
            mmio_base,
            MMIO_MAP_SIZE,
            CacheType::DeviceMemory,
            MmioProt::READ_WRITE,
        )
        .map_err(|err| format!("failed to map EHCI MMIO region: {err}"))?;

        let caplength = mmio.read8(CAPLENGTH);
        let op_base = registers::op_base(caplength);
        let hcsparams = mmio.read32(HCSPARAMS);
        let hccparams = mmio.read32(HCCPARAMS);
        let caps = HcCapParams::from_hcsparams(hcsparams);
        let n_ports = caps.n_ports;
        let has_64bit = (hccparams & HCCPARAMS_64BIT) != 0;

        if n_ports == 0 {
            return Err("EHCI controller reports zero ports".to_string());
        }

        info!("ports: {}, caplength: {}", n_ports, caplength);

        let mut frame_list = DmaBuffer::allocate(FRAME_LIST_LEN * size_of::<u32>(), 4096)
            .map_err(|err| format!("failed to allocate frame list: {err}"))?;
        init_frame_list(&mut frame_list);

        let async_qh = DmaBuffer::allocate(size_of::<QueueHead>(), 64)
            .map_err(|err| format!("failed to allocate async queue head: {err}"))?;

        let dma_segment = ensure_dma_segment(
            has_64bit,
            &[
                frame_list.physical_address() as u64,
                async_qh.physical_address() as u64,
            ],
        )?;

        let mut controller = Self {
            controller_name: device_path.to_string(),
            mmio,
            op_base,
            n_ports,
            frame_list,
            async_qh,
            dma_segment,
            has_64bit,
            next_address: 1,
            ports: vec![PortRecord::default(); usize::from(n_ports)],
        };

        controller.reset_and_start()?;
        controller.poll_ports_once();
        Ok(controller)
    }

    fn reset_and_start(&mut self) -> Result<(), String> {
        self.stop_controller()?;

        self.write_op32(USBCMD, CMD_HCRESET);
        self.wait_until(
            "controller reset completion",
            || self.read_op32(USBCMD) & CMD_HCRESET == 0,
            1000,
        )?;

        self.clear_interrupt_status();
        self.write_op32(CTRLDSSEGMENT, self.dma_segment);
        self.write_op32(
            PERIODICLISTBASE,
            low32(self.frame_list.physical_address() as u64),
        );

        self.initialize_async_qh(0, DEFAULT_CONTROL_MPS);
        self.write_op32(
            ASYNCLISTADDR,
            low32(self.async_qh.physical_address() as u64),
        );
        self.write_op32(
            USBINTR,
            INTR_USB_INTERRUPT_ENABLE
                | INTR_USB_ERROR_INTERRUPT_ENABLE
                | INTR_PORT_CHANGE_ENABLE
                | INTR_HOST_SYSTEM_ERROR_ENABLE
                | INTR_ASYNC_ADVANCE_ENABLE,
        );

        self.write_op32(
            USBCMD,
            CMD_RUN_STOP
                | CMD_FRAME_LIST_SIZE_1024
                | CMD_PERIODIC_SCHEDULE_ENABLE
                | CMD_ASYNC_SCHEDULE_ENABLE
                | interrupt_threshold(8),
        );

        self.wait_until(
            "host controller run state",
            || self.read_op32(USBSTS) & STS_HC_HALTED == 0,
            1000,
        )?;
        self.wait_until(
            "periodic schedule activation",
            || self.read_op32(USBSTS) & STS_PERIODIC_SCHEDULE_STATUS != 0,
            1000,
        )?;
        self.wait_until(
            "async schedule activation",
            || self.read_op32(USBSTS) & STS_ASYNC_SCHEDULE_STATUS != 0,
            1000,
        )?;

        self.write_op32(CONFIGFLAG, CF_FLAG);

        for port in 0..self.port_count() {
            self.ensure_port_power(port);
            let status = self.read_portsc(port);
            self.clear_port_changes(port, status);
        }

        info!(
            "ehcid: controller initialized, {} ports, async list at 0x{:08X}",
            self.n_ports,
            low32(self.async_qh.physical_address() as u64)
        );

        Ok(())
    }

    fn stop_controller(&mut self) -> Result<(), String> {
        let command = self.read_op32(USBCMD);
        if command & CMD_RUN_STOP != 0 {
            self.write_op32(USBCMD, command & !CMD_RUN_STOP);
            self.wait_until(
                "controller halt",
                || self.read_op32(USBSTS) & STS_HC_HALTED != 0,
                1000,
            )?;
        }

        Ok(())
    }

    fn wait_until<F>(&self, label: &str, mut predicate: F, iterations: usize) -> Result<(), String>
    where
        F: FnMut() -> bool,
    {
        for _ in 0..iterations {
            if predicate() {
                return Ok(());
            }
            thread::sleep(WAIT_STEP);
        }

        Err(format!("timed out waiting for {label}"))
    }

    fn clear_interrupt_status(&mut self) {
        let status = self.read_op32(USBSTS);
        if status & STS_HOST_SYSTEM_ERROR != 0 {
            warn!("EHCI host system error reported in USBSTS: 0x{status:08x}");
        }
        let clear = status & STATUS_CLEAR_BITS;
        if clear != 0 {
            self.write_op32(USBSTS, clear);
        }
    }

    fn initialize_async_qh(&mut self, device_address: u8, max_packet_size: u16) {
        let qh_phys = self.async_qh.physical_address() as u64;
        let mut qh = QueueHead::new();
        qh.horiz_link = qh_link_pointer(qh_phys);
        qh.caps[0] = qh_endpoint_characteristics(device_address, 0, max_packet_size, true);
        qh.caps[1] = qh_endpoint_capabilities();

        unsafe {
            std::ptr::write_volatile(self.async_qh.as_mut_ptr() as *mut QueueHead, qh);
        }
    }

    fn prepare_async_qh(&mut self, device_address: u8, max_packet_size: u16, first_td_phys: u32) {
        let qh_ptr = self.async_qh.as_mut_ptr() as *mut QueueHead;
        unsafe {
            let qh = &mut *qh_ptr;
            qh.horiz_link = qh_link_pointer(self.async_qh.physical_address() as u64);
            qh.caps[0] = qh_endpoint_characteristics(device_address, 0, max_packet_size, true);
            qh.caps[1] = qh_endpoint_capabilities();
            qh.current_qtd = 0;
            qh.overlay[0] = first_td_phys & !0x1F;
            qh.overlay[1] = TD_TERMINATE;
            qh.overlay[2] = 0;
            qh.overlay[3] = 0;
            qh.overlay[4] = 0;
            qh.overlay[5] = 0;
            qh.overlay[6] = 0;
            qh.overlay[7] = 0;
        }
    }

    fn disarm_async_qh(&mut self) {
        let qh_ptr = self.async_qh.as_mut_ptr() as *mut QueueHead;
        unsafe {
            let qh = &mut *qh_ptr;
            qh.current_qtd = 0;
            qh.overlay[0] = TD_TERMINATE;
            qh.overlay[1] = TD_TERMINATE;
            qh.overlay[2] = 0;
            qh.overlay[3] = 0;
            qh.overlay[4] = 0;
            qh.overlay[5] = 0;
            qh.overlay[6] = 0;
            qh.overlay[7] = 0;
        }
    }

    fn ensure_controller_running(&mut self) {
        let status = self.read_op32(USBSTS);
        let command = self.read_op32(USBCMD);
        let required = CMD_RUN_STOP | CMD_ASYNC_SCHEDULE_ENABLE | CMD_PERIODIC_SCHEDULE_ENABLE;

        if status & STS_HC_HALTED != 0
            || status & STS_ASYNC_SCHEDULE_STATUS == 0
            || status & STS_PERIODIC_SCHEDULE_STATUS == 0
            || command & required != required
        {
            self.write_op32(
                USBCMD,
                CMD_RUN_STOP
                    | CMD_FRAME_LIST_SIZE_1024
                    | CMD_PERIODIC_SCHEDULE_ENABLE
                    | CMD_ASYNC_SCHEDULE_ENABLE
                    | interrupt_threshold(8),
            );
        }
    }

    fn read32(&self, offset: usize) -> u32 {
        self.mmio.read32(offset)
    }

    fn write32(&self, offset: usize, value: u32) {
        self.mmio.write32(offset, value)
    }

    fn read_op32(&self, offset: usize) -> u32 {
        self.read32(self.op_base + offset)
    }

    fn write_op32(&self, offset: usize, value: u32) {
        self.write32(self.op_base + offset, value)
    }

    fn read_portsc(&self, port: usize) -> u32 {
        self.read_op32(portsc_offset(port))
    }

    fn write_portsc(&self, port: usize, value: u32) {
        self.write_op32(portsc_offset(port), value)
    }

    fn port_write_value(
        &self,
        current: u32,
        set_bits: u32,
        clear_bits: u32,
        clear_changes: u32,
    ) -> u32 {
        let mut value = current & PORTSC_WRITE_MASK;
        value &= !clear_bits;
        value |= set_bits & PORTSC_WRITE_MASK;
        value |= clear_changes & PORTSC_CHANGE_BITS;
        value
    }

    fn clear_port_changes(&self, port: usize, current: u32) {
        let clear = current & PORTSC_CHANGE_BITS;
        if clear != 0 {
            self.write_portsc(port, self.port_write_value(current, 0, 0, clear));
        }
    }

    fn ensure_port_power(&self, port: usize) {
        let current = self.read_portsc(port);
        if current & PORT_POWER == 0 {
            self.write_portsc(port, self.port_write_value(current, PORT_POWER, 0, current));
            thread::sleep(WAIT_STEP);
        }
    }

    fn handoff_to_companion(&mut self, port: usize) {
        let current = self.read_portsc(port);
        self.write_portsc(
            port,
            self.port_write_value(current, PORT_OWNER | PORT_POWER, PORT_RESET, current),
        );
        self.ports[port].companion_owned = true;
        self.ports[port].device = None;
        info!("ehcid: handed port {} to companion controller", port + 1);
    }

    fn poll_ports_once(&mut self) {
        self.clear_interrupt_status();

        for port in 0..self.port_count() {
            let portsc = self.read_portsc(port);
            let status = decode_port_status(portsc);
            let had_device = self.ports[port].device.is_some();
            let had_companion = self.ports[port].companion_owned;

            self.ports[port].last_portsc = portsc;
            self.ports[port].last_status = Some(status.clone());

            if portsc & PORTSC_CHANGE_BITS != 0 {
                self.clear_port_changes(port, portsc);
            }

            if !status.connected {
                if had_device || had_companion {
                    info!("ehcid: device disconnected from port {}", port + 1);
                }
                self.ports[port].device = None;
                self.ports[port].companion_owned = false;
                self.ports[port].last_error = None;
                continue;
            }

            if portsc & PORT_OWNER != 0 {
                self.ports[port].companion_owned = true;
                continue;
            }

            let should_probe = self.ports[port].device.is_none()
                && ((portsc & PORT_CONNECT_CHANGE != 0) || (portsc & PORT_ENABLE != 0));

            if should_probe {
                match self.initialize_port(port) {
                    Ok(()) => {}
                    Err(err) => {
                        warn!("ehcid: port {} initialization failed: {}", port + 1, err);
                        self.ports[port].last_error = Some(err);
                    }
                }
            }
        }
    }

    fn initialize_port(&mut self, port: usize) -> Result<(), String> {
        self.ports[port].device = None;
        self.ports[port].companion_owned = false;

        if !self.port_reset(port) {
            let portsc = self.read_portsc(port);
            if portsc & PORT_OWNER != 0 {
                self.ports[port].companion_owned = true;
                self.ports[port].last_error = None;
                return Ok(());
            }

            return Err("port reset did not produce an enabled high-speed port".to_string());
        }

        let device = self.enumerate_port_device(port)?;
        info!(
            "ehcid: port {} device {:04x}:{:04x} address {}",
            port + 1,
            device.vendor_id,
            device.product_id,
            device.address
        );

        self.ports[port].device = Some(device);
        self.ports[port].last_error = None;
        Ok(())
    }

    fn enumerate_port_device(&mut self, port: usize) -> Result<PortDevice, String> {
        let mut header = [0_u8; 8];
        let get_device_header = SetupPacket {
            request_type: 0x80,
            request: 0x06,
            value: 0x0100,
            index: 0,
            length: 8,
        };

        self.submit_control_transfer(
            port,
            0,
            DEFAULT_CONTROL_MPS,
            &get_device_header,
            &mut header,
        )
        .map_err(|err| format!("failed to fetch device descriptor header: {err:?}"))?;

        let max_packet_size0 = u16::from(header[7].max(8));
        let address = self.allocate_device_address()?;

        let set_address = SetupPacket {
            request_type: 0x00,
            request: 0x05,
            value: u16::from(address),
            index: 0,
            length: 0,
        };

        self.submit_control_transfer(port, 0, max_packet_size0, &set_address, &mut [])
            .map_err(|err| format!("failed to set device address {}: {err:?}", address))?;
        thread::sleep(PORT_RESET_SETTLE);

        let mut device_descriptor = [0_u8; 18];
        let get_device_descriptor = SetupPacket {
            request_type: 0x80,
            request: 0x06,
            value: 0x0100,
            index: 0,
            length: 18,
        };

        self.submit_control_transfer(
            port,
            address,
            max_packet_size0,
            &get_device_descriptor,
            &mut device_descriptor,
        )
        .map_err(|err| format!("failed to fetch full device descriptor: {err:?}"))?;

        let descriptor = parse_device_descriptor(&device_descriptor)
            .ok_or_else(|| "device descriptor parse failed".to_string())?;

        let config_descriptor = self.read_config_descriptor(port, address, max_packet_size0);

        Ok(PortDevice {
            address,
            max_packet_size0,
            device_descriptor: device_descriptor.to_vec(),
            config_descriptor,
            vendor_id: descriptor.vendor_id,
            product_id: descriptor.product_id,
            device_class: descriptor.device_class,
            device_subclass: descriptor.device_subclass,
            device_protocol: descriptor.device_protocol,
        })
    }

    fn read_config_descriptor(
        &mut self,
        port: usize,
        address: u8,
        max_packet_size0: u16,
    ) -> Vec<u8> {
        let header_request = SetupPacket {
            request_type: 0x80,
            request: 0x06,
            value: 0x0200,
            index: 0,
            length: 9,
        };

        let mut header = [0_u8; 9];
        if self
            .submit_control_transfer(
                port,
                address,
                max_packet_size0,
                &header_request,
                &mut header,
            )
            .is_err()
        {
            return Vec::new();
        }

        let Some(config) = parse_config_descriptor(&header) else {
            return Vec::new();
        };

        let total_length = usize::from(config.total_length).clamp(
            usize::from(header_request.length),
            MAX_CONFIG_DESCRIPTOR_LEN,
        );

        let full_request = SetupPacket {
            length: total_length as u16,
            ..header_request
        };
        let mut data = vec![0_u8; total_length];

        match self.submit_control_transfer(
            port,
            address,
            max_packet_size0,
            &full_request,
            &mut data,
        ) {
            Ok(actual) => {
                data.truncate(actual);
                data
            }
            Err(_) => Vec::new(),
        }
    }

    fn allocate_device_address(&mut self) -> Result<u8, String> {
        for _ in 0..127 {
            let candidate = self.next_address;
            self.next_address = if self.next_address >= 127 {
                1
            } else {
                self.next_address + 1
            };

            if !self.address_in_use(candidate) {
                return Ok(candidate);
            }
        }

        Err("no free USB device addresses remain".to_string())
    }

    fn address_in_use(&self, address: u8) -> bool {
        self.ports.iter().any(|record| {
            record
                .device
                .as_ref()
                .map(|device| device.address == address)
                .unwrap_or(false)
        })
    }

    fn ensure_dma_segment_matches(&self, phys: u64, label: &str) -> Result<u32, UsbError> {
        let segment = dma_segment(phys);
        if !self.has_64bit && segment != 0 {
            warn!(
                "ehcid: DMA buffer {} requires 64-bit addressing but the controller is 32-bit-only",
                label
            );
            return Err(UsbError::IoError);
        }

        if segment != self.dma_segment {
            warn!(
                "ehcid: DMA buffer {} is in segment 0x{:08x}, expected 0x{:08x}",
                label, segment, self.dma_segment
            );
            return Err(UsbError::IoError);
        }

        Ok(low32(phys))
    }

    fn submit_control_transfer(
        &mut self,
        port: usize,
        device_address: u8,
        max_packet_size: u16,
        setup: &SetupPacket,
        data: &mut [u8],
    ) -> Result<usize, UsbError> {
        if port >= self.port_count() {
            return Err(UsbError::NoDevice);
        }
        if usize::from(setup.length) != data.len() {
            return Err(UsbError::IoError);
        }
        if data.len() > 0x7FFF {
            return Err(UsbError::Unsupported);
        }
        if max_packet_size == 0 {
            return Err(UsbError::IoError);
        }

        self.ensure_controller_running();

        let setup_bytes = setup_packet_bytes(setup);
        let mut setup_dma =
            DmaBuffer::allocate(setup_bytes.len(), 8).map_err(|_| UsbError::IoError)?;
        dma_write_bytes(&mut setup_dma, &setup_bytes);
        self.ensure_dma_segment_matches(setup_dma.physical_address() as u64, "setup")?;

        let mut data_dma = if data.is_empty() {
            None
        } else {
            Some(DmaBuffer::allocate(data.len(), 4096).map_err(|_| UsbError::IoError)?)
        };

        if let Some(buffer) = data_dma.as_mut() {
            self.ensure_dma_segment_matches(buffer.physical_address() as u64, "data")?;
            if setup.request_type & 0x80 == 0 {
                dma_write_bytes(buffer, data);
            }
        }

        let mut td_dma =
            DmaBuffer::allocate(CONTROL_TD_COUNT * size_of::<TransferDescriptor>(), 32)
                .map_err(|_| UsbError::IoError)?;
        self.ensure_dma_segment_matches(td_dma.physical_address() as u64, "qtd")?;

        let td_pool = unsafe {
            std::slice::from_raw_parts_mut(
                td_dma.as_mut_ptr() as *mut TransferDescriptor,
                CONTROL_TD_COUNT,
            )
        };

        let first_td_phys = build_control_transfer(
            setup_dma.physical_address() as u64,
            &setup_bytes,
            data_dma
                .as_ref()
                .map(|buffer| buffer.physical_address() as u64)
                .unwrap_or(0),
            data.len(),
            setup.request_type & 0x80 != 0,
            td_pool,
            td_dma.physical_address() as u64,
        )
        .ok_or(UsbError::IoError)?;

        self.prepare_async_qh(device_address, max_packet_size, first_td_phys);
        self.clear_interrupt_status();
        fence(Ordering::SeqCst);

        let td_count = if data.is_empty() { 2 } else { 3 };
        for _ in 0..CONTROL_TRANSFER_TIMEOUT_POLLS {
            let mut active = false;
            let mut error_token = None;

            for index in 0..td_count {
                let token = read_td_token(&td_dma, index);
                if token & TD_ACTIVE != 0 {
                    active = true;
                }
                if token & (TD_HALTED | TD_BUFERR | TD_BABBLE | TD_XACTERR | TD_MISSED) != 0 {
                    error_token = Some(token);
                    break;
                }
            }

            if let Some(token) = error_token {
                self.disarm_async_qh();
                self.ports[port].last_error = Some(format!("transfer failure token=0x{token:08x}"));
                return Err(map_td_error(token));
            }

            if !active {
                let actual = if data.is_empty() {
                    0
                } else {
                    let data_token = read_td_token(&td_dma, 1);
                    let remaining =
                        ((data_token & TD_TOTAL_BYTES_MASK) >> TD_TOTAL_BYTES_SHIFT) as usize;
                    data.len().saturating_sub(remaining)
                };

                if let Some(buffer) = data_dma.as_ref() {
                    if setup.request_type & 0x80 != 0 && actual != 0 {
                        dma_read_bytes(buffer, &mut data[..actual]);
                    }
                }

                self.disarm_async_qh();
                self.clear_interrupt_status();
                return Ok(actual);
            }

            thread::sleep(WAIT_STEP);
        }

        self.disarm_async_qh();
        self.ports[port].last_error = Some("transfer timed out".to_string());
        Err(UsbError::Timeout)
    }

    fn port_record(&self, port: usize) -> Option<&PortRecord> {
        self.ports.get(port)
    }

    fn execute_control_request(
        &mut self,
        port: usize,
        request: &ControlRequest,
    ) -> Result<Vec<u8>, String> {
        let Some(device) = self
            .ports
            .get(port)
            .and_then(|record| record.device.clone())
        else {
            return Err(format!("port {} is not enumerated", port + 1));
        };

        let mut data = if request.request_type & 0x80 != 0 {
            vec![0_u8; usize::from(request.length)]
        } else {
            request.data.clone()
        };

        let setup = SetupPacket {
            request_type: request.request_type,
            request: request.request,
            value: request.value,
            index: request.index,
            length: request.length,
        };

        let actual = self
            .submit_control_transfer(
                port,
                device.address,
                device.max_packet_size0,
                &setup,
                &mut data,
            )
            .map_err(|err| format!("control transfer failed: {err:?}"))?;

        if request.request_type & 0x80 != 0 {
            data.truncate(actual);
            Ok(data)
        } else {
            Ok(format!("ok transferred={actual}\n").into_bytes())
        }
    }

    fn port_count(&self) -> usize {
        usize::from(self.n_ports)
    }
}

impl UsbHostController for EhciController {
    fn port_count(&self) -> usize {
        usize::from(self.n_ports)
    }

    fn port_status(&self, port: usize) -> Option<PortStatus> {
        if port >= usize::from(self.n_ports) {
            return None;
        }

        Some(decode_port_status(self.read_portsc(port)))
    }

    fn port_reset(&mut self, port: usize) -> bool {
        if port >= self.port_count() {
            return false;
        }

        self.ensure_port_power(port);
        let current = self.read_portsc(port);
        if current & PORT_CONNECT == 0 {
            return false;
        }

        self.clear_port_changes(port, current);
        let reset_value = self.port_write_value(current, PORT_POWER | PORT_RESET, 0, current);
        self.write_portsc(port, reset_value);
        thread::sleep(PORT_RESET_HOLD);

        let after_hold = self.read_portsc(port);
        let clear_reset = self.port_write_value(after_hold, PORT_POWER, PORT_RESET, after_hold);
        self.write_portsc(port, clear_reset);
        thread::sleep(PORT_RESET_SETTLE);

        for _ in 0..100 {
            let status = self.read_portsc(port);
            if status & PORT_OWNER != 0 {
                return false;
            }
            if status & PORT_CONNECT == 0 {
                return false;
            }
            if status & PORT_ENABLE != 0 {
                return true;
            }
            thread::sleep(WAIT_STEP);
        }

        self.handoff_to_companion(port);
        false
    }

    fn control_transfer(
        &mut self,
        device_address: u8,
        setup: &SetupPacket,
        data: &mut [u8],
    ) -> Result<usize, UsbError> {
        let Some(port) = address_port(self, device_address) else {
            return Err(UsbError::NoDevice);
        };
        let Some(max_packet_size0) = self
            .ports
            .get(port)
            .and_then(|record| record.device.as_ref().map(|device| device.max_packet_size0))
        else {
            return Err(UsbError::NoDevice);
        };

        self.submit_control_transfer(port, device_address, max_packet_size0, setup, data)
    }

    fn bulk_transfer(
        &mut self,
        _device_address: u8,
        _endpoint: u8,
        _data: &mut [u8],
        _direction: TransferDirection,
    ) -> Result<usize, UsbError> {
        Err(UsbError::Unsupported)
    }

    fn interrupt_transfer(
        &mut self,
        _device_address: u8,
        _endpoint: u8,
        _data: &mut [u8],
    ) -> Result<usize, UsbError> {
        Err(UsbError::Unsupported)
    }

    fn set_address(&mut self, device_address: u8) -> bool {
        device_address > 0 && device_address <= 127
    }

    fn name(&self) -> &str {
        &self.controller_name
    }
}

impl EhciScheme {
    fn new(controller: Arc<Mutex<EhciController>>) -> Self {
        Self {
            controller,
            handles: BTreeMap::new(),
            next_id: SCHEME_ROOT_ID + 1,
        }
    }

    fn alloc_handle(&mut self, kind: HandleKind) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.handles.insert(
            id,
            HandleState {
                kind,
                response: Vec::new(),
            },
        );
        id
    }

    fn handle(&self, id: usize) -> SysResult<&HandleState> {
        self.handles.get(&id).ok_or(SysError::new(EBADF))
    }

    fn parse_port_component(&self, component: &str) -> SysResult<usize> {
        let Some(raw_port) = component.strip_prefix("port") else {
            return Err(SysError::new(ENOENT));
        };

        let port_number = raw_port
            .parse::<usize>()
            .map_err(|_| SysError::new(ENOENT))?;
        if port_number == 0 {
            return Err(SysError::new(ENOENT));
        }

        let port_index = port_number - 1;
        let controller = lock_controller(&self.controller);
        if port_index >= controller.port_count() {
            return Err(SysError::new(ENOENT));
        }

        Ok(port_index)
    }

    fn resolve_root_path(&self, path: &str) -> SysResult<HandleKind> {
        let mut parts = path.split('/');
        let Some(port_component) = parts.next() else {
            return Err(SysError::new(ENOENT));
        };
        let port = self.parse_port_component(port_component)?;

        match (parts.next(), parts.next()) {
            (None, None) => Ok(HandleKind::PortDir { port }),
            (Some("status"), None) => Ok(HandleKind::Status { port }),
            (Some("descriptor"), None) => Ok(HandleKind::Descriptor { port }),
            (Some("control"), None) => Ok(HandleKind::Control { port }),
            _ => Err(SysError::new(ENOENT)),
        }
    }

    fn resolve_port_child(&self, port: usize, path: &str) -> SysResult<HandleKind> {
        match path {
            "status" => Ok(HandleKind::Status { port }),
            "descriptor" => Ok(HandleKind::Descriptor { port }),
            "control" => Ok(HandleKind::Control { port }),
            _ => Err(SysError::new(ENOENT)),
        }
    }

    fn root_listing(&self) -> Vec<u8> {
        let controller = lock_controller(&self.controller);
        let mut listing = String::new();
        for port in 0..controller.port_count() {
            let _ = writeln!(&mut listing, "port{}", port + 1);
        }
        listing.into_bytes()
    }

    fn status_bytes(&self, port: usize) -> SysResult<Vec<u8>> {
        let controller = lock_controller(&self.controller);
        let Some(record) = controller.port_record(port) else {
            return Err(SysError::new(ENOENT));
        };

        let status = record
            .last_status
            .clone()
            .unwrap_or_else(|| decode_port_status(record.last_portsc));

        let mut out = String::new();
        let _ = writeln!(&mut out, "port={}", port + 1);
        let _ = writeln!(&mut out, "portsc=0x{:08x}", record.last_portsc);
        let _ = writeln!(&mut out, "connected={}", bool_word(status.connected));
        let _ = writeln!(&mut out, "enabled={}", bool_word(status.enabled));
        let _ = writeln!(&mut out, "suspended={}", bool_word(status.suspended));
        let _ = writeln!(&mut out, "over_current={}", bool_word(status.over_current));
        let _ = writeln!(&mut out, "reset={}", bool_word(status.reset));
        let _ = writeln!(&mut out, "power={}", bool_word(status.power));
        let _ = writeln!(&mut out, "low_speed={}", bool_word(status.low_speed));
        let _ = writeln!(&mut out, "high_speed={}", bool_word(status.high_speed));
        let _ = writeln!(&mut out, "test_mode={}", bool_word(status.test_mode));
        let _ = writeln!(&mut out, "indicator={}", bool_word(status.indicator));
        let _ = writeln!(
            &mut out,
            "companion_owned={}",
            bool_word(record.companion_owned)
        );

        if let Some(device) = record.device.as_ref() {
            let _ = writeln!(&mut out, "address={}", device.address);
            let _ = writeln!(&mut out, "vendor_id=0x{:04x}", device.vendor_id);
            let _ = writeln!(&mut out, "product_id=0x{:04x}", device.product_id);
        }

        if let Some(last_error) = record.last_error.as_ref() {
            let _ = writeln!(&mut out, "last_error={}", last_error);
        }

        Ok(out.into_bytes())
    }

    fn descriptor_bytes(&self, port: usize) -> SysResult<Vec<u8>> {
        let controller = lock_controller(&self.controller);
        let Some(record) = controller.port_record(port) else {
            return Err(SysError::new(ENOENT));
        };

        let Some(device) = record.device.as_ref() else {
            return Ok(b"state=unenumerated\n".to_vec());
        };

        let mut out = String::new();
        let _ = writeln!(&mut out, "address={}", device.address);
        let _ = writeln!(&mut out, "vendor_id=0x{:04x}", device.vendor_id);
        let _ = writeln!(&mut out, "product_id=0x{:04x}", device.product_id);
        let _ = writeln!(&mut out, "device_class=0x{:02x}", device.device_class);
        let _ = writeln!(&mut out, "device_subclass=0x{:02x}", device.device_subclass);
        let _ = writeln!(&mut out, "device_protocol=0x{:02x}", device.device_protocol);
        let _ = writeln!(&mut out, "max_packet_size0={}", device.max_packet_size0);
        let _ = writeln!(
            &mut out,
            "device_descriptor={}",
            hex_encode(&device.device_descriptor)
        );

        if !device.config_descriptor.is_empty() {
            let _ = writeln!(
                &mut out,
                "config_descriptor={}",
                hex_encode(&device.config_descriptor)
            );
        }

        Ok(out.into_bytes())
    }

    fn handle_control_write(&mut self, port: usize, buf: &[u8]) -> SysResult<Vec<u8>> {
        let request = parse_control_request(buf).map_err(|_| SysError::new(EINVAL))?;
        let mut controller = lock_controller(&self.controller);
        controller
            .execute_control_request(port, &request)
            .map_err(|_| SysError::new(EINVAL))
    }

    fn handle_bytes(&self, id: usize) -> SysResult<Vec<u8>> {
        if id == SCHEME_ROOT_ID {
            return Ok(self.root_listing());
        }

        let handle = self.handle(id)?;
        match &handle.kind {
            HandleKind::PortDir { .. } => Ok(b"status\ndescriptor\ncontrol\n".to_vec()),
            HandleKind::Status { port } => self.status_bytes(*port),
            HandleKind::Descriptor { port } => self.descriptor_bytes(*port),
            HandleKind::Control { .. } => Ok(handle.response.clone()),
        }
    }

    fn handle_path(&self, id: usize) -> SysResult<String> {
        if id == SCHEME_ROOT_ID {
            return Ok(format!("{SCHEME_NAME}:/"));
        }

        let handle = self.handle(id)?;
        let path = match handle.kind {
            HandleKind::PortDir { port } => format!("{SCHEME_NAME}:/port{}", port + 1),
            HandleKind::Status { port } => format!("{SCHEME_NAME}:/port{}/status", port + 1),
            HandleKind::Descriptor { port } => {
                format!("{SCHEME_NAME}:/port{}/descriptor", port + 1)
            }
            HandleKind::Control { port } => format!("{SCHEME_NAME}:/port{}/control", port + 1),
        };
        Ok(path)
    }
}

impl SchemeSync for EhciScheme {
    fn scheme_root(&mut self) -> SysResult<usize> {
        Ok(SCHEME_ROOT_ID)
    }

    fn openat(
        &mut self,
        dirfd: usize,
        path: &str,
        _flags: usize,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> SysResult<OpenResult> {
        let cleaned = path.trim_matches('/');
        if cleaned.is_empty() {
            if dirfd == SCHEME_ROOT_ID {
                return Ok(OpenResult::ThisScheme {
                    number: SCHEME_ROOT_ID,
                    flags: NewFdFlags::POSITIONED,
                });
            }

            let kind = self.handle(dirfd)?.kind.clone();
            return Ok(OpenResult::ThisScheme {
                number: self.alloc_handle(kind),
                flags: NewFdFlags::POSITIONED,
            });
        }

        let kind = if dirfd == SCHEME_ROOT_ID {
            self.resolve_root_path(cleaned)?
        } else {
            match self.handle(dirfd)?.kind.clone() {
                HandleKind::PortDir { port } => self.resolve_port_child(port, cleaned)?,
                _ => return Err(SysError::new(EACCES)),
            }
        };

        Ok(OpenResult::ThisScheme {
            number: self.alloc_handle(kind),
            flags: NewFdFlags::POSITIONED,
        })
    }

    fn read(
        &mut self,
        id: usize,
        buf: &mut [u8],
        offset: u64,
        _flags: u32,
        _ctx: &CallerCtx,
    ) -> SysResult<usize> {
        let data = self.handle_bytes(id)?;
        copy_with_offset(buf, offset, &data)
    }

    fn write(
        &mut self,
        id: usize,
        buf: &[u8],
        _offset: u64,
        _flags: u32,
        _ctx: &CallerCtx,
    ) -> SysResult<usize> {
        let kind = if id == SCHEME_ROOT_ID {
            return Err(SysError::new(EROFS));
        } else {
            self.handle(id)?.kind.clone()
        };

        match kind {
            HandleKind::Control { port } => {
                let response = self.handle_control_write(port, buf)?;
                if let Some(handle) = self.handles.get_mut(&id) {
                    handle.response = response;
                }
                Ok(buf.len())
            }
            _ => Err(SysError::new(EROFS)),
        }
    }

    fn fstat(&mut self, id: usize, stat: &mut Stat, _ctx: &CallerCtx) -> SysResult<()> {
        let data_len = match id {
            SCHEME_ROOT_ID => self.root_listing().len(),
            _ => self.handle_bytes(id)?.len(),
        };

        stat.st_mode = if id == SCHEME_ROOT_ID {
            MODE_DIR | 0o755
        } else {
            match self.handle(id)?.kind {
                HandleKind::PortDir { .. } => MODE_DIR | 0o755,
                HandleKind::Status { .. } | HandleKind::Descriptor { .. } => MODE_FILE | 0o444,
                HandleKind::Control { .. } => MODE_FILE | 0o644,
            }
        };

        stat.st_size = match u64::try_from(data_len) {
            Ok(size) => size,
            Err(_) => u64::MAX,
        };
        Ok(())
    }

    fn fpath(&mut self, id: usize, buf: &mut [u8], _ctx: &CallerCtx) -> SysResult<usize> {
        let path = self.handle_path(id)?;
        copy_with_offset(buf, 0, path.as_bytes())
    }

    fn fsync(&mut self, id: usize, _ctx: &CallerCtx) -> SysResult<()> {
        if id != SCHEME_ROOT_ID {
            let _ = self.handle(id)?;
        }
        Ok(())
    }

    fn fcntl(&mut self, id: usize, _cmd: usize, _arg: usize, _ctx: &CallerCtx) -> SysResult<usize> {
        if id != SCHEME_ROOT_ID {
            let _ = self.handle(id)?;
        }
        Ok(0)
    }

    fn fevent(&mut self, id: usize, _flags: EventFlags, _ctx: &CallerCtx) -> SysResult<EventFlags> {
        if id != SCHEME_ROOT_ID {
            let _ = self.handle(id)?;
        }
        Ok(EventFlags::empty())
    }

    fn on_close(&mut self, id: usize) {
        if id != SCHEME_ROOT_ID {
            self.handles.remove(&id);
        }
    }
}

fn init_logging() {
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(LevelFilter::Info);
}

fn bool_word(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn lock_controller(shared: &Arc<Mutex<EhciController>>) -> MutexGuard<'_, EhciController> {
    match shared.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!("ehcid: controller mutex was poisoned; continuing with recovered state");
            poisoned.into_inner()
        }
    }
}

fn parse_mmio_bar(config: &[u8]) -> Result<u64, String> {
    let Some(bar0) = read_config_dword(config, 0x10) else {
        return Err("PCI config space is too short to contain BAR0".to_string());
    };

    if bar0 == 0 {
        return Err("BAR0 is zero".to_string());
    }
    if bar0 & 0x1 != 0 {
        return Err("BAR0 is I/O space; EHCI requires MMIO".to_string());
    }

    let mut base = u64::from(bar0 & 0xFFFF_FFF0);
    if bar0 & 0x6 == 0x4 {
        let Some(bar1) = read_config_dword(config, 0x14) else {
            return Err(
                "PCI config space is too short to contain BAR1 for a 64-bit BAR".to_string(),
            );
        };
        base |= u64::from(bar1) << 32;
    }

    Ok(base)
}

fn read_config_dword(config: &[u8], offset: usize) -> Option<u32> {
    if config.len() < offset.saturating_add(4) {
        return None;
    }

    Some(u32::from_le_bytes([
        config[offset],
        config[offset + 1],
        config[offset + 2],
        config[offset + 3],
    ]))
}

fn ensure_dma_segment(has_64bit: bool, phys_addrs: &[u64]) -> Result<u32, String> {
    let mut segment = None;

    for &phys_addr in phys_addrs {
        let current = dma_segment(phys_addr);
        if !has_64bit && current != 0 {
            return Err(format!(
                "controller is 32-bit-only but DMA buffer landed above 4GB: 0x{phys_addr:016x}"
            ));
        }

        match segment {
            Some(existing) if existing != current => {
                return Err(format!(
                    "EHCI data structures must share one DMA segment, found 0x{existing:08x} and 0x{current:08x}"
                ));
            }
            None => segment = Some(current),
            _ => {}
        }
    }

    Ok(segment.unwrap_or(0))
}

fn low32(value: u64) -> u32 {
    (value & u64::from(u32::MAX)) as u32
}

fn dma_segment(value: u64) -> u32 {
    (value >> 32) as u32
}

fn interrupt_threshold(microframes: u8) -> u32 {
    (u32::from(microframes) & 0xFF) << 16
}

fn init_frame_list(frame_list: &mut DmaBuffer) {
    let ptr = frame_list.as_mut_ptr() as *mut u32;
    for index in 0..FRAME_LIST_LEN {
        unsafe {
            std::ptr::write_volatile(ptr.add(index), QH_TERMINATE);
        }
    }
}

fn decode_port_status(portsc: u32) -> PortStatus {
    let line_status = portsc & PORT_LINE_STATUS;
    PortStatus {
        connected: portsc & PORT_CONNECT != 0,
        enabled: portsc & PORT_ENABLE != 0,
        suspended: portsc & PORT_SUSPEND != 0,
        over_current: portsc & PORT_OVER_CURRENT_ACTIVE != 0,
        reset: portsc & PORT_RESET != 0,
        power: portsc & PORT_POWER != 0,
        low_speed: line_status == PORT_LINE_STATUS_K,
        high_speed: portsc & PORT_ENABLE != 0,
        test_mode: portsc & PORT_TEST_CONTROL != 0,
        indicator: portsc & PORT_INDICATOR != 0,
    }
}

fn read_td_token(buffer: &DmaBuffer, index: usize) -> u32 {
    let td_ptr = buffer.as_ptr() as *const TransferDescriptor;
    unsafe { std::ptr::read_volatile(std::ptr::addr_of!((*td_ptr.add(index)).token)) }
}

fn dma_write_bytes(buffer: &mut DmaBuffer, data: &[u8]) {
    if data.is_empty() {
        return;
    }

    unsafe {
        std::ptr::copy_nonoverlapping(data.as_ptr(), buffer.as_mut_ptr(), data.len());
    }
}

fn dma_read_bytes(buffer: &DmaBuffer, output: &mut [u8]) {
    if output.is_empty() {
        return;
    }

    unsafe {
        std::ptr::copy_nonoverlapping(buffer.as_ptr(), output.as_mut_ptr(), output.len());
    }
}

fn map_td_error(token: u32) -> UsbError {
    if token & TD_BABBLE != 0 {
        UsbError::Babble
    } else if token & TD_HALTED != 0 {
        UsbError::Stall
    } else if token & (TD_BUFERR | TD_XACTERR | TD_MISSED) != 0 {
        UsbError::DataError
    } else {
        UsbError::IoError
    }
}

fn setup_packet_bytes(setup: &SetupPacket) -> [u8; 8] {
    let value = setup.value.to_le_bytes();
    let index = setup.index.to_le_bytes();
    let length = setup.length.to_le_bytes();

    [
        setup.request_type,
        setup.request,
        value[0],
        value[1],
        index[0],
        index[1],
        length[0],
        length[1],
    ]
}

fn parse_control_request(buf: &[u8]) -> Result<ControlRequest, String> {
    let text =
        std::str::from_utf8(buf).map_err(|err| format!("control request is not UTF-8: {err}"))?;
    let mut request_type = None;
    let mut request = None;
    let mut value = None;
    let mut index = None;
    let mut length = None;
    let mut data = None;

    for token in text.split_whitespace() {
        let Some((key, raw_value)) = token.split_once('=') else {
            return Err(format!("invalid token '{token}', expected key=value"));
        };

        match key {
            "request_type" | "bmRequestType" => {
                request_type = Some(parse_numeric::<u8>(raw_value)?)
            }
            "request" | "bRequest" => request = Some(parse_numeric::<u8>(raw_value)?),
            "value" | "wValue" => value = Some(parse_numeric::<u16>(raw_value)?),
            "index" | "wIndex" => index = Some(parse_numeric::<u16>(raw_value)?),
            "length" | "wLength" => length = Some(parse_numeric::<u16>(raw_value)?),
            "data" => data = Some(parse_hex_bytes(raw_value)?),
            _ => return Err(format!("unsupported control field '{key}'")),
        }
    }

    let request_type = request_type.ok_or_else(|| "missing request_type".to_string())?;
    let request = request.ok_or_else(|| "missing request".to_string())?;
    let value = value.ok_or_else(|| "missing value".to_string())?;
    let index = index.ok_or_else(|| "missing index".to_string())?;
    let length = length.ok_or_else(|| "missing length".to_string())?;

    if usize::from(length) > MAX_SCHEME_CONTROL_BYTES || usize::from(length) > 0x7FFF {
        return Err(format!(
            "requested control payload {} is outside the supported single-qTD range",
            length
        ));
    }

    let payload = if request_type & 0x80 != 0 {
        if data
            .as_ref()
            .map(|bytes| !bytes.is_empty())
            .unwrap_or(false)
        {
            return Err(
                "IN control requests must not provide an outgoing data payload".to_string(),
            );
        }
        Vec::new()
    } else {
        let bytes = data.unwrap_or_default();
        if bytes.len() != usize::from(length) {
            return Err(format!(
                "OUT control payload length mismatch: expected {}, got {}",
                length,
                bytes.len()
            ));
        }
        bytes
    };

    Ok(ControlRequest {
        request_type,
        request,
        value,
        index,
        length,
        data: payload,
    })
}

fn parse_numeric<T>(value: &str) -> Result<T, String>
where
    T: TryFrom<u64>,
{
    let parsed = if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).map_err(|err| format!("invalid hex value '{value}': {err}"))?
    } else {
        value
            .parse::<u64>()
            .map_err(|err| format!("invalid integer value '{value}': {err}"))?
    };

    T::try_from(parsed).map_err(|_| format!("value '{value}' is out of range"))
}

fn parse_hex_bytes(value: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = value
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace() && *ch != ':' && *ch != '-')
        .collect();

    if cleaned.is_empty() {
        return Ok(Vec::new());
    }
    if cleaned.len() % 2 != 0 {
        return Err(format!("hex payload '{value}' has an odd number of digits"));
    }

    let mut bytes = Vec::with_capacity(cleaned.len() / 2);
    for chunk in cleaned.as_bytes().chunks(2) {
        let chunk = std::str::from_utf8(chunk)
            .map_err(|err| format!("invalid hex payload '{value}': {err}"))?;
        let byte = u8::from_str_radix(chunk, 16)
            .map_err(|err| format!("invalid hex byte '{chunk}' in payload '{value}': {err}"))?;
        bytes.push(byte);
    }

    Ok(bytes)
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    for (index, byte) in bytes.iter().enumerate() {
        if index != 0 {
            out.push(' ');
        }
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

fn copy_with_offset(buf: &mut [u8], offset: u64, data: &[u8]) -> SysResult<usize> {
    let offset = usize::try_from(offset).map_err(|_| SysError::new(EINVAL))?;
    if offset >= data.len() {
        return Ok(0);
    }

    let count = (data.len() - offset).min(buf.len());
    buf[..count].copy_from_slice(&data[offset..offset + count]);
    Ok(count)
}

fn address_port(controller: &EhciController, address: u8) -> Option<usize> {
    controller.ports.iter().position(|record| {
        record
            .device
            .as_ref()
            .map(|device| device.address == address)
            .unwrap_or(false)
    })
}

fn poll_ports_loop(controller: Arc<Mutex<EhciController>>) {
    loop {
        {
            let mut controller = lock_controller(&controller);
            controller.poll_ports_once();
        }
        thread::sleep(PORT_POLL_INTERVAL);
    }
}

fn run_scheme(controller: Arc<Mutex<EhciController>>) -> Result<(), String> {
    let socket =
        Socket::create().map_err(|err| format!("failed to create scheme socket: {err}"))?;
    let mut scheme = EhciScheme::new(controller);
    let mut state = SchemeState::new();

    register_sync_scheme(&socket, SCHEME_NAME, &mut scheme)
        .map_err(|err| format!("failed to register scheme:{SCHEME_NAME}: {err}"))?;

    libredox::call::setrens(0, 0)
        .map_err(|err| format!("failed to enter null namespace: {err}"))?;

    info!("ehcid: registered /scheme/{SCHEME_NAME}");
    info!("ehcid: ready — polling for device connections");

    loop {
        let request = match socket.next_request(SignalBehavior::Restart) {
            Ok(Some(request)) => request,
            Ok(None) => {
                info!("ehcid: scheme socket closed, shutting down");
                break;
            }
            Err(err) => return Err(format!("failed to read scheme request: {err}")),
        };

        if let redox_scheme::RequestKind::Call(request) = request.kind() {
            let response = request.handle_sync(&mut scheme, &mut state);
            socket
                .write_response(response, SignalBehavior::Restart)
                .map_err(|err| format!("failed to write scheme response: {err}"))?;
        }
    }

    Ok(())
}

fn main() {
    init_logging();

    let channel_fd = match env::var("PCID_CLIENT_CHANNEL") {
        Ok(raw) => match raw.parse::<usize>() {
            Ok(fd) => fd,
            Err(_) => {
                error!("invalid PCID_CLIENT_CHANNEL");
                process::exit(1);
            }
        },
        Err(_) => {
            error!("PCID_CLIENT_CHANNEL not set");
            process::exit(1);
        }
    };

    let device_path = match env::var("PCID_DEVICE_PATH") {
        Ok(path) if !path.is_empty() => path,
        Ok(_) => {
            error!("PCID_DEVICE_PATH is empty");
            process::exit(1);
        }
        Err(_) => {
            error!("PCID_DEVICE_PATH not set");
            process::exit(1);
        }
    };

    let controller = match EhciController::new(&device_path, channel_fd) {
        Ok(controller) => Arc::new(Mutex::new(controller)),
        Err(err) => {
            error!("{err}");
            process::exit(1);
        }
    };

    if let Err(err) = thread::Builder::new()
        .name("ehci-port-poll".to_string())
        .spawn({
            let controller = Arc::clone(&controller);
            move || poll_ports_loop(controller)
        })
    {
        error!("failed to spawn EHCI port polling thread: {err}");
        process::exit(1);
    }

    if let Err(err) = run_scheme(controller) {
        error!("{err}");
        process::exit(1);
    }
}
