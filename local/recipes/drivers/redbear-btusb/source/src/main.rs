use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process;
#[cfg(target_os = "redox")]
use std::thread;
#[cfg(target_os = "redox")]
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

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
        }
    }

    fn detail_line(&self, index: usize) -> String {
        format!(
            "adapter_{index}=name={};vendor_id={:04x};device_id={:04x};bus={};device_path={}",
            self.name,
            self.vendor_id,
            self.device_id,
            self.bus,
            self.device_path.display()
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct TransportConfig {
    adapters: Vec<UsbBluetoothAdapter>,
    controller_family: String,
    status_file: PathBuf,
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
        adapters.push(UsbBluetoothAdapter {
            name: String::new(),
            vendor_id: descriptor.vendor_id,
            device_id: descriptor.device_id,
            bus: bus.to_string(),
            device_path: device_path.to_path_buf(),
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
    runtime_config.write_status_file()?;
    let _status_file_guard = StatusFileGuard {
        path: &config.status_file,
    };

    loop {
        thread::sleep(Duration::from_secs(30));
        runtime_config = config.refreshed();
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
}
