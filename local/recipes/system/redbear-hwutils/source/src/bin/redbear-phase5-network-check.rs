use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::{self, Command},
};

use redbear_hwutils::parse_args;
use serde_json::Value;

const PROGRAM: &str = "redbear-phase5-network-check";
const USAGE: &str = "Usage: redbear-phase5-network-check\n\nShow the installed Phase 5 networking/session plumbing surface inside the guest.";

const DBUS_SEND: &str = "dbus-send";
const POWER_ROOT: &str = "/scheme/acpi/power";
const UPOWER_DESTINATION: &str = "org.freedesktop.UPower";
const UPOWER_PATH: &str = "/org/freedesktop/UPower";
const UDISKS_DESTINATION: &str = "org.freedesktop.UDisks2";
const UDISKS_ROOT_PATH: &str = "/org/freedesktop/UDisks2";
const UDISKS_MANAGER_PATH: &str = "/org/freedesktop/UDisks2/Manager";
const UDISKS_BLOCK_PREFIX: &str = "/org/freedesktop/UDisks2/block_devices/";
const UDISKS_DRIVE_PREFIX: &str = "/org/freedesktop/UDisks2/drives/";

#[derive(Debug, Default)]
struct PowerRuntime {
    adapter_ids: Vec<String>,
    battery_ids: Vec<String>,
    native_paths_by_object: BTreeMap<String, String>,
}

#[derive(Debug, Default)]
struct DiskRuntime {
    block_paths: BTreeSet<String>,
    drive_paths: BTreeSet<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct RootKey {
    disk_number: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct PartitionKey {
    disk_number: u32,
    partition_number: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EntryKind {
    Root(RootKey),
    Partition(PartitionKey),
}

fn require_path(path: &str) -> Result<(), String> {
    if Path::new(path).exists() {
        println!("{path}");
        Ok(())
    } else {
        Err(format!("missing {path}"))
    }
}

fn require_json_field(value: &Value, field: &str) -> Result<(), String> {
    if value.get(field).is_some() {
        println!("{field}=present");
        Ok(())
    } else {
        Err(format!("redbear-info --json did not report {field}"))
    }
}

fn optional_require_path(path: &str, label: &str) {
    if Path::new(path).exists() {
        println!("{label}=present");
    }
}

fn require_one_path<'a>(paths: &'a [&'a str], label: &str) -> Result<&'a str, String> {
    for path in paths {
        if Path::new(path).exists() {
            println!("{label}={path}");
            return Ok(*path);
        }
    }

    Err(format!("missing any of: {}", paths.join(", ")))
}

fn list_dir_names(path: impl AsRef<Path>) -> Vec<String> {
    let mut names = match fs::read_dir(path) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect::<Vec<_>>(),
        Err(_) => Vec::new(),
    };
    names.sort();
    names
}

fn read_trimmed(path: impl AsRef<Path>) -> Option<String> {
    let value = fs::read_to_string(path).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn run_command_with_retry(
    program: &str,
    args: &[&str],
    label: &str,
    max_attempts: u32,
    delay_secs: u64,
) -> Result<String, String> {
    let mut last_err = String::new();
    for attempt in 1..=max_attempts {
        match run_command(program, args, label) {
            Ok(output) => return Ok(output),
            Err(err) => {
                if attempt < max_attempts {
                    eprintln!(
                        "{label}: attempt {attempt}/{max_attempts} failed ({err}), retrying in {delay_secs}s..."
                    );
                    std::thread::sleep(std::time::Duration::from_secs(delay_secs));
                }
                last_err = err;
            }
        }
    }
    Err(format!(
        "{label} failed after {max_attempts} attempts: {last_err}"
    ))
}

fn run_command(program: &str, args: &[&str], label: &str) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|err| format!("failed to run {label}: {err}"))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = if !stderr.trim().is_empty() {
            stderr.trim().to_string()
        } else if !stdout.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            String::from("no output")
        };
        return Err(format!(
            "{label} exited with status {}: {detail}",
            output.status
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn quoted_values_with_prefix(output: &str, prefix: &str) -> BTreeSet<String> {
    let mut values = BTreeSet::new();

    for line in output.lines() {
        let mut remainder = line;
        while let Some(start) = remainder.find('"') {
            remainder = &remainder[start + 1..];
            let Some(end) = remainder.find('"') else {
                break;
            };
            let candidate = &remainder[..end];
            if candidate.starts_with(prefix) {
                values.insert(candidate.to_string());
            }
            remainder = &remainder[end + 1..];
        }
    }

    values
}

fn quoted_value(line: &str) -> Option<&str> {
    let start = line.find('"')?;
    let remainder = &line[start + 1..];
    let end = remainder.find('"')?;
    Some(&remainder[..end])
}

fn managed_object_keys_with_prefix(output: &str, prefix: &str) -> BTreeSet<String> {
    let lines = output.lines().collect::<Vec<_>>();
    let mut values = BTreeSet::new();

    for window in lines.windows(2) {
        let current = window[0].trim();
        let next = window[1].trim();
        if current != "dict entry(" || !next.starts_with("object path ") {
            continue;
        }

        let Some(candidate) = quoted_value(next) else {
            continue;
        };
        if candidate.starts_with(prefix) {
            values.insert(candidate.to_string());
        }
    }

    values
}

fn summarize_set(values: &BTreeSet<String>) -> String {
    if values.is_empty() {
        String::from("none")
    } else {
        values.iter().cloned().collect::<Vec<_>>().join(", ")
    }
}

fn note_bus_name_registered(list_names_output: &str, bus_name: &str, label: &str) {
    if list_names_output.contains(bus_name) {
        println!("{label}=present");
    } else {
        println!("{label}=activated_lazily");
    }
}

fn contains_phase5_wifi_success(text: &str) -> bool {
    text.contains("PASS: bounded Intel Wi-Fi runtime path exercised inside target runtime")
        || text.contains("PASS: bounded Intel Wi-Fi runtime path exercised on target")
}

fn adapter_object_path(id: &str) -> String {
    format!("/org/freedesktop/UPower/devices/line_power_{id}")
}

fn battery_object_path(id: &str) -> String {
    format!("/org/freedesktop/UPower/devices/battery_{id}")
}

impl PowerRuntime {
    fn discover() -> Self {
        let adapter_ids = list_dir_names(PathBuf::from(POWER_ROOT).join("adapters"));
        let battery_ids = list_dir_names(PathBuf::from(POWER_ROOT).join("batteries"));
        let mut native_paths_by_object = BTreeMap::new();

        for adapter_id in &adapter_ids {
            if let Some(native_path) = read_trimmed(
                PathBuf::from(POWER_ROOT)
                    .join("adapters")
                    .join(adapter_id)
                    .join("path"),
            ) {
                native_paths_by_object.insert(adapter_object_path(adapter_id), native_path);
            }
        }

        for battery_id in &battery_ids {
            if let Some(native_path) = read_trimmed(
                PathBuf::from(POWER_ROOT)
                    .join("batteries")
                    .join(battery_id)
                    .join("path"),
            ) {
                native_paths_by_object.insert(battery_object_path(battery_id), native_path);
            }
        }

        Self {
            adapter_ids,
            battery_ids,
            native_paths_by_object,
        }
    }

    fn expected_device_paths(&self) -> BTreeSet<String> {
        let mut paths = BTreeSet::new();
        for adapter_id in &self.adapter_ids {
            paths.insert(adapter_object_path(adapter_id));
        }
        for battery_id in &self.battery_ids {
            paths.insert(battery_object_path(battery_id));
        }
        paths
    }
}

fn validate_upower(list_names_output: &str) -> Result<(), String> {
    let runtime = PowerRuntime::discover();
    let expected_device_paths = runtime.expected_device_paths();
    println!("UPOWER_RUNTIME_ADAPTERS={}", runtime.adapter_ids.len());
    println!("UPOWER_RUNTIME_BATTERIES={}", runtime.battery_ids.len());

    let enumerate_output = run_command_with_retry(
        DBUS_SEND,
        &[
            "--system",
            "--dest=org.freedesktop.UPower",
            "--type=method_call",
            "--print-reply",
            UPOWER_PATH,
            "org.freedesktop.UPower.EnumerateDevices",
        ],
        "dbus-send UPower EnumerateDevices",
        3,
        2,
    )?;
    let enumerated_device_paths =
        quoted_values_with_prefix(&enumerate_output, "/org/freedesktop/UPower/devices/");
    println!(
        "UPOWER_ENUMERATED_DEVICES={}",
        enumerated_device_paths.len()
    );
    note_bus_name_registered(list_names_output, UPOWER_DESTINATION, "UPOWER_BUS_NAME");

    let missing_device_paths = expected_device_paths
        .difference(&enumerated_device_paths)
        .cloned()
        .collect::<BTreeSet<_>>();
    if !missing_device_paths.is_empty() {
        return Err(format!(
            "UPower did not enumerate runtime-backed devices: {}",
            summarize_set(&missing_device_paths)
        ));
    }

    let unexpected_device_paths = enumerated_device_paths
        .difference(&expected_device_paths)
        .cloned()
        .collect::<BTreeSet<_>>();
    if !unexpected_device_paths.is_empty() {
        return Err(format!(
            "UPower enumerated devices not backed by /scheme/acpi/power: {}",
            summarize_set(&unexpected_device_paths)
        ));
    }

    for (object_path, expected_native_path) in &runtime.native_paths_by_object {
        let native_path_output = run_command(
            DBUS_SEND,
            &[
                "--system",
                "--dest=org.freedesktop.UPower",
                "--type=method_call",
                "--print-reply",
                object_path.as_str(),
                "org.freedesktop.DBus.Properties.Get",
                "string:org.freedesktop.UPower.Device",
                "string:NativePath",
            ],
            "dbus-send UPower Device.NativePath",
        )?;
        let reported_paths = quoted_values_with_prefix(&native_path_output, "/");
        let reported_native_path = reported_paths.iter().next().ok_or_else(|| {
            format!("UPower device {object_path} did not return a NativePath property value")
        })?;

        if reported_native_path != expected_native_path {
            return Err(format!(
                "UPower device {object_path} reported NativePath {reported_native_path}, expected {expected_native_path}"
            ));
        }
    }

    println!("UPOWER_NATIVE_PATHS=validated");
    Ok(())
}

impl DiskRuntime {
    fn discover() -> Self {
        let mut block_paths = BTreeSet::new();
        let mut drive_paths = BTreeSet::new();

        for scheme_name in list_dir_names("/scheme")
            .into_iter()
            .filter(|name| name.starts_with("disk."))
        {
            let scheme_path = PathBuf::from("/scheme").join(&scheme_name);

            for entry_name in list_dir_names(&scheme_path) {
                match parse_disk_entry_name(&entry_name) {
                    Some(EntryKind::Root(_)) => {
                        drive_paths.insert(drive_object_path(&scheme_name, &entry_name));
                        block_paths.insert(block_object_path(&scheme_name, &entry_name));
                    }
                    Some(EntryKind::Partition(_)) => {
                        block_paths.insert(block_object_path(&scheme_name, &entry_name));
                    }
                    None => {}
                }
            }
        }

        Self {
            block_paths,
            drive_paths,
        }
    }
}

fn parse_disk_entry_name(entry_name: &str) -> Option<EntryKind> {
    if let Some(position) = entry_name.find('p') {
        let disk_number = entry_name[..position].parse().ok()?;
        let partition_number = entry_name[position + 1..].parse().ok()?;
        return Some(EntryKind::Partition(PartitionKey {
            disk_number,
            partition_number,
        }));
    }

    Some(EntryKind::Root(RootKey {
        disk_number: entry_name.parse().ok()?,
    }))
}

fn block_object_path(scheme_name: &str, entry_name: &str) -> String {
    format!(
        "{UDISKS_BLOCK_PREFIX}{}",
        stable_object_name(scheme_name, entry_name)
    )
}

fn drive_object_path(scheme_name: &str, entry_name: &str) -> String {
    format!(
        "{UDISKS_DRIVE_PREFIX}{}",
        stable_object_name(scheme_name, entry_name)
    )
}

fn stable_object_name(scheme_name: &str, entry_name: &str) -> String {
    format!(
        "{}_{}",
        encode_path_component(scheme_name),
        encode_path_component(entry_name)
    )
}

fn encode_path_component(component: &str) -> String {
    let mut encoded = String::new();

    for byte in component.bytes() {
        if byte.is_ascii_alphanumeric() {
            encoded.push(byte as char);
        } else {
            encoded.push('_');
            encoded.push(hex_char(byte >> 4));
            encoded.push(hex_char(byte & 0x0f));
        }
    }

    if encoded.is_empty() {
        encoded.push('_');
        encoded.push('0');
        encoded.push('0');
    }

    encoded
}

fn hex_char(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'a' + (value - 10)) as char,
        _ => unreachable!("hex nibble out of range"),
    }
}

fn validate_udisks(list_names_output: &str) -> Result<(), String> {
    let runtime = DiskRuntime::discover();
    println!(
        "UDISKS_RUNTIME_DRIVE_SURFACES={}",
        runtime.drive_paths.len()
    );
    println!(
        "UDISKS_RUNTIME_BLOCK_SURFACES={}",
        runtime.block_paths.len()
    );

    let managed_objects_output = run_command(
        DBUS_SEND,
        &[
            "--system",
            "--dest=org.freedesktop.UDisks2",
            "--type=method_call",
            "--print-reply",
            UDISKS_ROOT_PATH,
            "org.freedesktop.DBus.ObjectManager.GetManagedObjects",
        ],
        "dbus-send UDisks2 GetManagedObjects",
    )?;

    let managed_object_paths =
        managed_object_keys_with_prefix(&managed_objects_output, "/org/freedesktop/UDisks2/");
    let managed_block_paths =
        managed_object_keys_with_prefix(&managed_objects_output, UDISKS_BLOCK_PREFIX);
    let managed_drive_paths =
        managed_object_keys_with_prefix(&managed_objects_output, UDISKS_DRIVE_PREFIX);
    let manager_present = managed_object_paths.contains(UDISKS_MANAGER_PATH);

    if !manager_present && (!runtime.block_paths.is_empty() || !runtime.drive_paths.is_empty()) {
        return Err(format!(
            "UDisks2 GetManagedObjects did not include manager object {UDISKS_MANAGER_PATH} while runtime disk surfaces were present"
        ));
    }

    println!("UDISKS_MANAGED_OBJECTS=present");
    note_bus_name_registered(list_names_output, UDISKS_DESTINATION, "UDISKS_BUS_NAME");

    let missing_block_paths = runtime
        .block_paths
        .difference(&managed_block_paths)
        .cloned()
        .collect::<BTreeSet<_>>();
    if !missing_block_paths.is_empty() {
        return Err(format!(
            "UDisks2 managed objects missed runtime-backed block devices: {}",
            summarize_set(&missing_block_paths)
        ));
    }

    let unexpected_block_paths = managed_block_paths
        .difference(&runtime.block_paths)
        .cloned()
        .collect::<BTreeSet<_>>();
    if !unexpected_block_paths.is_empty() {
        return Err(format!(
            "UDisks2 exposed block devices not backed by /scheme/disk.*: {}",
            summarize_set(&unexpected_block_paths)
        ));
    }

    let missing_drive_paths = runtime
        .drive_paths
        .difference(&managed_drive_paths)
        .cloned()
        .collect::<BTreeSet<_>>();
    if !missing_drive_paths.is_empty() {
        return Err(format!(
            "UDisks2 managed objects missed runtime-backed drives: {}",
            summarize_set(&missing_drive_paths)
        ));
    }

    let unexpected_drive_paths = managed_drive_paths
        .difference(&runtime.drive_paths)
        .cloned()
        .collect::<BTreeSet<_>>();
    if !unexpected_drive_paths.is_empty() {
        return Err(format!(
            "UDisks2 exposed drives not backed by /scheme/disk.*: {}",
            summarize_set(&unexpected_drive_paths)
        ));
    }

    println!("UDISKS_BLOCK_OBJECT_PATHS=validated");
    Ok(())
}

fn run() -> Result<(), String> {
    parse_args(PROGRAM, USAGE, std::env::args()).map_err(|err| {
        if err.is_empty() {
            process::exit(0);
        }
        err
    })?;

    println!("=== Red Bear OS Phase 5 Networking Check ===");
    require_path("/usr/bin/dbus-daemon")?;
    require_path("/usr/bin/dbus-send")?;
    let netctl_bin = require_one_path(
        &["/usr/bin/redbear-netctl", "/usr/bin/netctl"],
        "NETCTL_BIN",
    )?;
    require_path("/usr/bin/redbear-wifictl")?;

    let info_output = Command::new("redbear-info")
        .arg("--json")
        .output()
        .map_err(|err| format!("failed to run redbear-info --json: {err}"))?;
    if !info_output.status.success() {
        let stderr = String::from_utf8_lossy(&info_output.stderr);
        if stderr.trim().is_empty() {
            return Err(format!(
                "redbear-info exited with status {}",
                info_output.status
            ));
        }
        return Err(format!(
            "redbear-info exited with status {}: {}",
            info_output.status,
            stderr.trim()
        ));
    }

    let info_stdout = String::from_utf8_lossy(&info_output.stdout);
    let info_json: Value = serde_json::from_str(&info_stdout)
        .map_err(|err| format!("failed to parse redbear-info --json output: {err}"))?;
    if info_stdout.contains("virtio_net_present") {
        println!("virtio_net_present");
    } else {
        return Err("redbear-info --json did not report virtio_net_present".to_string());
    }

    let network = info_json
        .get("network")
        .ok_or_else(|| "redbear-info --json did not include network section".to_string())?;
    require_json_field(network, "wifi_control_state")?;
    require_json_field(network, "wifi_connect_result")?;
    require_json_field(network, "wifi_disconnect_result")?;

    let _ = Command::new(netctl_bin).arg("status").status();

    let wifictl_output = Command::new("redbear-wifictl")
        .arg("--probe")
        .output()
        .map_err(|err| format!("failed to run redbear-wifictl --probe: {err}"))?;
    if !wifictl_output.status.success() {
        return Err(format!(
            "redbear-wifictl --probe exited with status {}",
            wifictl_output.status
        ));
    }
    let wifictl_stdout = String::from_utf8_lossy(&wifictl_output.stdout);
    if wifictl_stdout.contains("interfaces=") {
        println!("WIFICTL_INTERFACES=present");
    } else {
        return Err("redbear-wifictl --probe did not report interfaces=".to_string());
    }
    if wifictl_stdout.contains("capabilities=") {
        println!("WIFICTL_CAPABILITIES=present");
    } else {
        return Err("redbear-wifictl --probe did not report capabilities=".to_string());
    }

    if Path::new("/run/dbus/system_bus_socket").exists() {
        println!("DBUS_SYSTEM_BUS=present");
    } else {
        println!("DBUS_SYSTEM_BUS=missing");
    }

    let list_names_output = run_command(
        DBUS_SEND,
        &[
            "--system",
            "--dest=org.freedesktop.DBus",
            "--type=method_call",
            "--print-reply",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus.ListNames",
        ],
        "dbus-send org.freedesktop.DBus.ListNames",
    )?;

    validate_upower(&list_names_output)?;
    validate_udisks(&list_names_output)?;

    let wifi_check_output = Command::new("redbear-phase5-wifi-check")
        .output()
        .map_err(|err| format!("failed to run redbear-phase5-wifi-check: {err}"))?;
    if !wifi_check_output.status.success() {
        let stderr = String::from_utf8_lossy(&wifi_check_output.stderr);
        if stderr.trim().is_empty() {
            return Err(format!(
                "redbear-phase5-wifi-check exited with status {}",
                wifi_check_output.status
            ));
        }
        return Err(format!(
            "redbear-phase5-wifi-check exited with status {}: {}",
            wifi_check_output.status,
            stderr.trim()
        ));
    }
    let wifi_check_stdout = String::from_utf8_lossy(&wifi_check_output.stdout);
    if contains_phase5_wifi_success(&wifi_check_stdout) {
        println!("PHASE5_WIFI_CHECK=pass");
    } else {
        return Err(
            "redbear-phase5-wifi-check did not report bounded Wi-Fi runtime success".to_string(),
        );
    }

    if let Ok(ifaces) = std::fs::read_dir("/scheme/wifictl/ifaces") {
        for entry in ifaces.flatten() {
            let iface = entry.file_name().to_string_lossy().into_owned();
            optional_require_path(
                &format!("/scheme/wifictl/ifaces/{iface}/connect-result"),
                "WIFICTL_CONNECT_RESULT_NODE",
            );
            optional_require_path(
                &format!("/scheme/wifictl/ifaces/{iface}/disconnect-result"),
                "WIFICTL_DISCONNECT_RESULT_NODE",
            );
        }
    }

    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{PROGRAM}: {err}");
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_json_field_accepts_present_key() {
        let value: Value = serde_json::json!({"wifi_connect_result": "unknown"});
        assert!(require_json_field(&value, "wifi_connect_result").is_ok());
    }

    #[test]
    fn require_json_field_rejects_missing_key() {
        let value: Value = serde_json::json!({"wifi_control_state": "present"});
        assert!(require_json_field(&value, "wifi_connect_result").is_err());
    }

    #[test]
    fn quoted_values_with_prefix_collects_expected_strings() {
        let output = r#"
            object path "/org/freedesktop/UPower/devices/line_power_AC"
            string "/scheme/acpi/power/adapters/AC"
            object path "/org/freedesktop/UDisks2/block_devices/disk_2e_nvme_0"
        "#;

        let upower_paths = quoted_values_with_prefix(output, "/org/freedesktop/UPower/devices/");
        assert!(upower_paths.contains("/org/freedesktop/UPower/devices/line_power_AC"));

        let disk_paths = quoted_values_with_prefix(output, UDISKS_BLOCK_PREFIX);
        assert!(disk_paths.contains("/org/freedesktop/UDisks2/block_devices/disk_2e_nvme_0"));
    }

    #[test]
    fn managed_object_keys_with_prefix_ignores_property_object_paths() {
        let output = r#"
            dict entry(
               object path "/org/freedesktop/UDisks2/block_devices/disk_2e_nvme_0"
               array [
                  dict entry(
                     string "org.freedesktop.UDisks2.Block"
                     array [
                        dict entry(
                           string "Drive"
                           variant                               object path "/org/freedesktop/UDisks2/drives/disk_2e_nvme_0"
                        )
                     ]
                  )
               ]
            )
        "#;

        let managed_blocks = managed_object_keys_with_prefix(output, UDISKS_BLOCK_PREFIX);
        assert_eq!(managed_blocks.len(), 1);
        assert!(managed_blocks.contains("/org/freedesktop/UDisks2/block_devices/disk_2e_nvme_0"));

        let managed_drives = managed_object_keys_with_prefix(output, UDISKS_DRIVE_PREFIX);
        assert!(managed_drives.is_empty());
    }

    #[test]
    fn stable_object_name_matches_udisks_inventory_encoding() {
        assert_eq!(stable_object_name("disk.nvme", "0p1"), "disk_2envme_0p1");
    }

    #[test]
    fn contains_phase5_wifi_success_accepts_current_and_legacy_markers() {
        assert!(contains_phase5_wifi_success(
            "PASS: bounded Intel Wi-Fi runtime path exercised inside target runtime"
        ));
        assert!(contains_phase5_wifi_success(
            "PASS: bounded Intel Wi-Fi runtime path exercised on target"
        ));
    }
}
