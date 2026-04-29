// Phase 4 KDE Plasma preflight check.
// Validates KF6 library presence, plasma binaries, and session entry points.
// Does NOT validate real KDE Plasma session behavior (blocked on Qt6Quick/QML + real KWin).

use std::process;

const PROGRAM: &str = "redbear-phase4-kde-check";
const USAGE: &str = "Usage: redbear-phase4-kde-check [--json]\n\n\
     Phase 4 KDE Plasma preflight check. Validates KF6 library and plasma binary\n\
     presence. Does NOT validate real KDE session behavior (gated on Qt6Quick/QML).";

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
            kf6_libs_present: bool, plasma_binaries_present: bool,
            session_entry: bool, kirigami_available: bool, checks: Vec<JsonCheck>,
        }
        let kf6_libs = self.checks.iter().find(|c| c.name == "KF6_LIBRARIES").map_or(false, |c| c.result == CheckResult::Pass);
        let plasma_bins = self.checks.iter().find(|c| c.name == "PLASMA_BINARIES").map_or(false, |c| c.result == CheckResult::Pass);
        let session_entry = self.checks.iter().find(|c| c.name == "SESSION_ENTRY").map_or(false, |c| c.result == CheckResult::Pass);
        let kirigami = self.checks.iter().find(|c| c.name == "KIRIGAMI_STATUS").map_or(false, |c| c.result == CheckResult::Pass);
        let checks: Vec<JsonCheck> = self.checks.iter().map(|c| JsonCheck {
            name: c.name.clone(), result: c.result.label().to_string(), detail: c.detail.clone(),
        }).collect();
        if let Err(err) = serde_json::to_writer(std::io::stdout(), &JsonReport { kf6_libs_present: kf6_libs, plasma_binaries_present: plasma_bins, session_entry, kirigami_available: kirigami, checks }) {
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
fn check_kf6_libraries() -> Check {
    let key_libs = [
        "/usr/lib/libKF6CoreAddons.so", "/usr/lib/libKF6ConfigCore.so",
        "/usr/lib/libKF6I18n.so", "/usr/lib/libKF6WindowSystem.so",
        "/usr/lib/libKF6Notifications.so", "/usr/lib/libKF6Service.so",
        "/usr/lib/libKF6WaylandClient.so",
    ];
    let mut found = 0usize;
    let mut missing = Vec::new();
    for lib in key_libs {
        if std::path::Path::new(lib).exists() {
            found += 1;
        } else {
            missing.push(lib);
        }
    }
    if found >= 6 {
        let preview: Vec<_> = missing.iter().take(3).map(|s| s.rsplit('/').next().unwrap_or(s)).collect();
        if missing.is_empty() {
            Check::pass("KF6_LIBRARIES", &format!("{}/{} key KF6 libs found", found, key_libs.len()))
        } else {
            Check::pass("KF6_LIBRARIES", &format!("{}/{} found, missing: {}", found, key_libs.len(), preview.join(", ")))
        }
    } else {
        Check::fail("KF6_LIBRARIES", &format!("only {}/{} key KF6 libs found", found, key_libs.len()))
    }
}

#[cfg(target_os = "redox")]
fn check_plasma_binaries() -> Check {
    let bins = ["/usr/bin/plasmashell", "/usr/bin/systemsettings", "/usr/bin/kwin_wayland_wrapper"];
    let mut found = 0usize;
    for bin in bins {
        if std::path::Path::new(bin).exists() { found += 1; }
    }
    if found >= 2 {
        Check::pass("PLASMA_BINARIES", &format!("{}/{} plasma binaries present", found, bins.len()))
    } else if found == 1 {
        Check::fail("PLASMA_BINARIES", &format!("only {}/{} plasma binaries present", found, bins.len()))
    } else {
        Check::fail("PLASMA_BINARIES", "no plasma binaries found")
    }
}

#[cfg(target_os = "redox")]
fn check_session_entry() -> Check {
    let entries = ["/usr/bin/startplasma-wayland", "/usr/lib/plasma-session"];
    for e in entries {
        if std::path::Path::new(e).exists() {
            return Check::pass("SESSION_ENTRY", e);
        }
    }
    Check::fail("SESSION_ENTRY", "no KDE session entry point found")
}

#[cfg(target_os = "redox")]
fn check_kirigami_status() -> Check {
    let kirigami_lib = "/usr/lib/libKF6Kirigami.so";
    if std::path::Path::new(kirigami_lib).exists() {
        Check::pass("KIRIGAMI_STATUS", "kirigami library present")
    } else {
        Check::skip("KIRIGAMI_STATUS", "kirigami not available (QML stub, requires Qt6Quick)")
    }
}

fn run() -> Result<(), String> {
    #[cfg(not(target_os = "redox"))]
    {
        if std::env::args().any(|a| a == "-h" || a == "--help") { println!("{USAGE}"); return Err(String::new()); }
        println!("{PROGRAM}: KDE Plasma check requires Redox runtime");
        return Ok(());
    }
    #[cfg(target_os = "redox")]
    {
        let json_mode = parse_args()?;
        let mut report = Report::new(json_mode);
        report.add(check_kf6_libraries());
        report.add(check_plasma_binaries());
        report.add(check_session_entry());
        report.add(check_kirigami_status());
        report.print();
        if report.any_failed() { return Err("one or more Phase 4 checks failed".to_string()); }
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
