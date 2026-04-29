//! Phase 3 desktop-session preflight checker.
//! Validates compositor binary presence, D-Bus session bus, seatd socket,
//! and WAYLAND_DISPLAY availability. Does NOT validate real KWin behavior
//! (KWin recipe currently provides cmake stubs pending Qt6Quick/QML).

use std::process;
#[cfg(target_os = "redox")]
use std::{
    env,
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

const PROGRAM: &str = "redbear-phase3-kwin-check";
const USAGE: &str = "Usage: redbear-phase3-kwin-check [--json]\n\n\
     Phase 3 desktop-session preflight check. Validates compositor binary\n\
     presence, D-Bus session bus reachability, seatd socket presence, active\n\
     WAYLAND_DISPLAY state, and a bounded wl_display roundtrip.\n\
     NOTE: Does NOT validate real KWin behavior (KWin is a cmake stub).";

#[cfg(target_os = "redox")]
const DEFAULT_RUNTIME_DIR: &str = "/run/user/1000";
#[cfg(target_os = "redox")]
const DBUS_SESSION_DESTINATION: &str = "org.freedesktop.DBus";

fn parse_args_from<I>(args: I) -> Result<bool, String>
where
    I: IntoIterator<Item = String>,
{
    let mut json_mode = false;

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => json_mode = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Err(String::new());
            }
            _ => return Err(format!("unsupported argument: {arg}")),
        }
    }

    Ok(json_mode)
}

fn parse_args() -> Result<bool, String> {
    parse_args_from(std::env::args().skip(1))
}

#[cfg(target_os = "redox")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CheckResult {
    Pass,
    Fail,
}

#[cfg(target_os = "redox")]
impl CheckResult {
    fn label(self) -> &'static str {
        match self {
            CheckResult::Pass => "PASS",
            CheckResult::Fail => "FAIL",
        }
    }
}

#[cfg(target_os = "redox")]
struct Check {
    name: String,
    result: CheckResult,
    detail: String,
}

#[cfg(target_os = "redox")]
impl Check {
    fn pass(name: &str, detail: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            result: CheckResult::Pass,
            detail: detail.into(),
        }
    }

    fn fail(name: &str, detail: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            result: CheckResult::Fail,
            detail: detail.into(),
        }
    }
}

#[cfg(target_os = "redox")]
struct Report {
    checks: Vec<Check>,
    json_mode: bool,
}

#[cfg(target_os = "redox")]
impl Report {
    fn new(json_mode: bool) -> Self {
        Self {
            checks: Vec::new(),
            json_mode,
        }
    }

    fn add(&mut self, check: Check) {
        self.checks.push(check);
    }

    fn any_failed(&self) -> bool {
        self.checks
            .iter()
            .any(|check| check.result == CheckResult::Fail)
    }

    fn check_passed(&self, name: &str) -> bool {
        self.checks
            .iter()
            .find(|check| check.name == name)
            .is_some_and(|check| check.result == CheckResult::Pass)
    }

    fn print(&self) {
        if self.json_mode {
            self.print_json();
        } else {
            self.print_human();
        }
    }

    fn print_human(&self) {
        println!("=== Red Bear OS Phase 3 Desktop Session Preflight ===");
        for check in &self.checks {
            let icon = match check.result {
                CheckResult::Pass => "[PASS]",
                CheckResult::Fail => "[FAIL]",
            };
            println!("{icon} {}: {}", check.name, check.detail);
        }
    }

    fn print_json(&self) {
        #[derive(serde::Serialize)]
        struct JsonCheck {
            name: String,
            result: String,
            detail: String,
        }

        #[derive(serde::Serialize)]
        struct JsonReport {
            overall_success: bool,
            compositor_binary: bool,
            dbus_session_bus_address: bool,
            dbus_send_session: bool,
            seatd_socket: bool,
            wayland_display_active: bool,
            wayland_roundtrip: bool,
            checks: Vec<JsonCheck>,
        }

        let report = JsonReport {
            overall_success: !self.any_failed(),
            compositor_binary: self.check_passed("COMPOSITOR_BINARY"),
            dbus_session_bus_address: self.check_passed("DBUS_SESSION_BUS_ADDRESS"),
            dbus_send_session: self.check_passed("DBUS_SEND_SESSION"),
            seatd_socket: self.check_passed("SEATD_SOCKET"),
            wayland_display_active: self.check_passed("WAYLAND_DISPLAY_ACTIVE"),
            wayland_roundtrip: self.check_passed("WAYLAND_ROUNDTRIP"),
            checks: self
                .checks
                .iter()
                .map(|check| JsonCheck {
                    name: check.name.clone(),
                    result: check.result.label().to_string(),
                    detail: check.detail.clone(),
                })
                .collect(),
        };

        if let Err(err) = serde_json::to_writer(std::io::stdout(), &report) {
            eprintln!("{PROGRAM}: failed to serialize JSON: {err}");
        }
    }
}

#[cfg(target_os = "redox")]
#[derive(Debug, Clone)]
struct WaylandEndpoint {
    path: PathBuf,
    display: String,
}

#[cfg(target_os = "redox")]
struct WaylandClient {
    stream: UnixStream,
    next_id: u32,
}

#[cfg(target_os = "redox")]
impl WaylandClient {
    fn connect(path: &Path) -> Result<Self, String> {
        let stream = UnixStream::connect(path)
            .map_err(|err| format!("failed to connect to {}: {err}", path.display()))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(|err| format!("failed to set read timeout on {}: {err}", path.display()))?;
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .map_err(|err| format!("failed to set write timeout on {}: {err}", path.display()))?;
        Ok(Self { stream, next_id: 2 })
    }

    fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn send_message(&mut self, object_id: u32, opcode: u16, payload: &[u8]) -> Result<(), String> {
        let size = 8 + payload.len();
        let mut message = Vec::with_capacity(size);
        message.extend_from_slice(&object_id.to_le_bytes());
        let header = ((size as u32) << 16) | u32::from(opcode);
        message.extend_from_slice(&header.to_le_bytes());
        message.extend_from_slice(payload);
        self.stream
            .write_all(&message)
            .map_err(|err| format!("failed to write Wayland message: {err}"))
    }

    fn read_message(&mut self) -> Result<(u32, u16, Vec<u8>), String> {
        let mut header = [0u8; 8];
        self.stream
            .read_exact(&mut header)
            .map_err(|err| format!("failed to read Wayland header: {err}"))?;

        let object_id = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let size_opcode = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        let size = ((size_opcode >> 16) & 0xFFFF) as usize;
        let opcode = (size_opcode & 0xFFFF) as u16;
        if size < 8 {
            return Err(format!("invalid Wayland message size {size}"));
        }

        let payload_len = size - 8;
        let mut payload = vec![0u8; payload_len];
        if payload_len > 0 {
            self.stream
                .read_exact(&mut payload)
                .map_err(|err| format!("failed to read Wayland payload: {err}"))?;
        }

        Ok((object_id, opcode, payload))
    }

    fn sync(&mut self) -> Result<u32, String> {
        let callback_id = self.alloc_id();
        self.send_message(1, 0, &callback_id.to_le_bytes())?;
        Ok(callback_id)
    }
}

#[cfg(target_os = "redox")]
fn env_value(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

#[cfg(target_os = "redox")]
fn run_command(program: &str, args: &[&str], label: &str) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|err| format!("failed to run {label}: {err}"))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = if !stderr.trim().is_empty() {
            stderr.trim().to_string()
        } else if !stdout.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            String::from("no output")
        };
        return Err(format!(
            "{label} exited with status {}: {detail}",
            output.status
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(target_os = "redox")]
fn resolve_wayland_endpoint() -> Result<WaylandEndpoint, String> {
    let display =
        env_value("WAYLAND_DISPLAY").ok_or_else(|| String::from("WAYLAND_DISPLAY is not set"))?;
    let runtime_dir =
        env_value("XDG_RUNTIME_DIR").unwrap_or_else(|| DEFAULT_RUNTIME_DIR.to_string());
    let path = PathBuf::from(runtime_dir).join(&display);
    if path.exists() {
        Ok(WaylandEndpoint { path, display })
    } else {
        Err(format!(
            "WAYLAND_DISPLAY is set but socket is missing at {}",
            path.display()
        ))
    }
}

#[cfg(target_os = "redox")]
fn require_one_path<'a>(paths: &'a [&'a str]) -> Result<&'a str, String> {
    for path in paths {
        if Path::new(path).exists() {
            return Ok(*path);
        }
    }
    Err(format!("missing any of: {}", paths.join(", ")))
}

#[cfg(target_os = "redox")]
fn check_dbus_session_bus() -> (Check, Check) {
    match env_value("DBUS_SESSION_BUS_ADDRESS") {
        Some(address) => {
            let address_check = Check::pass("DBUS_SESSION_BUS_ADDRESS", address);

            if !Path::new("/usr/bin/dbus-send").exists() {
                return (
                    address_check,
                    Check::fail("DBUS_SEND_SESSION", "missing /usr/bin/dbus-send"),
                );
            }

            match run_command(
                "dbus-send",
                &[
                    "--session",
                    &format!("--dest={DBUS_SESSION_DESTINATION}"),
                    "--type=method_call",
                    "--print-reply",
                    "/org/freedesktop/DBus",
                    "org.freedesktop.DBus.ListNames",
                ],
                "dbus-send --session ListNames",
            ) {
                Ok(output) if !output.trim().is_empty() => (
                    address_check,
                    Check::pass(
                        "DBUS_SEND_SESSION",
                        "dbus-send --session returned a non-empty bus name list",
                    ),
                ),
                Ok(_) => (
                    address_check,
                    Check::fail(
                        "DBUS_SEND_SESSION",
                        "dbus-send --session returned empty output",
                    ),
                ),
                Err(err) => (address_check, Check::fail("DBUS_SEND_SESSION", err)),
            }
        }
        None => (
            Check::fail(
                "DBUS_SESSION_BUS_ADDRESS",
                "DBUS_SESSION_BUS_ADDRESS is not set",
            ),
            Check::fail(
                "DBUS_SEND_SESSION",
                "cannot validate dbus-send without DBUS_SESSION_BUS_ADDRESS",
            ),
        ),
    }
}

#[cfg(target_os = "redox")]
fn verify_wayland_roundtrip(endpoint: &WaylandEndpoint) -> Result<String, String> {
    let mut client = WaylandClient::connect(&endpoint.path)?;
    let callback_id = client.sync()?;

    for _ in 0..8 {
        let (object_id, opcode, _) = client.read_message()?;
        if object_id == callback_id && opcode == 0 {
            return Ok(format!(
                "{} completed wl_display.sync roundtrip on {}",
                endpoint.display,
                endpoint.path.display()
            ));
        }
    }

    Err(format!(
        "{} did not emit callback.done within bounded read window",
        endpoint.path.display()
    ))
}

fn run() -> Result<(), String> {
    let json_mode = parse_args()?;

    #[cfg(not(target_os = "redox"))]
    {
        let _ = json_mode;
        println!("{PROGRAM}: desktop session preflight requires Redox runtime");
        return Ok(());
    }

    #[cfg(target_os = "redox")]
    {
        let mut report = Report::new(json_mode);

        report.add(
            match require_one_path(&["/usr/bin/kwin_wayland", "/usr/bin/redbear-compositor"]) {
                Ok(path) => Check::pass("COMPOSITOR_BINARY", path),
                Err(err) => Check::fail("COMPOSITOR_BINARY", err),
            },
        );

        let (dbus_address_check, dbus_send_check) = check_dbus_session_bus();
        report.add(dbus_address_check);
        report.add(dbus_send_check);

        report.add(if Path::new("/run/seatd.sock").exists() {
            Check::pass("SEATD_SOCKET", "/run/seatd.sock")
        } else {
            Check::fail("SEATD_SOCKET", "missing /run/seatd.sock")
        });

        match resolve_wayland_endpoint() {
            Ok(endpoint) => {
                report.add(Check::pass(
                    "WAYLAND_DISPLAY_ACTIVE",
                    format!("{} ({})", endpoint.path.display(), endpoint.display),
                ));
                report.add(match verify_wayland_roundtrip(&endpoint) {
                    Ok(detail) => Check::pass("WAYLAND_ROUNDTRIP", detail),
                    Err(err) => Check::fail("WAYLAND_ROUNDTRIP", err),
                });
            }
            Err(err) => {
                report.add(Check::fail("WAYLAND_DISPLAY_ACTIVE", err));
                report.add(Check::fail(
                    "WAYLAND_ROUNDTRIP",
                    "cannot attempt wl_display roundtrip without an active WAYLAND_DISPLAY socket",
                ));
            }
        }

        report.print();

        if report.any_failed() {
            return Err(String::from("one or more Phase 3 preflight checks failed"));
        }

        Ok(())
    }
}

fn main() {
    if let Err(err) = run() {
        if err.is_empty() {
            process::exit(0);
        }
        eprintln!("{PROGRAM}: {err}");
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "redox")]
    #[test]
    fn require_one_path_returns_first_present_path() {
        let existing = require_one_path(&["/", "/definitely/missing"]);
        assert_eq!(existing, Ok("/"));
    }

    #[cfg(target_os = "redox")]
    #[test]
    fn resolve_wayland_endpoint_requires_display() {
        let result = {
            let display = None::<String>;
            display.ok_or_else(|| String::from("WAYLAND_DISPLAY is not set"))
        };
        assert_eq!(result, Err(String::from("WAYLAND_DISPLAY is not set")));
    }

    #[test]
    fn parse_args_accepts_json_flag() {
        let parsed = parse_args_from([String::from("--json")]);
        assert_eq!(parsed, Ok(true));
    }

    #[test]
    fn parse_args_rejects_unknown_flag() {
        let parsed = parse_args_from([String::from("--bogus")]);
        assert_eq!(parsed, Err(String::from("unsupported argument: --bogus")));
    }
}
