use std::process::{self, Command};

use redbear_hwutils::parse_args;
use serde_json::Value;

const PROGRAM: &str = "redbear-phase5-wifi-link-check";
const USAGE: &str = "Usage: redbear-phase5-wifi-link-check\n\nCheck whether the current runtime exposes Wi-Fi interface/address/route signals beyond the bounded lifecycle layer.";

fn require_field<'a>(value: &'a Value, field: &str) -> Result<&'a Value, String> {
    value
        .get(field)
        .ok_or_else(|| format!("redbear-info --json did not report {field}"))
}

fn present_nonempty(value: &Value) -> bool {
    value
        .as_str()
        .map(|s| !s.trim().is_empty() && s != "unknown")
        .unwrap_or(false)
}

fn run() -> Result<(), String> {
    parse_args(PROGRAM, USAGE, std::env::args()).map_err(|err| {
        if err.is_empty() {
            process::exit(0);
        }
        err
    })?;

    println!("=== Red Bear OS Phase 5 Wi-Fi Link Check ===");
    let output = Command::new("redbear-info")
        .arg("--json")
        .output()
        .map_err(|err| format!("failed to run redbear-info --json: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("redbear-info --json failed: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&stdout)
        .map_err(|err| format!("failed to parse redbear-info --json output: {err}"))?;
    let network = json
        .get("network")
        .ok_or_else(|| "redbear-info --json did not include network section".to_string())?;

    let interface = require_field(network, "interface")?;
    let address = require_field(network, "address")?;
    let default_route = require_field(network, "default_route")?;
    let wifi_connect_result = require_field(network, "wifi_connect_result")?;

    if present_nonempty(interface) {
        println!("WIFI_INTERFACE=present");
    } else {
        return Err("Wi-Fi/network interface is not reported".to_string());
    }

    if present_nonempty(wifi_connect_result) {
        println!("WIFI_CONNECT_RESULT=present");
    } else {
        return Err("Wi-Fi connect result is not reported".to_string());
    }

    if present_nonempty(address) {
        println!("WIFI_ADDRESS=present");
    } else {
        println!("WIFI_ADDRESS=missing");
    }

    if present_nonempty(default_route) {
        println!("WIFI_DEFAULT_ROUTE=present");
    } else {
        println!("WIFI_DEFAULT_ROUTE=missing");
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
    use serde_json::json;

    #[test]
    fn present_nonempty_rejects_unknown() {
        assert!(!present_nonempty(&json!("unknown")));
        assert!(!present_nonempty(&json!("")));
    }

    #[test]
    fn present_nonempty_accepts_value() {
        assert!(present_nonempty(&json!("wlan0")));
        assert!(present_nonempty(&json!("10.0.0.44/24")));
    }
}
