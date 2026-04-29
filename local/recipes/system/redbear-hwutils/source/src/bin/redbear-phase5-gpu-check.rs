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
const DRM_IOCTL_BASE: usize = 0x00A0;
#[cfg(target_os = "redox")]
const DRM_IOCTL_GEM_CREATE: usize = DRM_IOCTL_BASE + 26;
#[cfg(target_os = "redox")]
const DRM_IOCTL_GEM_CLOSE: usize = DRM_IOCTL_BASE + 27;
#[cfg(target_os = "redox")]
const DRM_IOCTL_REDOX_PRIVATE_CS_SUBMIT: usize = DRM_IOCTL_BASE + 31;

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
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Skip => "SKIP",
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
    fn pass(name: &str, detail: &str) -> Self {
        Check {
            name: name.to_string(),
            result: CheckResult::Pass,
            detail: detail.to_string(),
        }
    }
    fn fail(name: &str, detail: &str) -> Self {
        Check {
            name: name.to_string(),
            result: CheckResult::Fail,
            detail: detail.to_string(),
        }
    }
    fn skip(name: &str, detail: &str) -> Self {
        Check {
            name: name.to_string(),
            result: CheckResult::Skip,
            detail: detail.to_string(),
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
        Report {
            checks: Vec::new(),
            json_mode,
        }
    }
    fn add(&mut self, check: Check) {
        self.checks.push(check);
    }
    fn any_failed(&self) -> bool {
        self.checks.iter().any(|c| c.result == CheckResult::Fail)
    }

    fn print(&self) {
        if self.json_mode {
            self.print_json();
        } else {
            self.print_human();
        }
    }

    fn print_human(&self) {
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
            drm_device: bool,
            gpu_firmware: bool,
            mesa_dri: bool,
            display_modes: bool,
            cs_ioctl: bool,
            gem_buffers: bool,
            hardware_rendering_ready: bool,
            checks: Vec<JsonCheck>,
        }
        let drm = self
            .checks
            .iter()
            .find(|c| c.name == "DRM_DEVICE")
            .map_or(false, |c| c.result == CheckResult::Pass);
        let firmware = self
            .checks
            .iter()
            .find(|c| c.name == "GPU_FIRMWARE")
            .map_or(false, |c| c.result == CheckResult::Pass);
        let mesa = self
            .checks
            .iter()
            .find(|c| c.name == "MESA_DRI")
            .map_or(false, |c| c.result == CheckResult::Pass);
        let modes = self
            .checks
            .iter()
            .find(|c| c.name == "DISPLAY_MODES")
            .map_or(false, |c| c.result == CheckResult::Pass);
        let cs_ioctl = self
            .checks
            .iter()
            .find(|c| c.name == "CS_IOCTL_PROTOCOL")
            .map_or(false, |c| c.result == CheckResult::Pass);
        let gem_buffers = self
            .checks
            .iter()
            .find(|c| c.name == "GEM_BUFFER_ALLOCATION")
            .map_or(false, |c| c.result == CheckResult::Pass);
        let hardware_ready = self
            .checks
            .iter()
            .find(|c| c.name == "HARDWARE_RENDERING_READY")
            .map_or(false, |c| c.result == CheckResult::Pass);
        let checks: Vec<JsonCheck> = self
            .checks
            .iter()
            .map(|c| JsonCheck {
                name: c.name.clone(),
                result: c.result.label().to_string(),
                detail: c.detail.clone(),
            })
            .collect();
        if let Err(err) = serde_json::to_writer(
            std::io::stdout(),
            &JsonReport {
                drm_device: drm,
                gpu_firmware: firmware,
                mesa_dri: mesa,
                display_modes: modes,
                cs_ioctl,
                gem_buffers,
                hardware_rendering_ready: hardware_ready,
                checks,
            },
        ) {
            eprintln!("{PROGRAM}: failed to serialize JSON: {err}");
        }
    }
}

#[cfg(target_os = "redox")]
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmGemCreateWire {
    size: u64,
    handle: u32,
    pad: u32,
}

#[cfg(target_os = "redox")]
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmGemCloseWire {
    handle: u32,
}

#[cfg(target_os = "redox")]
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct RedoxPrivateCsSubmit {
    src_handle: u32,
    dst_handle: u32,
    src_offset: u64,
    dst_offset: u64,
    byte_count: u64,
}

#[cfg(target_os = "redox")]
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct RedoxPrivateCsSubmitResult {
    seqno: u64,
}

#[cfg(target_os = "redox")]
fn parse_args() -> Result<bool, String> {
    let mut json_mode = false;
    for arg in std::env::args().skip(1) {
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

#[cfg(target_os = "redox")]
fn check_drm_device() -> Check {
    let scheme_path = "/scheme/drm/card0";
    if std::path::Path::new(scheme_path).exists() {
        return Check::pass("DRM_DEVICE", scheme_path);
    }
    let dev_alias = "/dev/dri/card0";
    if std::path::Path::new(dev_alias).exists() {
        return Check::fail(
            "DRM_DEVICE",
            "/dev/dri/card0 exists, but Phase 5 CS probing requires /scheme/drm/card0",
        );
    }
    Check::fail("DRM_DEVICE", "no DRM device found at /scheme/drm/card0")
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
        Check::skip(
            "GPU_FIRMWARE",
            "no GPU firmware found (may need fetch-firmware.sh)",
        )
    }
}

#[cfg(target_os = "redox")]
fn check_mesa_dri_hardware() -> Check {
    let hw_drivers = ["/usr/lib/dri/radeonsi_dri.so", "/usr/lib/dri/iris_dri.so"];
    let mut found = Vec::new();
    for d in hw_drivers {
        if std::path::Path::new(d).exists() {
            found.push(d);
        }
    }
    if !found.is_empty() {
        let names: Vec<_> = found
            .iter()
            .map(|s| s.rsplit('/').next().unwrap_or(s))
            .collect();
        Check::pass(
            "MESA_DRI",
            &format!(
                "{} hardware DRI driver(s): {}",
                found.len(),
                names.join(", ")
            ),
        )
    } else {
        Check::fail(
            "MESA_DRI",
            "no hardware DRI drivers found (llvmpipe software only)",
        )
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
        Err(_) => Check::skip(
            "DISPLAY_MODES",
            "cannot enumerate connectors (may need hardware GPU)",
        ),
    }
}

#[cfg(target_os = "redox")]
fn decode_wire_exact<T: Copy>(bytes: &[u8]) -> Result<T, String> {
    use std::mem::{MaybeUninit, size_of};

    if bytes.len() != size_of::<T>() {
        return Err(format!(
            "unexpected DRM response size: expected {} bytes, got {}",
            size_of::<T>(),
            bytes.len()
        ));
    }

    let mut out = MaybeUninit::<T>::uninit();
    unsafe {
        std::ptr::copy_nonoverlapping(
            bytes.as_ptr(),
            out.as_mut_ptr().cast::<u8>(),
            size_of::<T>(),
        );
        Ok(out.assume_init())
    }
}

#[cfg(target_os = "redox")]
fn bytes_of<T>(value: &T) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts((value as *const T).cast::<u8>(), std::mem::size_of::<T>())
    }
}

#[cfg(target_os = "redox")]
fn open_scheme_drm_card() -> Result<std::fs::File, String> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/scheme/drm/card0")
        .map_err(|err| format!("failed to open /scheme/drm/card0: {err}"))
}

#[cfg(target_os = "redox")]
fn drm_query(file: &mut std::fs::File, request: usize, payload: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::{Read, Write};

    let mut request_buf = request.to_le_bytes().to_vec();
    request_buf.extend_from_slice(payload);
    file.write_all(&request_buf)
        .map_err(|err| format!("failed to send DRM ioctl {request:#x}: {err}"))?;

    let mut response = vec![0u8; 4096];
    let len = file
        .read(&mut response)
        .map_err(|err| format!("failed to read DRM ioctl {request:#x} response: {err}"))?;
    response.truncate(len);
    Ok(response)
}

#[cfg(target_os = "redox")]
fn check_gem_buffer_allocation() -> Check {
    let mut card = match open_scheme_drm_card() {
        Ok(card) => card,
        Err(err) => return Check::fail("GEM_BUFFER_ALLOCATION", &err),
    };

    let request = DrmGemCreateWire {
        size: 4096,
        ..DrmGemCreateWire::default()
    };

    match drm_query(&mut card, DRM_IOCTL_GEM_CREATE, bytes_of(&request))
        .and_then(|response| decode_wire_exact::<DrmGemCreateWire>(&response))
    {
        Ok(created) => {
            let _ = drm_query(
                &mut card,
                DRM_IOCTL_GEM_CLOSE,
                bytes_of(&DrmGemCloseWire {
                    handle: created.handle,
                }),
            );
            Check::pass(
                "GEM_BUFFER_ALLOCATION",
                &format!(
                    "allocated GEM handle {} over /scheme/drm/card0",
                    created.handle
                ),
            )
        }
        Err(err) => Check::fail("GEM_BUFFER_ALLOCATION", &err),
    }
}

#[cfg(target_os = "redox")]
fn check_cs_ioctl_protocol() -> Check {
    let mut card = match open_scheme_drm_card() {
        Ok(card) => card,
        Err(err) => return Check::fail("CS_IOCTL_PROTOCOL", &err),
    };

    let first = DrmGemCreateWire {
        size: 4096,
        ..DrmGemCreateWire::default()
    };
    let second = first;

    let created_a = match drm_query(&mut card, DRM_IOCTL_GEM_CREATE, bytes_of(&first))
        .and_then(|response| decode_wire_exact::<DrmGemCreateWire>(&response))
    {
        Ok(created) => created,
        Err(err) => {
            return Check::fail(
                "CS_IOCTL_PROTOCOL",
                &format!("source GEM allocation failed before CS probe: {err}"),
            );
        }
    };

    let created_b = match drm_query(&mut card, DRM_IOCTL_GEM_CREATE, bytes_of(&second))
        .and_then(|response| decode_wire_exact::<DrmGemCreateWire>(&response))
    {
        Ok(created) => created,
        Err(err) => {
            let _ = drm_query(
                &mut card,
                DRM_IOCTL_GEM_CLOSE,
                bytes_of(&DrmGemCloseWire {
                    handle: created_a.handle,
                }),
            );
            return Check::fail(
                "CS_IOCTL_PROTOCOL",
                &format!("destination GEM allocation failed before CS probe: {err}"),
            );
        }
    };

    let submit = RedoxPrivateCsSubmit {
        src_handle: created_a.handle,
        dst_handle: created_b.handle,
        src_offset: 0,
        dst_offset: 0,
        byte_count: 64,
    };

    let result = drm_query(
        &mut card,
        DRM_IOCTL_REDOX_PRIVATE_CS_SUBMIT,
        bytes_of(&submit),
    )
    .and_then(|response| decode_wire_exact::<RedoxPrivateCsSubmitResult>(&response));

    let _ = drm_query(
        &mut card,
        DRM_IOCTL_GEM_CLOSE,
        bytes_of(&DrmGemCloseWire {
            handle: created_b.handle,
        }),
    );
    let _ = drm_query(
        &mut card,
        DRM_IOCTL_GEM_CLOSE,
        bytes_of(&DrmGemCloseWire {
            handle: created_a.handle,
        }),
    );

    match result {
        Ok(response) => Check::pass(
            "CS_IOCTL_PROTOCOL",
            &format!(
                "private CS submit accepted GEM {} -> {} (seqno {})",
                created_a.handle, created_b.handle, response.seqno
            ),
        ),
        Err(err) => Check::fail("CS_IOCTL_PROTOCOL", &err),
    }
}

#[cfg(target_os = "redox")]
fn check_hardware_rendering_ready(report: &Report) -> Check {
    let required = [
        "DRM_DEVICE",
        "GPU_FIRMWARE",
        "MESA_DRI",
        "DISPLAY_MODES",
        "GEM_BUFFER_ALLOCATION",
        "CS_IOCTL_PROTOCOL",
    ];
    let missing = required
        .iter()
        .copied()
        .filter(|name| {
            !report
                .checks
                .iter()
                .any(|check| check.name == *name && check.result == CheckResult::Pass)
        })
        .collect::<Vec<_>>();

    if missing.is_empty() {
        Check::pass(
            "HARDWARE_RENDERING_READY",
            "Phase 5 preflight prerequisites are present; real hardware rendering validation is still pending",
        )
    } else {
        Check::fail(
            "HARDWARE_RENDERING_READY",
            &format!(
                "missing hardware rendering prerequisites: {}",
                missing.join(", ")
            ),
        )
    }
}

fn run() -> Result<(), String> {
    #[cfg(not(target_os = "redox"))]
    {
        if std::env::args().any(|a| a == "-h" || a == "--help") {
            println!("{USAGE}");
            return Err(String::new());
        }
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
        report.add(check_gem_buffer_allocation());
        report.add(check_cs_ioctl_protocol());
        let readiness = check_hardware_rendering_ready(&report);
        report.add(readiness);
        report.print();
        if report.any_failed() {
            return Err("one or more Phase 5 checks failed".to_string());
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
