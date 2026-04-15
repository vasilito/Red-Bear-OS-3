use std::path::Path;
use std::process::{self, Command};

use redbear_hwutils::parse_args;

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

fn run() -> Result<(), String> {
    parse_args(PROGRAM, USAGE, std::env::args()).map_err(|err| {
        if err.is_empty() {
            process::exit(0);
        }
        err
    })?;

    println!("=== Red Bear OS Phase 5 Networking Check ===");
    require_path("/usr/bin/dbus-daemon")?;

    let info_status = Command::new("redbear-info")
        .arg("--json")
        .status()
        .map_err(|err| format!("failed to run redbear-info --json: {err}"))?;
    if !info_status.success() {
        return Err(format!("redbear-info exited with status {info_status}"));
    }

    let _ = Command::new("netctl").arg("status").status();
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
