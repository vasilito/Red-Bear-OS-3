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

fn require_wayland_smoke_marker() -> Result<(), String> {
    for path in [
        "/home/root/.qt6-bootstrap-minimal.ok",
        "/home/root/.qt6-plugin-minimal.ok",
        "/home/root/.qt6-wayland-smoke-minimal.ok",
        "/home/root/.qt6-wayland-smoke-offscreen.ok",
        "/home/root/.qt6-wayland-smoke-wayland.ok",
    ] {
        let marker = Path::new(path);
        if !marker.exists() {
            return Err(format!(
                "missing required Qt smoke marker {}",
                marker.display()
            ));
        }
        println!("{}", marker.display());
    }

    let ok = Path::new("/home/root/.qt6-wayland-smoke.ok");
    if ok.exists() {
        println!("{}", ok.display());
        println!("qt6-wayland-smoke");
        return Ok(());
    }

    let err = Path::new("/home/root/.qt6-wayland-smoke.err");
    if err.exists() {
        return Err(format!(
            "qt6-wayland-smoke marker missing; failure marker present at {}",
            err.display()
        ));
    }

    Err("qt6-wayland-smoke did not leave a success marker".to_string())
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
    require_path("/usr/bin/qt6-bootstrap-check")?;
    require_path("/usr/bin/qt6-plugin-check")?;
    require_path("/usr/bin/qt6-wayland-smoke")?;
    require_path("/home/root/.wayland-session.started")?;
    require_wayland_smoke_marker()?;

    let status = Command::new("redbear-info")
        .arg("--json")
        .output()
        .map_err(|err| format!("failed to run redbear-info --json: {err}"))?;
    return if status.status.success() {
        let stdout = String::from_utf8_lossy(&status.stdout);
        if stdout.contains("virtio_net_present") {
            Ok(())
        } else {
            Err("redbear-info --json did not report virtio_net_present".to_string())
        }
    } else {
        let stderr = String::from_utf8_lossy(&status.stderr);
        if stderr.trim().is_empty() {
            Err(format!("redbear-info exited with status {}", status.status))
        } else {
            Err(format!(
                "redbear-info exited with status {}: {}",
                status.status,
                stderr.trim()
            ))
        }
    };
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{PROGRAM}: {err}");
        process::exit(1);
    }
}
