use std::fs;
use std::path::Path;
use std::process::{self, Command};
use std::time::{SystemTime, UNIX_EPOCH};

use redbear_hwutils::parse_args;
use serde_json::json;

const PROGRAM: &str = "redbear-phase5-wifi-capture";
const USAGE: &str = "Usage: redbear-phase5-wifi-capture [PROFILE] [INTERFACE] [OUTPUT_PATH]\n\nCapture the current bounded Intel Wi-Fi runtime surfaces into a single JSON bundle.";

fn run_command(program: &str, args: &[&str]) -> serde_json::Value {
    match Command::new(program).args(args).output() {
        Ok(output) => json!({
            "ok": output.status.success(),
            "status": output.status.code(),
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr),
        }),
        Err(err) => json!({
            "ok": false,
            "status": null,
            "stdout": "",
            "stderr": format!("failed to run {} {:?}: {}", program, args, err),
        }),
    }
}

fn read_optional(path: &str) -> serde_json::Value {
    match fs::read_to_string(path) {
        Ok(content) => json!({"present": true, "value": content}),
        Err(err) => json!({"present": false, "error": err.to_string()}),
    }
}

fn list_optional(path: &str) -> serde_json::Value {
    match fs::read_dir(path) {
        Ok(entries) => {
            let mut values = entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            values.sort();
            json!({"present": true, "entries": values})
        }
        Err(err) => json!({"present": false, "error": err.to_string()}),
    }
}

fn unix_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    parse_args(PROGRAM, USAGE, args.clone().into_iter()).map_err(|err| {
        if err.is_empty() {
            process::exit(0);
        }
        err
    })?;

    let profile = args
        .get(1)
        .filter(|arg| !arg.starts_with('-'))
        .map(String::as_str)
        .unwrap_or("wifi-open-bounded");
    let iface = args
        .get(2)
        .filter(|arg| !arg.starts_with('-'))
        .map(String::as_str)
        .unwrap_or("wlan0");
    let output_path = args
        .get(3)
        .filter(|arg| !arg.starts_with('-'))
        .map(String::as_str);

    let scheme_root = format!("/scheme/wifictl/ifaces/{iface}");
    let payload = json!({
        "captured_at_unix": unix_timestamp_secs(),
        "profile": profile,
        "interface": iface,
        "installed": {
            "driver": Path::new("/usr/lib/drivers/redbear-iwlwifi").exists(),
            "wifictl": Path::new("/usr/bin/redbear-wifictl").exists(),
            "netctl": Path::new("/usr/bin/redbear-netctl").exists(),
            "redbear_info": Path::new("/usr/bin/redbear-info").exists(),
        },
        "host": {
            "uname": run_command("uname", &["-a"]),
        },
        "commands": {
            "driver_probe": run_command("redbear-iwlwifi", &["--probe"]),
            "driver_status": run_command("redbear-iwlwifi", &["--status", iface]),
            "wifictl_probe": run_command("redbear-wifictl", &["--probe"]),
            "wifictl_status": run_command("redbear-wifictl", &["--status", iface]),
            "netctl_list": run_command("redbear-netctl", &["list"]),
            "netctl_status_all": run_command("redbear-netctl", &["status"]),
            "netctl_status": run_command("redbear-netctl", &["status", profile]),
            "redbear_info_json": run_command("redbear-info", &["--json"]),
            "phase5_network_check": run_command("redbear-phase5-network-check", &[]),
            "phase5_wifi_check": run_command("redbear-phase5-wifi-check", &[profile, iface]),
            "lspci": run_command("lspci", &[]),
        },
        "filesystem": {
            "wifictl_ifaces": list_optional("/scheme/wifictl/ifaces"),
            "netcfg_ifaces": list_optional("/scheme/netcfg/ifaces"),
            "netctl_profiles": list_optional("/etc/netctl"),
            "active_profile": read_optional("/etc/netctl/active"),
            "profile_contents": read_optional(&format!("/etc/netctl/{profile}")),
        },
        "scheme": {
            "status": read_optional(&format!("{scheme_root}/status")),
            "link_state": read_optional(&format!("{scheme_root}/link-state")),
            "firmware_status": read_optional(&format!("{scheme_root}/firmware-status")),
            "transport_status": read_optional(&format!("{scheme_root}/transport-status")),
            "transport_init_status": read_optional(&format!("{scheme_root}/transport-init-status")),
            "activation_status": read_optional(&format!("{scheme_root}/activation-status")),
            "connect_result": read_optional(&format!("{scheme_root}/connect-result")),
            "disconnect_result": read_optional(&format!("{scheme_root}/disconnect-result")),
            "scan_results": read_optional(&format!("{scheme_root}/scan-results")),
            "last_error": read_optional(&format!("{scheme_root}/last-error")),
        }
    });

    let rendered = serde_json::to_string_pretty(&payload)
        .map_err(|err| format!("failed to serialize capture payload: {err}"))?;
    if let Some(output_path) = output_path {
        fs::write(output_path, &rendered)
            .map_err(|err| format!("failed to write capture bundle to {}: {err}", output_path))?;
    }
    println!("{}", rendered);
    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{PROGRAM}: {err}");
        process::exit(1);
    }
}
