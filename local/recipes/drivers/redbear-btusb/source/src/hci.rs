//! HCI (Host Controller Interface) protocol types for Bluetooth USB transport.
//!
//! Defines wire-format types for HCI command, event, ACL, SCO, and ISO packets,
//! along with constants for opcodes, event codes, status codes, and LE meta subevents.

// ---------------------------------------------------------------------------
// Constants — Opcodes (OGF | OCF, u16 little-endian on wire)
// ---------------------------------------------------------------------------

// Link Control
pub const OP_CREATE_CONNECTION: u16 = 0x0405;
pub const OP_DISCONNECT: u16 = 0x0406;

// Controller & Baseband
pub const OP_RESET: u16 = 0x0C03;
pub const OP_SET_EVENT_MASK: u16 = 0x0C01;
pub const OP_READ_LOCAL_VERSION: u16 = 0x1001;
pub const OP_READ_LOCAL_SUPPORTED_COMMANDS: u16 = 0x1002;
pub const OP_READ_BD_ADDR: u16 = 0x1009;
pub const OP_SET_EVENT_FILTER: u16 = 0x0C05;
pub const OP_WRITE_INQUIRY_MODE: u16 = 0x0C45;
pub const OP_WRITE_SCAN_ENABLE: u16 = 0x0C1A;

// LE Controller
pub const OP_LE_SET_EVENT_MASK: u16 = 0x2001;
pub const OP_LE_READ_BUFFER_SIZE: u16 = 0x2002;
pub const OP_LE_READ_LOCAL_SUPPORTED_FEATURES: u16 = 0x2003;
pub const OP_LE_SET_RANDOM_ADDRESS: u16 = 0x2005;
pub const OP_LE_SET_ADVERTISING_PARAMETERS: u16 = 0x2006;
pub const OP_LE_SET_ADVERTISING_DATA: u16 = 0x2008;
pub const OP_LE_SET_SCAN_PARAMETERS: u16 = 0x200B;
pub const OP_LE_SET_SCAN_ENABLE: u16 = 0x200C;
pub const OP_LE_CREATE_CONNECTION: u16 = 0x200D;
pub const OP_LE_CREATE_CONNECTION_CANCEL: u16 = 0x200E;
pub const OP_LE_CONNECTION_UPDATE: u16 = 0x2013;
pub const OP_LE_READ_REMOTE_FEATURES: u16 = 0x2016;

// ---------------------------------------------------------------------------
// Constants — Status Codes
// ---------------------------------------------------------------------------

pub const STATUS_SUCCESS: u8 = 0x00;
pub const STATUS_UNKNOWN_COMMAND: u8 = 0x01;
pub const STATUS_UNKNOWN_CONNECTION_ID: u8 = 0x02;
pub const STATUS_HARDWARE_FAILURE: u8 = 0x03;
pub const STATUS_PAGE_TIMEOUT: u8 = 0x04;
pub const STATUS_AUTHENTICATION_FAILURE: u8 = 0x05;

// ---------------------------------------------------------------------------
// Constants — Event Codes
// ---------------------------------------------------------------------------

pub const EVT_INQUIRY_COMPLETE: u8 = 0x01;
pub const EVT_INQUIRY_RESULT: u8 = 0x02;
pub const EVT_CONNECTION_COMPLETE: u8 = 0x03;
pub const EVT_DISCONNECTION_COMPLETE: u8 = 0x05;
pub const EVT_AUTHENTICATION_COMPLETE: u8 = 0x06;
pub const EVT_REMOTE_NAME_REQUEST_COMPLETE: u8 = 0x07;
pub const EVT_ENCRYPTION_CHANGE: u8 = 0x08;
pub const EVT_COMMAND_COMPLETE: u8 = 0x0E;
pub const EVT_COMMAND_STATUS: u8 = 0x0F;
pub const EVT_NUMBER_OF_COMPLETED_PACKETS: u8 = 0x13;
pub const EVT_LE_META: u8 = 0x3E;

// ---------------------------------------------------------------------------
// Constants — LE Meta Subevent Codes
// ---------------------------------------------------------------------------

pub const LE_CONNECTION_COMPLETE: u8 = 0x01;
pub const LE_ADVERTISING_REPORT: u8 = 0x02;
pub const LE_CONNECTION_UPDATE_COMPLETE: u8 = 0x03;
pub const LE_READ_REMOTE_FEATURES_COMPLETE: u8 = 0x04;
pub const LE_LONG_TERM_KEY_REQUEST: u8 = 0x05;

// ---------------------------------------------------------------------------
// Packet Indicator (u8 prefix on USB transport)
// ---------------------------------------------------------------------------

/// USB HCI transport packet indicator byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PacketIndicator {
    Command = 0x01,
    Acl = 0x02,
    Sco = 0x03,
    Event = 0x04,
    Iso = 0x05,
}

impl PacketIndicator {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x01 => Some(Self::Command),
            0x02 => Some(Self::Acl),
            0x03 => Some(Self::Sco),
            0x04 => Some(Self::Event),
            0x05 => Some(Self::Iso),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Parse Error
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HciParseError {
    InsufficientData { expected: usize, actual: usize },
    InvalidPacketIndicator(u8),
    InvalidParameterLength { declared: usize, available: usize },
}

// ---------------------------------------------------------------------------
// HciCommand
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HciCommand {
    pub opcode: u16,
    pub parameters: Vec<u8>,
}

impl HciCommand {
    pub fn new(opcode: u16, parameters: Vec<u8>) -> Self {
        Self { opcode, parameters }
    }

    /// Build the wire format: `[opcode_lo, opcode_hi, param_length, params...]`
    pub fn to_bytes(&self) -> Vec<u8> {
        let param_len = u8::try_from(self.parameters.len()).unwrap_or(0xFF);
        let mut buf = Vec::with_capacity(3 + self.parameters.len());
        buf.push(self.opcode as u8);
        buf.push((self.opcode >> 8) as u8);
        buf.push(param_len);
        buf.extend_from_slice(&self.parameters);
        buf
    }

    /// Parse from wire format (without the packet indicator byte).
    pub fn from_bytes(data: &[u8]) -> Result<Self, HciParseError> {
        if data.len() < 3 {
            return Err(HciParseError::InsufficientData {
                expected: 3,
                actual: data.len(),
            });
        }
        let opcode = u16::from_le_bytes([data[0], data[1]]);
        let param_len = data[2] as usize;
        if data.len() < 3 + param_len {
            return Err(HciParseError::InvalidParameterLength {
                declared: param_len,
                available: data.len() - 3,
            });
        }
        let parameters = data[3..3 + param_len].to_vec();
        Ok(Self { opcode, parameters })
    }

    /// Total wire length including 3-byte header.
    pub fn wire_length(&self) -> usize {
        3 + self.parameters.len()
    }
}

// ---------------------------------------------------------------------------
// HciEvent
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HciEvent {
    pub event_code: u8,
    pub parameters: Vec<u8>,
}

impl HciEvent {
    /// Parse from wire format (without the packet indicator byte).
    pub fn from_bytes(data: &[u8]) -> Result<Self, HciParseError> {
        if data.len() < 2 {
            return Err(HciParseError::InsufficientData {
                expected: 2,
                actual: data.len(),
            });
        }
        let event_code = data[0];
        let param_len = data[1] as usize;
        if data.len() < 2 + param_len {
            return Err(HciParseError::InvalidParameterLength {
                declared: param_len,
                available: data.len() - 2,
            });
        }
        let parameters = data[2..2 + param_len].to_vec();
        Ok(Self {
            event_code,
            parameters,
        })
    }

    /// Total wire length including 2-byte header.
    pub fn wire_length(&self) -> usize {
        2 + self.parameters.len()
    }

    /// Check if this is a Command Complete event.
    pub fn is_command_complete(&self) -> bool {
        self.event_code == EVT_COMMAND_COMPLETE
    }

    /// Check if this is a Command Status event.
    pub fn is_command_status(&self) -> bool {
        self.event_code == EVT_COMMAND_STATUS
    }

    /// Check if this is an LE Meta event.
    pub fn is_le_meta(&self) -> bool {
        self.event_code == EVT_LE_META
    }

    /// Extract return parameters from Command Complete event.
    ///
    /// CC format: `[num_hci_command_packets, opcode_lo, opcode_hi, status, return_params...]`
    ///
    /// Returns `(num_hci_command_packets, opcode, return_params)` or `None` if not CC
    /// or parameters too short.
    pub fn command_complete_params(&self) -> Option<(u8, u16, &[u8])> {
        if !self.is_command_complete() {
            return None;
        }
        // Need at least: num_hci_packets(1) + opcode(2) + status(1) = 4 bytes
        if self.parameters.len() < 4 {
            return None;
        }
        let num_packets = self.parameters[0];
        let opcode = u16::from_le_bytes([self.parameters[1], self.parameters[2]]);
        // return_params starts after status byte (index 3)
        let return_params = &self.parameters[4..];
        Some((num_packets, opcode, return_params))
    }

    /// Extract fields from Command Status event.
    ///
    /// CS format: `[status, num_hci_command_packets, opcode_lo, opcode_hi]`
    ///
    /// Returns `(status, num_hci_command_packets, opcode)` or `None` if not CS
    /// or parameters too short.
    pub fn command_status_params(&self) -> Option<(u8, u8, u16)> {
        if !self.is_command_status() {
            return None;
        }
        // Need: status(1) + num_packets(1) + opcode(2) = 4 bytes
        if self.parameters.len() < 4 {
            return None;
        }
        let status = self.parameters[0];
        let num_packets = self.parameters[1];
        let opcode = u16::from_le_bytes([self.parameters[2], self.parameters[3]]);
        Some((status, num_packets, opcode))
    }
}

// ---------------------------------------------------------------------------
// HciAcl
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HciAcl {
    pub handle: u16,
    pub pb_flag: u8,
    pub bc_flag: u8,
    pub data: Vec<u8>,
}

impl HciAcl {
    pub fn new(handle: u16, pb_flag: u8, bc_flag: u8, data: Vec<u8>) -> Self {
        Self {
            handle,
            pb_flag,
            bc_flag,
            data,
        }
    }

    /// Build wire format: `[handle_lo (with PB|BC), handle_hi, length_lo, length_hi, data...]`
    pub fn to_bytes(&self) -> Vec<u8> {
        let handle_word =
            (self.handle & 0x0FFF) | ((self.pb_flag as u16 & 0x03) << 12) | ((self.bc_flag as u16 & 0x03) << 14);
        let data_len = self.data.len();
        let mut buf = Vec::with_capacity(4 + data_len);
        buf.push(handle_word as u8);
        buf.push((handle_word >> 8) as u8);
        buf.push(data_len as u8);
        buf.push((data_len >> 8) as u8);
        buf.extend_from_slice(&self.data);
        buf
    }

    /// Parse from wire format (without the packet indicator byte).
    pub fn from_bytes(data: &[u8]) -> Result<Self, HciParseError> {
        if data.len() < 4 {
            return Err(HciParseError::InsufficientData {
                expected: 4,
                actual: data.len(),
            });
        }
        let handle_word = u16::from_le_bytes([data[0], data[1]]);
        let data_len = u16::from_le_bytes([data[2], data[3]]) as usize;
        if data.len() < 4 + data_len {
            return Err(HciParseError::InvalidParameterLength {
                declared: data_len,
                available: data.len() - 4,
            });
        }
        let handle = handle_word & 0x0FFF;
        let pb_flag = ((handle_word >> 12) & 0x03) as u8;
        let bc_flag = ((handle_word >> 14) & 0x03) as u8;
        let payload = data[4..4 + data_len].to_vec();
        Ok(Self {
            handle,
            pb_flag,
            bc_flag,
            data: payload,
        })
    }
}

// ---------------------------------------------------------------------------
// HciPacketData — parsed packet payload
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HciPacketData {
    Command(HciCommand),
    Event(HciEvent),
    Acl(HciAcl),
    Sco(Vec<u8>),
    Iso(Vec<u8>),
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Build a complete USB HCI command packet with packet indicator.
///
/// Result: `[0x01, opcode_lo, opcode_hi, param_len, params...]`
pub fn build_usb_command(opcode: u16, parameters: Vec<u8>) -> Vec<u8> {
    let cmd = HciCommand::new(opcode, parameters);
    let mut buf = vec![PacketIndicator::Command as u8];
    buf.extend_from_slice(&cmd.to_bytes());
    buf
}

/// Parse any HCI packet from USB transport data.
///
/// Returns the packet indicator and the parsed payload data.
pub fn parse_usb_hci_packet(data: &[u8]) -> Result<(PacketIndicator, HciPacketData), HciParseError> {
    if data.is_empty() {
        return Err(HciParseError::InsufficientData {
            expected: 1,
            actual: 0,
        });
    }
    let indicator = PacketIndicator::from_u8(data[0]).ok_or(HciParseError::InvalidPacketIndicator(data[0]))?;
    let payload = &data[1..];
    let packet_data = match indicator {
        PacketIndicator::Command => HciPacketData::Command(HciCommand::from_bytes(payload)?),
        PacketIndicator::Event => HciPacketData::Event(HciEvent::from_bytes(payload)?),
        PacketIndicator::Acl => HciPacketData::Acl(HciAcl::from_bytes(payload)?),
        PacketIndicator::Sco => HciPacketData::Sco(payload.to_vec()),
        PacketIndicator::Iso => HciPacketData::Iso(payload.to_vec()),
    };
    Ok((indicator, packet_data))
}

// ---------------------------------------------------------------------------
// Command Builders
// ---------------------------------------------------------------------------

/// HCI Reset command (opcode 0x0C03, no parameters).
pub fn cmd_reset() -> HciCommand {
    HciCommand::new(OP_RESET, Vec::new())
}

/// HCI Read BD_ADDR command (opcode 0x1009, no parameters).
pub fn cmd_read_bd_addr() -> HciCommand {
    HciCommand::new(OP_READ_BD_ADDR, Vec::new())
}

/// HCI Read Local Version Information (opcode 0x1001, no parameters).
pub fn cmd_read_local_version() -> HciCommand {
    HciCommand::new(OP_READ_LOCAL_VERSION, Vec::new())
}

/// HCI Read Local Supported Commands (opcode 0x1002, no parameters).
pub fn cmd_read_local_supported_commands() -> HciCommand {
    HciCommand::new(OP_READ_LOCAL_SUPPORTED_COMMANDS, Vec::new())
}

/// HCI Set Event Mask (opcode 0x0C01).
///
/// `mask` is the 8-byte event mask.
pub fn cmd_set_event_mask(mask: [u8; 8]) -> HciCommand {
    HciCommand::new(OP_SET_EVENT_MASK, mask.to_vec())
}

/// HCI LE Set Event Mask (opcode 0x2001).
///
/// `mask` is the 8-byte LE event mask.
pub fn cmd_le_set_event_mask(mask: [u8; 8]) -> HciCommand {
    HciCommand::new(OP_LE_SET_EVENT_MASK, mask.to_vec())
}

/// HCI LE Read Buffer Size (opcode 0x2002, no parameters).
pub fn cmd_le_read_buffer_size() -> HciCommand {
    HciCommand::new(OP_LE_READ_BUFFER_SIZE, Vec::new())
}

/// HCI LE Set Scan Parameters (opcode 0x200B).
///
/// * `scan_type` — 0 = passive, 1 = active
/// * `interval`, `window` — in 0.625 ms units (0x0004–0x4000)
/// * `own_address_type` — 0 = public, 1 = random
/// * `filter_policy` — 0 = accept all, 1 = ignore non-directed from whitelist
pub fn cmd_le_set_scan_parameters(
    scan_type: u8,
    interval: u16,
    window: u16,
    own_address_type: u8,
    filter_policy: u8,
) -> HciCommand {
    let mut params = Vec::with_capacity(7);
    params.push(scan_type);
    params.extend_from_slice(&interval.to_le_bytes());
    params.extend_from_slice(&window.to_le_bytes());
    params.push(own_address_type);
    params.push(filter_policy);
    HciCommand::new(OP_LE_SET_SCAN_PARAMETERS, params)
}

/// HCI LE Set Scan Enable (opcode 0x200C).
///
/// * `enable` — 0 = disable, 1 = enable
/// * `filter_duplicates` — 0 = disable, 1 = enable
pub fn cmd_le_set_scan_enable(enable: u8, filter_duplicates: u8) -> HciCommand {
    HciCommand::new(OP_LE_SET_SCAN_ENABLE, vec![enable, filter_duplicates])
}

/// HCI LE Create Connection (opcode 0x200D).
#[allow(clippy::too_many_arguments)]
pub fn cmd_le_create_connection(
    scan_interval: u16,
    scan_window: u16,
    initiator_filter_policy: u8,
    peer_address_type: u8,
    peer_address: &[u8; 6],
    own_address_type: u8,
    conn_interval_min: u16,
    conn_interval_max: u16,
    conn_latency: u16,
    supervision_timeout: u16,
    min_ce_length: u16,
    max_ce_length: u16,
) -> HciCommand {
    let mut params = Vec::with_capacity(25);
    params.extend_from_slice(&scan_interval.to_le_bytes());
    params.extend_from_slice(&scan_window.to_le_bytes());
    params.push(initiator_filter_policy);
    params.push(peer_address_type);
    params.extend_from_slice(peer_address);
    params.push(own_address_type);
    params.extend_from_slice(&conn_interval_min.to_le_bytes());
    params.extend_from_slice(&conn_interval_max.to_le_bytes());
    params.extend_from_slice(&conn_latency.to_le_bytes());
    params.extend_from_slice(&supervision_timeout.to_le_bytes());
    params.extend_from_slice(&min_ce_length.to_le_bytes());
    params.extend_from_slice(&max_ce_length.to_le_bytes());
    HciCommand::new(OP_LE_CREATE_CONNECTION, params)
}

/// HCI LE Create Connection Cancel (opcode 0x200E, no parameters).
pub fn cmd_le_create_connection_cancel() -> HciCommand {
    HciCommand::new(OP_LE_CREATE_CONNECTION_CANCEL, Vec::new())
}

/// HCI Disconnect (opcode 0x0406).
///
/// * `connection_handle` — the connection handle to disconnect
/// * `reason` — error code (e.g. 0x13 for remote user terminated, 0x16 for local host terminated)
pub fn cmd_disconnect(connection_handle: u16, reason: u8) -> HciCommand {
    let mut params = Vec::with_capacity(3);
    params.extend_from_slice(&connection_handle.to_le_bytes());
    params.push(reason);
    HciCommand::new(OP_DISCONNECT, params)
}

// ---------------------------------------------------------------------------
// Event Result Types
// ---------------------------------------------------------------------------

/// Result of HCI Read BD_ADDR Command Complete.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BdAddrResult {
    pub status: u8,
    pub address: [u8; 6],
}

/// Result of HCI Read Local Version Command Complete.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalVersionResult {
    pub status: u8,
    pub hci_version: u8,
    pub hci_revision: u16,
    pub lmp_version: u8,
    pub manufacturer_name: u16,
    pub lmp_subversion: u16,
}

/// Parsed LE Advertising Report entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeAdvertisingReport {
    /// 0 = ADV_IND, 1 = ADV_DIRECT_IND, 2 = ADV_SCAN_IND, 3 = SCAN_RSP, 4 = ADV_NONCONN_IND
    pub event_type: u8,
    /// 0 = public, 1 = random
    pub address_type: u8,
    pub address: [u8; 6],
    pub data: Vec<u8>,
    pub rssi: i8,
}

/// Parsed LE Connection Complete event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeConnectionComplete {
    pub status: u8,
    pub connection_handle: u16,
    pub role: u8,
    pub peer_address_type: u8,
    pub peer_address: [u8; 6],
    pub conn_interval: u16,
    pub conn_latency: u16,
    pub supervision_timeout: u16,
    pub master_clock_accuracy: u8,
}

/// Result of HCI LE Read Buffer Size Command Complete.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeBufferSizeResult {
    pub status: u8,
    pub le_acl_data_packet_length: u16,
    pub total_num_le_acl_data_packets: u8,
}

// ---------------------------------------------------------------------------
// Event Parsers
// ---------------------------------------------------------------------------

/// Parse Read BD_ADDR from Command Complete event return parameters.
///
/// Returns `None` if the event is not a Command Complete for `OP_READ_BD_ADDR`
/// or the return parameters are too short.
pub fn parse_read_bd_addr(event: &HciEvent) -> Option<BdAddrResult> {
    let (num_packets, opcode, return_params) = event.command_complete_params()?;
    if opcode != OP_READ_BD_ADDR {
        return None;
    }
    let _ = num_packets;
    // command_complete_params() returns parameters[4..] which already excludes
    // num_packets(1) + opcode(2) + status(1), so return_params is just the BD addr
    if return_params.len() < 6 {
        return None;
    }
    let mut address = [0u8; 6];
    address.copy_from_slice(&return_params[0..6]);
    Some(BdAddrResult {
        status: event.parameters.get(3).copied().unwrap_or(0xFF),
        address,
    })
}

/// Parse Read Local Version from Command Complete event return parameters.
pub fn parse_local_version(event: &HciEvent) -> Option<LocalVersionResult> {
    let (num_packets, opcode, return_params) = event.command_complete_params()?;
    if opcode != OP_READ_LOCAL_VERSION {
        return None;
    }
    let _ = num_packets;
    // return_params excludes status (already stripped by command_complete_params):
    // hci_version(1) + hci_revision(2) + lmp_version(1) + manufacturer(2) + lmp_subversion(2) = 8
    if return_params.len() < 8 {
        return None;
    }
    Some(LocalVersionResult {
        status: event.parameters.get(3).copied().unwrap_or(0xFF),
        hci_version: return_params[0],
        hci_revision: u16::from_le_bytes([return_params[1], return_params[2]]),
        lmp_version: return_params[3],
        manufacturer_name: u16::from_le_bytes([return_params[4], return_params[5]]),
        lmp_subversion: u16::from_le_bytes([return_params[6], return_params[7]]),
    })
}

/// Parse LE Advertising Report from LE Meta event.
///
/// Returns `None` if the event is not LE Meta or the subevent is not
/// `LE_ADVERTISING_REPORT`.
pub fn parse_le_advertising_reports(event: &HciEvent) -> Option<Vec<LeAdvertisingReport>> {
    if !event.is_le_meta() {
        return None;
    }
    if event.parameters.is_empty() || event.parameters[0] != LE_ADVERTISING_REPORT {
        return None;
    }
    if event.parameters.len() < 2 {
        return None;
    }
    let num_reports = event.parameters[1] as usize;
    let mut reports = Vec::with_capacity(num_reports);
    let mut offset = 2;
    for _ in 0..num_reports {
        // Need at least: event_type(1) + addr_type(1) + addr(6) + data_len(1) = 9 bytes
        if offset + 9 > event.parameters.len() {
            break;
        }
        let event_type = event.parameters[offset];
        let address_type = event.parameters[offset + 1];
        let mut address = [0u8; 6];
        address.copy_from_slice(&event.parameters[offset + 2..offset + 8]);
        let data_len = event.parameters[offset + 8] as usize;
        offset += 9;

        if offset + data_len + 1 > event.parameters.len() {
            break;
        }
        let data = event.parameters[offset..offset + data_len].to_vec();
        offset += data_len;
        let rssi = event.parameters[offset] as i8;
        offset += 1;

        reports.push(LeAdvertisingReport {
            event_type,
            address_type,
            address,
            data,
            rssi,
        });
    }
    Some(reports)
}

/// Parse LE Connection Complete from LE Meta event.
pub fn parse_le_connection_complete(event: &HciEvent) -> Option<LeConnectionComplete> {
    if !event.is_le_meta() {
        return None;
    }
    if event.parameters.is_empty() || event.parameters[0] != LE_CONNECTION_COMPLETE {
        return None;
    }
    // Subevent(1) + status(1) + handle(2) + role(1) + addr_type(1) + addr(6)
    // + interval(2) + latency(2) + timeout(2) + mca(1) = 19
    if event.parameters.len() < 19 {
        return None;
    }
    let mut address = [0u8; 6];
    address.copy_from_slice(&event.parameters[6..12]);
    Some(LeConnectionComplete {
        status: event.parameters[1],
        connection_handle: u16::from_le_bytes([event.parameters[2], event.parameters[3]]),
        role: event.parameters[4],
        peer_address_type: event.parameters[5],
        peer_address: address,
        conn_interval: u16::from_le_bytes([event.parameters[12], event.parameters[13]]),
        conn_latency: u16::from_le_bytes([event.parameters[14], event.parameters[15]]),
        supervision_timeout: u16::from_le_bytes([event.parameters[16], event.parameters[17]]),
        master_clock_accuracy: event.parameters[18],
    })
}

/// Parse LE Read Buffer Size from Command Complete event.
pub fn parse_le_buffer_size(event: &HciEvent) -> Option<LeBufferSizeResult> {
    let (_num_packets, opcode, return_params) = event.command_complete_params()?;
    if opcode != OP_LE_READ_BUFFER_SIZE {
        return None;
    }
    if return_params.len() < 2 {
        return None;
    }
    Some(LeBufferSizeResult {
        status: event.parameters.get(3).copied().unwrap_or(0xFF),
        le_acl_data_packet_length: u16::from_le_bytes([return_params[0], return_params[1]]),
        total_num_le_acl_data_packets: return_params.get(2).copied().unwrap_or(0),
    })
}

// ---------------------------------------------------------------------------
// ATT (Attribute Protocol) Constants
// ---------------------------------------------------------------------------

// ATT opcodes
pub const ATT_ERROR_RSP: u8 = 0x01;
pub const ATT_EXCHANGE_MTU_REQ: u8 = 0x02;
pub const ATT_EXCHANGE_MTU_RSP: u8 = 0x03;
pub const ATT_FIND_INFORMATION_REQ: u8 = 0x04;
pub const ATT_FIND_INFORMATION_RSP: u8 = 0x05;
pub const ATT_FIND_BY_TYPE_VALUE_REQ: u8 = 0x06;
pub const ATT_FIND_BY_TYPE_VALUE_RSP: u8 = 0x07;
pub const ATT_READ_BY_TYPE_REQ: u8 = 0x08;
pub const ATT_READ_BY_TYPE_RSP: u8 = 0x09;
pub const ATT_READ_REQ: u8 = 0x0A;
pub const ATT_READ_RSP: u8 = 0x0B;
pub const ATT_READ_BLOB_REQ: u8 = 0x0C;
pub const ATT_READ_BLOB_RSP: u8 = 0x0D;
pub const ATT_READ_MULTIPLE_REQ: u8 = 0x0E;
pub const ATT_READ_MULTIPLE_RSP: u8 = 0x0F;
pub const ATT_READ_BY_GROUP_TYPE_REQ: u8 = 0x10;
pub const ATT_READ_BY_GROUP_TYPE_RSP: u8 = 0x11;
pub const ATT_WRITE_REQ: u8 = 0x12;
pub const ATT_WRITE_RSP: u8 = 0x13;
pub const ATT_WRITE_CMD: u8 = 0x52;
pub const ATT_SIGNED_WRITE_CMD: u8 = 0xD2;
pub const ATT_PREPARE_WRITE_REQ: u8 = 0x16;
pub const ATT_PREPARE_WRITE_RSP: u8 = 0x17;
pub const ATT_EXECUTE_WRITE_REQ: u8 = 0x18;
pub const ATT_EXECUTE_WRITE_RSP: u8 = 0x19;
pub const ATT_HANDLE_VALUE_NTF: u8 = 0x1B;
pub const ATT_HANDLE_VALUE_IND: u8 = 0x1D;
pub const ATT_HANDLE_VALUE_CFM: u8 = 0x1E;

// ATT error codes
pub const ATT_ERR_INVALID_HANDLE: u8 = 0x01;
pub const ATT_ERR_READ_NOT_PERMITTED: u8 = 0x02;
pub const ATT_ERR_WRITE_NOT_PERMITTED: u8 = 0x03;
pub const ATT_ERR_INVALID_PDU: u8 = 0x04;
pub const ATT_ERR_REQUEST_NOT_SUPPORTED: u8 = 0x06;
pub const ATT_ERR_INVALID_OFFSET: u8 = 0x07;
pub const ATT_ERR_ATTRIBUTE_NOT_FOUND: u8 = 0x0A;
pub const ATT_ERR_INVALID_ATTRIBUTE_LENGTH: u8 = 0x0D;
pub const ATT_ERR_UNLIKELY: u8 = 0x0E;
pub const ATT_ERR_UNSUPPORTED_GROUP_TYPE: u8 = 0x10;
pub const ATT_ERR_INSUFFICIENT_RESOURCES: u8 = 0x11;

// Default BLE ATT MTU
pub const ATT_DEFAULT_MTU: u16 = 23;

// ---------------------------------------------------------------------------
// GATT Service / Characteristic UUIDs (16-bit)
// ---------------------------------------------------------------------------

// Well-known GATT UUIDs (Bluetooth Base UUID: 0000XXXX-0000-1000-8000-00805f9b34fb)
pub const UUID_GAP_SERVICE: u16 = 0x1800;
pub const UUID_GATT_SERVICE: u16 = 0x1801;
pub const UUID_BATTERY_SERVICE: u16 = 0x180F;
pub const UUID_HEART_RATE_SERVICE: u16 = 0x180D;
pub const UUID_DEVICE_INFO_SERVICE: u16 = 0x180A;

// Characteristic UUIDs
pub const UUID_BATTERY_LEVEL: u16 = 0x2A19;
pub const UUID_HEART_RATE_MEASUREMENT: u16 = 0x2A37;
pub const UUID_SYSTEM_ID: u16 = 0x2A23;
pub const UUID_MODEL_NUMBER: u16 = 0x2A24;
pub const UUID_FIRMWARE_REVISION: u16 = 0x2A26;
pub const UUID_MANUFACTURER_NAME: u16 = 0x2A29;

// GATT declaration types
pub const UUID_PRIMARY_SERVICE: u16 = 0x2800;
pub const UUID_SECONDARY_SERVICE: u16 = 0x2801;
pub const UUID_CHARACTERISTIC: u16 = 0x2803;

// GATT client characteristic configuration
pub const UUID_CLIENT_CHAR_CONFIG: u16 = 0x2902;
pub const CCC_NOTIFICATIONS_ENABLED: &[u8; 2] = &[0x01, 0x00];
pub const CCC_INDICATIONS_ENABLED: &[u8; 2] = &[0x02, 0x00];

// Characteristic property flags
pub const CHAR_PROP_BROADCAST: u8 = 0x01;
pub const CHAR_PROP_READ: u8 = 0x02;
pub const CHAR_PROP_WRITE_NO_RSP: u8 = 0x04;
pub const CHAR_PROP_WRITE: u8 = 0x08;
pub const CHAR_PROP_NOTIFY: u8 = 0x10;
pub const CHAR_PROP_INDICATE: u8 = 0x20;
pub const CHAR_PROP_AUTHENTICATED_WRITE: u8 = 0x40;
pub const CHAR_PROP_EXTENDED_PROPERTIES: u8 = 0x80;

// L2CAP channel ID for ATT
pub const L2CAP_ATT_CID: u16 = 0x0004;

// ---------------------------------------------------------------------------
// ATT PDU
// ---------------------------------------------------------------------------

/// An ATT protocol data unit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttPdu {
    pub opcode: u8,
    pub parameters: Vec<u8>,
}

impl AttPdu {
    pub fn new(opcode: u8, parameters: Vec<u8>) -> Self {
        Self { opcode, parameters }
    }

    /// Build an ATT Read By Type Request for a given 16-bit UUID.
    pub fn read_by_type_req(start_handle: u16, end_handle: u16, uuid: u16) -> Self {
        let mut params = Vec::with_capacity(6);
        params.extend_from_slice(&start_handle.to_le_bytes());
        params.extend_from_slice(&end_handle.to_le_bytes());
        params.extend_from_slice(&uuid.to_le_bytes());
        Self::new(ATT_READ_BY_TYPE_REQ, params)
    }

    /// Build an ATT Read By Type Request with 128-bit UUID.
    pub fn read_by_type_req_128(start_handle: u16, end_handle: u16, uuid: &[u8; 16]) -> Self {
        let mut params = Vec::with_capacity(20);
        params.extend_from_slice(&start_handle.to_le_bytes());
        params.extend_from_slice(&end_handle.to_le_bytes());
        params.extend_from_slice(uuid);
        Self::new(ATT_READ_BY_TYPE_REQ, params)
    }

    /// Build an ATT Read Request for a given handle.
    pub fn read_req(handle: u16) -> Self {
        Self::new(ATT_READ_REQ, handle.to_le_bytes().to_vec())
    }

    /// Build an ATT Write Request for a given handle.
    pub fn write_req(handle: u16, value: &[u8]) -> Self {
        let mut params = Vec::with_capacity(2 + value.len());
        params.extend_from_slice(&handle.to_le_bytes());
        params.extend_from_slice(value);
        Self::new(ATT_WRITE_REQ, params)
    }

    /// Build an ATT Read By Group Type Request (discover primary services).
    pub fn read_by_group_type_req(start_handle: u16, end_handle: u16) -> Self {
        let mut params = Vec::with_capacity(6);
        params.extend_from_slice(&start_handle.to_le_bytes());
        params.extend_from_slice(&end_handle.to_le_bytes());
        params.extend_from_slice(&UUID_PRIMARY_SERVICE.to_le_bytes());
        Self::new(ATT_READ_BY_GROUP_TYPE_REQ, params)
    }

    /// Build an ATT Find By Type Value Request.
    pub fn find_by_type_value_req(
        start_handle: u16,
        end_handle: u16,
        uuid: u16,
        value: &[u8],
    ) -> Self {
        let mut params = Vec::with_capacity(6 + value.len());
        params.extend_from_slice(&start_handle.to_le_bytes());
        params.extend_from_slice(&end_handle.to_le_bytes());
        params.extend_from_slice(&uuid.to_le_bytes());
        params.extend_from_slice(value);
        Self::new(ATT_FIND_BY_TYPE_VALUE_REQ, params)
    }

    /// Build an ATT Exchange MTU Request.
    pub fn exchange_mtu_req(mtu: u16) -> Self {
        Self::new(ATT_EXCHANGE_MTU_REQ, mtu.to_le_bytes().to_vec())
    }

    /// Serialize ATT PDU to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(1 + self.parameters.len());
        buf.push(self.opcode);
        buf.extend_from_slice(&self.parameters);
        buf
    }

    /// Parse ATT PDU from bytes.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.is_empty() {
            return None;
        }
        Some(Self::new(data[0], data[1..].to_vec()))
    }
}

// ---------------------------------------------------------------------------
// GATT Discovery Types
// ---------------------------------------------------------------------------

/// A discovered GATT service (from Read By Group Type Response).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GattService {
    pub start_handle: u16,
    pub end_handle: u16,
    pub uuid: Vec<u8>, // 2 bytes for 16-bit UUID, 16 bytes for 128-bit
}

/// A discovered GATT characteristic (from Read By Type Response).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GattCharacteristic {
    pub handle: u16,
    pub properties: u8,
    pub value_handle: u16,
    pub uuid: Vec<u8>,
}

// ---------------------------------------------------------------------------
// ATT Response Parsers
// ---------------------------------------------------------------------------

/// Parse an ATT Read By Type Response to extract GATT characteristics.
pub fn parse_read_by_type_rsp(pdu: &AttPdu) -> Result<Vec<GattCharacteristic>, String> {
    if pdu.opcode != ATT_READ_BY_TYPE_RSP {
        return Err(format!(
            "expected ATT_READ_BY_TYPE_RSP (0x{:02X}), got 0x{:02X}",
            ATT_READ_BY_TYPE_RSP, pdu.opcode
        ));
    }
    if pdu.parameters.len() < 2 {
        return Err("response too short".to_string());
    }
    let length = pdu.parameters[0] as usize;
    if length < 5 || pdu.parameters.len() < 1 + length {
        return Err(format!("invalid entry length {length}"));
    }
    let mut chars = Vec::new();
    let mut offset = 1;
    while offset + length <= pdu.parameters.len() {
        let entry = &pdu.parameters[offset..offset + length];
        let properties = entry[0];
        let value_handle = u16::from_le_bytes([entry[1], entry[2]]);
        let uuid = entry[3..].to_vec();
        chars.push(GattCharacteristic {
            handle: u16::from_le_bytes([entry[1], entry[2]]).wrapping_sub(1),
            properties,
            value_handle,
            uuid,
        });
        offset += length;
    }
    Ok(chars)
}

/// Parse an ATT Read By Group Type Response to extract services.
pub fn parse_read_by_group_type_rsp(pdu: &AttPdu) -> Result<Vec<GattService>, String> {
    if pdu.opcode != ATT_READ_BY_GROUP_TYPE_RSP {
        return Err(format!(
            "expected ATT_READ_BY_GROUP_TYPE_RSP (0x{:02X}), got 0x{:02X}",
            ATT_READ_BY_GROUP_TYPE_RSP, pdu.opcode
        ));
    }
    if pdu.parameters.len() < 2 {
        return Err("response too short".to_string());
    }
    let length = pdu.parameters[0] as usize;
    if length < 6 || pdu.parameters.len() < 1 + length {
        return Err(format!("invalid entry length {length}"));
    }
    let mut services = Vec::new();
    let mut offset = 1;
    while offset + length <= pdu.parameters.len() {
        let entry = &pdu.parameters[offset..offset + length];
        let start_handle = u16::from_le_bytes([entry[0], entry[1]]);
        let end_handle = u16::from_le_bytes([entry[2], entry[3]]);
        let uuid = entry[4..].to_vec();
        services.push(GattService {
            start_handle,
            end_handle,
            uuid,
        });
        offset += length;
    }
    Ok(services)
}

/// Parse an ATT Read Response (returns raw value bytes).
pub fn parse_read_rsp(pdu: &AttPdu) -> Result<Vec<u8>, String> {
    if pdu.opcode != ATT_READ_RSP {
        return Err(format!(
            "expected ATT_READ_RSP (0x{:02X}), got 0x{:02X}",
            ATT_READ_RSP, pdu.opcode
        ));
    }
    Ok(pdu.parameters.clone())
}

/// Check if an ATT PDU is an error response.
pub fn is_att_error(pdu: &AttPdu) -> bool {
    pdu.opcode == ATT_ERROR_RSP
}

/// Parse ATT error response into (request_opcode, handle, error_code).
pub fn parse_att_error(pdu: &AttPdu) -> Option<(u8, u16, u8)> {
    if pdu.opcode != ATT_ERROR_RSP || pdu.parameters.len() < 5 {
        return None;
    }
    let req_opcode = pdu.parameters[0];
    let handle = u16::from_le_bytes([pdu.parameters[1], pdu.parameters[2]]);
    let error_code = pdu.parameters[3];
    Some((req_opcode, handle, error_code))
}

// ---------------------------------------------------------------------------
// ATT-over-ACL Helpers
// ---------------------------------------------------------------------------

/// Wrap an ATT PDU in an L2CAP/ACL packet for sending.
pub fn att_to_acl(connection_handle: u16, att: &AttPdu) -> HciAcl {
    let att_bytes = att.to_bytes();
    let l2cap_len = (att_bytes.len() as u16).to_le_bytes();
    let cid = L2CAP_ATT_CID.to_le_bytes();

    let mut payload = Vec::with_capacity(4 + att_bytes.len());
    payload.extend_from_slice(&l2cap_len);
    payload.extend_from_slice(&cid);
    payload.extend_from_slice(&att_bytes);

    HciAcl::new(connection_handle, 0x00, 0x00, payload)
}

/// Extract ATT PDU from an incoming ACL/L2CAP packet.
pub fn acl_to_att(acl: &HciAcl) -> Option<AttPdu> {
    if acl.data.len() < 4 {
        return None;
    }
    let _l2cap_len = u16::from_le_bytes([acl.data[0], acl.data[1]]);
    let cid = u16::from_le_bytes([acl.data[2], acl.data[3]]);
    if cid != L2CAP_ATT_CID {
        return None;
    }
    AttPdu::from_bytes(&acl.data[4..])
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- PacketIndicator tests ----------------------------------------------

    #[test]
    fn packet_indicator_from_u8_all_valid() {
        assert_eq!(PacketIndicator::from_u8(0x01), Some(PacketIndicator::Command));
        assert_eq!(PacketIndicator::from_u8(0x02), Some(PacketIndicator::Acl));
        assert_eq!(PacketIndicator::from_u8(0x03), Some(PacketIndicator::Sco));
        assert_eq!(PacketIndicator::from_u8(0x04), Some(PacketIndicator::Event));
        assert_eq!(PacketIndicator::from_u8(0x05), Some(PacketIndicator::Iso));
    }

    #[test]
    fn packet_indicator_from_u8_invalid() {
        assert_eq!(PacketIndicator::from_u8(0x00), None);
        assert_eq!(PacketIndicator::from_u8(0x06), None);
        assert_eq!(PacketIndicator::from_u8(0xFF), None);
    }

    #[test]
    fn packet_indicator_repr_values() {
        assert_eq!(PacketIndicator::Command as u8, 0x01);
        assert_eq!(PacketIndicator::Acl as u8, 0x02);
        assert_eq!(PacketIndicator::Sco as u8, 0x03);
        assert_eq!(PacketIndicator::Event as u8, 0x04);
        assert_eq!(PacketIndicator::Iso as u8, 0x05);
    }

    // -- HciCommand tests ---------------------------------------------------

    #[test]
    fn hci_command_to_bytes_round_trip() {
        let cmd = HciCommand::new(0x0C03, vec![0x00, 0x01, 0x02]);
        let bytes = cmd.to_bytes();
        // opcode 0x0C03 → [0x03, 0x0C], param_len 3, params [0x00, 0x01, 0x02]
        assert_eq!(bytes, vec![0x03, 0x0C, 0x03, 0x00, 0x01, 0x02]);
        let parsed = HciCommand::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, cmd);
    }

    #[test]
    fn hci_command_empty_params() {
        let cmd = HciCommand::new(0x1009, vec![]);
        let bytes = cmd.to_bytes();
        assert_eq!(bytes, vec![0x09, 0x10, 0x00]);
        let parsed = HciCommand::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, cmd);
    }

    #[test]
    fn hci_command_wire_length() {
        let cmd = HciCommand::new(0x0C03, vec![0xAA, 0xBB]);
        assert_eq!(cmd.wire_length(), 5); // 3 header + 2 params
    }

    #[test]
    fn hci_command_parse_truncated_header() {
        let result = HciCommand::from_bytes(&[0x03]);
        assert_eq!(
            result,
            Err(HciParseError::InsufficientData {
                expected: 3,
                actual: 1,
            })
        );
    }

    #[test]
    fn hci_command_parse_truncated_params() {
        // Declares 5 params but only provides 2
        let result = HciCommand::from_bytes(&[0x03, 0x0C, 0x05, 0x00, 0x01]);
        assert_eq!(
            result,
            Err(HciParseError::InvalidParameterLength {
                declared: 5,
                available: 2,
            })
        );
    }

    #[test]
    fn hci_command_parse_extra_data_ignored() {
        // Extra trailing bytes beyond declared param_len are not consumed
        let data = vec![0x03, 0x0C, 0x02, 0xAA, 0xBB, 0xCC, 0xDD];
        let parsed = HciCommand::from_bytes(&data).unwrap();
        assert_eq!(parsed.opcode, 0x0C03);
        assert_eq!(parsed.parameters, vec![0xAA, 0xBB]);
    }

    // -- HciEvent tests -----------------------------------------------------

    #[test]
    fn hci_event_command_complete_reset() {
        // Command Complete for HCI Reset (opcode 0x0C03), status 0x00
        // Event wire: [event_code=0x0E, param_len, num_packets, opcode_lo, opcode_hi, status]
        let wire: Vec<u8> = vec![0x0E, 0x04, 0x01, 0x03, 0x0C, 0x00];
        let evt = HciEvent::from_bytes(&wire).unwrap();
        assert_eq!(evt.event_code, EVT_COMMAND_COMPLETE);
        assert!(evt.is_command_complete());
        assert!(!evt.is_command_status());
        assert!(!evt.is_le_meta());
        assert_eq!(evt.parameters.len(), 4);

        let (num_packets, opcode, return_params) = evt.command_complete_params().unwrap();
        assert_eq!(num_packets, 1);
        assert_eq!(opcode, 0x0C03);
        assert!(return_params.is_empty());
    }

    #[test]
    fn hci_event_command_status() {
        // Command Status: [event_code=0x0F, param_len=4, status=0x00, num_packets=1, opcode_lo, opcode_hi]
        let wire: Vec<u8> = vec![0x0F, 0x04, 0x00, 0x01, 0x05, 0x04];
        let evt = HciEvent::from_bytes(&wire).unwrap();
        assert_eq!(evt.event_code, EVT_COMMAND_STATUS);
        assert!(evt.is_command_status());

        let (status, num_packets, opcode) = evt.command_status_params().unwrap();
        assert_eq!(status, STATUS_SUCCESS);
        assert_eq!(num_packets, 1);
        assert_eq!(opcode, 0x0405);
    }

    #[test]
    fn hci_event_command_complete_not_cs() {
        let wire: Vec<u8> = vec![0x0E, 0x04, 0x01, 0x03, 0x0C, 0x00];
        let evt = HciEvent::from_bytes(&wire).unwrap();
        // CC event should not return CS params
        assert!(evt.command_status_params().is_none());
    }

    #[test]
    fn hci_event_command_status_not_cc() {
        let wire: Vec<u8> = vec![0x0F, 0x04, 0x00, 0x01, 0x05, 0x04];
        let evt = HciEvent::from_bytes(&wire).unwrap();
        assert!(evt.command_complete_params().is_none());
    }

    #[test]
    fn hci_event_le_advertising_report() {
        // LE Meta event containing an Advertising Report
        // Payload after header: subevent(1)+num_reports(1)+event_type(1)+addr_type(1)+addr(6)+data_len(1)+data(2)+rssi(1) = 14
        let wire: Vec<u8> = vec![
            0x3E, 0x0E, // event_code, param_len=14
            0x02, 0x01, // subevent=LE_ADVERTISING_REPORT, num_reports=1
            0x00, // event_type=ADV_IND
            0x00, // addr_type=public
            0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, // addr
            0x02, // data_len=2
            0x01, 0x02, // data
            0xC5, // rssi
        ];
        let evt = HciEvent::from_bytes(&wire).unwrap();
        assert_eq!(evt.event_code, EVT_LE_META);
        assert!(evt.is_le_meta());
        assert_eq!(evt.parameters[0], LE_ADVERTISING_REPORT);
        assert_eq!(evt.parameters[1], 1); // num_reports
    }

    #[test]
    fn hci_event_wire_length() {
        let wire: Vec<u8> = vec![0x0E, 0x04, 0x01, 0x03, 0x0C, 0x00];
        let evt = HciEvent::from_bytes(&wire).unwrap();
        assert_eq!(evt.wire_length(), 6);
    }

    #[test]
    fn hci_event_parse_truncated() {
        let result = HciEvent::from_bytes(&[0x0E]);
        assert_eq!(
            result,
            Err(HciParseError::InsufficientData {
                expected: 2,
                actual: 1,
            })
        );
    }

    #[test]
    fn hci_event_parse_truncated_params() {
        // Declares 4 bytes of params but only 1 is present
        let result = HciEvent::from_bytes(&[0x0E, 0x04, 0x01]);
        assert_eq!(
            result,
            Err(HciParseError::InvalidParameterLength {
                declared: 4,
                available: 1,
            })
        );
    }

    #[test]
    fn hci_event_command_complete_params_too_short() {
        // CC event with only 1 byte of params (needs at least 4)
        let wire: Vec<u8> = vec![0x0E, 0x01, 0x01];
        let evt = HciEvent::from_bytes(&wire).unwrap();
        assert!(evt.command_complete_params().is_none());
    }

    #[test]
    fn hci_event_command_status_params_too_short() {
        // CS event with only 2 bytes of params (needs at least 4)
        let wire: Vec<u8> = vec![0x0F, 0x02, 0x00, 0x01];
        let evt = HciEvent::from_bytes(&wire).unwrap();
        assert!(evt.command_status_params().is_none());
    }

    // -- HciAcl tests -------------------------------------------------------

    #[test]
    fn hci_acl_round_trip() {
        let acl = HciAcl::new(0x0001, 0x02, 0x01, vec![0xDE, 0xAD, 0xBE, 0xEF]);
        let bytes = acl.to_bytes();
        let parsed = HciAcl::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, acl);
    }

    #[test]
    fn hci_acl_wire_format() {
        let acl = HciAcl::new(0x0042, 0b01, 0b00, vec![0xCA, 0xFE]);
        let bytes = acl.to_bytes();
        // handle=0x0042, PB=01, BC=00 → handle_word = 0x0042 | (0x01 << 12) = 0x1042
        assert_eq!(bytes[0], 0x42); // lo byte of 0x1042
        assert_eq!(bytes[1], 0x10); // hi byte of 0x1042
        assert_eq!(bytes[2], 0x02); // data_len lo
        assert_eq!(bytes[3], 0x00); // data_len hi
        assert_eq!(&bytes[4..], &[0xCA, 0xFE]);
    }

    #[test]
    fn hci_acl_empty_data() {
        let acl = HciAcl::new(0x0000, 0x00, 0x00, vec![]);
        let bytes = acl.to_bytes();
        assert_eq!(bytes.len(), 4);
        let parsed = HciAcl::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, acl);
    }

    #[test]
    fn hci_acl_parse_truncated() {
        let result = HciAcl::from_bytes(&[0x42, 0x10]);
        assert_eq!(
            result,
            Err(HciParseError::InsufficientData {
                expected: 4,
                actual: 2,
            })
        );
    }

    #[test]
    fn hci_acl_parse_truncated_data() {
        // Declares 8 bytes of data but only provides 2
        let result = HciAcl::from_bytes(&[0x42, 0x10, 0x08, 0x00, 0xCA, 0xFE]);
        assert_eq!(
            result,
            Err(HciParseError::InvalidParameterLength {
                declared: 8,
                available: 2,
            })
        );
    }

    #[test]
    fn hci_acl_max_handle_bits() {
        // Handle uses 12 bits, PB 2 bits, BC 2 bits
        let acl = HciAcl::new(0x0FFF, 0x03, 0x03, vec![0x01]);
        let bytes = acl.to_bytes();
        let parsed = HciAcl::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.handle, 0x0FFF);
        assert_eq!(parsed.pb_flag, 0x03);
        assert_eq!(parsed.bc_flag, 0x03);
        assert_eq!(parsed.data, vec![0x01]);
    }

    // -- build_usb_command tests -------------------------------------------

    #[test]
    fn build_usb_command_produces_correct_sequence() {
        let packet = build_usb_command(0x0C03, vec![]);
        assert_eq!(packet, vec![0x01, 0x03, 0x0C, 0x00]);
    }

    #[test]
    fn build_usb_command_with_params() {
        let packet = build_usb_command(0x1009, vec![0xAA, 0xBB]);
        assert_eq!(packet, vec![0x01, 0x09, 0x10, 0x02, 0xAA, 0xBB]);
    }

    // -- parse_usb_hci_packet tests ----------------------------------------

    #[test]
    fn parse_usb_command_packet() {
        let wire = build_usb_command(0x0C03, vec![0x00]);
        let (indicator, data) = parse_usb_hci_packet(&wire).unwrap();
        assert_eq!(indicator, PacketIndicator::Command);
        match data {
            HciPacketData::Command(cmd) => {
                assert_eq!(cmd.opcode, 0x0C03);
                assert_eq!(cmd.parameters, vec![0x00]);
            }
            other => panic!("expected Command, got {other:?}"),
        }
    }

    #[test]
    fn parse_usb_event_packet() {
        // Build: [0x04, 0x0E, 0x04, 0x01, 0x03, 0x0C, 0x00]
        let mut wire = vec![0x04];
        wire.extend_from_slice(&[0x0E, 0x04, 0x01, 0x03, 0x0C, 0x00]);
        let (indicator, data) = parse_usb_hci_packet(&wire).unwrap();
        assert_eq!(indicator, PacketIndicator::Event);
        match data {
            HciPacketData::Event(evt) => {
                assert_eq!(evt.event_code, EVT_COMMAND_COMPLETE);
            }
            other => panic!("expected Event, got {other:?}"),
        }
    }

    #[test]
    fn parse_usb_acl_packet() {
        let acl = HciAcl::new(0x0001, 0x00, 0x00, vec![0xDE]);
        let mut wire = vec![0x02];
        wire.extend_from_slice(&acl.to_bytes());
        let (indicator, data) = parse_usb_hci_packet(&wire).unwrap();
        assert_eq!(indicator, PacketIndicator::Acl);
        match data {
            HciPacketData::Acl(parsed) => {
                assert_eq!(parsed, acl);
            }
            other => panic!("expected Acl, got {other:?}"),
        }
    }

    #[test]
    fn parse_usb_sco_packet() {
        let wire: Vec<u8> = vec![0x03, 0x01, 0x02, 0x03];
        let (indicator, data) = parse_usb_hci_packet(&wire).unwrap();
        assert_eq!(indicator, PacketIndicator::Sco);
        match data {
            HciPacketData::Sco(payload) => {
                assert_eq!(payload, vec![0x01, 0x02, 0x03]);
            }
            other => panic!("expected Sco, got {other:?}"),
        }
    }

    #[test]
    fn parse_usb_iso_packet() {
        let wire: Vec<u8> = vec![0x05, 0xAA, 0xBB];
        let (indicator, data) = parse_usb_hci_packet(&wire).unwrap();
        assert_eq!(indicator, PacketIndicator::Iso);
        match data {
            HciPacketData::Iso(payload) => {
                assert_eq!(payload, vec![0xAA, 0xBB]);
            }
            other => panic!("expected Iso, got {other:?}"),
        }
    }

    #[test]
    fn parse_usb_empty_data() {
        let result = parse_usb_hci_packet(&[]);
        assert_eq!(
            result,
            Err(HciParseError::InsufficientData {
                expected: 1,
                actual: 0,
            })
        );
    }

    #[test]
    fn parse_usb_invalid_indicator() {
        let result = parse_usb_hci_packet(&[0x00, 0x01, 0x02]);
        assert_eq!(result, Err(HciParseError::InvalidPacketIndicator(0x00)));
    }

    #[test]
    fn parse_usb_command_truncated_payload() {
        // Indicator byte says command, but no payload
        let result = parse_usb_hci_packet(&[0x01]);
        assert_eq!(
            result,
            Err(HciParseError::InsufficientData {
                expected: 3,
                actual: 0,
            })
        );
    }

    // -- Command builder tests -----------------------------------------------

    #[test]
    fn cmd_reset_builds_correct_command() {
        let cmd = cmd_reset();
        assert_eq!(cmd.opcode, 0x0C03);
        assert!(cmd.parameters.is_empty());
    }

    #[test]
    fn cmd_read_bd_addr_builds_correct_command() {
        let cmd = cmd_read_bd_addr();
        assert_eq!(cmd.opcode, 0x1009);
        assert!(cmd.parameters.is_empty());
    }

    #[test]
    fn cmd_le_set_scan_parameters_builds_correct_packet() {
        let cmd = cmd_le_set_scan_parameters(0x01, 0x0030, 0x0020, 0x00, 0x00);
        assert_eq!(cmd.opcode, 0x200B);
        assert_eq!(cmd.parameters.len(), 7);
        assert_eq!(cmd.parameters[0], 0x01); // scan_type
        assert_eq!(u16::from_le_bytes([cmd.parameters[1], cmd.parameters[2]]), 0x0030); // interval
        assert_eq!(u16::from_le_bytes([cmd.parameters[3], cmd.parameters[4]]), 0x0020); // window
        assert_eq!(cmd.parameters[5], 0x00); // own_address_type
        assert_eq!(cmd.parameters[6], 0x00); // filter_policy
    }

    #[test]
    fn cmd_le_set_scan_enable_builds_correct_packet() {
        let cmd = cmd_le_set_scan_enable(0x01, 0x01);
        assert_eq!(cmd.opcode, 0x200C);
        assert_eq!(cmd.parameters, vec![0x01, 0x01]);
    }

    #[test]
    fn cmd_le_create_connection_builds_correct_packet() {
        let peer_addr: [u8; 6] = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        let cmd = cmd_le_create_connection(
            0x0060, // scan_interval
            0x0030, // scan_window
            0x00,   // initiator_filter_policy
            0x00,   // peer_address_type
            &peer_addr,
            0x00,   // own_address_type
            0x0006, // conn_interval_min
            0x000C, // conn_interval_max
            0x0000, // conn_latency
            0x00C8, // supervision_timeout
            0x0001, // min_ce_length
            0x0002, // max_ce_length
        );
        assert_eq!(cmd.opcode, 0x200D);
        assert_eq!(cmd.parameters.len(), 25);
        // scan_interval (2) + scan_window (2) + filter_policy (1) + addr_type (1)
        // + addr (6) + own_addr_type (1) + interval_min (2) + interval_max (2)
        // + latency (2) + timeout (2) + min_ce (2) + max_ce (2) = 25
        assert_eq!(u16::from_le_bytes([cmd.parameters[0], cmd.parameters[1]]), 0x0060);
        assert_eq!(u16::from_le_bytes([cmd.parameters[2], cmd.parameters[3]]), 0x0030);
        assert_eq!(cmd.parameters[4], 0x00); // filter_policy
        assert_eq!(cmd.parameters[5], 0x00); // peer_address_type
        assert_eq!(&cmd.parameters[6..12], &peer_addr);
        assert_eq!(cmd.parameters[12], 0x00); // own_address_type
        assert_eq!(u16::from_le_bytes([cmd.parameters[13], cmd.parameters[14]]), 0x0006);
        assert_eq!(u16::from_le_bytes([cmd.parameters[15], cmd.parameters[16]]), 0x000C);
        assert_eq!(u16::from_le_bytes([cmd.parameters[17], cmd.parameters[18]]), 0x0000);
        assert_eq!(u16::from_le_bytes([cmd.parameters[19], cmd.parameters[20]]), 0x00C8);
        assert_eq!(u16::from_le_bytes([cmd.parameters[21], cmd.parameters[22]]), 0x0001);
        assert_eq!(u16::from_le_bytes([cmd.parameters[23], cmd.parameters[24]]), 0x0002);
    }

    #[test]
    fn cmd_disconnect_builds_correct_packet() {
        let cmd = cmd_disconnect(0x0023, 0x13);
        assert_eq!(cmd.opcode, 0x0406);
        assert_eq!(cmd.parameters.len(), 3);
        assert_eq!(u16::from_le_bytes([cmd.parameters[0], cmd.parameters[1]]), 0x0023);
        assert_eq!(cmd.parameters[2], 0x13);
    }

    // -- Event parser tests --------------------------------------------------

    fn make_cc_event(opcode: u16, return_params: &[u8]) -> HciEvent {
        // CC event: event_code=0x0E, params=[num_packets, opcode_lo, opcode_hi, return_params...]
        let mut params = vec![0x01]; // num_packets = 1
        params.push(opcode as u8);
        params.push((opcode >> 8) as u8);
        params.extend_from_slice(return_params);
        HciEvent {
            event_code: EVT_COMMAND_COMPLETE,
            parameters: params,
        }
    }

    fn make_le_meta_event(subevent_params: &[u8]) -> HciEvent {
        let mut params = subevent_params.to_vec();
        HciEvent {
            event_code: EVT_LE_META,
            parameters: params,
        }
    }

    #[test]
    fn parse_read_bd_addr_extracts_address() {
        let return_params = [0x00, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]; // status + addr
        let event = make_cc_event(OP_READ_BD_ADDR, &return_params);
        let result = parse_read_bd_addr(&event);
        let parsed = result.expect("should parse");
        assert_eq!(parsed.status, 0x00);
        assert_eq!(parsed.address, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
    }

    #[test]
    fn parse_read_bd_addr_returns_none_for_wrong_opcode() {
        let return_params = [0x00, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        let event = make_cc_event(OP_RESET, &return_params);
        assert!(parse_read_bd_addr(&event).is_none());
    }

    #[test]
    fn parse_local_version_extracts_all_fields() {
        // status(1) + hci_version(1) + hci_revision(2) + lmp_version(1) + manufacturer(2) + lmp_sub(2) = 9
        let return_params: [u8; 9] = [
            0x00, // status
            0x09, // hci_version (Bluetooth 5.0)
            0x00, 0x00, // hci_revision
            0x09, // lmp_version
            0x02, 0x00, // manufacturer_name (0x0002)
            0x01, 0x00, // lmp_subversion (0x0001)
        ];
        let event = make_cc_event(OP_READ_LOCAL_VERSION, &return_params);
        let result = parse_local_version(&event);
        let parsed = result.expect("should parse");
        assert_eq!(parsed.status, 0x00);
        assert_eq!(parsed.hci_version, 0x09);
        assert_eq!(parsed.hci_revision, 0x0000);
        assert_eq!(parsed.lmp_version, 0x09);
        assert_eq!(parsed.manufacturer_name, 0x0002);
        assert_eq!(parsed.lmp_subversion, 0x0001);
    }

    #[test]
    fn parse_le_advertising_reports_parses_single_report() {
        // subevent(1) + num_reports(1) + event_type(1) + addr_type(1) + addr(6) + data_len(1) + data(2) + rssi(1)
        let subevent_params: Vec<u8> = vec![
            LE_ADVERTISING_REPORT, // subevent
            0x01,                  // num_reports = 1
            0x00,                  // event_type = ADV_IND
            0x01,                  // address_type = random
            0x11, 0x22, 0x33, 0x44, 0x55, 0x66, // address
            0x02,                  // data_len = 2
            0xDE, 0xAD,           // data
            0xC5,                  // rssi
        ];
        let event = make_le_meta_event(&subevent_params);
        let reports = parse_le_advertising_reports(&event).expect("should parse");
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].event_type, 0x00);
        assert_eq!(reports[0].address_type, 0x01);
        assert_eq!(reports[0].address, [0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
        assert_eq!(reports[0].data, vec![0xDE, 0xAD]);
        assert_eq!(reports[0].rssi, 0xC5u8 as i8);
    }

    #[test]
    fn parse_le_advertising_reports_parses_multiple_reports() {
        // Two advertising reports back-to-back
        let subevent_params: Vec<u8> = vec![
            LE_ADVERTISING_REPORT, // subevent
            0x02,                  // num_reports = 2
            // Report 1
            0x00,                  // event_type
            0x00,                  // address_type
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, // address
            0x01,                  // data_len = 1
            0xFF,                  // data
            0x10,                  // rssi
            // Report 2
            0x03,                  // event_type = SCAN_RSP
            0x01,                  // address_type = random
            0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, // address
            0x00,                  // data_len = 0
            0x20,                  // rssi
        ];
        let event = make_le_meta_event(&subevent_params);
        let reports = parse_le_advertising_reports(&event).expect("should parse");
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[0].event_type, 0x00);
        assert_eq!(reports[0].address, [0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
        assert_eq!(reports[0].data, vec![0xFF]);
        assert_eq!(reports[0].rssi, 0x10i8);
        assert_eq!(reports[1].event_type, 0x03);
        assert_eq!(reports[1].address, [0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F]);
        assert!(reports[1].data.is_empty());
        assert_eq!(reports[1].rssi, 0x20i8);
    }

    #[test]
    fn parse_le_connection_complete_extracts_all_fields() {
        // Subevent(1) + status(1) + handle(2) + role(1) + addr_type(1) + addr(6)
        // + interval(2) + latency(2) + timeout(2) + mca(1) = 19
        let mut subevent_params: Vec<u8> = vec![
            LE_CONNECTION_COMPLETE, // subevent
            0x00,                   // status = success
            0x01, 0x00,             // connection_handle = 0x0001
            0x00,                   // role = master
            0x00,                   // peer_address_type = public
        ];
        subevent_params.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]); // peer_address
        subevent_params.extend_from_slice(&0x0024u16.to_le_bytes()); // conn_interval
        subevent_params.extend_from_slice(&0x0000u16.to_le_bytes()); // conn_latency
        subevent_params.extend_from_slice(&0x01F4u16.to_le_bytes()); // supervision_timeout
        subevent_params.push(0x01);                                   // master_clock_accuracy
        let event = make_le_meta_event(&subevent_params);
        let result = parse_le_connection_complete(&event);
        let parsed = result.expect("should parse");
        assert_eq!(parsed.status, 0x00);
        assert_eq!(parsed.connection_handle, 0x0001);
        assert_eq!(parsed.role, 0x00);
        assert_eq!(parsed.peer_address_type, 0x00);
        assert_eq!(parsed.peer_address, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
        assert_eq!(parsed.conn_interval, 0x0024);
        assert_eq!(parsed.conn_latency, 0x0000);
        assert_eq!(parsed.supervision_timeout, 0x01F4);
        assert_eq!(parsed.master_clock_accuracy, 0x01);
    }

    #[test]
    fn parse_le_connection_complete_returns_none_for_wrong_subevent() {
        let mut subevent_params = vec![LE_ADVERTISING_REPORT, 0x01]; // wrong subevent
        subevent_params.extend_from_slice(&[0u8; 16]);
        let event = make_le_meta_event(&subevent_params);
        assert!(parse_le_connection_complete(&event).is_none());
    }

    #[test]
    fn parse_le_buffer_size_extracts_fields() {
        // status(1) + le_acl_data_packet_length(2) + total_num_le_acl_data_packets(1) = 4
        let return_params: [u8; 4] = [
            0x00,       // status
            0x1B, 0x02, // le_acl_data_packet_length = 0x021B (539)
            0x05,       // total_num_le_acl_data_packets = 5
        ];
        let event = make_cc_event(OP_LE_READ_BUFFER_SIZE, &return_params);
        let result = parse_le_buffer_size(&event);
        let parsed = result.expect("should parse");
        assert_eq!(parsed.status, 0x00);
        assert_eq!(parsed.le_acl_data_packet_length, 0x021B);
        assert_eq!(parsed.total_num_le_acl_data_packets, 5);
    }

    #[test]
    fn parse_functions_return_none_for_truncated_data() {
        // Truncated Read BD_ADDR — only 3 bytes of return params (need 7)
        let return_params_short = [0x00, 0xAA, 0xBB];
        let event = make_cc_event(OP_READ_BD_ADDR, &return_params_short);
        assert!(parse_read_bd_addr(&event).is_none());

        // Truncated Local Version — only 5 bytes (need 9)
        let return_params_short = [0x00, 0x09, 0x00, 0x00, 0x09];
        let event = make_cc_event(OP_READ_LOCAL_VERSION, &return_params_short);
        assert!(parse_local_version(&event).is_none());

        // Truncated LE Connection Complete — too short
        let subevent_params = vec![LE_CONNECTION_COMPLETE, 0x00, 0x01, 0x00];
        let event = make_le_meta_event(&subevent_params);
        assert!(parse_le_connection_complete(&event).is_none());

        // Truncated LE Buffer Size — only 2 bytes of return params (need 3)
        let return_params_short = [0x00, 0x1B];
        let event = make_cc_event(OP_LE_READ_BUFFER_SIZE, &return_params_short);
        assert!(parse_le_buffer_size(&event).is_none());

        // Non-CC event for BD_ADDR parser
        let event = HciEvent {
            event_code: EVT_COMMAND_STATUS,
            parameters: vec![0x00, 0x01, 0x09, 0x10],
        };
        assert!(parse_read_bd_addr(&event).is_none());

        // Non-LE-Meta event for advertising parser
        let event = HciEvent {
            event_code: EVT_COMMAND_COMPLETE,
            parameters: vec![],
        };
        assert!(parse_le_advertising_reports(&event).is_none());
    }

    // -- ATT/GATT tests -------------------------------------------------------

    #[test]
    fn att_pdu_to_bytes_round_trip() {
        let pdu = AttPdu::new(ATT_READ_REQ, vec![0x0A, 0x00]);
        let bytes = pdu.to_bytes();
        assert_eq!(bytes, vec![ATT_READ_REQ, 0x0A, 0x00]);
        let restored = AttPdu::from_bytes(&bytes).unwrap();
        assert_eq!(restored, pdu);
    }

    #[test]
    fn att_read_by_type_req_builds_correct_params() {
        let pdu = AttPdu::read_by_type_req(0x0001, 0xFFFF, UUID_BATTERY_LEVEL);
        assert_eq!(pdu.opcode, ATT_READ_BY_TYPE_REQ);
        assert_eq!(pdu.parameters.len(), 6);
        assert_eq!(u16::from_le_bytes([pdu.parameters[0], pdu.parameters[1]]), 0x0001);
        assert_eq!(u16::from_le_bytes([pdu.parameters[2], pdu.parameters[3]]), 0xFFFF);
        assert_eq!(u16::from_le_bytes([pdu.parameters[4], pdu.parameters[5]]), UUID_BATTERY_LEVEL);
    }

    #[test]
    fn att_read_req_builds_correct_handle() {
        let pdu = AttPdu::read_req(0x0025);
        assert_eq!(pdu.opcode, ATT_READ_REQ);
        assert_eq!(pdu.parameters, vec![0x25, 0x00]);
    }

    #[test]
    fn att_write_req_builds_correct_handle_and_value() {
        let pdu = AttPdu::write_req(0x002A, &[0x01, 0x00]);
        assert_eq!(pdu.opcode, ATT_WRITE_REQ);
        assert_eq!(pdu.parameters.len(), 4);
        assert_eq!(u16::from_le_bytes([pdu.parameters[0], pdu.parameters[1]]), 0x002A);
        assert_eq!(&pdu.parameters[2..], &[0x01, 0x00]);
    }

    #[test]
    fn att_read_by_group_type_req_uses_primary_service_uuid() {
        let pdu = AttPdu::read_by_group_type_req(0x0001, 0xFFFF);
        assert_eq!(pdu.opcode, ATT_READ_BY_GROUP_TYPE_REQ);
        assert_eq!(pdu.parameters.len(), 6);
        assert_eq!(
            u16::from_le_bytes([pdu.parameters[4], pdu.parameters[5]]),
            UUID_PRIMARY_SERVICE
        );
    }

    #[test]
    fn att_to_acl_wraps_in_l2cap_att_channel() {
        let att = AttPdu::read_req(0x0003);
        let acl = att_to_acl(0x0042, &att);
        assert_eq!(acl.handle, 0x0042);
        assert_eq!(acl.pb_flag, 0x00);
        assert_eq!(acl.bc_flag, 0x00);
        // data = l2cap_len(2) + cid(2) + att_bytes
        assert!(acl.data.len() >= 4);
        let cid = u16::from_le_bytes([acl.data[2], acl.data[3]]);
        assert_eq!(cid, L2CAP_ATT_CID);
        let l2cap_len = u16::from_le_bytes([acl.data[0], acl.data[1]]) as usize;
        assert_eq!(l2cap_len, acl.data.len() - 4);
    }

    #[test]
    fn acl_to_att_extracts_att_pdu() {
        let original = AttPdu::write_req(0x0029, &[0xAA, 0xBB]);
        let acl = att_to_acl(0x0001, &original);
        let extracted = acl_to_att(&acl).unwrap();
        assert_eq!(extracted, original);
    }

    #[test]
    fn acl_to_att_returns_none_for_non_att_cid() {
        let acl = HciAcl::new(0x0001, 0x00, 0x00, vec![0x01, 0x00, 0x05, 0x00, 0xFF]);
        assert!(acl_to_att(&acl).is_none());
    }

    #[test]
    fn parse_read_by_type_rsp_extracts_characteristics() {
        let mut params = vec![5]; // length: props(1) + handle(2) + uuid(2) = 5
        params.push(CHAR_PROP_READ | CHAR_PROP_NOTIFY); // properties
        params.extend_from_slice(&0x0010u16.to_le_bytes()); // value_handle
        params.extend_from_slice(&UUID_BATTERY_LEVEL.to_le_bytes()); // uuid
        let pdu = AttPdu::new(ATT_READ_BY_TYPE_RSP, params);
        let chars = parse_read_by_type_rsp(&pdu).unwrap();
        assert_eq!(chars.len(), 1);
        assert_eq!(chars[0].properties, CHAR_PROP_READ | CHAR_PROP_NOTIFY);
        assert_eq!(chars[0].value_handle, 0x0010);
        assert_eq!(chars[0].handle, 0x000F);
        assert_eq!(
            u16::from_le_bytes([chars[0].uuid[0], chars[0].uuid[1]]),
            UUID_BATTERY_LEVEL
        );
    }

    #[test]
    fn parse_read_by_group_type_rsp_extracts_services() {
        let mut params = vec![6]; // length: start(2) + end(2) + uuid(2) = 6
        params.extend_from_slice(&0x0001u16.to_le_bytes()); // start_handle
        params.extend_from_slice(&0x0005u16.to_le_bytes()); // end_handle
        params.extend_from_slice(&UUID_BATTERY_SERVICE.to_le_bytes()); // uuid
        let pdu = AttPdu::new(ATT_READ_BY_GROUP_TYPE_RSP, params);
        let services = parse_read_by_group_type_rsp(&pdu).unwrap();
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].start_handle, 0x0001);
        assert_eq!(services[0].end_handle, 0x0005);
        assert_eq!(
            u16::from_le_bytes([services[0].uuid[0], services[0].uuid[1]]),
            UUID_BATTERY_SERVICE
        );
    }

    #[test]
    fn parse_att_error_extracts_fields() {
        let params = vec![
            ATT_READ_REQ,       // request opcode
            0x0A, 0x00,         // handle (0x000A)
            ATT_ERR_ATTRIBUTE_NOT_FOUND, // error code
            0x00,               // padding to satisfy len < 5 check
        ];
        let pdu = AttPdu::new(ATT_ERROR_RSP, params);
        let result = parse_att_error(&pdu).unwrap();
        assert_eq!(result.0, ATT_READ_REQ);
        assert_eq!(result.1, 0x000A);
        assert_eq!(result.2, ATT_ERR_ATTRIBUTE_NOT_FOUND);
    }

    #[test]
    fn is_att_error_identifies_error_responses() {
        let err = AttPdu::new(ATT_ERROR_RSP, vec![0x08, 0x01, 0x00, 0x0A, 0x00]);
        assert!(is_att_error(&err));
        let not_err = AttPdu::new(ATT_READ_RSP, vec![0x42]);
        assert!(!is_att_error(&not_err));
    }
}
