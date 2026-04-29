use std::process;

#[cfg(target_os = "redox")]
use std::{fs, io::Read};

use serde_json::json;

const PROGRAM: &str = "redbear-phase1-udev-check";
const USAGE: &str = "Usage: redbear-phase1-udev-check [--keyboard] [--pointer] [--drm] [--json]\n\nValidate bounded udev-shim device enumeration inside the Red Bear guest.";

#[cfg(target_os = "redox")]
const MAX_DEVICE_INFO_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
struct Config {
    keyboard: bool,
    pointer: bool,
    drm: bool,
    json: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Report {
    udev_scheme: bool,
    keyboard_count: usize,
    pointer_count: usize,
    drm_count: usize,
}

#[cfg(target_os = "redox")]
#[derive(Clone, Debug, Eq, PartialEq)]
enum CheckStatus {
    Pass(String),
    Fail(String),
    Skip,
}

#[cfg(target_os = "redox")]
impl CheckStatus {
    fn render(&self, label: &str) {
        match self {
            Self::Pass(detail) => println!("PASS {label}: {detail}"),
            Self::Fail(detail) => println!("FAIL {label}: {detail}"),
            Self::Skip => println!("SKIP {label}: not requested"),
        }
    }
}

fn main() {
    match parse_args(std::env::args()) {
        Ok(config) => match run(&config) {
            Ok(success) => process::exit(if success { 0 } else { 1 }),
            Err(err) => {
                eprintln!("{PROGRAM}: {err}");
                process::exit(1);
            }
        },
        Err(err) if err.is_empty() => process::exit(0),
        Err(err) => {
            eprintln!("{PROGRAM}: {err}");
            process::exit(1);
        }
    }
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Config, String> {
    let mut keyboard = false;
    let mut pointer = false;
    let mut drm = false;
    let mut json = false;

    let mut args = args.into_iter();
    let _program = args.next();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--keyboard" => keyboard = true,
            "--pointer" => pointer = true,
            "--drm" => drm = true,
            "--json" => json = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Err(String::new());
            }
            _ => return Err(format!("unsupported argument: {arg}")),
        }
    }

    if !keyboard && !pointer && !drm {
        keyboard = true;
        pointer = true;
        drm = true;
    }

    Ok(Config {
        keyboard,
        pointer,
        drm,
        json,
    })
}

fn run(config: &Config) -> Result<bool, String> {
    #[cfg(not(target_os = "redox"))]
    {
        let report = Report::default();
        if config.json {
            let payload = serde_json::to_string(&json!({
                "udev_scheme": report.udev_scheme,
                "keyboard_count": report.keyboard_count,
                "pointer_count": report.pointer_count,
                "drm_count": report.drm_count,
            }))
            .map_err(|err| format!("failed to serialize JSON output: {err}"))?;
            eprintln!("udev-shim check requires Redox runtime");
            println!("{payload}");
        } else {
            println!("udev-shim check requires Redox runtime");
        }
        Ok(true)
    }

    #[cfg(target_os = "redox")]
    {
        run_redox(config)
    }
}

#[cfg(target_os = "redox")]
fn run_redox(config: &Config) -> Result<bool, String> {
    let udev_scheme_present = fs::metadata("/scheme/udev").is_ok();
    let device_entries = match list_dir_names("/scheme/udev/devices") {
        Ok(entries) => entries,
        Err(_) => Vec::new(),
    };

    let report = Report {
        udev_scheme: udev_scheme_present,
        keyboard_count: count_devices_with_property(&device_entries, "ID_INPUT_KEYBOARD", "1"),
        pointer_count: count_devices_with_property(&device_entries, "ID_INPUT_MOUSE", "1"),
        drm_count: count_drm_devices(),
    };

    let scheme_status = if report.udev_scheme {
        CheckStatus::Pass(format!(
            "enumerated {} /scheme/udev/devices entries",
            device_entries.len()
        ))
    } else {
        CheckStatus::Fail("could not enumerate any /scheme/udev/devices entries".to_string())
    };
    let keyboard_status = if config.keyboard {
        count_status(report.keyboard_count, "keyboard")
    } else {
        CheckStatus::Skip
    };
    let pointer_status = if config.pointer {
        count_status(report.pointer_count, "pointer")
    } else {
        CheckStatus::Skip
    };
    let drm_status = if config.drm {
        count_status(report.drm_count, "DRM")
    } else {
        CheckStatus::Skip
    };

    if config.json {
        let payload = serde_json::to_string(&json!({
            "udev_scheme": report.udev_scheme,
            "keyboard_count": report.keyboard_count,
            "pointer_count": report.pointer_count,
            "drm_count": report.drm_count,
        }))
        .map_err(|err| format!("failed to serialize JSON output: {err}"))?;
        println!("{payload}");
    } else {
        scheme_status.render("udev scheme");
        keyboard_status.render("keyboard devices");
        pointer_status.render("pointer devices");
        drm_status.render("DRM devices");
    }

    Ok(overall_success(&report, &config))
}

#[cfg(any(target_os = "redox", test))]
fn overall_success(report: &Report, config: &Config) -> bool {
    let checks: Vec<CheckStatus> = [
        (!config.keyboard || report.keyboard_count > 0),
        (!config.pointer || report.pointer_count > 0),
        (!config.drm || report.drm_count > 0),
    ].iter().map(|&pass| if pass { CheckStatus::Pass("ok".to_string()) } else { CheckStatus::Fail("none found".to_string()) }).collect();
    checks.iter().all(|c| matches!(c, CheckStatus::Pass(_)))
}

#[cfg(target_os = "redox")]
fn count_status(count: usize, label: &str) -> CheckStatus {
    if count > 0 {
        CheckStatus::Pass(format!("{} {} device(s) found", count, label))
    } else {
        CheckStatus::Fail(format!("no {} devices found", label))
    }
}

#[cfg(target_os = "redox")]
fn list_dir_names(path: &str) -> Result<Vec<String>, String> {
    let entries = fs::read_dir(path).map_err(|err| format!("failed to read {path}: {err}"))?;
    let mut names = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect::<Vec<_>>();
    names.sort();
    Ok(names)
}

#[cfg(target_os = "redox")]
fn count_devices_with_property(device_entries: &[String], key: &str, value: &str) -> usize {
    device_entries
        .iter()
        .filter(|entry| {
            let path = format!("/scheme/udev/devices/{entry}");
            let Ok(info) = read_text_with_limit(&path, MAX_DEVICE_INFO_BYTES) else {
                return false;
            };
            has_property(&info, key, value)
        })
        .count()
}

#[cfg(target_os = "redox")]
fn count_drm_devices() -> usize {
    list_dir_names("/dev/dri")
        .map(|entries| {
            entries
                .into_iter()
                .filter(|name| name.starts_with("card"))
                .count()
        })
        .unwrap_or(0)
}

#[cfg(target_os = "redox")]
fn read_text_with_limit(path: &str, max_bytes: usize) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|err| format!("failed to open {path}: {err}"))?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take((max_bytes + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|err| format!("failed to read {path}: {err}"))?;

    if bytes.len() > max_bytes {
        return Err(format!("{path} exceeds maximum size of {max_bytes} bytes"));
    }

    String::from_utf8(bytes).map_err(|err| format!("{path} is not valid UTF-8: {err}"))
}

#[cfg(any(target_os = "redox", test))]
fn has_property(info: &str, key: &str, expected: &str) -> bool {
    let prefix = format!("E={key}=");
    info.lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .map(|value| value.trim() == expected)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vec_args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn parse_args_defaults_to_all_checks() {
        let config = parse_args(vec_args(&[PROGRAM])).unwrap();
        assert!(config.keyboard);
        assert!(config.pointer);
        assert!(config.drm);
        assert!(!config.json);
    }

    #[test]
    fn parse_args_accepts_targeted_flags() {
        let config = parse_args(vec_args(&[PROGRAM, "--keyboard", "--json"])).unwrap();
        assert!(config.keyboard);
        assert!(!config.pointer);
        assert!(!config.drm);
        assert!(config.json);
    }

    #[test]
    fn parse_args_rejects_unknown_flag() {
        let err = parse_args(vec_args(&[PROGRAM, "--bogus"])).unwrap_err();
        assert!(err.contains("unsupported argument"));
    }

    #[test]
    fn has_property_matches_expected_key_and_value() {
        let info = "P=/devices/platform/evdev-keyboard0\nE=ID_INPUT=1\nE=ID_INPUT_KEYBOARD=1\n";
        assert!(has_property(info, "ID_INPUT_KEYBOARD", "1"));
        assert!(!has_property(info, "ID_INPUT_MOUSE", "1"));
    }

    #[test]
    fn overall_success_requires_all_requested_runtime_surfaces() {
        let all_flags = Config {
            keyboard: true,
            pointer: true,
            drm: true,
            json: false,
        };
        let passing = Report {
            udev_scheme: true,
            keyboard_count: 1,
            pointer_count: 1,
            drm_count: 1,
        };
        let missing_drm = Report {
            drm_count: 0,
            ..passing.clone()
        };

        assert!(overall_success(&passing, &all_flags));
        assert!(!overall_success(&missing_drm, &all_flags));
    }

    #[test]
    fn overall_success_respects_targeted_flags() {
        let passing = Report {
            udev_scheme: true,
            keyboard_count: 1,
            pointer_count: 0,
            drm_count: 0,
        };
        let keyboard_only = Config {
            keyboard: true,
            pointer: false,
            drm: false,
            json: false,
        };

        assert!(overall_success(&passing, &keyboard_only));
    }
}
