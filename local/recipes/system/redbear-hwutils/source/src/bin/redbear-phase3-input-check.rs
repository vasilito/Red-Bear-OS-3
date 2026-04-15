use std::process::{self, Command};

use redbear_hwutils::parse_args;

const PROGRAM: &str = "redbear-phase3-input-check";
const USAGE: &str =
    "Usage: redbear-phase3-input-check\n\nRun the Phase 3 input-path check inside the guest.";

fn run_cmd(name: &str) -> Result<(), String> {
    let status = Command::new(name)
        .status()
        .map_err(|err| format!("failed to run {name}: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{name} exited with status {status}"))
    }
}

fn run() -> Result<(), String> {
    parse_args(PROGRAM, USAGE, std::env::args()).map_err(|err| {
        if err.is_empty() {
            process::exit(0);
        }
        err
    })?;

    run_cmd("redbear-input-inject")?;
    run_cmd("redbear-evtest")?;
    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{PROGRAM}: {err}");
        process::exit(1);
    }
}
