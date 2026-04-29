// Boot process runtime validation check.
// Validates service ordering, DRM device readiness, compositor socket,
// and greeter service health. Follows Phase 1-5 check pattern.

use std::process;

const PROGRAM: &str = "redbear-boot-check";
const USAGE: &str = "Usage: redbear-boot-check [--json]\n\n\
     Boot process runtime check. Validates critical boot services are\n\
     properly ordered, DRM device is ready, and greeter is healthy.";

#[cfg(target_os = "redox")]
use std::fs;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CheckResult { Pass, Fail, Skip }

impl CheckResult {
    fn label(self) -> &'static str {
        match self { Self::Pass => "PASS", Self::Fail => "FAIL", Self::Skip => "SKIP" }
    }
}

struct Check { name: String, result: CheckResult, detail: String }

impl Check {
    fn pass(name: &str, detail: &str) -> Self {
        Check { name: name.to_string(), result: CheckResult::Pass, detail: detail.to_string() }
    }
    fn fail(name: &str, detail: &str) -> Self {
        Check { name: name.to_string(), result: CheckResult::Fail, detail: detail.to_string() }
    }
    fn skip(name: &str, detail: &str) -> Self {
        Check { name: name.to_string(), result: CheckResult::Skip, detail: detail.to_string() }
    }
}

struct Report { checks: Vec<Check>, json_mode: bool }

impl Report {
    fn new(json_mode: bool) -> Self { Report { checks: Vec::new(), json_mode } }
    fn add(&mut self, check: Check) { self.checks.push(check); }
    fn any_failed(&self) -> bool { self.checks.iter().any(|c| c.result == CheckResult::Fail) }
    fn print(&self) {
        if self.json_mode { self.print_json(); } else { self.print_human(); }
    }
    fn print_human(&self) {
        for check in &self.checks {
            let icon = match check.result {
                CheckResult::Pass => "[PASS]", CheckResult::Fail => "[FAIL]", CheckResult::Skip => "[SKIP]",
            };
            println!("{icon} {}: {}", check.name, check.detail);
        }
    }
    fn print_json(&self) {
        #[derive(serde::Serialize)]
        struct JsonCheck { name: String, result: String, detail: String }
        #[derive(serde::Serialize)]
        struct JsonReport {
            pcid_spawner: bool, drm_device: bool, compositor_socket: bool,
            greeter_service: bool, checks: Vec<JsonCheck>,
        }
        let pcid = self.checks.iter().find(|c| c.name == "PCID_SPAWNER").map_or(false, |c| c.result == CheckResult::Pass);
        let drm = self.checks.iter().find(|c| c.name == "DRM_DEVICE").map_or(false, |c| c.result == CheckResult::Pass);
        let socket = self.checks.iter().find(|c| c.name == "COMPOSITOR_SOCKET").map_or(false, |c| c.result == CheckResult::Pass);
        let greeter = self.checks.iter().find(|c| c.name == "GREETER_SERVICE").map_or(false, |c| c.result == CheckResult::Pass);
        let checks: Vec<JsonCheck> = self.checks.iter().map(|c| JsonCheck {
            name: c.name.clone(), result: c.result.label().to_string(), detail: c.detail.clone(),
        }).collect();
        if let Err(err) = serde_json::to_writer(std::io::stdout(), &JsonReport { pcid_spawner: pcid, drm_device: drm, compositor_socket: socket, greeter_service: greeter, checks }) {
            eprintln!("{PROGRAM}: failed to serialize JSON: {err}");
        }
    }
}

#[cfg(target_os = "redox")]
fn parse_args() -> Result<bool, String> {
    let mut json_mode = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--json" => json_mode = true,
            "-h" | "--help" => { println!("{USAGE}"); return Err(String::new()); }
            _ => return Err(format!("unsupported argument: {arg}")),
        }
    }
    Ok(json_mode)
}

#[cfg(target_os = "redox")]
fn check_pcid_spawner() -> Check {
    let service = "/usr/lib/init.d/00_pcid-spawner.service";
    if std::path::Path::new(service).exists() {
        if std::path::Path::new("/scheme/pci").exists() {
            Check::pass("PCID_SPAWNER", "pcid-spawner service present, /scheme/pci registered")
        } else {
            Check::fail("PCID_SPAWNER", "pcid-spawner installed but /scheme/pci not registered")
        }
    } else {
        Check::fail("PCID_SPAWNER", "pcid-spawner service not found")
    }
}

#[cfg(target_os = "redox")]
fn check_drm_device() -> Check {
    if std::path::Path::new("/scheme/drm/card0").exists() {
        Check::pass("DRM_DEVICE", "/scheme/drm/card0 registered")
    } else {
        Check::fail("DRM_DEVICE", "/scheme/drm/card0 not found — DRM backend unavailable")
    }
}

#[cfg(target_os = "redox")]
fn check_compositor_socket() -> Check {
    let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp/run/redbear-greeter".into());
    let display = std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".into());
    let socket = format!("{}/{}", runtime, display);
    if std::path::Path::new(&socket).exists() {
        Check::pass("COMPOSITOR_SOCKET", &socket)
    } else {
        Check::fail("COMPOSITOR_SOCKET", &format!("{} not found", socket))
    }
}

#[cfg(target_os = "redox")]
fn check_greeter_service() -> Check {
    let service = "/usr/lib/init.d/20_greeter.service";
    if !std::path::Path::new(service).exists() {
        return Check::fail("GREETER_SERVICE", "greeter service definition not found");
    }
    let greeterd = "/usr/bin/redbear-greeterd";
    if std::path::Path::new(greeterd).exists() {
        if std::path::Path::new("/run/redbear-greeterd.sock").exists() {
            Check::pass("GREETER_SERVICE", "greeterd binary present, socket active")
        } else {
            Check::pass("GREETER_SERVICE", "greeterd binary present (socket may not be ready yet)")
        }
    } else {
        Check::fail("GREETER_SERVICE", "greeterd binary not found")
    }
}

#[cfg(not(target_os = "redox"))]
fn check_pcid_spawner() -> Check { Check::skip("PCID_SPAWNER", "requires Redox runtime") }
#[cfg(not(target_os = "redox"))]
fn check_drm_device() -> Check { Check::skip("DRM_DEVICE", "requires Redox runtime") }
#[cfg(not(target_os = "redox"))]
fn check_compositor_socket() -> Check { Check::skip("COMPOSITOR_SOCKET", "requires Redox runtime") }
#[cfg(not(target_os = "redox"))]
fn check_greeter_service() -> Check { Check::skip("GREETER_SERVICE", "requires Redox runtime") }

fn run() -> Result<(), String> {
    #[cfg(not(target_os = "redox"))]
    {
        if std::env::args().any(|a| a == "-h" || a == "--help") { println!("{USAGE}"); return Err(String::new()); }
        println!("{PROGRAM}: boot check requires Redox runtime");
        return Ok(());
    }
    #[cfg(target_os = "redox")]
    {
        let json_mode = parse_args()?;
        let mut report = Report::new(json_mode);
        report.add(check_pcid_spawner());
        report.add(check_drm_device());
        report.add(check_compositor_socket());
        report.add(check_greeter_service());
        report.print();
        if report.any_failed() { return Err("one or more boot checks failed".to_string()); }
        Ok(())
    }
}

fn main() {
    if let Err(err) = run() {
        if err.is_empty() { process::exit(0); }
        eprintln!("{PROGRAM}: {err}");
        process::exit(1);
    }
}
