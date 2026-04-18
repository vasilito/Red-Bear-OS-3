use std::path::Path;
use std::process::{self, Command};

use redbear_hwutils::parse_args;

const PROGRAM: &str = "redbear-phase-iommu-check";
const USAGE: &str = "Usage: redbear-phase-iommu-check\n\nShow the installed IOMMU validation surface inside the guest.";

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

    println!("=== Red Bear OS IOMMU Runtime Check ===");
    require_path("/usr/bin/iommu")?;

    let output = Command::new("/usr/bin/iommu")
        .env("IOMMU_LOG", "info")
        .arg("--self-test-init")
        .output()
        .map_err(|err| format!("failed to run /usr/bin/iommu --self-test-init: {err}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    print!("{}", stdout);
    print!("{}", stderr);

    if !output.status.success() {
        return Err(format!(
            "iommu self-test exited with status {:?}",
            output.status.code()
        ));
    }
    if !stdout.contains("units_detected=") {
        return Err("iommu self-test did not report detected unit count".to_string());
    }
    if !stdout.contains("discovery_source=") {
        return Err("iommu self-test did not report discovery source".to_string());
    }
    if !stdout.contains("units_initialized_now=") {
        return Err("iommu self-test did not report initialized unit count".to_string());
    }
    if !stdout.contains("units_initialized_after=") {
        return Err("iommu self-test did not report initialized-after count".to_string());
    }
    if !stdout.contains("events_drained=") {
        return Err("iommu self-test did not report drained events".to_string());
    }

    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{PROGRAM}: {err}");
        process::exit(1);
    }
}
