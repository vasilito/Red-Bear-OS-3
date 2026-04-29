//! Phase 1 DRM/KMS smoke test.
#[cfg(target_os = "redox")]
use std::fs::{self, File};
#[cfg(target_os = "redox")]
use std::io::Read;
#[cfg(target_os = "redox")]
use std::path::Path;
use std::process;

const PROGRAM: &str = "redbear-phase1-drm-check";
const USAGE: &str = "Usage: redbear-phase1-drm-check [--json] [--verbose]\n\n\
     Phase 1 DRM/KMS smoke test. Validates scheme:drm/card0 registration and\n\
     bounded connector/mode queries. Lighter alternative to redbear-drm-display-check.";

#[cfg(target_os = "redox")]
const DRM_SCHEME: &str = "/scheme/drm";
#[cfg(target_os = "redox")]
const DRM_CARD: &str = "/scheme/drm/card0";

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
    verbose: bool,
}

#[cfg(target_os = "redox")]
impl Report {
    fn new(json_mode: bool, verbose: bool) -> Self {
        Report {
            checks: Vec::new(),
            json_mode,
            verbose,
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
            if self.verbose || check.result != CheckResult::Skip {
                let icon = match check.result {
                    CheckResult::Pass => "[PASS]",
                    CheckResult::Fail => "[FAIL]",
                    CheckResult::Skip => "[SKIP]",
                };
                println!("{icon} {}: {}", check.name, check.detail);
            }
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
            drm_scheme: bool,
            card0_present: bool,
            connectors: usize,
            modes: usize,
            checks: Vec<JsonCheck>,
        }

        let drm_scheme = self
            .checks
            .iter()
            .find(|c| c.name == "DRM_SCHEME_REGISTERED")
            .map_or(false, |c| c.result == CheckResult::Pass);

        let card0_present = self
            .checks
            .iter()
            .find(|c| c.name == "CARD0_NODE")
            .map_or(false, |c| c.result == CheckResult::Pass);

        let connectors = self
            .checks
            .iter()
            .find(|c| c.name == "CONNECTOR_ENUM")
            .and_then(|c| {
                c.detail
                    .strip_prefix("found ")
                    .and_then(|s| s.split(' ').next())
                    .and_then(|s| s.parse::<usize>().ok())
            })
            .unwrap_or(0);

        let modes = self
            .checks
            .iter()
            .find(|c| c.name == "MODE_ENUM")
            .and_then(|c| {
                c.detail
                    .strip_prefix("found ")
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
            drm_scheme,
            card0_present,
            connectors,
            modes,
            checks,
        };

        if let Err(err) = serde_json::to_writer(std::io::stdout(), &report) {
            eprintln!("{PROGRAM}: failed to serialize JSON: {err}");
        }
    }
}

#[cfg(target_os = "redox")]
fn parse_args() -> Result<(bool, bool), String> {
    let mut json_mode = false;
    let mut verbose = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => json_mode = true,
            "--verbose" => verbose = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Err(String::new());
            }
            _ => return Err(format!("unsupported argument: {arg}")),
        }
    }

    Ok((json_mode, verbose))
}

#[cfg(target_os = "redox")]
fn check_scheme_registered() -> Check {
    match fs::read_dir(DRM_SCHEME) {
        Ok(_) => Check::pass("DRM_SCHEME_REGISTERED", DRM_SCHEME),
        Err(err) => Check::fail(
            "DRM_SCHEME_REGISTERED",
            &format!("cannot read {DRM_SCHEME}: {err}"),
        ),
    }
}

#[cfg(target_os = "redox")]
fn check_card0_node() -> Check {
    if Path::new(DRM_CARD).exists() {
        Check::pass("CARD0_NODE", DRM_CARD)
    } else {
        Check::fail("CARD0_NODE", &format!("{DRM_CARD} not found"))
    }
}

#[cfg(target_os = "redox")]
fn read_card_node() -> Result<Vec<u8>, String> {
    let mut file =
        File::open(DRM_CARD).map_err(|err| format!("failed to open {DRM_CARD}: {err}"))?;
    let mut buf = vec![0u8; 4096];
    let n = file
        .read(&mut buf)
        .map_err(|err| format!("failed to read {DRM_CARD}: {err}"))?;
    buf.truncate(n);
    Ok(buf)
}

#[cfg(target_os = "redox")]
fn check_card_responds() -> Check {
    match read_card_node() {
        Ok(content) if !content.is_empty() => Check::pass(
            "CARD0_RESPONDS",
            &format!("{} byte(s) from card node", content.len()),
        ),
        Ok(content) => Check::fail("CARD0_RESPONDS", "card node returned empty response"),
        Err(msg) => Check::fail("CARD0_RESPONDS", &msg),
    }
}

#[cfg(target_os = "redox")]
fn enumerate_connectors() -> Check {
    let dir_path = format!("{DRM_CARD}/connectors");
    match fs::read_dir(&dir_path) {
        Ok(entries) => {
            let connectors: Vec<_> = entries.filter_map(|e| e.ok()).collect();
            if connectors.is_empty() {
                Check::fail("CONNECTOR_ENUM", "no connectors found in card0/connectors/")
            } else {
                let preview: Vec<String> = connectors
                    .iter()
                    .take(4)
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect();
                Check::pass(
                    "CONNECTOR_ENUM",
                    &format!("found {}: {}", connectors.len(), preview.join(", ")),
                )
            }
        }
        Err(err) => Check::fail("CONNECTOR_ENUM", &format!("cannot list {dir_path}: {err}")),
    }
}

#[cfg(target_os = "redox")]
fn enumerate_modes() -> Check {
    let dir_path = format!("{DRM_CARD}/modes");
    match fs::read_dir(&dir_path) {
        Ok(entries) => {
            let modes: Vec<_> = entries.filter_map(|e| e.ok()).collect();
            if modes.is_empty() {
                Check::fail("MODE_ENUM", "no modes found in card0/modes/")
            } else {
                let preview: Vec<String> = modes
                    .iter()
                    .take(4)
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect();
                Check::pass(
                    "MODE_ENUM",
                    &format!("found {}: {}", modes.len(), preview.join(", ")),
                )
            }
        }
        Err(err) => Check::fail("MODE_ENUM", &format!("cannot list {dir_path}: {err}")),
    }
}

fn run() -> Result<(), String> {
    #[cfg(not(target_os = "redox"))]
    {
        if std::env::args().any(|a| a == "-h" || a == "--help") {
            println!("{USAGE}");
            return Err(String::new());
        }
        println!("{PROGRAM}: DRM check requires Redox runtime");
        return Ok(());
    }

    #[cfg(target_os = "redox")]
    {
        let (json_mode, verbose) = parse_args()?;
        let mut report = Report::new(json_mode, verbose);

        report.add(check_scheme_registered());
        report.add(check_card0_node());
        report.add(check_card_responds());
        report.add(enumerate_connectors());
        report.add(enumerate_modes());

        report.print();

        if report.any_failed() {
            return Err("one or more DRM checks failed".to_string());
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

    fn parse_args_with<'a>(args: &[&'a str]) -> Result<(bool, bool), String> {
        let mut json_mode = false;
        let mut verbose = false;

        let mut args_iter = args.iter();
        while let Some(arg) = args_iter.next() {
            match *arg {
                "--json" => json_mode = true,
                "--verbose" => verbose = true,
                _ => return Err(format!("unsupported argument: {arg}")),
            }
        }

        Ok((json_mode, verbose))
    }

    #[test]
    fn parse_args_accepts_json_flag() {
        let result = parse_args_with(&["--json"]);
        let (json_mode, _verbose) = result.expect("parse_args should succeed");
        assert!(json_mode, "json_mode should be true with --json flag");
    }

    #[test]
    fn parse_args_accepts_verbose_flag() {
        let result = parse_args_with(&["--verbose"]);
        let (_json_mode, verbose) = result.expect("parse_args should succeed");
        assert!(verbose, "verbose should be true with --verbose flag");
    }

    #[test]
    fn parse_args_rejects_unknown() {
        let result = parse_args_with(&["--unknown-flag"]);
        assert!(result.is_err(), "parse_args should reject unknown argument");
    }

    #[test]
    fn parse_args_default_values() {
        let result = parse_args_with(&[]);
        let (json_mode, verbose) = result.expect("parse_args should succeed");
        assert!(!json_mode, "json_mode should be false by default");
        assert!(!verbose, "verbose should be false by default");
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
