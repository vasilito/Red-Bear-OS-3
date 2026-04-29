//! Phase 2 Wayland compositor proof checker.

#[cfg(target_os = "redox")]
use std::{
    env, fs,
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
use std::process;

const PROGRAM: &str = "redbear-phase2-wayland-check";
const USAGE: &str = "Usage: redbear-phase2-wayland-check [--json]\n\n\
     Phase 2 Wayland compositor proof checker. Validates the compositor socket,\n\
     compositor process, Wayland protocol connectivity, EGL/GBM presence,\n\
     software renderer evidence, and the optional qt6-wayland-smoke client.";

#[cfg(target_os = "redox")]
const DEFAULT_RUNTIME_DIR: &str = "/run/user/1000";
#[cfg(target_os = "redox")]
const DEFAULT_WAYLAND_DISPLAY: &str = "wayland-0";
#[cfg(target_os = "redox")]
const QT6_WAYLAND_SMOKE: &str = "/usr/bin/qt6-wayland-smoke";

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
    Skip,
}

#[cfg(target_os = "redox")]
impl CheckResult {
    fn label(self) -> &'static str {
        match self {
            CheckResult::Pass => "PASS",
            CheckResult::Fail => "FAIL",
            CheckResult::Skip => "SKIP",
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

    fn skip(name: &str, detail: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            result: CheckResult::Skip,
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
        self.checks.iter().any(|check| check.result == CheckResult::Fail)
    }

    fn check_passed(&self, name: &str) -> bool {
        self.checks
            .iter()
            .find(|check| check.name == name)
            .is_some_and(|check| check.result == CheckResult::Pass)
    }

    fn optional_check_passed(&self, name: &str) -> Option<bool> {
        self.checks
            .iter()
            .find(|check| check.name == name)
            .and_then(|check| match check.result {
                CheckResult::Pass => Some(true),
                CheckResult::Fail => Some(false),
                CheckResult::Skip => None,
            })
    }

    fn print(&self) {
        if self.json_mode {
            self.print_json();
        } else {
            self.print_human();
        }
    }

    fn print_human(&self) {
        println!("=== Red Bear OS Phase 2 Wayland Compositor Check ===");
        for check in &self.checks {
            let icon = match check.result {
                CheckResult::Pass => "[PASS]",
                CheckResult::Fail => "[FAIL]",
                CheckResult::Skip => "[SKIP]",
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
            compositor_socket: bool,
            compositor_process: bool,
            wayland_registry: bool,
            egl_present: bool,
            gbm_present: bool,
            software_renderer: bool,
            qt6_wayland_smoke_present: Option<bool>,
            checks: Vec<JsonCheck>,
        }

        let report = JsonReport {
            overall_success: !self.any_failed(),
            compositor_socket: self.check_passed("WAYLAND_SOCKET"),
            compositor_process: self.check_passed("COMPOSITOR_PROCESS"),
            wayland_registry: self.check_passed("WAYLAND_PROTOCOL_REGISTRY"),
            egl_present: self.check_passed("LIBEGL_PRESENT"),
            gbm_present: self.check_passed("LIBGBM_PRESENT"),
            software_renderer: self.check_passed("SOFTWARE_RENDERER"),
            qt6_wayland_smoke_present: self.optional_check_passed("QT6_WAYLAND_SMOKE"),
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

    fn get_registry(&mut self) -> Result<u32, String> {
        let registry_id = self.alloc_id();
        self.send_message(1, 1, &registry_id.to_le_bytes())?;
        Ok(registry_id)
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
fn wayland_socket_candidates(runtime_dir: Option<&str>, display: Option<&str>) -> Vec<PathBuf> {
    let display = display.unwrap_or(DEFAULT_WAYLAND_DISPLAY);
    let mut candidates = Vec::new();

    if let Some(runtime_dir) = runtime_dir {
        candidates.push(PathBuf::from(runtime_dir).join(display));
    }

    let fallback = PathBuf::from(DEFAULT_RUNTIME_DIR).join(DEFAULT_WAYLAND_DISPLAY);
    if !candidates.iter().any(|candidate| candidate == &fallback) {
        candidates.push(fallback);
    }

    candidates
}

#[cfg(target_os = "redox")]
fn resolve_wayland_endpoint() -> Result<WaylandEndpoint, String> {
    let runtime_dir = env_value("XDG_RUNTIME_DIR");
    let display = env_value("WAYLAND_DISPLAY").unwrap_or_else(|| DEFAULT_WAYLAND_DISPLAY.to_string());
    let candidates = wayland_socket_candidates(runtime_dir.as_deref(), Some(&display));

    for candidate in candidates {
        if candidate.exists() {
            return Ok(WaylandEndpoint {
                path: candidate,
                display: display.clone(),
            });
        }
    }

    let paths = wayland_socket_candidates(runtime_dir.as_deref(), Some(&display))
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!("missing Wayland socket at any of: {paths}"))
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
        return Err(format!("{label} exited with status {}: {detail}", output.status));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(target_os = "redox")]
fn detect_compositor_process(output: &str) -> Option<&'static str> {
    if output.contains("redbear-compositor") {
        Some("redbear-compositor")
    } else if output.contains("kwin_wayland") {
        Some("kwin_wayland")
    } else {
        None
    }
}

#[cfg(target_os = "redox")]
fn check_compositor_process() -> Check {
    match run_command("ps", &[], "ps") {
        Ok(output) => match detect_compositor_process(&output) {
            Some(process_name) => Check::pass(
                "COMPOSITOR_PROCESS",
                format!("{process_name} appears in process list"),
            ),
            None => Check::fail(
                "COMPOSITOR_PROCESS",
                "neither redbear-compositor nor kwin_wayland appears in ps output",
            ),
        },
        Err(err) => Check::fail("COMPOSITOR_PROCESS", err),
    }
}

#[cfg(target_os = "redox")]
fn verify_registry_roundtrip(endpoint: &WaylandEndpoint) -> Result<String, String> {
    let mut client = WaylandClient::connect(&endpoint.path)?;
    let registry_id = client.get_registry()?;
    let callback_id = client.sync()?;

    for _ in 0..8 {
        let (object_id, opcode, _) = client.read_message()?;
        if object_id == registry_id {
            return Ok(format!(
                "{} responded to wl_display.get_registry with opcode {} on {}",
                endpoint.display,
                opcode,
                endpoint.path.display()
            ));
        }
        if object_id == callback_id {
            return Ok(format!(
                "{} completed bounded roundtrip after wl_display.get_registry on {}",
                endpoint.display,
                endpoint.path.display()
            ));
        }
    }

    Err(format!(
        "{} did not answer wl_display.get_registry within bounded read window",
        endpoint.path.display()
    ))
}

#[cfg(target_os = "redox")]
fn contains_software_renderer_text(output: &str) -> bool {
    let lower = output.to_ascii_lowercase();
    lower.contains("llvmpipe")
        || lower.contains("software rasterizer")
        || lower.contains("kms_swrast")
        || lower.contains("swrast")
}

#[cfg(target_os = "redox")]
fn is_software_driver_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("llvmpipe") || lower.contains("kms_swrast") || lower.contains("swrast")
}

#[cfg(target_os = "redox")]
fn software_driver_names_in_dir(dir: &Path) -> Result<Vec<String>, String> {
    let entries = fs::read_dir(dir)
        .map_err(|err| format!("cannot list {}: {err}", dir.display()))?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| is_software_driver_name(name))
        .collect::<Vec<_>>();

    Ok(entries)
}

#[cfg(target_os = "redox")]
fn check_software_renderer() -> Check {
    let mut details = Vec::new();

    if Path::new("/usr/bin/glxinfo").exists() {
        match run_command("glxinfo", &[], "glxinfo") {
            Ok(output) if contains_software_renderer_text(&output) => {
                return Check::pass("SOFTWARE_RENDERER", "glxinfo reports llvmpipe/software renderer");
            }
            Ok(_) => details.push(String::from("glxinfo ran but did not report llvmpipe")),
            Err(err) => details.push(err),
        }
    } else {
        details.push(String::from("/usr/bin/glxinfo not installed"));
    }

    let dri_dir = Path::new("/usr/lib/dri");
    match software_driver_names_in_dir(dri_dir) {
        Ok(driver_names) if !driver_names.is_empty() => Check::pass(
            "SOFTWARE_RENDERER",
            format!(
                "software DRI driver(s) present in {}: {}",
                dri_dir.display(),
                driver_names.join(", ")
            ),
        ),
        Ok(_) => {
            details.push(format!("{} has no llvmpipe/swrast-style drivers", dri_dir.display()));
            Check::fail("SOFTWARE_RENDERER", details.join("; "))
        }
        Err(err) => {
            details.push(err);
            Check::fail("SOFTWARE_RENDERER", details.join("; "))
        }
    }
}

#[cfg(target_os = "redox")]
fn check_optional_qt_smoke() -> Check {
    if Path::new(QT6_WAYLAND_SMOKE).exists() {
        Check::pass("QT6_WAYLAND_SMOKE", QT6_WAYLAND_SMOKE)
    } else {
        Check::skip(
            "QT6_WAYLAND_SMOKE",
            format!("optional binary not installed at {QT6_WAYLAND_SMOKE}"),
        )
    }
}

fn run() -> Result<(), String> {
    let json_mode = parse_args()?;

    #[cfg(not(target_os = "redox"))]
    {
        let _ = json_mode;
        println!("{PROGRAM}: Wayland compositor check requires Redox runtime");
        return Ok(());
    }

    #[cfg(target_os = "redox")]
    {
        let mut report = Report::new(json_mode);

        match resolve_wayland_endpoint() {
            Ok(endpoint) => {
                report.add(Check::pass(
                    "WAYLAND_SOCKET",
                    format!("{} ({})", endpoint.path.display(), endpoint.display),
                ));
                report.add(check_compositor_process());
                report.add(match verify_registry_roundtrip(&endpoint) {
                    Ok(detail) => Check::pass("WAYLAND_PROTOCOL_REGISTRY", detail),
                    Err(err) => Check::fail("WAYLAND_PROTOCOL_REGISTRY", err),
                });
            }
            Err(err) => {
                report.add(Check::fail("WAYLAND_SOCKET", err));
                report.add(check_compositor_process());
                report.add(Check::fail(
                    "WAYLAND_PROTOCOL_REGISTRY",
                    "cannot attempt wl_display.get_registry without a Wayland socket",
                ));
            }
        }

        report.add(if Path::new("/usr/lib/libEGL.so").exists() {
            Check::pass("LIBEGL_PRESENT", "/usr/lib/libEGL.so")
        } else {
            Check::fail("LIBEGL_PRESENT", "missing /usr/lib/libEGL.so")
        });

        report.add(if Path::new("/usr/lib/libGBM.so").exists() {
            Check::pass("LIBGBM_PRESENT", "/usr/lib/libGBM.so")
        } else {
            Check::fail("LIBGBM_PRESENT", "missing /usr/lib/libGBM.so")
        });

        report.add(check_software_renderer());
        report.add(check_optional_qt_smoke());
        report.print();

        if report.any_failed() {
            return Err(String::from("one or more Phase 2 Wayland checks failed"));
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
    fn wayland_socket_candidates_include_runtime_then_default() {
        let candidates = wayland_socket_candidates(Some("/tmp/runtime"), Some("wayland-9"));
        assert_eq!(candidates[0], PathBuf::from("/tmp/runtime/wayland-9"));
        assert!(candidates.contains(&PathBuf::from("/run/user/1000/wayland-0")));
    }

    #[cfg(target_os = "redox")]
    #[test]
    fn detect_compositor_process_matches_kwin_wrapper_line() {
        let output = "123 kwin_wayland_wrapper --virtual\n";
        assert_eq!(detect_compositor_process(output), Some("kwin_wayland"));
    }

    #[cfg(target_os = "redox")]
    #[test]
    fn contains_software_renderer_text_detects_llvmpipe() {
        assert!(contains_software_renderer_text(
            "OpenGL renderer string: llvmpipe (LLVM 18.1, 256 bits)"
        ));
    }

    #[cfg(target_os = "redox")]
    #[test]
    fn is_software_driver_name_detects_swrast_variants() {
        assert!(is_software_driver_name("kms_swrast_dri.so"));
        assert!(is_software_driver_name("swrast_dri.so"));
        assert!(!is_software_driver_name("iris_dri.so"));
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
