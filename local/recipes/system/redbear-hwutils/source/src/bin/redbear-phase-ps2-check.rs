use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::{self, Command};

use syscall::O_NONBLOCK;

use redbear_hwutils::parse_args;

const PROGRAM: &str = "redbear-phase-ps2-check";
const USAGE: &str =
    "Usage: redbear-phase-ps2-check\n\nRun the bounded PS/2 and serio proof check inside the guest.";

fn require_path(path: &str) -> Result<(), String> {
    if Path::new(path).exists()
        || OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(O_NONBLOCK as i32)
            .open(path)
            .is_ok()
    {
        println!("present={path}");
        Ok(())
    } else {
        Err(format!("missing {path}"))
    }
}

fn run_phase3_input_check() -> Result<(), String> {
    let status = Command::new("redbear-phase3-input-check")
        .status()
        .map_err(|err| format!("failed to run redbear-phase3-input-check: {err}"))?;

    if status.success() {
        println!("phase3_input_check=ok");
        Ok(())
    } else {
        Err(format!("redbear-phase3-input-check exited with status {status}"))
    }
}

fn run() -> Result<(), String> {
    parse_args(PROGRAM, USAGE, std::env::args()).map_err(|err| {
        if err.is_empty() {
            process::exit(0);
        }
        err
    })?;

    println!("=== Red Bear OS PS/2 Runtime Check ===");
    require_path("/scheme/serio/0")?;
    require_path("/scheme/serio/1")?;
    require_path("/usr/bin/redbear-phase3-input-check")?;
    run_phase3_input_check()?;
    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{PROGRAM}: {err}");
        process::exit(1);
    }
}
