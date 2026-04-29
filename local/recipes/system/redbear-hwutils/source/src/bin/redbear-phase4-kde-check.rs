// Phase 4 KDE Plasma session check.
// Validates the installed KDE session entry point plus a bounded runtime surface
// exposed by the Red Bear session launcher and helper service.

use std::process;

const PROGRAM: &str = "redbear-phase4-kde-check";
const USAGE: &str = "Usage: redbear-phase4-kde-check [--json]\n\n\
     Phase 4 KDE Plasma session check. Validates KF6 library presence, the\n\
     Red Bear KDE session entry point, KDE session environment capture, core\n\
     helper processes, and a basic panel-readiness proxy.";

#[cfg(target_os = "redox")]
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(target_os = "redox")]
const REDBEAR_KDE_SESSION_ENV_FILE: &str = "redbear-kde-session.env";
#[cfg(target_os = "redox")]
const REDBEAR_KDE_SESSION_READY_FILE: &str = "redbear-kde-session.ready";
#[cfg(target_os = "redox")]
const REDBEAR_KDE_SESSION_PANEL_READY_FILE: &str = "redbear-kde-session.panel-ready";
#[cfg(target_os = "redox")]
const KEY_KF6_LIBRARIES: &[&str] = &[
    "/usr/lib/libKF6CoreAddons.so",
    "/usr/lib/libKF6ConfigCore.so",
    "/usr/lib/libKF6I18n.so",
    "/usr/lib/libKF6WindowSystem.so",
    "/usr/lib/libKF6Notifications.so",
    "/usr/lib/libKF6Service.so",
    "/usr/lib/libKF6WaylandClient.so",
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
            kf6_libs_present: bool,
            kf6_library_versions: bool,
            plasma_binaries_present: bool,
            session_entry: bool,
            session_environment: bool,
            plasmashell_process: bool,
            kded6_process: bool,
            panel_rendering_ready: bool,
            kirigami_available: bool,
            checks: Vec<JsonCheck>,
        }

        let checks = self
            .checks
            .iter()
            .map(|check| JsonCheck {
                name: check.name.clone(),
                result: check.result.label().to_string(),
                detail: check.detail.clone(),
            })
            .collect::<Vec<_>>();

        let report = JsonReport {
            overall_success: !self.any_failed(),
            kf6_libs_present: self.check_passed("KF6_LIBRARIES"),
            kf6_library_versions: self.check_passed("KF6_LIBRARY_VERSIONS"),
            plasma_binaries_present: self.check_passed("PLASMA_BINARIES"),
            session_entry: self.check_passed("SESSION_ENTRY"),
            session_environment: self.check_passed("SESSION_ENVIRONMENT"),
            plasmashell_process: self.check_passed("PLASMASHELL_PROCESS"),
            kded6_process: self.check_passed("KDED6_PROCESS"),
            panel_rendering_ready: self.check_passed("PANEL_RENDERING_READY"),
            kirigami_available: self.check_passed("KIRIGAMI_STATUS"),
            checks,
        };

        if let Err(err) = serde_json::to_writer(std::io::stdout(), &report) {
            eprintln!("{PROGRAM}: failed to serialize JSON: {err}");
        }
    }
}

#[cfg(target_os = "redox")]
#[derive(Clone, Debug)]
struct SessionEnvironment {
    source: String,
    values: BTreeMap<String, String>,
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
fn check_kf6_libraries() -> Check {
    let mut found = 0usize;
    let mut missing = Vec::new();

    for lib in KEY_KF6_LIBRARIES {
        if Path::new(lib).exists() {
            found += 1;
        } else {
            missing.push(lib);
        }
    }

    if found >= 6 {
        if missing.is_empty() {
            Check::pass(
                "KF6_LIBRARIES",
                format!("{found}/{} key KF6 libraries found", KEY_KF6_LIBRARIES.len()),
            )
        } else {
            let preview = missing
                .iter()
                .take(3)
                .map(|path| path.rsplit('/').next().unwrap_or(path))
                .collect::<Vec<_>>()
                .join(", ");
            Check::pass(
                "KF6_LIBRARIES",
                format!("{found}/{} found, missing: {preview}", KEY_KF6_LIBRARIES.len()),
            )
        }
    } else {
        Check::fail(
            "KF6_LIBRARIES",
            format!(
                "only {found}/{} key KF6 libraries found",
                KEY_KF6_LIBRARIES.len()
            ),
        )
    }
}

#[cfg(target_os = "redox")]
fn library_display_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

#[cfg(target_os = "redox")]
fn detect_shared_library_version(path: &Path) -> Result<String, String> {
    let resolved = fs::canonicalize(path)
        .map_err(|err| format!("failed to resolve {}: {err}", path.display()))?;
    let file_name = resolved
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("failed to read resolved file name for {}", path.display()))?;

    file_name
        .rsplit_once(".so.")
        .map(|(_, version)| version.to_string())
        .ok_or_else(|| {
            format!(
                "resolved library {} does not contain a version suffix",
                resolved.display()
            )
        })
}

#[cfg(target_os = "redox")]
fn check_kf6_library_versions() -> Check {
    let mut versions = BTreeMap::<String, Vec<String>>::new();
    let mut unresolved = Vec::new();

    for lib in KEY_KF6_LIBRARIES {
        let lib_path = Path::new(lib);
        if !lib_path.exists() {
            continue;
        }

        match detect_shared_library_version(lib_path) {
            Ok(version) => versions
                .entry(version)
                .or_default()
                .push(library_display_name(lib).to_string()),
            Err(err) => unresolved.push(err),
        }
    }

    let detected = versions.values().map(Vec::len).sum::<usize>();
    if detected >= 6 {
        let mut detail_parts = versions
            .iter()
            .map(|(version, libs)| format!("{version} [{}]", libs.join(", ")))
            .collect::<Vec<_>>();
        if !unresolved.is_empty() {
            detail_parts.push(format!("unresolved: {}", unresolved.join("; ")));
        }

        Check::pass(
            "KF6_LIBRARY_VERSIONS",
            format!(
                "detected version suffixes for {detected}/{} key KF6 libraries: {}",
                KEY_KF6_LIBRARIES.len(),
                detail_parts.join(" | ")
            ),
        )
    } else {
        let detail = if unresolved.is_empty() {
            String::from("no versioned KF6 libraries could be resolved")
        } else {
            unresolved.join("; ")
        };
        Check::fail("KF6_LIBRARY_VERSIONS", detail)
    }
}

#[cfg(target_os = "redox")]
fn check_plasma_binaries() -> Check {
    let required = [
        "/usr/bin/redbear-kde-session",
        "/usr/bin/kwin_wayland_wrapper",
        "/usr/bin/plasmashell",
        "/usr/bin/kded6",
    ];
    let optional: &[&str] = &[];

    let missing_required = required
        .iter()
        .copied()
        .filter(|path| !Path::new(path).exists())
        .collect::<Vec<_>>();
    if !missing_required.is_empty() {
        return Check::fail(
            "PLASMA_BINARIES",
            format!(
                "missing required session binaries: {}",
                missing_required.join(", ")
            ),
        );
    }

    let found_optional = optional
        .iter()
        .copied()
        .filter(|path| Path::new(path).exists())
        .collect::<Vec<_>>();

    Check::pass(
        "PLASMA_BINARIES",
        format!(
            "required session binaries present; optional helpers found: {}/{}",
            found_optional.len(),
            optional.len()
        ),
    )
}

#[cfg(target_os = "redox")]
fn check_session_entry() -> Check {
    let entry = "/usr/bin/redbear-kde-session";
    if Path::new(entry).exists() {
        Check::pass("SESSION_ENTRY", entry)
    } else {
        Check::fail("SESSION_ENTRY", "missing /usr/bin/redbear-kde-session")
    }
}

#[cfg(target_os = "redox")]
fn env_value(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

#[cfg(target_os = "redox")]
fn candidate_state_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/run"),
        PathBuf::from("/run/redbear-display-session"),
    ];

    if let Some(dir) = env_value("XDG_RUNTIME_DIR") {
        let runtime_dir = PathBuf::from(dir);
        if !dirs.contains(&runtime_dir) {
            dirs.push(runtime_dir);
        }
    }

    dirs
}

#[cfg(target_os = "redox")]
fn candidate_state_files(file_name: &str) -> Vec<PathBuf> {
    candidate_state_dirs()
        .into_iter()
        .map(|dir| dir.join(file_name))
        .collect::<Vec<_>>()
}

#[cfg(target_os = "redox")]
fn parse_key_value_file(path: &Path) -> Result<BTreeMap<String, String>, String> {
    let contents = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let mut values = BTreeMap::new();

    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = line.split_once('=') {
            values.insert(key.to_string(), value.to_string());
        }
    }

    Ok(values)
}

#[cfg(target_os = "redox")]
fn load_session_environment() -> Result<SessionEnvironment, String> {
    for path in candidate_state_files(REDBEAR_KDE_SESSION_ENV_FILE) {
        if path.exists() {
            let values = parse_key_value_file(&path)?;
            return Ok(SessionEnvironment {
                source: path.display().to_string(),
                values,
            });
        }
    }

    let mut values = BTreeMap::new();
    for key in [
        "XDG_SESSION_TYPE",
        "XDG_CURRENT_DESKTOP",
        "KDE_FULL_SESSION",
        "QT_PLUGIN_PATH",
        "QT_QPA_PLATFORM_PLUGIN_PATH",
        "QML2_IMPORT_PATH",
        "WAYLAND_DISPLAY",
        "XDG_RUNTIME_DIR",
    ] {
        if let Some(value) = env_value(key) {
            values.insert(key.to_string(), value);
        }
    }

    if values.is_empty() {
        let paths = candidate_state_files(REDBEAR_KDE_SESSION_ENV_FILE)
            .into_iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        Err(format!("no KDE session environment file found in: {paths}"))
    } else {
        Ok(SessionEnvironment {
            source: String::from("current process environment"),
            values,
        })
    }
}

#[cfg(target_os = "redox")]
fn check_required_env_value(
    values: &BTreeMap<String, String>,
    key: &str,
    expected: &str,
) -> Result<(), String> {
    match values.get(key) {
        Some(value) if value == expected => Ok(()),
        Some(value) => Err(format!("{key}={value} (expected {expected})")),
        None => Err(format!("missing {key}")),
    }
}

#[cfg(target_os = "redox")]
fn check_nonempty_env_value(values: &BTreeMap<String, String>, key: &str) -> Result<(), String> {
    match values.get(key) {
        Some(value) if !value.trim().is_empty() => Ok(()),
        Some(_) => Err(format!("{key} is empty")),
        None => Err(format!("missing {key}")),
    }
}

#[cfg(target_os = "redox")]
fn check_session_environment() -> Check {
    match load_session_environment() {
        Ok(session) => {
            let checks = [
                check_required_env_value(&session.values, "XDG_SESSION_TYPE", "wayland"),
                check_required_env_value(&session.values, "XDG_CURRENT_DESKTOP", "KDE"),
                check_required_env_value(&session.values, "KDE_FULL_SESSION", "true"),
                check_nonempty_env_value(&session.values, "QT_PLUGIN_PATH"),
                check_nonempty_env_value(&session.values, "QT_QPA_PLATFORM_PLUGIN_PATH"),
                check_nonempty_env_value(&session.values, "QML2_IMPORT_PATH"),
            ];

            let failures = checks
                .into_iter()
                .filter_map(Result::err)
                .collect::<Vec<_>>();
            if failures.is_empty() {
                Check::pass(
                    "SESSION_ENVIRONMENT",
                    format!("captured KDE session environment from {}", session.source),
                )
            } else {
                Check::fail(
                    "SESSION_ENVIRONMENT",
                    format!(
                        "invalid KDE session environment from {}: {}",
                        session.source,
                        failures.join("; ")
                    ),
                )
            }
        }
        Err(err) => Check::fail("SESSION_ENVIRONMENT", err),
    }
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
fn process_output() -> Result<String, String> {
    run_command("ps", &[], "ps")
}

#[cfg(target_os = "redox")]
fn output_has_process(output: &str, process_name: &str) -> bool {
    output.lines().any(|line| line.contains(process_name))
}

#[cfg(target_os = "redox")]
fn check_required_process(process_name: &str, binary_path: &str, check_name: &str) -> Check {
    if !Path::new(binary_path).exists() {
        return Check::fail(check_name, format!("{binary_path} is not installed"));
    }

    match process_output() {
        Ok(output) => {
            if output_has_process(&output, process_name) {
                Check::pass(check_name, format!("{process_name} appears in ps output"))
            } else {
                Check::fail(
                    check_name,
                    format!("{process_name} is not present in ps output"),
                )
            }
        }
        Err(err) => Check::fail(check_name, err),
    }
}

#[cfg(target_os = "redox")]
fn first_existing_state_file(file_name: &str) -> Option<PathBuf> {
    candidate_state_files(file_name)
        .into_iter()
        .find(|path| path.exists())
}

#[cfg(target_os = "redox")]
fn wayland_socket_from_session_env(values: &BTreeMap<String, String>) -> Option<PathBuf> {
    let runtime_dir = values.get("XDG_RUNTIME_DIR")?;
    let display = values.get("WAYLAND_DISPLAY")?;
    Some(PathBuf::from(runtime_dir).join(display))
}

#[cfg(target_os = "redox")]
fn check_panel_rendering_readiness() -> Check {
    if !Path::new("/usr/bin/plasmashell").exists() {
        return Check::skip(
            "PANEL_RENDERING_READY",
            "plasmashell is not installed, panel readiness cannot be checked",
        );
    }

    if let Some(path) = first_existing_state_file(REDBEAR_KDE_SESSION_PANEL_READY_FILE) {
        return Check::pass(
            "PANEL_RENDERING_READY",
            format!("panel readiness marker present at {}", path.display()),
        );
    }

    let session = match load_session_environment() {
        Ok(session) => session,
        Err(err) => return Check::fail("PANEL_RENDERING_READY", err),
    };
    let socket_path = match wayland_socket_from_session_env(&session.values) {
        Some(path) => path,
        None => {
            return Check::fail(
                "PANEL_RENDERING_READY",
                "session environment is missing XDG_RUNTIME_DIR or WAYLAND_DISPLAY",
            );
        }
    };

    let processes = match process_output() {
        Ok(output) => output,
        Err(err) => return Check::fail("PANEL_RENDERING_READY", err),
    };

    if output_has_process(&processes, "plasmashell") && socket_path.exists() {
        Check::pass(
            "PANEL_RENDERING_READY",
            format!(
                "plasmashell is running and Wayland socket is present at {}",
                socket_path.display()
            ),
        )
    } else {
        Check::fail(
            "PANEL_RENDERING_READY",
            format!(
                "missing panel marker and runtime proxy (plasmashell process/socket {})",
                socket_path.display()
            ),
        )
    }
}

#[cfg(target_os = "redox")]
fn check_session_ready_marker() -> Check {
    if let Some(path) = first_existing_state_file(REDBEAR_KDE_SESSION_READY_FILE) {
        Check::pass(
            "SESSION_READY_MARKER",
            format!("session readiness marker present at {}", path.display()),
        )
    } else {
        let paths = candidate_state_files(REDBEAR_KDE_SESSION_READY_FILE)
            .into_iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        Check::fail(
            "SESSION_READY_MARKER",
            format!("no readiness marker found in: {paths}"),
        )
    }
}

#[cfg(target_os = "redox")]
fn check_kirigami_status() -> Check {
    let kirigami_lib = "/usr/lib/libKF6Kirigami.so";
    if Path::new(kirigami_lib).exists() {
        Check::pass("KIRIGAMI_STATUS", "kirigami library present")
    } else {
        Check::skip(
            "KIRIGAMI_STATUS",
            "kirigami not available (QML stub, requires Qt6Quick)",
        )
    }
}

fn run() -> Result<(), String> {
    #[cfg(not(target_os = "redox"))]
    {
        if std::env::args().any(|arg| arg == "-h" || arg == "--help") {
            println!("{USAGE}");
            return Err(String::new());
        }
        println!("{PROGRAM}: KDE Plasma check requires Redox runtime");
        return Ok(());
    }

    #[cfg(target_os = "redox")]
    {
        let json_mode = parse_args()?;
        let mut report = Report::new(json_mode);

        report.add(check_kf6_libraries());
        report.add(check_kf6_library_versions());
        report.add(check_plasma_binaries());
        report.add(check_session_entry());
        report.add(check_session_environment());
        report.add(check_session_ready_marker());
        report.add(check_required_process(
            "plasmashell",
            "/usr/bin/plasmashell",
            "PLASMASHELL_PROCESS",
        ));
        report.add(check_required_process(
            "kded6",
            "/usr/bin/kded6",
            "KDED6_PROCESS",
        ));
        report.add(check_panel_rendering_readiness());
        report.add(check_kirigami_status());

        report.print();
        if report.any_failed() {
            return Err(String::from("one or more Phase 4 KDE checks failed"));
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

#[cfg(all(test, target_os = "redox"))]
mod tests {
    use super::*;

    #[test]
    fn parse_key_value_file_collects_session_values() {
        let temp_dir = std::env::temp_dir().join("redbear-phase4-kde-check-tests");
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let path = temp_dir.join("env.txt");
        fs::write(
            &path,
            "XDG_SESSION_TYPE=wayland\nKDE_FULL_SESSION=true\nQML2_IMPORT_PATH=/usr/qml\n",
        )
        .expect("env file should be written");

        let parsed = parse_key_value_file(&path).expect("env file should parse");
        assert_eq!(
            parsed.get("XDG_SESSION_TYPE"),
            Some(&String::from("wayland"))
        );
        assert_eq!(parsed.get("KDE_FULL_SESSION"), Some(&String::from("true")));
        assert_eq!(
            parsed.get("QML2_IMPORT_PATH"),
            Some(&String::from("/usr/qml"))
        );
    }

    #[test]
    fn check_required_env_value_matches_expected_value() {
        let mut values = BTreeMap::new();
        values.insert(String::from("XDG_SESSION_TYPE"), String::from("wayland"));
        assert!(check_required_env_value(&values, "XDG_SESSION_TYPE", "wayland").is_ok());
        assert!(check_required_env_value(&values, "XDG_SESSION_TYPE", "x11").is_err());
    }
}
