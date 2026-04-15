use std::path::Path;
use std::process::{self, Command};

use redbear_hwutils::parse_args;

const PROGRAM: &str = "redbear-phase4-wayland-check";
const USAGE: &str = "Usage: redbear-phase4-wayland-check\n\nShow the installed Phase 4 Wayland launch surface inside the guest.";

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

    println!("=== Red Bear OS Phase 4 Wayland Runtime Check ===");
    require_path("/usr/bin/orbital-wayland")?;
    require_path("/usr/bin/wayland-session")?;
    require_path("/usr/bin/smallvil")?;

    let status = Command::new("redbear-info")
        .arg("--json")
        .status()
        .map_err(|err| format!("failed to run redbear-info --json: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("redbear-info exited with status {status}"))
    }
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{PROGRAM}: {err}");
        process::exit(1);
    }
}
