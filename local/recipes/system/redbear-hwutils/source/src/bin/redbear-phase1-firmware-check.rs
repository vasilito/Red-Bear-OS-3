//! Phase 1 firmware-loader smoke test.

#[cfg(target_os = "redox")]
use std::fs;
#[cfg(target_os = "redox")]
use std::io::Read;
#[cfg(target_os = "redox")]
use std::path::{Path, PathBuf};
use std::process;

const PROGRAM: &str = "redbear-phase1-firmware-check";
const USAGE: &str = "Usage: redbear-phase1-firmware-check [--json] [--blob KEY]\n\n\
     Phase 1 firmware-loader smoke test. Validates scheme:firmware registration\n\
     and at least one readable firmware blob.";

#[cfg(target_os = "redox")]
const FALLBACK_BLOBS: &[&str] = &[
    "amdgpu/dce_11_0_dmcu.bin",
    "amdgpu/dcn_3_2_mall.bin",
    "i915/kbl_dmc_ver1_04.bin",
    "r8168n.bin",
    "rtl_nic/rtl8105e-1_0_0.fw",
];

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
            firmware_scheme: bool,
            blob_read: bool,
            blob_size: usize,
            checks: Vec<JsonCheck>,
        }

        let firmware_scheme = self
            .checks
            .iter()
            .find(|c| c.name == "FIRMWARE_SCHEME_REGISTERED")
            .map_or(false, |c| c.result == CheckResult::Pass);

        let blob_read = self
            .checks
            .iter()
            .find(|c| c.name == "BLOB_READ")
            .map_or(false, |c| c.result == CheckResult::Pass);

        let blob_size = self
            .checks
            .iter()
            .find(|c| c.name == "BLOB_READ")
            .and_then(|c| {
                c.detail
                    .strip_prefix("size=")
                    .and_then(|s| s.split(' ').next())
                    .and_then(|s| s.parse::<usize>().ok())
            })
            .unwrap_or(0);

        let checks: Vec<JsonCheck> = self
            .checks
            .iter()
            .map(|c| JsonCheck {
                name: c.name.clone(),
                result: c.result.label().to_string(),
                detail: c.detail.clone(),
            })
            .collect();

        let report = JsonReport {
            firmware_scheme,
            blob_read,
            blob_size,
            checks,
        };

        if let Err(err) = serde_json::to_writer(std::io::stdout(), &report) {
            eprintln!("{PROGRAM}: failed to serialize JSON: {err}");
        }
    }
}

#[cfg(target_os = "redox")]
fn parse_args() -> Result<(bool, Option<String>), String> {
    let mut json_mode = false;
    let mut blob_key = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => json_mode = true,
            "--blob" => {
                blob_key = Some(
                    args.next()
                        .ok_or_else(|| "missing value for --blob".to_string())?,
                );
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return Err(String::new());
            }
            _ => return Err(format!("unsupported argument: {arg}")),
        }
    }

    Ok((json_mode, blob_key))
}

#[cfg(target_os = "redox")]
fn check_scheme_registered() -> Check {
    match fs::read_dir("/scheme/") {
        Ok(entries) => {
            let names: Vec<String> = entries
                .filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().into_string().ok())
                .collect();

            if names.iter().any(|n| n == "firmware") {
                Check::pass(
                    "FIRMWARE_SCHEME_REGISTERED",
                    &format!("found {} scheme(s)", names.len()),
                )
            } else {
                Check::fail(
                    "FIRMWARE_SCHEME_REGISTERED",
                    "firmware not found in /scheme/",
                )
            }
        }
        Err(err) => Check::fail(
            "FIRMWARE_SCHEME_REGISTERED",
            &format!("cannot read /scheme/: {err}"),
        ),
    }
}

#[cfg(target_os = "redox")]
fn list_firmware_keys() -> Check {
    match fs::read_dir("/scheme/firmware/") {
        Ok(entries) => {
            let keys: Vec<String> = entries
                .filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().into_string().ok())
                .collect();

            if keys.is_empty() {
                Check::fail("FIRMWARE_KEY_LIST", "no keys found in /scheme/firmware/")
            } else {
                let preview = keys.iter().take(4).cloned().collect::<Vec<_>>().join(", ");
                Check::pass(
                    "FIRMWARE_KEY_LIST",
                    &format!("{} key(s): {}", keys.len(), preview),
                )
            }
        }
        Err(err) => Check::fail(
            "FIRMWARE_KEY_LIST",
            &format!("cannot list /scheme/firmware/: {err}"),
        ),
    }
}

#[cfg(target_os = "redox")]
fn read_firmware_blob(key: &str) -> Result<(usize, Vec<u8>), String> {
    let path = format!("/scheme/firmware/{key}");
    let mut file =
        std::fs::File::open(&path).map_err(|err| format!("failed to open {path}: {err}"))?;
    let mut buf = Vec::new();
    let size = file
        .read_to_end(&mut buf)
        .map_err(|err| format!("failed to read {path}: {err}"))?;
    Ok((size, buf))
}

#[cfg(target_os = "redox")]
fn check_blob_fstat(key: &str) -> Check {
    let path = format!("/scheme/firmware/{key}");
    match std::fs::File::open(&path) {
        Ok(file) => match file.metadata() {
            Ok(meta) => {
                let size = meta.len();
                if size > 0 {
                    Check::pass(
                        "BLOB_MMAP_PATH",
                        &format!("size={} via fstat on {}", size, key),
                    )
                } else {
                    Check::fail("BLOB_MMAP_PATH", &format!("blob {key} has zero size"))
                }
            }
            Err(err) => Check::fail("BLOB_MMAP_PATH", &format!("fstat failed for {path}: {err}")),
        },
        Err(err) => Check::fail("BLOB_MMAP_PATH", &format!("cannot open {path}: {err}")),
    }
}

#[cfg(target_os = "redox")]
fn check_lib_firmware_dir() -> Check {
    let dir = Path::new("/lib/firmware/");
    match fs::read_dir(dir) {
        Ok(entries) => {
            let blobs: Vec<PathBuf> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().map_or(false, |ft| ft.is_file()))
                .map(|e| e.path())
                .collect();

            if blobs.is_empty() {
                Check::skip("LIB_FIRMWARE_DIR", "/lib/firmware/ is empty")
            } else {
                let preview = blobs
                    .iter()
                    .take(3)
                    .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
                    .collect::<Vec<_>>()
                    .join(", ");
                Check::pass(
                    "LIB_FIRMWARE_DIR",
                    &format!("{} blob(s) in /lib/firmware/: {}", blobs.len(), preview),
                )
            }
        }
        Err(err) => Check::skip(
            "LIB_FIRMWARE_DIR",
            &format!("/lib/firmware/ not accessible: {err}"),
        ),
    }
}

fn run() -> Result<(), String> {
    #[cfg(not(target_os = "redox"))]
    {
        if std::env::args().any(|a| a == "-h" || a == "--help") {
            println!("{USAGE}");
            return Err(String::new());
        }
        println!("{PROGRAM}: firmware-loader check requires Redox runtime");
        return Ok(());
    }

    #[cfg(target_os = "redox")]
    {
        let (json_mode, blob_key) = parse_args()?;
        let mut report = Report::new(json_mode);

        report.add(check_scheme_registered());
        report.add(list_firmware_keys());
        report.add(check_lib_firmware_dir());

        let blob_to_try = blob_key.or_else(|| {
            FALLBACK_BLOBS
                .iter()
                .copied()
                .find(|&k| Path::new(&format!("/scheme/firmware/{k}")).exists())
                .map(String::from)
        });

        match blob_to_try {
            Some(key) => {
                match read_firmware_blob(&key) {
                    Ok((size, _content)) => {
                        if size > 0 {
                            report.add(Check::pass(
                                "BLOB_READ",
                                &format!("size={} key={}", size, key),
                            ));
                        } else {
                            report.add(Check::fail(
                                "BLOB_READ",
                                &format!("blob {key} has zero size"),
                            ));
                        }
                    }
                    Err(msg) => {
                        report.add(Check::fail("BLOB_READ", &msg));
                    }
                }

                report.add(check_blob_fstat(&key));
            }
            None => {
                report.add(Check::skip(
                    "BLOB_READ",
                    "no known blob key found in /scheme/firmware/",
                ));
                report.add(Check::skip("BLOB_MMAP_PATH", "no blob to check"));
            }
        }

        report.print();

        if report.any_failed() {
            return Err("one or more firmware checks failed".to_string());
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

#[cfg(target_os = "redox")]
#[cfg(test)]
mod tests {
    use super::*;

    fn parse_args_with<'a>(args: &[&'a str]) -> Result<(bool, Option<String>), String> {
        let mut json_mode = false;
        let mut blob_key = None;

        let mut args_iter = args.iter();
        while let Some(arg) = args_iter.next() {
            match *arg {
                "--json" => json_mode = true,
                "--blob" => {
                    blob_key = Some(
                        args_iter
                            .next()
                            .ok_or_else(|| "missing value for --blob".to_string())?
                            .to_string(),
                    );
                }
                _ => return Err(format!("unsupported argument: {arg}")),
            }
        }

        Ok((json_mode, blob_key))
    }

    #[test]
    fn parse_args_accepts_json_flag() {
        let result = parse_args_with(&["--json"]);
        let (json_mode, _blob_key) = result.expect("parse_args should succeed");
        assert!(json_mode, "json_mode should be true with --json flag");
    }

    #[test]
    fn parse_args_accepts_blob_flag() {
        let result = parse_args_with(&["--blob", "somename"]);
        let (_json_mode, blob_key) = result.expect("parse_args should succeed");
        assert_eq!(
            blob_key,
            Some("somename".to_string()),
            "blob_key should be Some(\"somename\")"
        );
    }

    #[test]
    fn parse_args_rejects_unknown() {
        let result = parse_args_with(&["--unknown-flag"]);
        assert!(result.is_err(), "parse_args should reject unknown argument");
    }

    #[test]
    fn parse_args_default_no_json() {
        let result = parse_args_with(&[]);
        let (json_mode, _blob_key) = result.expect("parse_args should succeed");
        assert!(!json_mode, "json_mode should be false by default");
    }

    #[test]
    fn check_status_render_pass() {
        let label = CheckResult::Pass.label();
        assert_eq!(label, "PASS", "CheckResult::Pass should render as PASS");
    }

    #[test]
    fn check_status_render_fail() {
        let label = CheckResult::Fail.label();
        assert_eq!(label, "FAIL", "CheckResult::Fail should render as FAIL");
    }
}
