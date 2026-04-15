use std::path::Path;
use std::process::{self, Command};

use redbear_hwutils::parse_args;

const PROGRAM: &str = "redbear-phase6-kde-check";
const USAGE: &str = "Usage: redbear-phase6-kde-check\n\nShow the installed Phase 6 KDE session surface inside the guest.";

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

    println!("=== Red Bear OS Phase 6 KDE Runtime Check ===");
    require_path("/usr/bin/orbital-kde")?;
    require_path("/usr/bin/kwin_wayland")?;
    require_path("/usr/bin/dbus-daemon")?;
    require_path("/usr/bin/seatd")?;

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
