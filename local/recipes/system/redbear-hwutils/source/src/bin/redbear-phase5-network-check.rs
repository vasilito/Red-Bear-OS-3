use std::path::Path;
use std::process::{self, Command};

use redbear_hwutils::parse_args;
use serde_json::Value;

const PROGRAM: &str = "redbear-phase5-network-check";
const USAGE: &str = "Usage: redbear-phase5-network-check\n\nShow the installed Phase 5 networking/session plumbing surface inside the guest.";

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

fn run() -> Result<(), String> {
    parse_args(PROGRAM, USAGE, std::env::args()).map_err(|err| {
        if err.is_empty() {
            process::exit(0);
        }
        err
    })?;

    println!("=== Red Bear OS Phase 5 Networking Check ===");
    require_path("/usr/bin/dbus-daemon")?;
    require_path("/usr/bin/netctl")?;
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

    let _ = Command::new("netctl").arg("status").status();

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
    if wifi_check_stdout.contains("PASS: bounded Intel Wi-Fi runtime path exercised on target") {
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

    if Path::new("/run/dbus/system_bus_socket").exists() {
        println!("DBUS_SYSTEM_BUS=present");
    } else {
        println!("DBUS_SYSTEM_BUS=missing");
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
}
