use std::path::Path;
use std::process::{self, Command};

use redbear_hwutils::parse_args;

const PROGRAM: &str = "redbear-phase5-wifi-check";
const USAGE: &str = "Usage: redbear-phase5-wifi-check [PROFILE] [INTERFACE]\n\nExercise the bounded Intel Wi-Fi runtime path inside a Red Bear OS guest or target runtime. The packaged runtime path defaults to the bounded open-profile flow; WPA2-PSK remains implemented and host/unit-verified, but is not yet the default packaged runtime proof.";

fn require_path(path: &str) -> Result<(), String> {
    if Path::new(path).exists() {
        println!("{path}");
        Ok(())
    } else {
        Err(format!("missing {path}"))
    }
}

fn require_contains(label: &str, haystack: &str, needle: &str) -> Result<(), String> {
    if haystack.contains(needle) {
        println!("{label}={needle}");
        Ok(())
    } else {
        Err(format!("{label} missing {needle}"))
    }
}

fn run_command(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|err| format!("failed to run {} {:?}: {err}", program, args))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.trim().is_empty() {
            return Err(format!(
                "{} {:?} exited with status {}",
                program, args, output.status
            ));
        }
        return Err(format!(
            "{} {:?} exited with status {}: {}",
            program,
            args,
            output.status,
            stderr.trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
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

    println!("=== Red Bear OS Phase 5 Wi-Fi Check ===");
    println!("profile={profile}");
    println!("interface={iface}");

    require_path("/usr/lib/drivers/redbear-iwlwifi")?;
    require_path("/usr/bin/redbear-wifictl")?;
    require_path("/usr/bin/redbear-netctl")?;
    require_path("/usr/bin/redbear-info")?;

    let driver_probe = run_command("redbear-iwlwifi", &["--probe"])?;
    print!("{driver_probe}");
    require_contains("driver_probe", &driver_probe, "candidates=")?;

    let probe = run_command("redbear-wifictl", &["--probe"])?;
    print!("{probe}");
    require_contains("wifictl_probe", &probe, "interfaces=")?;
    require_contains("wifictl_probe", &probe, iface)?;

    let connect = run_command("redbear-wifictl", &["--connect", iface, "demo", "open"])?;
    print!("{connect}");
    require_contains("connect", &connect, "status=connected")
        .or_else(|_| require_contains("connect", &connect, "status=associated"))
        .or_else(|_| require_contains("connect", &connect, "status=associating"))?;
    require_contains("connect", &connect, "connect_result=")?;

    let disconnect = run_command("redbear-wifictl", &["--disconnect", iface])?;
    print!("{disconnect}");
    require_contains("disconnect", &disconnect, "disconnect_result=")?;

    let start = run_command("redbear-netctl", &["start", profile])?;
    print!("{start}");
    let status = run_command("redbear-netctl", &["status", profile])?;
    print!("{status}");
    require_contains("netctl_status", &status, &format!("interface={iface}"))?;
    require_contains("netctl_status", &status, "connect_result=")?;

    let stop = run_command("redbear-netctl", &["stop", profile])?;
    print!("{stop}");

    let info = run_command("redbear-info", &["--json"])?;
    print!("{info}");
    require_contains("redbear_info", &info, "wifi_control_state")?;
    require_contains("redbear_info", &info, "wifi_connect_result")?;
    require_contains("redbear_info", &info, "wifi_disconnect_result")?;

    println!("PASS: bounded Intel Wi-Fi runtime path exercised inside target runtime");
    println!(
        "NOTE: the packaged runtime checker currently validates the bounded open-profile path by default; WPA2-PSK is implemented and host/unit-verified elsewhere in-repo but is not yet the default packaged runtime proof"
    );
    println!(
        "NOTE: this still does not prove real AP scan/auth/association, packet flow, DHCP success over Wi-Fi, or validated end-to-end connectivity"
    );
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
    fn require_contains_accepts_present_substring() {
        assert!(
            require_contains("test", "abc wifi_control_state xyz", "wifi_control_state").is_ok()
        );
    }

    #[test]
    fn require_contains_rejects_missing_substring() {
        assert!(require_contains("test", "abc", "wifi_connect_result").is_err());
    }
}
