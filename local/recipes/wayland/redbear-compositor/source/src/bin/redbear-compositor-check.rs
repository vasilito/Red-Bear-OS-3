// Red Bear Compositor Runtime Check — verifies the compositor and greeter surface are healthy.
// Usage: redbear-compositor-check [--verbose]

use std::os::unix::net::UnixStream;
use std::io::{Read, Write};
use std::time::Duration;

fn check_wayland_socket() -> Result<(), String> {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| "/tmp/run/redbear-greeter".into());
    let display = std::env::var("WAYLAND_DISPLAY")
        .unwrap_or_else(|_| "wayland-0".into());
    let socket_path = format!("{}/{}", runtime_dir, display);

    if !std::path::Path::new(&socket_path).exists() {
        return Err(format!("Wayland socket {} does not exist", socket_path));
    }

    let mut stream = UnixStream::connect(&socket_path)
        .map_err(|e| format!("failed to connect to {}: {}", socket_path, e))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| format!("failed to set timeout: {}", e))?;

    // Send wl_display.sync request to verify protocol
    let display_id = 1u32;
    let callback_id = 2u32;
    let mut msg = Vec::new();
    msg.extend_from_slice(&display_id.to_ne_bytes());
    let size = 12u32;
    let opcode = 0u16; // wl_display.sync
    msg.extend_from_slice(&((size << 16) | opcode as u32).to_ne_bytes());
    msg.extend_from_slice(&callback_id.to_ne_bytes());
    stream.write_all(&msg)
        .map_err(|e| format!("wl_display.sync failed: {}", e))?;

    // Read response
    let mut buf = [0u8; 256];
    let n = stream.read(&mut buf)
        .map_err(|e| format!("read failed: {}", e))?;

    if n < 8 {
        return Err(format!("short response: {} bytes", n));
    }

    Ok(())
}

fn check_binaries() -> Result<(), Vec<String>> {
    let mut missing = Vec::new();
    for bin in &["/usr/bin/redbear-compositor", "/usr/bin/redbear-greeterd",
                 "/usr/bin/redbear-greeter-ui", "/usr/bin/redbear-authd",
                 "/usr/bin/kwin_wayland_wrapper"] {
        if !std::path::Path::new(bin).exists() {
            missing.push(bin.to_string());
        }
    }
    if missing.is_empty() { Ok(()) } else { Err(missing) }
}

fn check_framebuffer() -> Result<(), String> {
    let width = std::env::var("FRAMEBUFFER_WIDTH").unwrap_or_default();
    let height = std::env::var("FRAMEBUFFER_HEIGHT").unwrap_or_default();
    let addr = std::env::var("FRAMEBUFFER_ADDR").unwrap_or_default();

    if width.is_empty() || height.is_empty() || addr.is_empty() {
        return Err("FRAMEBUFFER_* environment not set — bootloader didn't provide framebuffer".into());
    }

    let w: u32 = width.parse().map_err(|_| format!("invalid FRAMEBUFFER_WIDTH: {}", width))?;
    let h: u32 = height.parse().map_err(|_| format!("invalid FRAMEBUFFER_HEIGHT: {}", height))?;

    if w == 0 || h == 0 {
        return Err("framebuffer dimensions are zero".into());
    }

    Ok(())
}

fn check_services() -> Result<(), Vec<String>> {
    let mut issues = Vec::new();
    let checks = [
        ("/run/seatd.sock", "seatd socket"),
        ("/run/redbear-authd.sock", "authd socket"),
        ("/run/dbus/system_bus_socket", "D-Bus system bus"),
        ("/scheme/drm/card0", "DRM device"),
    ];
    for (path, name) in checks {
        if !std::path::Path::new(path).exists() {
            issues.push(format!("{} not found at {}", name, path));
        }
    }
    if issues.is_empty() { Ok(()) } else { Err(issues) }
}

fn main() {
    let verbose = std::env::args().any(|a| a == "--verbose");
    let mut exit = 0i32;

    macro_rules! check {
        ($label:expr, $check:expr) => {
            match $check {
                Ok(()) => {
                    if verbose {
                        println!("  PASS {}", $label);
                    }
                }
                Err(e) => {
                    eprintln!("  FAIL {}: {}", $label, e);
                    exit = 1;
                }
            }
        };
        ($label:expr, $check:expr, vec) => {
            match $check {
                Ok(()) => {
                    if verbose { println!("  PASS {}", $label); }
                }
                Err(errs) => {
                    for e in errs {
                        eprintln!("  FAIL {}: {}", $label, e);
                    }
                    exit = 1;
                }
            }
        };
    }

    println!("redbear-compositor-check: verifying compositor surface");

    if verbose {
        println!("  Checking binaries...");
    }
    check!("greeter binaries present", check_binaries(), vec);

    if verbose {
        println!("  Checking framebuffer...");
    }
    check!("framebuffer environment", check_framebuffer());

    if verbose {
        println!("  Checking services...");
    }
    check!("runtime services", check_services(), vec);

    if verbose {
        println!("  Checking Wayland socket...");
    }
    check!("Wayland compositor socket", check_wayland_socket());

    if exit == 0 {
        println!("redbear-compositor-check: all checks passed");
    } else {
        eprintln!("redbear-compositor-check: {} check(s) failed", if exit == 1 { "1" } else { "some" });
        std::process::exit(1);
    }
}
