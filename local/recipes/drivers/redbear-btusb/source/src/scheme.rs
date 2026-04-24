//! HCI scheme daemon (`scheme:hciN`) for Bluetooth USB transport.
//!
//! Exposes an HCI controller through the Redox scheme filesystem so that
//! the host daemon (redbear-btctl) can send HCI commands and receive HCI
//! events through standard file I/O.

use std::collections::BTreeMap;

use redox_scheme::scheme::SchemeSync;
use redox_scheme::{CallerCtx, OpenResult};
use syscall::error::{Error, Result, EBADF, EINVAL, ENOENT, EROFS};
use syscall::flag::{EventFlags, MODE_DIR, MODE_FILE};
use syscall::schemev2::NewFdFlags;
use syscall::Stat;

use crate::hci::{
    acl_to_att, att_to_acl, parse_read_by_group_type_rsp, parse_read_by_type_rsp, parse_read_rsp,
    is_att_error, parse_att_error, AttPdu, GattCharacteristic, GattService,
    ATT_ERROR_RSP, ATT_READ_BY_GROUP_TYPE_REQ, ATT_READ_BY_GROUP_TYPE_RSP, ATT_READ_BY_TYPE_RSP,
    ATT_READ_RSP, UUID_CHARACTERISTIC,
    cmd_disconnect, cmd_le_create_connection, cmd_le_set_scan_enable, HciAcl, HciCommand, HciEvent,
};
use crate::usb_transport::UsbHciTransport;
use crate::ControllerInfo;

const SCHEME_ROOT_ID: usize = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
enum HandleKind {
    Root,
    Status,
    Info,
    Command,
    Events,
    AclOut,
    AclIn,
    LeScan,
    LeScanResults,
    Connect,
    Disconnect,
    Connections,
    GattDiscoverServices,
    GattDiscoverChars,
    GattReadChar,
    GattServices,
    GattCharacteristics,
}

pub struct HciScheme {
    transport: Box<dyn UsbHciTransport>,
    controller_info: ControllerInfo,
    le_scan_active: bool,
    le_scan_results: Vec<String>,
    le_connections: Vec<(u16, [u8; 6])>,
    gatt_services: Vec<GattService>,
    gatt_characteristics: Vec<GattCharacteristic>,
    gatt_read_result: Option<Vec<u8>>,
    gatt_last_error: Option<String>,
    next_id: usize,
    handles: BTreeMap<usize, HandleKind>,
}

impl HciScheme {
    pub fn new(transport: Box<dyn UsbHciTransport>, controller_info: ControllerInfo) -> Self {
        Self {
            transport,
            controller_info,
            le_scan_active: false,
            le_scan_results: Vec::new(),
            le_connections: Vec::new(),
            gatt_services: Vec::new(),
            gatt_characteristics: Vec::new(),
            gatt_read_result: None,
            gatt_last_error: None,
            next_id: SCHEME_ROOT_ID + 1,
            handles: BTreeMap::new(),
        }
    }

    pub fn new_for_test(transport: Box<dyn UsbHciTransport>, controller_info: ControllerInfo) -> Self {
        Self::new(transport, controller_info)
    }

    fn alloc_handle(&mut self, kind: HandleKind) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.handles.insert(id, kind);
        id
    }

    fn handle(&self, id: usize) -> Result<&HandleKind> {
        if id == SCHEME_ROOT_ID {
            static ROOT: HandleKind = HandleKind::Root;
            return Ok(&ROOT);
        }
        self.handles.get(&id).ok_or(Error::new(EBADF))
    }

    fn format_status(&self) -> String {
        let state_str = match self.controller_info.state {
            crate::ControllerState::Closed => "closed",
            crate::ControllerState::Initializing => "initializing",
            crate::ControllerState::Active => "active",
            crate::ControllerState::Error => "error",
        };
        let mut lines = vec![format!("controller_state={state_str}")];
        if let Some(addr) = &self.controller_info.bd_address {
            lines.push(format!(
                "bd_address={:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                addr[5], addr[4], addr[3], addr[2], addr[1], addr[0]
            ));
        }
        if let Some(version) = self.controller_info.hci_version {
            lines.push(format!("hci_version={version}"));
        }
        if let Some(revision) = self.controller_info.hci_revision {
            lines.push(format!("hci_revision={revision}"));
        }
        if let Some(manufacturer) = self.controller_info.manufacturer_name {
            lines.push(format!("manufacturer={manufacturer}"));
        }
        lines.push(format!("le_scan_active={}", self.le_scan_active));
        lines.push(format!("le_connections={}", self.le_connections.len()));
        if let Some(err) = &self.controller_info.init_error {
            lines.push(format!("init_error={err}"));
        }
        format!("{}\n", lines.join("\n"))
    }

    fn format_info(&self) -> String {
        let mut lines = Vec::new();
        if let Some(addr) = &self.controller_info.bd_address {
            lines.push(format!(
                "bd_address={:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                addr[5], addr[4], addr[3], addr[2], addr[1], addr[0]
            ));
        } else {
            lines.push("bd_address=00:00:00:00:00:00".to_string());
        }
        lines.push(format!(
            "hci_version={}",
            self.controller_info.hci_version.unwrap_or(0)
        ));
        lines.push(format!(
            "hci_revision={}",
            self.controller_info.hci_revision.unwrap_or(0)
        ));
        lines.push(format!(
            "manufacturer={}",
            self.controller_info.manufacturer_name.unwrap_or(0)
        ));
        format!("{}\n", lines.join("\n"))
    }

    fn format_scan_results(&self) -> String {
        if self.le_scan_results.is_empty() {
            "\n".to_string()
        } else {
            format!("{}\n", self.le_scan_results.join("\n"))
        }
    }

    fn format_connections(&self) -> String {
        if self.le_connections.is_empty() {
            "\n".to_string()
        } else {
            let lines: Vec<String> = self
                .le_connections
                .iter()
                .map(|(handle, addr)| {
                    format!(
                        "handle={handle:04X};addr={:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                        addr[5], addr[4], addr[3], addr[2], addr[1], addr[0]
                    )
                })
                .collect();
            format!("{}\n", lines.join("\n"))
        }
    }

    fn parse_addr(text: &str) -> Option<[u8; 6]> {
        let cleaned = text.trim();
        let prefix = cleaned.strip_prefix("addr=")?;
        let parts: Vec<&str> = prefix.split(':').collect();
        if parts.len() != 6 {
            return None;
        }
        let bytes: Vec<u8> = parts.iter().filter_map(|p| u8::from_str_radix(p, 16).ok()).collect();
        if bytes.len() != 6 {
            return None;
        }
        let mut addr = [0u8; 6];
        addr.copy_from_slice(&bytes);
        Some(addr)
    }

    fn parse_handle(text: &str) -> Option<u16> {
        let cleaned = text.trim();
        let prefix = cleaned.strip_prefix("handle=")?;
        let hex_str = prefix.strip_prefix("0x").unwrap_or(prefix);
        u16::from_str_radix(hex_str, 16).ok()
    }

    fn parse_gatt_kv<'a>(text: &'a str, key: &str) -> Option<&'a str> {
        for part in text.split(';') {
            let part = part.trim();
            if let Some(val) = part.strip_prefix(key) {
                let val = val.strip_prefix('=').unwrap_or(val);
                return Some(val);
            }
        }
        None
    }

    fn parse_gatt_handle(text: &str) -> Option<u16> {
        Self::parse_gatt_kv(text, "handle").and_then(|v| {
            let hex_str = v.strip_prefix("0x").unwrap_or(v);
            u16::from_str_radix(hex_str, 16).ok()
        })
    }

    fn parse_gatt_start(text: &str) -> Option<u16> {
        Self::parse_gatt_kv(text, "start").and_then(|v| {
            let hex_str = v.strip_prefix("0x").unwrap_or(v);
            u16::from_str_radix(hex_str, 16).ok()
        })
    }

    fn parse_gatt_end(text: &str) -> Option<u16> {
        Self::parse_gatt_kv(text, "end").and_then(|v| {
            let hex_str = v.strip_prefix("0x").unwrap_or(v);
            u16::from_str_radix(hex_str, 16).ok()
        })
    }

    fn parse_gatt_addr(text: &str) -> Option<u16> {
        Self::parse_gatt_kv(text, "addr").and_then(|v| {
            let hex_str = v.strip_prefix("0x").unwrap_or(v);
            u16::from_str_radix(hex_str, 16).ok()
        })
    }

    fn format_gatt_services(&self) -> String {
        if self.gatt_services.is_empty() {
            return "\n".to_string();
        }
        let lines: Vec<String> = self
            .gatt_services
            .iter()
            .map(|svc| {
                let uuid_str = if svc.uuid.len() == 2 {
                    format!("{:04X}", u16::from_le_bytes([svc.uuid[0], svc.uuid[1]]))
                } else {
                    svc.uuid.iter().rev().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join("")
                };
                format!(
                    "service=start_handle={:04X};end_handle={:04X};uuid={}",
                    svc.start_handle, svc.end_handle, uuid_str
                )
            })
            .collect();
        format!("{}\n", lines.join("\n"))
    }

    fn format_gatt_characteristics(&self) -> String {
        if self.gatt_characteristics.is_empty() {
            return "\n".to_string();
        }
        let lines: Vec<String> = self
            .gatt_characteristics
            .iter()
            .map(|ch| {
                let uuid_str = if ch.uuid.len() == 2 {
                    format!("{:04X}", u16::from_le_bytes([ch.uuid[0], ch.uuid[1]]))
                } else {
                    ch.uuid.iter().rev().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join("")
                };
                format!(
                    "char=handle={:04X};value_handle={:04X};properties={:02X};uuid={}",
                    ch.handle, ch.value_handle, ch.properties, uuid_str
                )
            })
            .collect();
        format!("{}\n", lines.join("\n"))
    }

    fn perform_gatt_discover_services(&mut self, conn_handle: u16) -> Result<()> {
        let att_req = AttPdu::read_by_group_type_req(0x0001, 0xFFFF);
        let acl = att_to_acl(conn_handle, &att_req);
        self.transport.send_acl(&acl).map_err(|_| Error::new(EINVAL))?;
        let acl_rsp = self.transport.recv_acl().map_err(|_| Error::new(EINVAL))?;
        match acl_rsp {
            Some(acl_rsp) => {
                match acl_to_att(&acl_rsp) {
                    Some(att_rsp) => {
                        if is_att_error(&att_rsp) {
                            if let Some((_req_op, handle, err_code)) = parse_att_error(&att_rsp) {
                                self.gatt_last_error = Some(format!(
                                    "ATT error: req_opcode=0x{:02X} handle=0x{:04X} error_code=0x{:02X}",
                                    _req_op, handle, err_code
                                ));
                            }
                            self.gatt_services.clear();
                            return Ok(());
                        }
                        match parse_read_by_group_type_rsp(&att_rsp) {
                            Ok(services) => {
                                self.gatt_services = services;
                                self.gatt_last_error = None;
                            }
                            Err(e) => {
                                self.gatt_last_error = Some(e);
                                self.gatt_services.clear();
                            }
                        }
                    }
                    None => {
                        self.gatt_last_error = Some("ACL response not on ATT channel".to_string());
                        self.gatt_services.clear();
                    }
                }
            }
            None => {
                self.gatt_last_error = Some("no ACL response received".to_string());
                self.gatt_services.clear();
            }
        }
        Ok(())
    }

    fn perform_gatt_discover_chars(&mut self, conn_handle: u16, start: u16, end: u16) -> Result<()> {
        let att_req = AttPdu::read_by_type_req(start, end, UUID_CHARACTERISTIC);
        let acl = att_to_acl(conn_handle, &att_req);
        self.transport.send_acl(&acl).map_err(|_| Error::new(EINVAL))?;
        let acl_rsp = self.transport.recv_acl().map_err(|_| Error::new(EINVAL))?;
        match acl_rsp {
            Some(acl_rsp) => {
                match acl_to_att(&acl_rsp) {
                    Some(att_rsp) => {
                        if is_att_error(&att_rsp) {
                            if let Some((_req_op, handle, err_code)) = parse_att_error(&att_rsp) {
                                self.gatt_last_error = Some(format!(
                                    "ATT error: req_opcode=0x{:02X} handle=0x{:04X} error_code=0x{:02X}",
                                    _req_op, handle, err_code
                                ));
                            }
                            self.gatt_characteristics.clear();
                            return Ok(());
                        }
                        match parse_read_by_type_rsp(&att_rsp) {
                            Ok(chars) => {
                                self.gatt_characteristics = chars;
                                self.gatt_last_error = None;
                            }
                            Err(e) => {
                                self.gatt_last_error = Some(e);
                                self.gatt_characteristics.clear();
                            }
                        }
                    }
                    None => {
                        self.gatt_last_error = Some("ACL response not on ATT channel".to_string());
                        self.gatt_characteristics.clear();
                    }
                }
            }
            None => {
                self.gatt_last_error = Some("no ACL response received".to_string());
                self.gatt_characteristics.clear();
            }
        }
        Ok(())
    }

    fn perform_gatt_read_char(&mut self, conn_handle: u16, attr_handle: u16) -> Result<()> {
        let att_req = AttPdu::read_req(attr_handle);
        let acl = att_to_acl(conn_handle, &att_req);
        self.transport.send_acl(&acl).map_err(|_| Error::new(EINVAL))?;
        let acl_rsp = self.transport.recv_acl().map_err(|_| Error::new(EINVAL))?;
        match acl_rsp {
            Some(acl_rsp) => {
                match acl_to_att(&acl_rsp) {
                    Some(att_rsp) => {
                        if is_att_error(&att_rsp) {
                            if let Some((_req_op, handle, err_code)) = parse_att_error(&att_rsp) {
                                self.gatt_last_error = Some(format!(
                                    "ATT error: req_opcode=0x{:02X} handle=0x{:04X} error_code=0x{:02X}",
                                    _req_op, handle, err_code
                                ));
                            }
                            self.gatt_read_result = None;
                            return Ok(());
                        }
                        match parse_read_rsp(&att_rsp) {
                            Ok(value) => {
                                self.gatt_read_result = Some(value);
                                self.gatt_last_error = None;
                            }
                            Err(e) => {
                                self.gatt_last_error = Some(e);
                                self.gatt_read_result = None;
                            }
                        }
                    }
                    None => {
                        self.gatt_last_error = Some("ACL response not on ATT channel".to_string());
                        self.gatt_read_result = None;
                    }
                }
            }
            None => {
                self.gatt_last_error = Some("no ACL response received".to_string());
                self.gatt_read_result = None;
            }
        }
        Ok(())
    }

    fn read_handle(&mut self, kind: &HandleKind) -> Result<Vec<u8>> {
        match kind {
            HandleKind::Root => Ok("status\ninfo\ncommand\nevents\nacl-out\nacl-in\nle-scan\nle-scan-results\nconnect\ndisconnect\nconnections\ngatt-discover-services\ngatt-discover-chars\ngatt-read-char\ngatt-services\ngatt-characteristics\n".to_string().into_bytes()),
            HandleKind::Status => Ok(self.format_status().into_bytes()),
            HandleKind::Info => Ok(self.format_info().into_bytes()),
            HandleKind::LeScanResults => Ok(self.format_scan_results().into_bytes()),
            HandleKind::Connections => Ok(self.format_connections().into_bytes()),
            HandleKind::Events => {
                let event = self
                    .transport
                    .recv_event()
                    .map_err(|_| Error::new(EINVAL))?;
                match event {
                    Some(event) => Ok(event_to_bytes(&event)),
                    None => Ok(Vec::new()),
                }
            }
            HandleKind::AclIn => {
                let acl = self.transport.recv_acl().map_err(|_| Error::new(EINVAL))?;
                match acl {
                    Some(acl) => Ok(acl.to_bytes()),
                    None => Ok(Vec::new()),
                }
            }
            HandleKind::GattDiscoverServices | HandleKind::GattServices => {
                Ok(self.format_gatt_services().into_bytes())
            }
            HandleKind::GattDiscoverChars | HandleKind::GattCharacteristics => {
                Ok(self.format_gatt_characteristics().into_bytes())
            }
            HandleKind::GattReadChar => {
                match &self.gatt_read_result {
                    Some(data) => Ok(data.clone()),
                    None => Ok(Vec::new()),
                }
            }
            _ => Ok(Vec::new()),
        }
    }

    fn write_handle(&mut self, kind: &HandleKind, buf: &[u8]) -> Result<()> {
        match kind {
            HandleKind::Command => {
                let cmd = HciCommand::from_bytes(buf).map_err(|_| Error::new(EINVAL))?;
                self.transport
                    .send_command(&cmd)
                    .map_err(|_| Error::new(EINVAL))?;
                Ok(())
            }
            HandleKind::AclOut => {
                let acl = HciAcl::from_bytes(buf).map_err(|_| Error::new(EINVAL))?;
                self.transport
                    .send_acl(&acl)
                    .map_err(|_| Error::new(EINVAL))?;
                Ok(())
            }
            HandleKind::LeScan => {
                let text =
                    std::str::from_utf8(buf).map_err(|_| Error::new(EINVAL))?;
                match text.trim() {
                    "start" => {
                        let cmd = cmd_le_set_scan_enable(0x01, 0x00);
                        self.transport
                            .send_command(&cmd)
                            .map_err(|_| Error::new(EINVAL))?;
                        self.le_scan_active = true;
                        self.le_scan_results.clear();
                        Ok(())
                    }
                    "stop" => {
                        let cmd = cmd_le_set_scan_enable(0x00, 0x00);
                        self.transport
                            .send_command(&cmd)
                            .map_err(|_| Error::new(EINVAL))?;
                        self.le_scan_active = false;
                        Ok(())
                    }
                    _ => Err(Error::new(EINVAL)),
                }
            }
            HandleKind::Connect => {
                let text =
                    std::str::from_utf8(buf).map_err(|_| Error::new(EINVAL))?;
                let addr = Self::parse_addr(text).ok_or(Error::new(EINVAL))?;
                let cmd = cmd_le_create_connection(
                    0x0060, 0x0030, 0x00, 0x00, &addr, 0x00,
                    0x0006, 0x000C, 0x0000, 0x00C8, 0x0001, 0x0002,
                );
                self.transport
                    .send_command(&cmd)
                    .map_err(|_| Error::new(EINVAL))?;
                Ok(())
            }
            HandleKind::Disconnect => {
                let text =
                    std::str::from_utf8(buf).map_err(|_| Error::new(EINVAL))?;
                let handle_val = Self::parse_handle(text).ok_or(Error::new(EINVAL))?;
                let cmd = cmd_disconnect(handle_val, 0x13);
                self.transport
                    .send_command(&cmd)
                    .map_err(|_| Error::new(EINVAL))?;
                Ok(())
            }
            HandleKind::GattDiscoverServices => {
                let text =
                    std::str::from_utf8(buf).map_err(|_| Error::new(EINVAL))?;
                let conn_handle = Self::parse_handle(text).ok_or(Error::new(EINVAL))?;
                self.perform_gatt_discover_services(conn_handle)
            }
            HandleKind::GattDiscoverChars => {
                let text =
                    std::str::from_utf8(buf).map_err(|_| Error::new(EINVAL))?;
                let conn_handle = Self::parse_gatt_handle(text).ok_or(Error::new(EINVAL))?;
                let start = Self::parse_gatt_start(text).ok_or(Error::new(EINVAL))?;
                let end = Self::parse_gatt_end(text).ok_or(Error::new(EINVAL))?;
                self.perform_gatt_discover_chars(conn_handle, start, end)
            }
            HandleKind::GattReadChar => {
                let text =
                    std::str::from_utf8(buf).map_err(|_| Error::new(EINVAL))?;
                let conn_handle = Self::parse_gatt_handle(text).ok_or(Error::new(EINVAL))?;
                let addr = Self::parse_gatt_addr(text).ok_or(Error::new(EINVAL))?;
                self.perform_gatt_read_char(conn_handle, addr)
            }
            _ => Err(Error::new(EROFS)),
        }
    }
}

fn event_to_bytes(event: &HciEvent) -> Vec<u8> {
    let param_len = u8::try_from(event.parameters.len()).unwrap_or(0xFF);
    let mut buf = Vec::with_capacity(2 + event.parameters.len());
    buf.push(event.event_code);
    buf.push(param_len);
    buf.extend_from_slice(&event.parameters);
    buf
}

impl SchemeSync for HciScheme {
    fn scheme_root(&mut self) -> Result<usize> {
        Ok(SCHEME_ROOT_ID)
    }

    fn openat(
        &mut self,
        dirfd: usize,
        path: &str,
        _flags: usize,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<OpenResult> {
        let kind = if dirfd == SCHEME_ROOT_ID {
            match path.trim_matches('/') {
                "" => HandleKind::Root,
                "status" => HandleKind::Status,
                "info" => HandleKind::Info,
                "command" => HandleKind::Command,
                "events" => HandleKind::Events,
                "acl-out" => HandleKind::AclOut,
                "acl-in" => HandleKind::AclIn,
                "le-scan" => HandleKind::LeScan,
                "le-scan-results" => HandleKind::LeScanResults,
                "connect" => HandleKind::Connect,
                "disconnect" => HandleKind::Disconnect,
                "connections" => HandleKind::Connections,
                "gatt-discover-services" => HandleKind::GattDiscoverServices,
                "gatt-discover-chars" => HandleKind::GattDiscoverChars,
                "gatt-read-char" => HandleKind::GattReadChar,
                "gatt-services" => HandleKind::GattServices,
                "gatt-characteristics" => HandleKind::GattCharacteristics,
                _ => return Err(Error::new(ENOENT)),
            }
        } else {
            let parent = self.handle(dirfd)?.clone();
            match parent {
                HandleKind::Root => match path.trim_matches('/') {
                    "status" => HandleKind::Status,
                    "info" => HandleKind::Info,
                    "command" => HandleKind::Command,
                    "events" => HandleKind::Events,
                    "acl-out" => HandleKind::AclOut,
                    "acl-in" => HandleKind::AclIn,
                    "le-scan" => HandleKind::LeScan,
                    "le-scan-results" => HandleKind::LeScanResults,
                    "connect" => HandleKind::Connect,
                    "disconnect" => HandleKind::Disconnect,
                    "connections" => HandleKind::Connections,
                    "gatt-discover-services" => HandleKind::GattDiscoverServices,
                    "gatt-discover-chars" => HandleKind::GattDiscoverChars,
                    "gatt-read-char" => HandleKind::GattReadChar,
                    "gatt-services" => HandleKind::GattServices,
                    "gatt-characteristics" => HandleKind::GattCharacteristics,
                    _ => return Err(Error::new(ENOENT)),
                },
                _ => return Err(Error::new(EINVAL)),
            }
        };

        Ok(OpenResult::ThisScheme {
            number: self.alloc_handle(kind),
            flags: NewFdFlags::empty(),
        })
    }

    fn read(
        &mut self,
        id: usize,
        buf: &mut [u8],
        offset: u64,
        _flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let kind = self.handle(id)?.clone();
        let data = self.read_handle(&kind)?;
        let offset = usize::try_from(offset).map_err(|_| Error::new(EINVAL))?;
        if offset >= data.len() {
            return Ok(0);
        }
        let count = (data.len() - offset).min(buf.len());
        buf[..count].copy_from_slice(&data[offset..offset + count]);
        Ok(count)
    }

    fn write(
        &mut self,
        id: usize,
        buf: &[u8],
        _offset: u64,
        _flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let kind = self.handle(id)?.clone();
        let len = buf.len();
        self.write_handle(&kind, buf)?;
        Ok(len)
    }

    fn fstat(&mut self, id: usize, stat: &mut Stat, _ctx: &CallerCtx) -> Result<()> {
        let kind = self.handle(id)?;
        stat.st_mode = match kind {
            HandleKind::Root => MODE_DIR | 0o755,
            _ => MODE_FILE | 0o444,
        };
        Ok(())
    }

    fn fsync(&mut self, id: usize, _ctx: &CallerCtx) -> Result<()> {
        let _ = self.handle(id)?;
        Ok(())
    }

    fn fpath(&mut self, id: usize, buf: &mut [u8], _ctx: &CallerCtx) -> Result<usize> {
        let path = match self.handle(id)? {
            HandleKind::Root => "hci0:/".to_string(),
            HandleKind::Status => "hci0:/status".to_string(),
            HandleKind::Info => "hci0:/info".to_string(),
            HandleKind::Command => "hci0:/command".to_string(),
            HandleKind::Events => "hci0:/events".to_string(),
            HandleKind::AclOut => "hci0:/acl-out".to_string(),
            HandleKind::AclIn => "hci0:/acl-in".to_string(),
            HandleKind::LeScan => "hci0:/le-scan".to_string(),
            HandleKind::LeScanResults => "hci0:/le-scan-results".to_string(),
            HandleKind::Connect => "hci0:/connect".to_string(),
            HandleKind::Disconnect => "hci0:/disconnect".to_string(),
            HandleKind::Connections => "hci0:/connections".to_string(),
            HandleKind::GattDiscoverServices => "hci0:/gatt-discover-services".to_string(),
            HandleKind::GattDiscoverChars => "hci0:/gatt-discover-chars".to_string(),
            HandleKind::GattReadChar => "hci0:/gatt-read-char".to_string(),
            HandleKind::GattServices => "hci0:/gatt-services".to_string(),
            HandleKind::GattCharacteristics => "hci0:/gatt-characteristics".to_string(),
        };
        let bytes = path.as_bytes();
        let count = bytes.len().min(buf.len());
        buf[..count].copy_from_slice(&bytes[..count]);
        Ok(count)
    }

    fn fevent(&mut self, id: usize, _flags: EventFlags, _ctx: &CallerCtx) -> Result<EventFlags> {
        let _ = self.handle(id)?;
        Ok(EventFlags::empty())
    }

    fn on_close(&mut self, id: usize) {
        if id != SCHEME_ROOT_ID {
            self.handles.remove(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hci::{
        EVT_COMMAND_COMPLETE, OP_DISCONNECT, OP_LE_CREATE_CONNECTION, OP_LE_SET_SCAN_ENABLE,
    };
    use crate::usb_transport::TransportState;
    use std::cell::RefCell;
    use std::io;
    use std::rc::Rc;

    struct TestTransportInner {
        sent_commands: Vec<HciCommand>,
        sent_acl: Vec<HciAcl>,
        pending_events: Vec<HciEvent>,
        pending_acl: Vec<HciAcl>,
    }

    impl TestTransportInner {
        fn new() -> Self {
            Self {
                sent_commands: Vec::new(),
                sent_acl: Vec::new(),
                pending_events: Vec::new(),
                pending_acl: Vec::new(),
            }
        }
    }

    struct TestTransport {
        inner: Rc<RefCell<TestTransportInner>>,
    }

    impl TestTransport {
        fn new(inner: &Rc<RefCell<TestTransportInner>>) -> Self {
            Self { inner: Rc::clone(inner) }
        }
    }

    impl UsbHciTransport for TestTransport {
        fn send_command(&mut self, command: &HciCommand) -> io::Result<()> {
            self.inner.borrow_mut().sent_commands.push(command.clone());
            Ok(())
        }
        fn recv_event(&mut self) -> io::Result<Option<HciEvent>> {
            let mut inner = self.inner.borrow_mut();
            Ok(if inner.pending_events.is_empty() {
                None
            } else {
                Some(inner.pending_events.remove(0))
            })
        }
        fn send_acl(&mut self, acl: &HciAcl) -> io::Result<()> {
            self.inner.borrow_mut().sent_acl.push(acl.clone());
            Ok(())
        }
        fn recv_acl(&mut self) -> io::Result<Option<HciAcl>> {
            let mut inner = self.inner.borrow_mut();
            Ok(if inner.pending_acl.is_empty() {
                None
            } else {
                Some(inner.pending_acl.remove(0))
            })
        }
        fn state(&self) -> TransportState {
            TransportState::Active
        }
        fn close(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn active_info() -> ControllerInfo {
        ControllerInfo {
            state: crate::ControllerState::Active,
            bd_address: Some([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]),
            hci_version: Some(9),
            hci_revision: Some(1),
            manufacturer_name: Some(2),
            init_error: None,
        }
    }

    fn make_scheme() -> HciScheme {
        let inner = Rc::new(RefCell::new(TestTransportInner::new()));
        HciScheme::new_for_test(Box::new(TestTransport::new(&inner)), active_info())
    }

    fn make_scheme_with_inner() -> (HciScheme, Rc<RefCell<TestTransportInner>>) {
        let inner = Rc::new(RefCell::new(TestTransportInner::new()));
        let scheme = HciScheme::new_for_test(Box::new(TestTransport::new(&inner)), active_info());
        (scheme, inner)
    }

    fn alloc(scheme: &mut HciScheme, kind: HandleKind) -> usize {
        scheme.alloc_handle(kind)
    }

    #[test]
    fn root_lists_all_nodes() {
        let mut scheme = make_scheme();
        let data = scheme.read_handle(&HandleKind::Root).unwrap();
        let text = String::from_utf8_lossy(&data);
        assert!(text.contains("status"));
        assert!(text.contains("info"));
        assert!(text.contains("command"));
        assert!(text.contains("events"));
        assert!(text.contains("acl-out"));
        assert!(text.contains("acl-in"));
        assert!(text.contains("le-scan"));
        assert!(text.contains("le-scan-results"));
        assert!(text.contains("connect"));
        assert!(text.contains("disconnect"));
        assert!(text.contains("connections"));
    }

    #[test]
    fn read_status_shows_active_state() {
        let mut scheme = make_scheme();
        let data = scheme.read_handle(&HandleKind::Status).unwrap();
        let text = String::from_utf8_lossy(&data);
        assert!(text.contains("controller_state=active"));
        assert!(text.contains("bd_address=FF:EE:DD:CC:BB:AA"));
        assert!(text.contains("hci_version=9"));
        assert!(text.contains("hci_revision=1"));
        assert!(text.contains("manufacturer=2"));
        assert!(text.contains("le_scan_active=false"));
        assert!(text.contains("le_connections=0"));
    }

    #[test]
    fn read_info_shows_bd_address_and_version() {
        let mut scheme = make_scheme();
        let data = scheme.read_handle(&HandleKind::Info).unwrap();
        let text = String::from_utf8_lossy(&data);
        assert!(text.contains("bd_address=FF:EE:DD:CC:BB:AA"));
        assert!(text.contains("hci_version=9"));
        assert!(text.contains("hci_revision=1"));
        assert!(text.contains("manufacturer=2"));
    }

    #[test]
    fn write_command_sends_correct_opcode_to_transport() {
        let (mut scheme, inner) = make_scheme_with_inner();
        let wire = vec![0x03, 0x0C, 0x00];
        scheme.write_handle(&HandleKind::Command, &wire).unwrap();
        let sent = inner.borrow_mut().sent_commands.clone();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].opcode, 0x0C03);
    }

    #[test]
    fn write_command_with_params_round_trips() {
        let (mut scheme, inner) = make_scheme_with_inner();
        let wire = vec![0x09, 0x10, 0x02, 0xAA, 0xBB];
        scheme.write_handle(&HandleKind::Command, &wire).unwrap();
        let sent = inner.borrow_mut().sent_commands.clone();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].opcode, 0x1009);
        assert_eq!(sent[0].parameters, vec![0xAA, 0xBB]);
    }

    #[test]
    fn write_command_invalid_bytes_returns_einval() {
        let mut scheme = make_scheme();
        let result = scheme.write_handle(&HandleKind::Command, &[0x03]);
        assert!(result.is_err());
    }

    #[test]
    fn read_events_returns_serialized_event() {
        let inner = Rc::new(RefCell::new(TestTransportInner::new()));
        let event = HciEvent {
            event_code: EVT_COMMAND_COMPLETE,
            parameters: vec![0x01, 0x03, 0x0C, 0x00],
        };
        inner.borrow_mut().pending_events.push(event);
        let mut scheme = HciScheme::new_for_test(
            Box::new(TestTransport::new(&inner)),
            active_info(),
        );
        let data = scheme.read_handle(&HandleKind::Events).unwrap();
        assert_eq!(data.len(), 6);
        assert_eq!(data[0], EVT_COMMAND_COMPLETE);
        assert_eq!(data[1], 4);
        assert_eq!(&data[2..6], &[0x01, 0x03, 0x0C, 0x00]);
    }

    #[test]
    fn read_events_returns_empty_when_no_events() {
        let mut scheme = make_scheme();
        let data = scheme.read_handle(&HandleKind::Events).unwrap();
        assert!(data.is_empty());
    }

    #[test]
    fn write_le_scan_start_sets_flag_and_sends_command() {
        let (mut scheme, inner) = make_scheme_with_inner();
        scheme.write_handle(&HandleKind::LeScan, b"start").unwrap();
        assert!(scheme.le_scan_active);
        let sent = inner.borrow_mut().sent_commands.clone();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].opcode, OP_LE_SET_SCAN_ENABLE);
        assert_eq!(sent[0].parameters, vec![0x01, 0x00]);
    }

    #[test]
    fn write_le_scan_start_and_stop_cycle() {
        let mut scheme = make_scheme();
        scheme.write_handle(&HandleKind::LeScan, b"start").unwrap();
        assert!(scheme.le_scan_active);
        scheme.write_handle(&HandleKind::LeScan, b"stop").unwrap();
        assert!(!scheme.le_scan_active);
    }

    #[test]
    fn write_le_scan_invalid_text_returns_einval() {
        let mut scheme = make_scheme();
        let result = scheme.write_handle(&HandleKind::LeScan, b"invalid");
        assert!(result.is_err());
    }

    #[test]
    fn write_connect_parses_address_and_sends_command() {
        let (mut scheme, inner) = make_scheme_with_inner();
        scheme
            .write_handle(&HandleKind::Connect, b"addr=AA:BB:CC:DD:EE:FF")
            .unwrap();
        let sent = inner.borrow_mut().sent_commands.clone();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].opcode, OP_LE_CREATE_CONNECTION);
        assert_eq!(&sent[0].parameters[6..12], &[0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
    }

    #[test]
    fn write_connect_invalid_format_returns_einval() {
        let mut scheme = make_scheme();
        let result = scheme.write_handle(&HandleKind::Connect, b"invalid");
        assert!(result.is_err());
    }

    #[test]
    fn write_disconnect_parses_handle_and_sends_command() {
        let (mut scheme, inner) = make_scheme_with_inner();
        scheme
            .write_handle(&HandleKind::Disconnect, b"handle=0023")
            .unwrap();
        let sent = inner.borrow_mut().sent_commands.clone();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].opcode, OP_DISCONNECT);
        assert_eq!(
            u16::from_le_bytes([sent[0].parameters[0], sent[0].parameters[1]]),
            0x0023
        );
    }

    #[test]
    fn write_disconnect_hex_format() {
        let (mut scheme, inner) = make_scheme_with_inner();
        scheme
            .write_handle(&HandleKind::Disconnect, b"handle=0x0023")
            .unwrap();
        let sent = inner.borrow_mut().sent_commands.clone();
        assert_eq!(sent.len(), 1);
        assert_eq!(
            u16::from_le_bytes([sent[0].parameters[0], sent[0].parameters[1]]),
            0x0023
        );
    }

    #[test]
    fn write_disconnect_invalid_format_returns_einval() {
        let mut scheme = make_scheme();
        let result = scheme.write_handle(&HandleKind::Disconnect, b"invalid");
        assert!(result.is_err());
    }

    #[test]
    fn read_connections_shows_active_le_connections() {
        let mut scheme = make_scheme();
        scheme.le_connections.push((0x0023, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]));
        let data = scheme.read_handle(&HandleKind::Connections).unwrap();
        let text = String::from_utf8_lossy(&data);
        assert!(text.contains("handle=0023"));
        assert!(text.contains("addr=FF:EE:DD:CC:BB:AA"));
    }

    #[test]
    fn read_connections_empty_returns_newline() {
        let mut scheme = make_scheme();
        let data = scheme.read_handle(&HandleKind::Connections).unwrap();
        assert_eq!(data, b"\n");
    }

    #[test]
    fn read_scan_results_shows_accumulated_results() {
        let mut scheme = make_scheme();
        scheme.le_scan_results.push(
            "addr=AA:BB:CC:DD:EE:FF;rssi=-59;type=ADV_IND".to_string(),
        );
        let data = scheme.read_handle(&HandleKind::LeScanResults).unwrap();
        let text = String::from_utf8_lossy(&data);
        assert!(text.contains("addr=AA:BB:CC:DD:EE:FF"));
        assert!(text.contains("rssi=-59"));
    }

    #[test]
    fn read_scan_results_empty_returns_newline() {
        let mut scheme = make_scheme();
        let data = scheme.read_handle(&HandleKind::LeScanResults).unwrap();
        assert_eq!(data, b"\n");
    }

    #[test]
    fn write_to_readonly_handle_returns_erofs() {
        let mut scheme = make_scheme();
        let result = scheme.write_handle(&HandleKind::Status, b"test");
        assert!(result.is_err());
    }

    #[test]
    fn write_to_events_handle_returns_erofs() {
        let mut scheme = make_scheme();
        let result = scheme.write_handle(&HandleKind::Events, b"test");
        assert!(result.is_err());
    }

    #[test]
    fn read_acl_in_returns_bytes_from_transport() {
        let inner = Rc::new(RefCell::new(TestTransportInner::new()));
        let acl = HciAcl::new(0x0001, 0x00, 0x00, vec![0xDE, 0xAD]);
        inner.borrow_mut().pending_acl.push(acl);
        let mut scheme = HciScheme::new_for_test(
            Box::new(TestTransport::new(&inner)),
            active_info(),
        );
        let data = scheme.read_handle(&HandleKind::AclIn).unwrap();
        assert_eq!(data.len(), 6);
        let parsed = HciAcl::from_bytes(&data).unwrap();
        assert_eq!(parsed.handle, 0x0001);
        assert_eq!(parsed.data, vec![0xDE, 0xAD]);
    }

    #[test]
    fn read_acl_in_empty_returns_empty() {
        let mut scheme = make_scheme();
        let data = scheme.read_handle(&HandleKind::AclIn).unwrap();
        assert!(data.is_empty());
    }

    #[test]
    fn write_acl_out_sends_to_transport() {
        let (mut scheme, inner) = make_scheme_with_inner();
        let acl = HciAcl::new(0x0001, 0x00, 0x00, vec![0xCA, 0xFE]);
        let wire = acl.to_bytes();
        scheme.write_handle(&HandleKind::AclOut, &wire).unwrap();
        let sent = inner.borrow_mut().sent_acl.clone();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0], acl);
    }

    #[test]
    fn write_acl_out_invalid_bytes_returns_einval() {
        let mut scheme = make_scheme();
        let result = scheme.write_handle(&HandleKind::AclOut, &[0x42]);
        assert!(result.is_err());
    }

    #[test]
    fn on_close_removes_handle() {
        let mut scheme = make_scheme();
        let id = alloc(&mut scheme, HandleKind::Status);
        assert!(scheme.handle(id).is_ok());
        scheme.on_close(id);
        assert!(scheme.handle(id).is_err());
    }

    #[test]
    fn on_close_does_not_remove_root() {
        let mut scheme = make_scheme();
        scheme.on_close(SCHEME_ROOT_ID);
        assert!(scheme.handle(SCHEME_ROOT_ID).is_ok());
    }

    #[test]
    fn parse_addr_valid() {
        let addr = HciScheme::parse_addr("addr=AA:BB:CC:DD:EE:FF").unwrap();
        assert_eq!(addr, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
    }

    #[test]
    fn parse_addr_invalid_returns_none() {
        assert!(HciScheme::parse_addr("invalid").is_none());
        assert!(HciScheme::parse_addr("addr=AA:BB:CC").is_none());
        assert!(HciScheme::parse_addr("addr=GG:HH:II:JJ:KK:LL").is_none());
    }

    #[test]
    fn parse_handle_without_0x_prefix() {
        assert_eq!(HciScheme::parse_handle("handle=002A"), Some(0x002A));
    }

    #[test]
    fn parse_handle_hex() {
        assert_eq!(HciScheme::parse_handle("handle=0x0023"), Some(0x0023));
    }

    #[test]
    fn parse_handle_invalid_returns_none() {
        assert!(HciScheme::parse_handle("invalid").is_none());
        assert!(HciScheme::parse_handle("handle=").is_none());
    }

    #[test]
    fn event_to_bytes_serializes_correctly() {
        let event = HciEvent {
            event_code: EVT_COMMAND_COMPLETE,
            parameters: vec![0x01, 0x02, 0x03],
        };
        let bytes = event_to_bytes(&event);
        assert_eq!(bytes, vec![EVT_COMMAND_COMPLETE, 0x03, 0x01, 0x02, 0x03]);
    }

    // -- GATT scheme tests -----------------------------------------------------

    /// Helper: build an ACL packet wrapping an ATT PDU over L2CAP ATT CID.
    fn make_acl_att_response(conn_handle: u16, att_opcode: u8, att_data: &[u8]) -> HciAcl {
        let att_len = (1 + att_data.len()) as u16; // opcode + params
        let l2cap_payload_len = 2 + 2 + 1 + att_data.len(); // l2cap_len + cid + opcode + data
        let mut payload = Vec::with_capacity(l2cap_payload_len);
        payload.extend_from_slice(&att_len.to_le_bytes()); // L2CAP length
        payload.extend_from_slice(&0x0004u16.to_le_bytes()); // ATT CID
        payload.push(att_opcode);
        payload.extend_from_slice(att_data);
        HciAcl::new(conn_handle, 0x00, 0x00, payload)
    }

    #[test]
    fn gatt_discover_services_sends_acl_and_caches_results() {
        let inner = Rc::new(RefCell::new(TestTransportInner::new()));
        // Build ATT Read By Group Type Response with one service:
        //   length=6 (2 start + 2 end + 2 uuid16), entry: start=0x0001, end=0x0005, uuid=0x180F
        let mut rsp_data = Vec::new();
        rsp_data.push(0x06); // length
        rsp_data.extend_from_slice(&0x0001u16.to_le_bytes()); // start handle
        rsp_data.extend_from_slice(&0x0005u16.to_le_bytes()); // end handle
        rsp_data.extend_from_slice(&0x180Fu16.to_le_bytes()); // UUID (Battery Service)
        inner.borrow_mut().pending_acl.push(
            make_acl_att_response(0x0042, ATT_READ_BY_GROUP_TYPE_RSP, &rsp_data)
        );
        let mut scheme = HciScheme::new_for_test(
            Box::new(TestTransport::new(&inner)),
            active_info(),
        );
        scheme.write_handle(&HandleKind::GattDiscoverServices, b"handle=0042").unwrap();
        assert_eq!(scheme.gatt_services.len(), 1);
        assert_eq!(scheme.gatt_services[0].start_handle, 0x0001);
        assert_eq!(scheme.gatt_services[0].end_handle, 0x0005);
        assert_eq!(scheme.gatt_services[0].uuid, vec![0x0F, 0x18]);
    }

    #[test]
    fn gatt_discover_services_read_formats_text() {
        let mut scheme = make_scheme();
        scheme.gatt_services.push(GattService {
            start_handle: 0x0001,
            end_handle: 0xFFFF,
            uuid: vec![0x0F, 0x18],
        });
        scheme.gatt_services.push(GattService {
            start_handle: 0x0010,
            end_handle: 0x0020,
            uuid: vec![0x0A, 0x18],
        });
        let data = scheme.read_handle(&HandleKind::GattDiscoverServices).unwrap();
        let text = String::from_utf8_lossy(&data);
        assert!(text.contains("service=start_handle=0001;end_handle=FFFF;uuid=180F"));
        assert!(text.contains("service=start_handle=0010;end_handle=0020;uuid=180A"));
    }

    #[test]
    fn gatt_discover_chars_sends_acl_and_caches_results() {
        let inner = Rc::new(RefCell::new(TestTransportInner::new()));
        // Build ATT Read By Type Response with one characteristic:
        //   length=7 (1 props + 2 value_handle + 2 uuid + 2 extra... standard 16-bit: 7 bytes)
        //   Actually per spec: length = 1(props) + 2(value_handle) + 2(uuid16) = 5 for 16-bit UUID
        //   BUT parse_read_by_type_rsp expects: entry[0]=props, entry[1..3]=value_handle, entry[3..]=uuid
        //   So length=5 gives entry[0]=props, entry[1,2]=value_handle, entry[3,4]=uuid
        let mut rsp_data = Vec::new();
        rsp_data.push(0x05); // length
        rsp_data.push(0x12); // properties
        rsp_data.extend_from_slice(&0x0016u16.to_le_bytes()); // value handle
        rsp_data.extend_from_slice(&0x2A19u16.to_le_bytes()); // UUID (Battery Level)
        inner.borrow_mut().pending_acl.push(
            make_acl_att_response(0x0042, ATT_READ_BY_TYPE_RSP, &rsp_data)
        );
        let mut scheme = HciScheme::new_for_test(
            Box::new(TestTransport::new(&inner)),
            active_info(),
        );
        scheme.write_handle(&HandleKind::GattDiscoverChars, b"handle=0042;start=0001;end=FFFF").unwrap();
        assert_eq!(scheme.gatt_characteristics.len(), 1);
        assert_eq!(scheme.gatt_characteristics[0].properties, 0x12);
        assert_eq!(scheme.gatt_characteristics[0].value_handle, 0x0016);
        assert_eq!(scheme.gatt_characteristics[0].uuid, vec![0x19, 0x2A]);
    }

    #[test]
    fn gatt_discover_chars_read_formats_text() {
        let mut scheme = make_scheme();
        scheme.gatt_characteristics.push(GattCharacteristic {
            handle: 0x0015,
            properties: 0x12,
            value_handle: 0x0016,
            uuid: vec![0x19, 0x2A],
        });
        let data = scheme.read_handle(&HandleKind::GattDiscoverChars).unwrap();
        let text = String::from_utf8_lossy(&data);
        assert!(text.contains("char=handle=0015;value_handle=0016;properties=12;uuid=2A19"));
    }

    #[test]
    fn gatt_read_char_sends_att_read_and_caches_value() {
        let inner = Rc::new(RefCell::new(TestTransportInner::new()));
        // Build ATT Read Response with value bytes
        inner.borrow_mut().pending_acl.push(
            make_acl_att_response(0x0042, ATT_READ_RSP, &[0x64, 0x00])
        );
        let mut scheme = HciScheme::new_for_test(
            Box::new(TestTransport::new(&inner)),
            active_info(),
        );
        scheme.write_handle(&HandleKind::GattReadChar, b"handle=0042;addr=0016").unwrap();
        assert_eq!(scheme.gatt_read_result, Some(vec![0x64, 0x00]));
    }

    #[test]
    fn gatt_read_char_read_returns_raw_bytes() {
        let mut scheme = make_scheme();
        scheme.gatt_read_result = Some(vec![0x64, 0x00, 0xFF]);
        let data = scheme.read_handle(&HandleKind::GattReadChar).unwrap();
        assert_eq!(data, vec![0x64, 0x00, 0xFF]);
    }

    #[test]
    fn gatt_services_read_empty_returns_newline() {
        let mut scheme = make_scheme();
        let data = scheme.read_handle(&HandleKind::GattServices).unwrap();
        assert_eq!(data, b"\n");
    }

    #[test]
    fn gatt_characteristics_read_empty_returns_newline() {
        let mut scheme = make_scheme();
        let data = scheme.read_handle(&HandleKind::GattCharacteristics).unwrap();
        assert_eq!(data, b"\n");
    }

    #[test]
    fn gatt_discover_services_invalid_format_returns_einval() {
        let mut scheme = make_scheme();
        let result = scheme.write_handle(&HandleKind::GattDiscoverServices, b"invalid");
        assert!(result.is_err());
    }

    #[test]
    fn gatt_discover_chars_invalid_format_returns_einval() {
        let mut scheme = make_scheme();
        let result = scheme.write_handle(&HandleKind::GattDiscoverChars, b"invalid");
        assert!(result.is_err());
    }

    #[test]
    fn gatt_read_char_invalid_format_returns_einval() {
        let mut scheme = make_scheme();
        let result = scheme.write_handle(&HandleKind::GattReadChar, b"invalid");
        assert!(result.is_err());
    }

    #[test]
    fn gatt_discover_services_error_response_caches_error() {
        let inner = Rc::new(RefCell::new(TestTransportInner::new()));
        // ATT Error Response: req_opcode=0x10, handle=0x0001, error_code=0x0A (attribute not found)
        let err_data = vec![ATT_READ_BY_GROUP_TYPE_REQ, 0x01, 0x00, 0x0A, 0x00];
        inner.borrow_mut().pending_acl.push(
            make_acl_att_response(0x0042, ATT_ERROR_RSP, &err_data)
        );
        let mut scheme = HciScheme::new_for_test(
            Box::new(TestTransport::new(&inner)),
            active_info(),
        );
        scheme.write_handle(&HandleKind::GattDiscoverServices, b"handle=0042").unwrap();
        assert!(scheme.gatt_services.is_empty());
        assert!(scheme.gatt_last_error.is_some());
        let err = scheme.gatt_last_error.unwrap();
        assert!(err.contains("ATT error"));
    }

    #[test]
    fn root_lists_gatt_nodes() {
        let mut scheme = make_scheme();
        let data = scheme.read_handle(&HandleKind::Root).unwrap();
        let text = String::from_utf8_lossy(&data);
        assert!(text.contains("gatt-discover-services"));
        assert!(text.contains("gatt-discover-chars"));
        assert!(text.contains("gatt-read-char"));
        assert!(text.contains("gatt-services"));
        assert!(text.contains("gatt-characteristics"));
    }

    #[test]
    fn gatt_read_char_read_empty_returns_empty_vec() {
        let mut scheme = make_scheme();
        let data = scheme.read_handle(&HandleKind::GattReadChar).unwrap();
        assert!(data.is_empty());
    }
}
