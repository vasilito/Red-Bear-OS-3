// Phase 5 Hardware GPU preflight check.
// Validates DRM device presence, GPU firmware, and rendering infrastructure.
// Does NOT validate real hardware GPU rendering (requires hardware + CS ioctl).

use std::process;

const PROGRAM: &str = "redbear-phase5-gpu-check";
const USAGE: &str = "Usage: redbear-phase5-gpu-check [--json]\n\n\
     Phase 5 hardware GPU preflight check. Validates DRM device registration,\n\
     GPU firmware, and Mesa rendering infrastructure. Hardware validation\n\
     requires real AMD/Intel GPU + command submission (CS ioctl).";

#[cfg(target_os = "redox")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CheckResult { Pass, Fail, Skip }

#[cfg(target_os = "redox")]
impl CheckResult {
    fn label(self) -> &'static str {
        match self { Self::Pass => "PASS", Self::Fail => "FAIL", Self::Skip => "SKIP" }
    }
}

#[cfg(target_os = "redox")]
struct Check { name: String, result: CheckResult, detail: String }

#[cfg(target_os = "redox")]
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

#[cfg(target_os = "redox")]
struct Report { checks: Vec<Check>, json_mode: bool }

#[cfg(target_os = "redox")]
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
            drm_device: bool, gpu_firmware: bool, mesa_dri: bool,
            display_modes: bool, checks: Vec<JsonCheck>,
        }
        let drm = self.checks.iter().find(|c| c.name == "DRM_DEVICE").map_or(false, |c| c.result == CheckResult::Pass);
        let firmware = self.checks.iter().find(|c| c.name == "GPU_FIRMWARE").map_or(false, |c| c.result == CheckResult::Pass);
        let mesa = self.checks.iter().find(|c| c.name == "MESA_DRI").map_or(false, |c| c.result == CheckResult::Pass);
        let modes = self.checks.iter().find(|c| c.name == "DISPLAY_MODES").map_or(false, |c| c.result == CheckResult::Pass);
        let checks: Vec<JsonCheck> = self.checks.iter().map(|c| JsonCheck {
            name: c.name.clone(), result: c.result.label().to_string(), detail: c.detail.clone(),
        }).collect();
        if let Err(err) = serde_json::to_writer(std::io::stdout(), &JsonReport { drm_device: drm, gpu_firmware: firmware, mesa_dri: mesa, display_modes: modes, checks }) {
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
fn check_drm_device() -> Check {
    let paths = ["/scheme/drm/card0", "/dev/dri/card0"];
    for p in paths {
        if std::path::Path::new(p).exists() {
            return Check::pass("DRM_DEVICE", p);
        }
    }
    Check::fail("DRM_DEVICE", "no DRM device found at /scheme/drm/card0 or /dev/dri/card0")
}

#[cfg(target_os = "redox")]
fn check_gpu_firmware() -> Check {
    let firmware_dirs = ["/lib/firmware/amdgpu", "/lib/firmware/i915"];
    let mut found = false;
    for dir in firmware_dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            let count = entries.filter_map(|e| e.ok()).count();
            if count > 0 {
                found = true;
                break;
            }
        }
    }
    if found {
        Check::pass("GPU_FIRMWARE", "GPU firmware blobs present")
    } else {
        Check::skip("GPU_FIRMWARE", "no GPU firmware found (may need fetch-firmware.sh)")
    }
}

#[cfg(target_os = "redox")]
fn check_mesa_dri_hardware() -> Check {
    let hw_drivers = ["/usr/lib/dri/radeonsi_dri.so", "/usr/lib/dri/iris_dri.so"];
    let mut found = Vec::new();
    for d in hw_drivers {
        if std::path::Path::new(d).exists() { found.push(d); }
    }
    if !found.is_empty() {
        let names: Vec<_> = found.iter().map(|s| s.rsplit('/').next().unwrap_or(s)).collect();
        Check::pass("MESA_DRI", &format!("{} hardware DRI driver(s): {}", found.len(), names.join(", ")))
    } else {
        Check::fail("MESA_DRI", "no hardware DRI drivers found (llvmpipe software only)")
    }
}

#[cfg(target_os = "redox")]
fn check_display_modes() -> Check {
    let connector_dir = "/scheme/drm/card0/connectors";
    match std::fs::read_dir(connector_dir) {
        Ok(entries) => {
            let count = entries.filter_map(|e| e.ok()).count();
            if count > 0 {
                Check::pass("DISPLAY_MODES", &format!("{} connector(s) found", count))
            } else {
                Check::fail("DISPLAY_MODES", "no connectors found")
            }
        }
        Err(_) => Check::skip("DISPLAY_MODES", "cannot enumerate connectors (may need hardware GPU)")
    }
}

fn run() -> Result<(), String> {
    #[cfg(not(target_os = "redox"))]
    {
        if std::env::args().any(|a| a == "-h" || a == "--help") { println!("{USAGE}"); return Err(String::new()); }
        println!("{PROGRAM}: GPU check requires Redox runtime");
        return Ok(());
    }
    #[cfg(target_os = "redox")]
    {
        let json_mode = parse_args()?;
        let mut report = Report::new(json_mode);
        report.add(check_drm_device());
        report.add(check_gpu_firmware());
        report.add(check_mesa_dri_hardware());
        report.add(check_display_modes());
        report.print();
        if report.any_failed() { return Err("one or more Phase 5 checks failed".to_string()); }
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
