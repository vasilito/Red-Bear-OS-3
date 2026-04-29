// Phase 5 GPU command-submission validation checker.
// Validates DRM command-submission protocol reachability over /scheme/drm/card0.
// Does NOT claim real hardware render validation yet.

use std::process;

const PROGRAM: &str = "redbear-phase5-cs-check";
const USAGE: &str = "Usage: redbear-phase5-cs-check [--json]\n\n\
     Phase 5 GPU command-submission validation. Probes DRM private CS ioctls,\n\
     PRIME buffer sharing, GEM allocation, and fence/wait support. Real\n\
     hardware rendering validation is still pending.";

#[cfg(target_os = "redox")]
const DRM_IOCTL_BASE: usize = 0x00A0;
#[cfg(target_os = "redox")]
const DRM_IOCTL_GEM_CREATE: usize = DRM_IOCTL_BASE + 26;
#[cfg(target_os = "redox")]
const DRM_IOCTL_GEM_CLOSE: usize = DRM_IOCTL_BASE + 27;
#[cfg(target_os = "redox")]
const DRM_IOCTL_PRIME_HANDLE_TO_FD: usize = DRM_IOCTL_BASE + 29;
#[cfg(target_os = "redox")]
const DRM_IOCTL_PRIME_FD_TO_HANDLE: usize = DRM_IOCTL_BASE + 30;
#[cfg(target_os = "redox")]
const DRM_IOCTL_REDOX_PRIVATE_CS_SUBMIT: usize = DRM_IOCTL_BASE + 31;
#[cfg(target_os = "redox")]
const DRM_IOCTL_REDOX_PRIVATE_CS_WAIT: usize = DRM_IOCTL_BASE + 32;

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
        Self {
            name: name.to_string(),
            result: CheckResult::Pass,
            detail: detail.to_string(),
        }
    }

    fn fail(name: &str, detail: &str) -> Self {
        Self {
            name: name.to_string(),
            result: CheckResult::Fail,
            detail: detail.to_string(),
        }
    }

    fn skip(name: &str, detail: &str) -> Self {
        Self {
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
            command_submission_protocol: bool,
            prime_buffer_sharing: bool,
            gem_buffer_allocation: bool,
            fence_sync_support: bool,
            hardware_validation_pending: bool,
            checks: Vec<JsonCheck>,
        }

        let check_passed = |name: &str| {
            self.checks
                .iter()
                .find(|check| check.name == name)
                .is_some_and(|check| check.result == CheckResult::Pass)
        };

        let checks = self
            .checks
            .iter()
            .map(|check| JsonCheck {
                name: check.name.clone(),
                result: check.result.label().to_string(),
                detail: check.detail.clone(),
            })
            .collect::<Vec<_>>();

        if let Err(err) = serde_json::to_writer(
            std::io::stdout(),
            &JsonReport {
                command_submission_protocol: check_passed("CS_IOCTL_PROTOCOL"),
                prime_buffer_sharing: check_passed("PRIME_BUFFER_SHARING"),
                gem_buffer_allocation: check_passed("GEM_BUFFER_ALLOCATION"),
                fence_sync_support: check_passed("FENCE_SYNC_SUPPORT"),
                hardware_validation_pending: true,
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
struct DrmPrimeHandleToFdWire {
    handle: u32,
    flags: u32,
}

#[cfg(target_os = "redox")]
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmPrimeHandleToFdResponseWire {
    fd: i32,
    pad: u32,
}

#[cfg(target_os = "redox")]
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmPrimeFdToHandleWire {
    fd: i32,
    pad: u32,
}

#[cfg(target_os = "redox")]
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmPrimeFdToHandleResponseWire {
    handle: u32,
    pad: u32,
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
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct RedoxPrivateCsWait {
    seqno: u64,
    timeout_ns: u64,
}

#[cfg(target_os = "redox")]
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct RedoxPrivateCsWaitResult {
    completed: u8,
    pad: [u8; 7],
    completed_seqno: u64,
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
fn open_drm_card(path: &str) -> Result<std::fs::File, String> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|err| format!("failed to open {path}: {err}"))
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
fn close_gem(file: &mut std::fs::File, handle: u32) {
    let request = DrmGemCloseWire { handle };
    let _ = drm_query(file, DRM_IOCTL_GEM_CLOSE, bytes_of(&request));
}

#[cfg(target_os = "redox")]
fn run_redox(json_mode: bool) -> Result<(), String> {
    let mut report = Report::new(json_mode);
    let card_path = "/scheme/drm/card0";

    if !std::path::Path::new(card_path).exists() {
        report.add(Check::fail(
            "CS_IOCTL_PROTOCOL",
            "/scheme/drm/card0 missing; cannot probe command submission",
        ));
        report.add(Check::skip(
            "GEM_BUFFER_ALLOCATION",
            "blocked: DRM card is unavailable",
        ));
        report.add(Check::skip(
            "PRIME_BUFFER_SHARING",
            "blocked: DRM card is unavailable",
        ));
        report.add(Check::skip(
            "FENCE_SYNC_SUPPORT",
            "blocked: DRM card is unavailable",
        ));
        report.add(Check::skip(
            "HARDWARE_VALIDATION_PENDING",
            "real hardware rendering validation still requires bare-metal evidence",
        ));
        report.print();
        return Err("one or more Phase 5 CS checks failed".to_string());
    }

    let mut exporter = match open_drm_card(card_path) {
        Ok(file) => file,
        Err(err) => {
            report.add(Check::fail("CS_IOCTL_PROTOCOL", &err));
            report.add(Check::skip(
                "GEM_BUFFER_ALLOCATION",
                "blocked: DRM card could not be opened",
            ));
            report.add(Check::skip(
                "PRIME_BUFFER_SHARING",
                "blocked: DRM card could not be opened",
            ));
            report.add(Check::skip(
                "FENCE_SYNC_SUPPORT",
                "blocked: DRM card could not be opened",
            ));
            report.add(Check::skip(
                "HARDWARE_VALIDATION_PENDING",
                "real hardware rendering validation still requires bare-metal evidence",
            ));
            report.print();
            return Err("one or more Phase 5 CS checks failed".to_string());
        }
    };

    let mut importer = match open_drm_card(card_path) {
        Ok(file) => file,
        Err(err) => {
            report.add(Check::fail(
                "CS_IOCTL_PROTOCOL",
                &format!("opened exporter but importer failed: {err}"),
            ));
            report.add(Check::skip(
                "GEM_BUFFER_ALLOCATION",
                "blocked: second DRM handle could not be opened",
            ));
            report.add(Check::skip(
                "PRIME_BUFFER_SHARING",
                "blocked: second DRM handle could not be opened",
            ));
            report.add(Check::skip(
                "FENCE_SYNC_SUPPORT",
                "blocked: second DRM handle could not be opened",
            ));
            report.add(Check::skip(
                "HARDWARE_VALIDATION_PENDING",
                "real hardware rendering validation still requires bare-metal evidence",
            ));
            report.print();
            return Err("one or more Phase 5 CS checks failed".to_string());
        }
    };

    let mut exporter_handle = None;
    let mut importer_src_handle = None;
    let mut importer_dst_handle = None;

    let create_exporter = DrmGemCreateWire {
        size: 4096,
        ..DrmGemCreateWire::default()
    };
    match drm_query(
        &mut exporter,
        DRM_IOCTL_GEM_CREATE,
        bytes_of(&create_exporter),
    )
    .and_then(|response| decode_wire_exact::<DrmGemCreateWire>(&response))
    {
        Ok(created) => {
            exporter_handle = Some(created.handle);
            report.add(Check::pass(
                "GEM_BUFFER_ALLOCATION",
                &format!(
                    "allocated exporter GEM handle {} (4096 bytes)",
                    created.handle
                ),
            ));
        }
        Err(err) => {
            report.add(Check::fail("GEM_BUFFER_ALLOCATION", &err));
            report.add(Check::skip(
                "PRIME_BUFFER_SHARING",
                "blocked: GEM allocation failed",
            ));
            report.add(Check::skip(
                "CS_IOCTL_PROTOCOL",
                "blocked: GEM allocation failed",
            ));
            report.add(Check::skip(
                "FENCE_SYNC_SUPPORT",
                "blocked: GEM allocation failed",
            ));
            report.add(Check::skip(
                "HARDWARE_VALIDATION_PENDING",
                "real hardware rendering validation still requires bare-metal evidence",
            ));
            report.print();
            return Err("one or more Phase 5 CS checks failed".to_string());
        }
    }

    if let Some(handle) = exporter_handle {
        let export = DrmPrimeHandleToFdWire { handle, flags: 0 };
        let prime_result = drm_query(
            &mut exporter,
            DRM_IOCTL_PRIME_HANDLE_TO_FD,
            bytes_of(&export),
        )
        .and_then(|response| decode_wire_exact::<DrmPrimeHandleToFdResponseWire>(&response))
        .and_then(|exported| {
            if exported.fd < 0 {
                return Err(format!(
                    "PRIME export returned invalid token {} for GEM {}",
                    exported.fd, handle
                ));
            }

            let import = DrmPrimeFdToHandleWire {
                fd: exported.fd,
                pad: 0,
            };
            drm_query(
                &mut importer,
                DRM_IOCTL_PRIME_FD_TO_HANDLE,
                bytes_of(&import),
            )
            .and_then(|response| decode_wire_exact::<DrmPrimeFdToHandleResponseWire>(&response))
            .map(|imported| (exported.fd, imported.handle))
        });

        match prime_result {
            Ok((token, imported_handle)) => {
                importer_src_handle = Some(imported_handle);
                report.add(Check::pass(
                    "PRIME_BUFFER_SHARING",
                    &format!(
                        "export token {} imported as GEM handle {} on a second DRM fd",
                        token, imported_handle
                    ),
                ));
            }
            Err(err) => {
                report.add(Check::fail("PRIME_BUFFER_SHARING", &err));
                report.add(Check::skip(
                    "CS_IOCTL_PROTOCOL",
                    "blocked: PRIME import/export failed",
                ));
                report.add(Check::skip(
                    "FENCE_SYNC_SUPPORT",
                    "blocked: PRIME import/export failed",
                ));
                report.add(Check::skip(
                    "HARDWARE_VALIDATION_PENDING",
                    "real hardware rendering validation still requires bare-metal evidence",
                ));
                close_gem(&mut exporter, handle);
                report.print();
                return Err("one or more Phase 5 CS checks failed".to_string());
            }
        }
    }

    let create_importer = DrmGemCreateWire {
        size: 4096,
        ..DrmGemCreateWire::default()
    };
    match drm_query(
        &mut importer,
        DRM_IOCTL_GEM_CREATE,
        bytes_of(&create_importer),
    )
    .and_then(|response| decode_wire_exact::<DrmGemCreateWire>(&response))
    {
        Ok(created) => importer_dst_handle = Some(created.handle),
        Err(err) => {
            report.add(Check::fail(
                "CS_IOCTL_PROTOCOL",
                &format!("secondary GEM allocation for CS submit failed: {err}"),
            ));
            report.add(Check::skip(
                "FENCE_SYNC_SUPPORT",
                "blocked: no destination GEM for CS submit",
            ));
            report.add(Check::skip(
                "HARDWARE_VALIDATION_PENDING",
                "real hardware rendering validation still requires bare-metal evidence",
            ));
            if let Some(handle) = importer_src_handle {
                close_gem(&mut importer, handle);
            }
            if let Some(handle) = exporter_handle {
                close_gem(&mut exporter, handle);
            }
            report.print();
            return Err("one or more Phase 5 CS checks failed".to_string());
        }
    }

    let submit_result = match (importer_src_handle, importer_dst_handle) {
        (Some(src_handle), Some(dst_handle)) => {
            let submit = RedoxPrivateCsSubmit {
                src_handle,
                dst_handle,
                src_offset: 0,
                dst_offset: 0,
                byte_count: 64,
            };
            drm_query(
                &mut importer,
                DRM_IOCTL_REDOX_PRIVATE_CS_SUBMIT,
                bytes_of(&submit),
            )
            .and_then(|response| decode_wire_exact::<RedoxPrivateCsSubmitResult>(&response))
            .map(|result| (src_handle, dst_handle, result.seqno))
        }
        _ => Err("command submission prerequisites were incomplete".to_string()),
    };

    match submit_result {
        Ok((src_handle, dst_handle, seqno)) => {
            report.add(Check::pass(
                "CS_IOCTL_PROTOCOL",
                &format!(
                    "private CS submit accepted shared GEM {} -> local GEM {} (seqno {})",
                    src_handle, dst_handle, seqno
                ),
            ));

            let wait = RedoxPrivateCsWait {
                seqno,
                timeout_ns: 0,
            };
            match drm_query(
                &mut importer,
                DRM_IOCTL_REDOX_PRIVATE_CS_WAIT,
                bytes_of(&wait),
            )
            .and_then(|response| decode_wire_exact::<RedoxPrivateCsWaitResult>(&response))
            {
                Ok(wait_result) => {
                    let completed = match wait_result.completed {
                        0 => false,
                        1 => true,
                        value => {
                            report.add(Check::fail(
                                "FENCE_SYNC_SUPPORT",
                                &format!(
                                    "wait ioctl returned invalid completion flag {} for seqno {}",
                                    value, seqno
                                ),
                            ));
                            report.add(Check::skip(
                                "HARDWARE_VALIDATION_PENDING",
                                "protocol-level CS proof exists, but real hardware rendering validation is still pending",
                            ));
                            report.print();
                            return Err("one or more Phase 5 CS checks failed".to_string());
                        }
                    };
                    report.add(Check::pass(
                        "FENCE_SYNC_SUPPORT",
                        &format!(
                            "bounded wait ioctl responded for seqno {} (completed={}, completed_seqno={}); real sync-object validation is still pending",
                            seqno, completed, wait_result.completed_seqno
                        ),
                    ));
                }
                Err(err) => {
                    report.add(Check::fail("FENCE_SYNC_SUPPORT", &err));
                }
            }
        }
        Err(err) => {
            report.add(Check::fail("CS_IOCTL_PROTOCOL", &err));
            report.add(Check::skip(
                "FENCE_SYNC_SUPPORT",
                "blocked: command submission ioctl failed",
            ));
        }
    }

    if let Some(handle) = importer_dst_handle {
        close_gem(&mut importer, handle);
    }
    if let Some(handle) = importer_src_handle {
        close_gem(&mut importer, handle);
    }
    if let Some(handle) = exporter_handle {
        close_gem(&mut exporter, handle);
    }

    report.add(Check::skip(
        "HARDWARE_VALIDATION_PENDING",
        "protocol-level CS proof exists, but real hardware rendering validation is still pending",
    ));
    report.print();

    if report.any_failed() {
        return Err("one or more Phase 5 CS checks failed".to_string());
    }

    Ok(())
}

fn run() -> Result<(), String> {
    #[cfg(not(target_os = "redox"))]
    {
        if std::env::args().any(|arg| arg == "-h" || arg == "--help") {
            println!("{USAGE}");
            return Err(String::new());
        }
        println!("{PROGRAM}: CS check requires Redox runtime");
        Ok(())
    }

    #[cfg(target_os = "redox")]
    {
        let json_mode = parse_args()?;
        run_redox(json_mode)
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
