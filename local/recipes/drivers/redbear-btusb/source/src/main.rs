mod hci;
mod usb_transport;

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process;
#[cfg(target_os = "redox")]
use std::thread;
#[cfg(target_os = "redox")]
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use hci::{cmd_read_bd_addr, cmd_read_local_version, cmd_reset, parse_read_bd_addr, parse_local_version};
use usb_transport::UsbHciTransport;
#[cfg(target_os = "redox")]
use usb_transport::{StubTransport, UsbTransportConfig};

const STATUS_FRESHNESS_SECS: u64 = 90;
const BLUETOOTH_USB_CLASS: u8 = 0xE0;
const BLUETOOTH_USB_SUBCLASS: u8 = 0x01;
const BLUETOOTH_USB_PROTOCOL: u8 = 0x01;
const KNOWN_BLUETOOTH_USB_VENDORS: [u16; 4] = [0x8087, 0x0BDA, 0x0A5C, 0x0A12];

#[derive(Clone, Debug, PartialEq, Eq)]
struct UsbBluetoothAdapter {
    name: String,
    vendor_id: u16,
    device_id: u16,
    bus: String,
    device_path: PathBuf,
    endpoints: HciEndpoints,
}

impl UsbBluetoothAdapter {
    #[cfg(any(not(target_os = "redox"), test))]
    fn stub(name: String) -> Self {
        Self {
            device_path: PathBuf::from(format!("/scheme/usb/stub/{name}")),
            name,
            vendor_id: 0,
            device_id: 0,
            bus: "stub".to_string(),
            endpoints: HciEndpoints::default(),
        }
    }

    fn detail_line(&self, index: usize) -> String {
        format!(
            "adapter_{index}=name={};vendor_id={:04x};device_id={:04x};bus={};device_path={};event_ep={};acl_in_ep={};acl_out_ep={}",
            self.name,
            self.vendor_id,
            self.device_id,
            self.bus,
            self.device_path.display(),
            self.endpoints.event_endpoint,
            self.endpoints.acl_in_endpoint,
            self.endpoints.acl_out_endpoint,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct UsbDeviceDescriptor {
    vendor_id: u16,
    device_id: u16,
    class: u8,
    subclass: u8,
    protocol: u8,
}

impl UsbDeviceDescriptor {
    fn looks_like_bluetooth(self) -> bool {
        (self.class, self.subclass, self.protocol)
            == (
                BLUETOOTH_USB_CLASS,
                BLUETOOTH_USB_SUBCLASS,
                BLUETOOTH_USB_PROTOCOL,
            )
            || KNOWN_BLUETOOTH_USB_VENDORS.contains(&self.vendor_id)
    }
}

/// USB HCI transport endpoint addresses for a Bluetooth controller
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HciEndpoints {
    /// Interrupt IN endpoint for HCI events (required)
    pub event_endpoint: u8,
    /// Bulk IN endpoint for ACL data from controller (required)
    pub acl_in_endpoint: u8,
    /// Bulk OUT endpoint for ACL data to controller (required)
    pub acl_out_endpoint: u8,
    /// Maximum packet size for the event endpoint
    pub event_max_packet_size: u16,
    /// Maximum packet size for the bulk IN endpoint
    pub acl_in_max_packet_size: u16,
    /// Maximum packet size for the bulk OUT endpoint
    pub acl_out_max_packet_size: u16,
}

/// Controller initialization state
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControllerState {
    /// No communication with controller yet
    #[default]
    Closed,
    /// Sending HCI initialization commands
    Initializing,
    /// Controller is ready for use
    Active,
    /// Initialization or communication failed
    Error,
}

/// Information gathered from the controller during initialization
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ControllerInfo {
    pub state: ControllerState,
    pub bd_address: Option<[u8; 6]>,
    pub hci_version: Option<u8>,
    pub hci_revision: Option<u16>,
    pub manufacturer_name: Option<u16>,
    pub init_error: Option<String>,
}

/// USB endpoint descriptor fields extracted from raw descriptor data
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct UsbEndpointDescriptor {
    endpoint_address: u8,
    attributes: u8,
    max_packet_size: u16,
    interval: u8,
}

/// USB transfer type from bmAttributes bits 0-1
const USB_TRANSFER_INTERRUPT: u8 = 3;
const USB_TRANSFER_BULK: u8 = 2;

/// Parse a single USB endpoint descriptor from raw bytes.
///
/// USB endpoint descriptor is 7 bytes:
///   [0] bLength = 7
///   [1] bDescriptorType = 5 (ENDPOINT)
///   [2] bEndpointAddress (bit 7 = direction: 0=OUT, 1=IN; bits 0-3 = endpoint number)
///   [3] bmAttributes (bits 0-1 = transfer type: 0=control, 1=isochronous, 2=bulk, 3=interrupt)
///   [4-5] wMaxPacketSize (little-endian)
///   [6] bInterval
fn parse_usb_endpoint_descriptor(raw: &[u8]) -> Option<UsbEndpointDescriptor> {
    if raw.len() < 7 {
        return None;
    }
    if raw[0] != 7 {
        return None; // bLength must be 7
    }
    if raw[1] != 5 {
        return None; // bDescriptorType must be ENDPOINT (5)
    }
    Some(UsbEndpointDescriptor {
        endpoint_address: raw[2],
        attributes: raw[3],
        max_packet_size: u16::from_le_bytes([raw[4], raw[5]]),
        interval: raw[6],
    })
}

/// Parse HCI endpoints from raw USB descriptors blob.
///
/// Walks the descriptor blob looking for endpoint descriptors that match
/// the Bluetooth HCI interface (interrupt IN, bulk IN, bulk OUT).
pub fn parse_hci_endpoints_from_descriptors(raw: &[u8]) -> Result<HciEndpoints, String> {
    let mut endpoints = HciEndpoints::default();
    let mut found_interrupt_in = false;
    let mut found_bulk_in = false;
    let mut found_bulk_out = false;

    let mut offset = 0;
    while offset + 2 <= raw.len() {
        let desc_len = raw[offset] as usize;
        let desc_type = raw[offset + 1];

        if desc_len < 2 || offset + desc_len > raw.len() {
            break;
        }

        if desc_type == 5 {
            // ENDPOINT descriptor
            if let Some(ep) = parse_usb_endpoint_descriptor(&raw[offset..]) {
                let direction_in = (ep.endpoint_address & 0x80) != 0;
                let endpoint_num = ep.endpoint_address & 0x0F;
                let transfer_type = ep.attributes & 0x03;

                match (transfer_type, direction_in) {
                    (USB_TRANSFER_INTERRUPT, true) => {
                        endpoints.event_endpoint = endpoint_num;
                        endpoints.event_max_packet_size = ep.max_packet_size;
                        found_interrupt_in = true;
                    }
                    (USB_TRANSFER_BULK, true) => {
                        endpoints.acl_in_endpoint = endpoint_num;
                        endpoints.acl_in_max_packet_size = ep.max_packet_size;
                        found_bulk_in = true;
                    }
                    (USB_TRANSFER_BULK, false) => {
                        endpoints.acl_out_endpoint = endpoint_num;
                        endpoints.acl_out_max_packet_size = ep.max_packet_size;
                        found_bulk_out = true;
                    }
                    _ => {}
                }
            }
        }

        offset += desc_len.max(1); // Avoid infinite loop on zero-length descriptor
    }

    if !found_interrupt_in {
        return Err("missing HCI interrupt IN endpoint in USB descriptors".to_string());
    }
    if !found_bulk_in {
        return Err("missing HCI bulk IN endpoint in USB descriptors".to_string());
    }
    if !found_bulk_out {
        return Err("missing HCI bulk OUT endpoint in USB descriptors".to_string());
    }

    Ok(endpoints)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TransportConfig {
    adapters: Vec<UsbBluetoothAdapter>,
    controller_family: String,
    status_file: PathBuf,
    controller_info: ControllerInfo,
}

impl TransportConfig {
    fn from_env() -> Self {
        Self {
            adapters: default_adapters_from_env(),
            controller_family: std::env::var("REDBEAR_BTUSB_STUB_FAMILY")
                .unwrap_or_else(|_| "usb-generic-bounded".to_string()),
            status_file: std::env::var_os("REDBEAR_BTUSB_STATUS_FILE")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/var/run/redbear-btusb/status")),
            controller_info: ControllerInfo::default(),
        }
    }

    fn adapter_names(&self) -> Vec<String> {
        self.adapters
            .iter()
            .map(|adapter| adapter.name.clone())
            .collect()
    }

    fn refreshed(&self) -> Self {
        let mut refreshed = self.clone();
        if let Ok(adapters) = runtime_usb_bluetooth_adapters() {
            refreshed.adapters = adapters;
        }
        refreshed
    }

    fn probe_lines(&self) -> Vec<String> {
        let mut lines = vec![
            format!("adapters={}", self.adapter_names().join(",")),
            "transport=usb".to_string(),
            "startup=explicit".to_string(),
            "mode=ble-first".to_string(),
            format!("controller_family={}", self.controller_family),
            format!("adapter_count={}", self.adapters.len()),
        ];
        lines.extend(
            self.adapters
                .iter()
                .enumerate()
                .map(|(index, adapter)| adapter.detail_line(index)),
        );
        lines
    }

    fn render_status_lines(&self, runtime_visible: bool) -> Vec<String> {
        let mut lines = self.probe_lines();
        lines.push(format!("updated_at_epoch={}", current_epoch_seconds()));
        lines.push(format!(
            "runtime_visibility={}",
            if runtime_visible {
                "runtime-visible"
            } else {
                "installed-only"
            }
        ));
        lines.push(format!(
            "daemon_status={}",
            if runtime_visible {
                "running"
            } else {
                "inactive"
            }
        ));
        lines.push(format!("status_file={}", self.status_file.display()));

        let state_str = match self.controller_info.state {
            ControllerState::Closed => "closed",
            ControllerState::Initializing => "initializing",
            ControllerState::Active => "active",
            ControllerState::Error => "error",
        };
        lines.push(format!("controller_state={state_str}"));

        if let Some(addr) = &self.controller_info.bd_address {
            lines.push(format!(
                "bd_address={:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                addr[5], addr[4], addr[3], addr[2], addr[1], addr[0]
            ));
        }
        if let Some(version) = &self.controller_info.hci_version {
            lines.push(format!("hci_version={version}"));
        }
        if let Some(revision) = &self.controller_info.hci_revision {
            lines.push(format!("hci_revision={revision}"));
        }
        if let Some(manufacturer) = &self.controller_info.manufacturer_name {
            lines.push(format!("manufacturer={manufacturer}"));
        }
        if let Some(err) = &self.controller_info.init_error {
            lines.push(format!("init_error={err}"));
        }

        lines
    }

    fn current_status_lines(&self) -> Vec<String> {
        read_status_lines(&self.status_file)
            .filter(|lines| status_lines_are_fresh(lines))
            .unwrap_or_else(|| self.render_status_lines(false))
    }

    fn write_status_file(&self) -> Result<(), String> {
        if let Some(parent) = self.status_file.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                format!(
                    "failed to create transport status directory {}: {err}",
                    parent.display()
                )
            })?;
        }
        fs::write(
            &self.status_file,
            format_lines(&self.render_status_lines(true)),
        )
        .map_err(|err| {
            format!(
                "failed to write transport status file {}: {err}",
                self.status_file.display()
            )
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
    Probe,
    Status,
    Daemon,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CommandOutcome {
    Print(String),
    RunDaemon,
}

#[cfg(any(not(target_os = "redox"), test))]
fn default_adapters_from_names(names: Vec<String>) -> Vec<UsbBluetoothAdapter> {
    names.into_iter().map(UsbBluetoothAdapter::stub).collect()
}

#[cfg(target_os = "redox")]
fn default_adapters_from_env() -> Vec<UsbBluetoothAdapter> {
    Vec::new()
}

#[cfg(not(target_os = "redox"))]
fn default_adapters_from_env() -> Vec<UsbBluetoothAdapter> {
    default_adapters_from_names(parse_list(
        std::env::var("REDBEAR_BTUSB_STUB_ADAPTERS").ok().as_deref(),
        &["hci0"],
    ))
}

#[cfg(target_os = "redox")]
fn runtime_usb_bluetooth_adapters() -> Result<Vec<UsbBluetoothAdapter>, String> {
    probe_usb_bluetooth_adapters()
}

#[cfg(not(target_os = "redox"))]
fn runtime_usb_bluetooth_adapters() -> Result<Vec<UsbBluetoothAdapter>, String> {
    Ok(default_adapters_from_env())
}

#[cfg(any(not(target_os = "redox"), test))]
fn parse_list(raw: Option<&str>, default: &[&str]) -> Vec<String> {
    raw.map(|value| {
        value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>()
    })
    .filter(|values| !values.is_empty())
    .unwrap_or_else(|| default.iter().map(|value| (*value).to_string()).collect())
}

fn format_lines(lines: &[String]) -> String {
    if lines.is_empty() {
        "\n".to_string()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

fn current_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn read_status_lines(path: &Path) -> Option<Vec<String>> {
    let content = fs::read_to_string(path).ok()?;
    let lines = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    Some(lines)
}

fn status_lines_are_fresh(lines: &[String]) -> bool {
    let updated_at = lines.iter().find_map(|line| {
        line.strip_prefix("updated_at_epoch=")
            .and_then(|value| value.parse::<u64>().ok())
    });

    updated_at
        .map(|timestamp| current_epoch_seconds().saturating_sub(timestamp) <= STATUS_FRESHNESS_SECS)
        .unwrap_or(false)
}

fn parse_command(args: &[String]) -> Result<Command, String> {
    match args.first().map(String::as_str) {
        Some("--probe") => Ok(Command::Probe),
        Some("--status") => Ok(Command::Status),
        Some("--daemon") | None => Ok(Command::Daemon),
        Some(other) => Err(format!("unknown argument: {other}")),
    }
}

fn execute(command: Command, config: &TransportConfig) -> CommandOutcome {
    let effective_config = match command {
        Command::Probe | Command::Status => config.refreshed(),
        Command::Daemon => config.clone(),
    };

    match command {
        Command::Probe => CommandOutcome::Print(format_lines(&effective_config.probe_lines())),
        Command::Status => {
            CommandOutcome::Print(format_lines(&effective_config.current_status_lines()))
        }
        Command::Daemon => CommandOutcome::RunDaemon,
    }
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let config = TransportConfig::from_env();

    let command = match parse_command(&args) {
        Ok(command) => command,
        Err(err) => {
            eprintln!("redbear-btusb: {err}");
            process::exit(1);
        }
    };

    match execute(command, &config) {
        CommandOutcome::Print(output) => {
            print!("{output}");
        }
        CommandOutcome::RunDaemon => {
            if let Err(err) = daemon_main(&config) {
                eprintln!("redbear-btusb: {err}");
                process::exit(1);
            }
        }
    }
}

fn parse_numeric_value(value: &str) -> Result<u64, String> {
    let trimmed = value.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).map_err(|err| format!("invalid hex value {trimmed:?}: {err}"))
    } else {
        trimmed
            .parse::<u64>()
            .map_err(|err| format!("invalid numeric value {trimmed:?}: {err}"))
    }
}

fn extract_jsonish_u64(raw: &str, keys: &[&str]) -> Result<u64, String> {
    for key in keys {
        let needle = format!("\"{key}\"");
        let Some(key_start) = raw.find(&needle) else {
            continue;
        };
        let after_key = &raw[key_start + needle.len()..];
        let Some(colon_index) = after_key.find(':') else {
            continue;
        };

        let after_colon = after_key[colon_index + 1..].trim_start();
        if after_colon.is_empty() {
            continue;
        }

        let token = if let Some(quoted) = after_colon.strip_prefix('"') {
            let Some(end_quote) = quoted.find('"') else {
                continue;
            };
            &quoted[..end_quote]
        } else {
            let end = after_colon
                .find(|ch: char| matches!(ch, ',' | '}' | '\n' | '\r'))
                .unwrap_or(after_colon.len());
            after_colon[..end].trim()
        };

        if token.is_empty() {
            continue;
        }

        return parse_numeric_value(token);
    }

    Err(format!(
        "missing descriptor field; expected one of {keys:?}"
    ))
}

fn parse_usb_device_descriptor(raw: &[u8]) -> Result<UsbDeviceDescriptor, String> {
    let text = String::from_utf8_lossy(raw);
    let vendor_id = extract_jsonish_u64(&text, &["vendor", "vendor_id"]).and_then(|value| {
        u16::try_from(value).map_err(|_| format!("vendor ID out of range: {value}"))
    })?;
    let device_id = extract_jsonish_u64(&text, &["product", "device", "product_id", "device_id"])
        .and_then(|value| {
        u16::try_from(value).map_err(|_| format!("device ID out of range: {value}"))
    })?;
    let class = extract_jsonish_u64(&text, &["class", "device_class"]).and_then(|value| {
        u8::try_from(value).map_err(|_| format!("USB class out of range: {value}"))
    })?;
    let subclass = extract_jsonish_u64(&text, &["sub_class", "subclass", "device_subclass"])
        .and_then(|value| {
            u8::try_from(value).map_err(|_| format!("USB subclass out of range: {value}"))
        })?;
    let protocol =
        extract_jsonish_u64(&text, &["protocol", "device_protocol"]).and_then(|value| {
            u8::try_from(value).map_err(|_| format!("USB protocol out of range: {value}"))
        })?;

    Ok(UsbDeviceDescriptor {
        vendor_id,
        device_id,
        class,
        subclass,
        protocol,
    })
}

fn try_collect_bluetooth_adapter(
    adapters: &mut Vec<UsbBluetoothAdapter>,
    bus: &str,
    device_path: &Path,
) -> Result<(), String> {
    let descriptor_path = device_path.join("descriptors");
    let raw = match fs::read(&descriptor_path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
        Err(_) => return Ok(()),
    };

    let descriptor = match parse_usb_device_descriptor(&raw) {
        Ok(descriptor) => descriptor,
        Err(_) => return Ok(()),
    };
    if descriptor.looks_like_bluetooth() {
        let endpoints = parse_hci_endpoints_from_descriptors(&raw).unwrap_or_default();
        adapters.push(UsbBluetoothAdapter {
            name: String::new(),
            vendor_id: descriptor.vendor_id,
            device_id: descriptor.device_id,
            bus: bus.to_string(),
            device_path: device_path.to_path_buf(),
            endpoints,
        });
    }

    Ok(())
}

fn probe_usb_bluetooth_adapters_in(root: &Path) -> Result<Vec<UsbBluetoothAdapter>, String> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(format!(
                "failed to read USB scheme root {}: {err}",
                root.display()
            ));
        }
    };

    let mut adapters = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };

        let bus_name = entry.file_name().to_string_lossy().into_owned();
        if bus_name.is_empty() {
            continue;
        }

        let bus_path = root.join(&bus_name);
        try_collect_bluetooth_adapter(&mut adapters, &bus_name, &bus_path)?;

        let nested_entries = match fs::read_dir(&bus_path) {
            Ok(nested_entries) => nested_entries,
            Err(err) if err.kind() == ErrorKind::NotFound => continue,
            Err(_) => continue,
        };

        for nested_entry in nested_entries {
            let nested_entry = match nested_entry {
                Ok(nested_entry) => nested_entry,
                Err(_) => continue,
            };

            let nested_name = nested_entry.file_name().to_string_lossy().into_owned();
            if nested_name.is_empty() {
                continue;
            }

            let device_path = bus_path.join(&nested_name);
            try_collect_bluetooth_adapter(&mut adapters, &bus_name, &device_path)?;
        }
    }

    adapters.sort_by(|left, right| {
        left.bus
            .cmp(&right.bus)
            .then(left.device_path.cmp(&right.device_path))
            .then(left.vendor_id.cmp(&right.vendor_id))
            .then(left.device_id.cmp(&right.device_id))
    });

    for (index, adapter) in adapters.iter_mut().enumerate() {
        adapter.name = format!("hci{index}");
    }

    Ok(adapters)
}

#[cfg(target_os = "redox")]
fn probe_usb_bluetooth_adapters() -> Result<Vec<UsbBluetoothAdapter>, String> {
    probe_usb_bluetooth_adapters_in(Path::new("/scheme/usb"))
}

#[cfg(not(target_os = "redox"))]
#[allow(dead_code)]
fn probe_usb_bluetooth_adapters() -> Result<Vec<UsbBluetoothAdapter>, String> {
    probe_usb_bluetooth_adapters_in(Path::new("/scheme/usb"))
}

fn hci_init_sequence(transport: &mut dyn UsbHciTransport) -> Result<ControllerInfo, String> {
    let mut info = ControllerInfo::default();
    info.state = ControllerState::Initializing;

    let reset_cmd = cmd_reset();
    transport
        .send_command(&reset_cmd)
        .map_err(|err| format!("HCI Reset send failed: {err}"))?;

    let event = transport
        .recv_event()
        .map_err(|err| format!("HCI Reset response failed: {err}"))?;

    let Some(event) = event else {
        return Err("HCI Reset: no response from controller".to_string());
    };

    if !event.is_command_complete() {
        return Err(format!(
            "HCI Reset: unexpected event code 0x{:02X}",
            event.event_code
        ));
    }

    let addr_cmd = cmd_read_bd_addr();
    transport
        .send_command(&addr_cmd)
        .map_err(|err| format!("HCI Read BD Addr send failed: {err}"))?;

    if let Some(event) =
        transport
            .recv_event()
            .map_err(|err| format!("HCI Read BD Addr response: {err}"))?
    {
        if let Some(result) = parse_read_bd_addr(&event) {
            if result.status == 0x00 {
                info.bd_address = Some(result.address);
            }
        }
    }

    let version_cmd = cmd_read_local_version();
    transport
        .send_command(&version_cmd)
        .map_err(|err| format!("HCI Read Local Version send failed: {err}"))?;

    if let Some(event) = transport
        .recv_event()
        .map_err(|err| format!("HCI Read Local Version response: {err}"))?
    {
        if let Some(result) = parse_local_version(&event) {
            if result.status == 0x00 {
                info.hci_version = Some(result.hci_version);
                info.hci_revision = Some(result.hci_revision);
                info.manufacturer_name = Some(result.manufacturer_name);
            }
        }
    }

    info.state = ControllerState::Active;
    Ok(info)
}

#[cfg(not(target_os = "redox"))]
fn daemon_main(_config: &TransportConfig) -> Result<(), String> {
    Err("daemon mode is only supported on Redox; use --probe or --status on host".to_string())
}

#[cfg(target_os = "redox")]
fn daemon_main(config: &TransportConfig) -> Result<(), String> {
    struct StatusFileGuard<'a> {
        path: &'a Path,
    }

    impl Drop for StatusFileGuard<'_> {
        fn drop(&mut self) {
            let _ = fs::remove_file(self.path);
        }
    }

    let mut runtime_config = config.refreshed();

    for adapter in &runtime_config.adapters {
        let transport_config = UsbTransportConfig {
            device_path: adapter.device_path.clone(),
            vendor_id: adapter.vendor_id,
            device_id: adapter.device_id,
            interrupt_endpoint: adapter.endpoints.event_endpoint,
            bulk_in_endpoint: adapter.endpoints.acl_in_endpoint,
            bulk_out_endpoint: adapter.endpoints.acl_out_endpoint,
        };

        let mut transport = StubTransport::new(transport_config);

        match hci_init_sequence(&mut transport) {
            Ok(info) => {
                runtime_config.controller_info = info;
            }
            Err(err) => {
                runtime_config.controller_info.state = ControllerState::Error;
                runtime_config.controller_info.init_error = Some(err);
            }
        }
        break;
    }

    runtime_config.write_status_file()?;
    let _status_file_guard = StatusFileGuard {
        path: &config.status_file,
    };

    loop {
        thread::sleep(Duration::from_secs(30));
        let controller_info = runtime_config.controller_info.clone();
        runtime_config = config.refreshed();
        runtime_config.controller_info = controller_info;
        runtime_config.write_status_file()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("{name}-{stamp}"))
    }

    fn stub_adapter(name: &str) -> UsbBluetoothAdapter {
        UsbBluetoothAdapter::stub(name.to_string())
    }

    fn test_config(status_file: PathBuf) -> TransportConfig {
        TransportConfig {
            adapters: vec![stub_adapter("hci0")],
            controller_family: "usb-bounded-test".to_string(),
            status_file,
            controller_info: ControllerInfo::default(),
        }
    }

    fn write_descriptor(path: &Path, body: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, body).unwrap();
    }

    #[test]
    fn probe_contract_is_bounded_and_usb_scoped() {
        let output = execute(Command::Probe, &test_config(temp_path("rbos-btusb-status")));
        let CommandOutcome::Print(output) = output else {
            panic!("expected printable output");
        };
        assert!(output.contains("adapters=hci0"));
        assert!(output.contains("transport=usb"));
        assert!(output.contains("startup=explicit"));
        assert!(output.contains("mode=ble-first"));
    }

    #[test]
    fn status_defaults_to_installed_only_without_runtime_file() {
        let status_file = temp_path("rbos-btusb-status-missing");
        let output = execute(Command::Status, &test_config(status_file));
        let CommandOutcome::Print(output) = output else {
            panic!("expected printable output");
        };
        assert!(output.contains("runtime_visibility=installed-only"));
        assert!(output.contains("daemon_status=inactive"));
    }

    #[test]
    fn status_uses_runtime_file_when_present() {
        let status_file = temp_path("rbos-btusb-status-present");
        let config = test_config(status_file.clone());
        config.write_status_file().unwrap();

        let output = execute(Command::Status, &config);
        let CommandOutcome::Print(output) = output else {
            panic!("expected printable output");
        };
        assert!(output.contains("runtime_visibility=runtime-visible"));
        assert!(output.contains("daemon_status=running"));

        fs::remove_file(status_file).unwrap();
    }

    #[test]
    fn stale_status_file_is_treated_as_installed_only() {
        let status_file = temp_path("rbos-btusb-status-stale");
        fs::write(
            &status_file,
            "adapters=hci0\ntransport=usb\nstartup=explicit\nmode=ble-first\nupdated_at_epoch=1\nruntime_visibility=runtime-visible\ndaemon_status=running\n",
        )
        .unwrap();

        let output = execute(Command::Status, &test_config(status_file.clone()));
        let CommandOutcome::Print(output) = output else {
            panic!("expected printable output");
        };
        assert!(output.contains("runtime_visibility=installed-only"));
        assert!(output.contains("daemon_status=inactive"));

        fs::remove_file(status_file).unwrap();
    }

    #[test]
    fn parse_command_accepts_probe_status_and_daemon() {
        assert_eq!(
            parse_command(&["--probe".to_string()]).unwrap(),
            Command::Probe
        );
        assert_eq!(
            parse_command(&["--status".to_string()]).unwrap(),
            Command::Status
        );
        assert_eq!(parse_command(&[]).unwrap(), Command::Daemon);
    }

    #[test]
    fn probe_usb_bluetooth_adapters_filters_and_enumerates_devices() {
        let root = temp_path("rbos-btusb-usb-root");
        write_descriptor(
            &root
                .join("usb.0000:00:14.0")
                .join("port1")
                .join("descriptors"),
            r#"{"class":224,"sub_class":1,"protocol":1,"vendor":32903,"product":50}"#,
        );
        write_descriptor(
            &root
                .join("usb.0000:00:14.0")
                .join("port2")
                .join("descriptors"),
            r#"{"class":3,"sub_class":1,"protocol":1,"vendor":4660,"product":22136}"#,
        );
        write_descriptor(
            &root
                .join("usb.0000:00:15.0")
                .join("port3")
                .join("descriptors"),
            r#"{"class":224,"sub_class":1,"protocol":1,"vendor":3034,"product":4660}"#,
        );

        let adapters = probe_usb_bluetooth_adapters_in(&root).unwrap();
        assert_eq!(adapters.len(), 2);
        assert_eq!(adapters[0].name, "hci0");
        assert_eq!(adapters[0].vendor_id, 0x8087u16);
        assert_eq!(adapters[0].device_id, 0x0032u16);
        assert_eq!(adapters[0].bus, "usb.0000:00:14.0");
        assert!(adapters[0]
            .device_path
            .ends_with(Path::new("usb.0000:00:14.0/port1")));
        assert_eq!(adapters[1].name, "hci1");
        assert_eq!(adapters[1].vendor_id, 0x0bdau16);
        assert_eq!(adapters[1].device_id, 0x1234u16);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn probe_usb_bluetooth_adapters_accepts_known_vendor_fallback() {
        let root = temp_path("rbos-btusb-known-vendor");
        write_descriptor(
            &root
                .join("usb.0000:00:16.0")
                .join("port7")
                .join("descriptors"),
            r#"{"class":255,"sub_class":255,"protocol":255,"vendor":2652,"product":4660}"#,
        );

        let adapters = probe_usb_bluetooth_adapters_in(&root).unwrap();
        assert_eq!(adapters.len(), 1);
        assert_eq!(adapters[0].name, "hci0");
        assert_eq!(adapters[0].vendor_id, 0x0a5cu16);
        assert_eq!(adapters[0].device_id, 0x1234u16);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn probe_usb_bluetooth_adapters_handles_missing_usb_tree() {
        let root = temp_path("rbos-btusb-missing-root");
        let adapters = probe_usb_bluetooth_adapters_in(&root).unwrap();
        assert!(adapters.is_empty());
    }

    #[test]
    fn parse_usb_endpoint_descriptor_valid() {
        let raw: &[u8] = &[7, 5, 0x81, 0x03, 0x40, 0x00, 0x01];
        let ep = parse_usb_endpoint_descriptor(raw).unwrap();
        assert_eq!(ep.endpoint_address, 0x81);
        assert_eq!(ep.attributes, 0x03);
        assert_eq!(ep.max_packet_size, 64);
        assert_eq!(ep.interval, 1);
    }

    #[test]
    fn parse_usb_endpoint_descriptor_too_short() {
        let raw: &[u8] = &[7, 5, 0x81, 0x03, 0x40];
        assert!(parse_usb_endpoint_descriptor(raw).is_none());
    }

    #[test]
    fn parse_usb_endpoint_descriptor_wrong_length() {
        let raw: &[u8] = &[9, 5, 0x81, 0x03, 0x40, 0x00, 0x01];
        assert!(parse_usb_endpoint_descriptor(raw).is_none());
    }

    #[test]
    fn parse_usb_endpoint_descriptor_wrong_type() {
        let raw: &[u8] = &[7, 4, 0x81, 0x03, 0x40, 0x00, 0x01];
        assert!(parse_usb_endpoint_descriptor(raw).is_none());
    }

    #[test]
    fn parse_hci_endpoints_from_descriptors_extracts_all_three() {
        let blob: Vec<u8> = vec![
            // Interface descriptor (9 bytes)
            9, 4, 0, 0, 3, 0xE0, 0x01, 0x01, 0x00,
            // Interrupt IN endpoint: address=0x81 (EP1 IN), attributes=0x03 (interrupt), max_packet=64
            7, 5, 0x81, 0x03, 0x40, 0x00, 0x01,
            // Bulk IN endpoint: address=0x82 (EP2 IN), attributes=0x02 (bulk), max_packet=512
            7, 5, 0x82, 0x02, 0x00, 0x02, 0x00,
            // Bulk OUT endpoint: address=0x02 (EP2 OUT), attributes=0x02 (bulk), max_packet=512
            7, 5, 0x02, 0x02, 0x00, 0x02, 0x00,
        ];

        let endpoints = parse_hci_endpoints_from_descriptors(&blob).unwrap();
        assert_eq!(endpoints.event_endpoint, 1);
        assert_eq!(endpoints.event_max_packet_size, 64);
        assert_eq!(endpoints.acl_in_endpoint, 2);
        assert_eq!(endpoints.acl_in_max_packet_size, 512);
        assert_eq!(endpoints.acl_out_endpoint, 2);
        assert_eq!(endpoints.acl_out_max_packet_size, 512);
    }

    #[test]
    fn parse_hci_endpoints_from_descriptors_missing_interrupt_in() {
        let blob: Vec<u8> = vec![
            // Bulk IN only, no interrupt IN
            7, 5, 0x82, 0x02, 0x00, 0x02, 0x00,
            7, 5, 0x02, 0x02, 0x00, 0x02, 0x00,
        ];
        let result = parse_hci_endpoints_from_descriptors(&blob);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("interrupt IN"));
    }

    #[test]
    fn parse_hci_endpoints_from_descriptors_empty_blob() {
        let result = parse_hci_endpoints_from_descriptors(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_hci_endpoints_from_descriptors_ignores_other_endpoints() {
        let blob: Vec<u8> = vec![
            // Isochronous IN endpoint (should be ignored)
            7, 5, 0x83, 0x01, 0x00, 0x01, 0x01,
            // Control endpoint (should be ignored)
            7, 5, 0x04, 0x00, 0x40, 0x00, 0x00,
            // Interrupt IN endpoint (should be picked up)
            7, 5, 0x81, 0x03, 0x40, 0x00, 0x01,
            // Bulk IN endpoint (should be picked up)
            7, 5, 0x82, 0x02, 0x00, 0x02, 0x00,
            // Bulk OUT endpoint (should be picked up)
            7, 5, 0x02, 0x02, 0x00, 0x02, 0x00,
        ];

        let endpoints = parse_hci_endpoints_from_descriptors(&blob).unwrap();
        assert_eq!(endpoints.event_endpoint, 1);
        assert_eq!(endpoints.acl_in_endpoint, 2);
        assert_eq!(endpoints.acl_out_endpoint, 2);
    }

    #[test]
    fn hci_endpoints_default_is_zeroed() {
        let ep = HciEndpoints::default();
        assert_eq!(ep.event_endpoint, 0);
        assert_eq!(ep.acl_in_endpoint, 0);
        assert_eq!(ep.acl_out_endpoint, 0);
        assert_eq!(ep.event_max_packet_size, 0);
        assert_eq!(ep.acl_in_max_packet_size, 0);
        assert_eq!(ep.acl_out_max_packet_size, 0);
    }

    #[test]
    fn detail_line_includes_endpoint_fields() {
        let adapter = UsbBluetoothAdapter {
            name: "hci0".to_string(),
            vendor_id: 0x8087,
            device_id: 0x0032,
            bus: "usb.0000:00:14.0".to_string(),
            device_path: PathBuf::from("/scheme/usb/usb.0000:00:14.0/port1"),
            endpoints: HciEndpoints {
                event_endpoint: 1,
                acl_in_endpoint: 2,
                acl_out_endpoint: 2,
                event_max_packet_size: 64,
                acl_in_max_packet_size: 512,
                acl_out_max_packet_size: 512,
            },
        };
        let line = adapter.detail_line(0);
        assert!(line.contains("event_ep=1"));
        assert!(line.contains("acl_in_ep=2"));
        assert!(line.contains("acl_out_ep=2"));
        assert!(line.contains("vendor_id=8087"));
        assert!(line.contains("device_id=0032"));
    }

    // -- Controller state and HCI init tests --------------------------------

    #[test]
    fn controller_state_default_is_closed() {
        let info = ControllerInfo::default();
        assert_eq!(info.state, ControllerState::Closed);
        assert!(info.bd_address.is_none());
        assert!(info.hci_version.is_none());
        assert!(info.hci_revision.is_none());
        assert!(info.manufacturer_name.is_none());
        assert!(info.init_error.is_none());
    }

    struct TestTransport {
        pending_events: Vec<hci::HciEvent>,
    }

    impl TestTransport {
        fn new() -> Self {
            Self {
                pending_events: Vec::new(),
            }
        }

        fn inject_event(&mut self, event: hci::HciEvent) {
            self.pending_events.push(event);
        }
    }

    impl usb_transport::UsbHciTransport for TestTransport {
        fn send_command(&mut self, _command: &hci::HciCommand) -> std::io::Result<()> {
            Ok(())
        }
        fn recv_event(&mut self) -> std::io::Result<Option<hci::HciEvent>> {
            Ok(if self.pending_events.is_empty() {
                None
            } else {
                Some(self.pending_events.remove(0))
            })
        }
        fn send_acl(&mut self, _acl: &hci::HciAcl) -> std::io::Result<()> {
            Ok(())
        }
        fn recv_acl(&mut self) -> std::io::Result<Option<hci::HciAcl>> {
            Ok(None)
        }
        fn state(&self) -> usb_transport::TransportState {
            usb_transport::TransportState::Active
        }
        fn close(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn inject_cc_event(transport: &mut TestTransport, opcode: u16, return_params: Vec<u8>) {
        let mut params = vec![0x01];
        params.push(opcode as u8);
        params.push((opcode >> 8) as u8);
        params.extend(return_params);
        let event = hci::HciEvent {
            event_code: hci::EVT_COMMAND_COMPLETE,
            parameters: params,
        };
        transport.inject_event(event);
    }

    #[test]
    fn hci_init_sequence_with_stub_succeeds() {
        let mut transport = TestTransport::new();

        // Reset CC: status=0x00
        inject_cc_event(&mut transport, hci::OP_RESET, vec![0x00]);

        // Read BD Addr CC: status=0x00 + 6-byte address
        inject_cc_event(
            &mut transport,
            hci::OP_READ_BD_ADDR,
            vec![0x00, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF],
        );

        // Read Local Version CC: status + hci_version + hci_revision(2) + lmp_version + manufacturer(2) + lmp_subversion(2)
        inject_cc_event(
            &mut transport,
            hci::OP_READ_LOCAL_VERSION,
            vec![0x00, 0x09, 0x01, 0x00, 0x09, 0x02, 0x00, 0x01, 0x00],
        );

        let info = hci_init_sequence(&mut transport).expect("init should succeed");
        assert_eq!(info.state, ControllerState::Active);
        assert_eq!(info.bd_address, Some([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]));
        assert_eq!(info.hci_version, Some(0x09));
        assert_eq!(info.hci_revision, Some(0x0001));
        assert_eq!(info.manufacturer_name, Some(0x0002));
        assert!(info.init_error.is_none());
    }

    #[test]
    fn hci_init_sequence_fails_when_reset_gets_no_response() {
        let mut transport = TestTransport::new();
        // No events injected — recv_event returns None
        let result = hci_init_sequence(&mut transport);
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(
            err.contains("no response"),
            "expected 'no response' error, got: {err}"
        );
    }

    #[test]
    fn status_lines_include_controller_state() {
        let status_file = temp_path("rbos-btusb-status-active");
        let mut config = test_config(status_file);
        config.controller_info = ControllerInfo {
            state: ControllerState::Active,
            bd_address: Some([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]),
            hci_version: Some(9),
            hci_revision: Some(1),
            manufacturer_name: Some(2),
            init_error: None,
        };
        let lines = config.render_status_lines(true);
        let output = lines.join("\n");
        assert!(
            output.contains("controller_state=active"),
            "missing controller_state=active, got: {output}"
        );
        assert!(
            output.contains("bd_address="),
            "missing bd_address, got: {output}"
        );
        assert!(
            output.contains("hci_version=9"),
            "missing hci_version, got: {output}"
        );
        assert!(
            output.contains("manufacturer=2"),
            "missing manufacturer, got: {output}"
        );
    }

    #[test]
    fn status_lines_include_init_error() {
        let status_file = temp_path("rbos-btusb-status-error");
        let mut config = test_config(status_file);
        config.controller_info = ControllerInfo {
            state: ControllerState::Error,
            bd_address: None,
            hci_version: None,
            hci_revision: None,
            manufacturer_name: None,
            init_error: Some("HCI Reset send failed: transport is closed".to_string()),
        };
        let lines = config.render_status_lines(true);
        let output = lines.join("\n");
        assert!(
            output.contains("controller_state=error"),
            "missing controller_state=error, got: {output}"
        );
        assert!(
            output.contains("init_error=HCI Reset send failed"),
            "missing init_error, got: {output}"
        );
    }
}
