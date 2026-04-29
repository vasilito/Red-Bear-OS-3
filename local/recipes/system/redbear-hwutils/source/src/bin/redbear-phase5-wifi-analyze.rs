use std::fs;
use std::process;

use redbear_hwutils::parse_args;
use serde_json::Value;

const PROGRAM: &str = "redbear-phase5-wifi-analyze";
const USAGE: &str = "Usage: redbear-phase5-wifi-analyze <capture.json>\n\nSummarize a Wi-Fi capture bundle into likely blocker categories.";

fn read_text<'a>(value: &'a Value, path: &[&str]) -> &'a str {
    let mut current = value;
    for segment in path {
        match current.get(*segment) {
            Some(next) => current = next,
            None => return "",
        }
    }
    current.as_str().unwrap_or("")
}

fn classify(capture: &Value) -> Vec<&'static str> {
    let mut out = Vec::new();

    let driver_probe = read_text(capture, &["commands", "driver_probe", "stdout"]);
    let connect = read_text(capture, &["commands", "phase5_wifi_check", "stdout"]);
    let connect_result = read_text(capture, &["scheme", "connect_result", "value"]);
    let disconnect_result = read_text(capture, &["scheme", "disconnect_result", "value"]);
    let last_error = read_text(capture, &["scheme", "last_error", "value"]);
    let netctl_status = read_text(capture, &["commands", "netctl_status", "stdout"]);
    let redbear_info = read_text(capture, &["commands", "redbear_info_json", "stdout"]);

    if !driver_probe.contains("candidates=") || driver_probe.contains("candidates=0") {
        out.push("device-detection");
    }
    if connect.contains("missing firmware") || last_error.contains("firmware") {
        out.push("firmware");
    }
    if connect_result.is_empty() || connect_result.contains("not-run") {
        out.push("association-control-path");
    }
    if disconnect_result.is_empty() || disconnect_result.contains("not-run") {
        out.push("disconnect-lifecycle");
    }
    if !netctl_status.contains("address=") || netctl_status.contains("address=unknown") {
        out.push("dhcp-or-addressing");
    }
    if !redbear_info.contains("wifi_connect_result")
        || !redbear_info.contains("wifi_disconnect_result")
    {
        out.push("reporting-surface");
    }
    if last_error.contains("timed out") || last_error.contains("failed") {
        out.push("runtime-failure");
    }

    if out.is_empty() {
        out.push("bounded-lifecycle-pass-no-real-link-proof");
    }
    out
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    parse_args(PROGRAM, USAGE, args.clone().into_iter()).map_err(|err| {
        if err.is_empty() {
            process::exit(0);
        }
        err
    })?;

    let path = args
        .get(1)
        .ok_or_else(|| "missing capture.json path".to_string())?;
    let text = fs::read_to_string(path).map_err(|err| format!("failed to read {}: {err}", path))?;
    let capture: Value = serde_json::from_str(&text)
        .map_err(|err| format!("failed to parse {} as JSON: {err}", path))?;

    println!("=== Red Bear Wi-Fi Capture Analysis ===");
    println!("capture={path}");
    println!(
        "profile={}",
        capture
            .get("profile")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    );
    println!(
        "interface={}",
        capture
            .get("interface")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    );

    let classes = classify(&capture);
    println!("classification={}", classes.join(","));
    for item in classes {
        println!("blocker={item}");
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
    fn classify_flags_missing_detection() {
        let capture = json!({
            "commands": {
                "driver_probe": {"stdout": "candidates=0"},
                "phase5_wifi_check": {"stdout": ""},
                "netctl_status": {"stdout": "address=unknown"},
                "redbear_info_json": {"stdout": "{}"}
            },
            "scheme": {
                "connect_result": {"value": ""},
                "disconnect_result": {"value": ""},
                "last_error": {"value": ""}
            }
        });
        let classes = classify(&capture);
        assert!(classes.contains(&"device-detection"));
    }

    #[test]
    fn classify_pass_path_when_only_bounded_state_exists() {
        let capture = json!({
            "commands": {
                "driver_probe": {"stdout": "candidates=1"},
                "phase5_wifi_check": {"stdout": "PASS: bounded Intel Wi-Fi runtime path exercised on target"},
                "netctl_status": {"stdout": "address=10.0.0.44/24"},
                "redbear_info_json": {"stdout": "wifi_connect_result wifi_disconnect_result"}
            },
            "scheme": {
                "connect_result": {"value": "connect_result=bounded-associated"},
                "disconnect_result": {"value": "disconnect_result=bounded-disconnected"},
                "last_error": {"value": "none"}
            }
        });
        let classes = classify(&capture);
        assert_eq!(classes, vec!["bounded-lifecycle-pass-no-real-link-proof"]);
    }
}
