use std::{process, time::Duration};

#[cfg(target_os = "redox")]
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read},
    time::Instant,
};

#[cfg(target_os = "redox")]
use std::os::unix::fs::OpenOptionsExt;

use serde_json::json;

#[cfg(target_os = "redox")]
use syscall::O_NONBLOCK;

const PROGRAM: &str = "redbear-phase1-evdev-check";
const USAGE: &str = "Usage: redbear-phase1-evdev-check [--keyboard] [--mouse] [--timeout SECS] [--json]\n\nValidate the bounded evdevd keyboard and mouse paths inside the Red Bear guest.";

const DEFAULT_TIMEOUT_SECS: u64 = 5;
const MAX_TIMEOUT_SECS: u64 = 300;
#[cfg(target_os = "redox")]
const MAX_METADATA_BYTES: usize = 64 * 1024;
#[cfg(any(target_os = "redox", test))]
const EV_KEY: u16 = 0x01;
#[cfg(any(target_os = "redox", test))]
const EV_REL: u16 = 0x02;
#[cfg(any(target_os = "redox", test))]
const LEGACY_EVENT_SIZE: usize = 16;
#[cfg(any(target_os = "redox", test))]
const CURRENT_EVENT_SIZE: usize = 24;

#[cfg(any(target_os = "redox", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct InputEvent {
    event_type: u16,
    code: u16,
    value: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Config {
    keyboard: bool,
    mouse: bool,
    timeout: Duration,
    json: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Report {
    evdev_scheme: bool,
    keyboard_events: bool,
    mouse_events: bool,
}

#[cfg(target_os = "redox")]
#[derive(Clone, Debug, Eq, PartialEq)]
enum CheckStatus {
    Pass(String),
    Fail(String),
    Timeout(String),
    Skip,
}

#[cfg(any(target_os = "redox", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InputKind {
    Keyboard,
    Mouse,
}

#[cfg(any(target_os = "redox", test))]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct EventMetadata {
    keyboard: bool,
    mouse: bool,
}

#[cfg(any(target_os = "redox", test))]
impl InputEvent {
    fn from_legacy_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != LEGACY_EVENT_SIZE {
            return Err(format!(
                "expected {LEGACY_EVENT_SIZE} bytes, got {}",
                bytes.len()
            ));
        }

        Ok(Self {
            event_type: u16::from_le_bytes([bytes[8], bytes[9]]),
            code: u16::from_le_bytes([bytes[10], bytes[11]]),
            value: i32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
        })
    }

    fn from_current_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != CURRENT_EVENT_SIZE {
            return Err(format!(
                "expected {CURRENT_EVENT_SIZE} bytes, got {}",
                bytes.len()
            ));
        }

        Ok(Self {
            event_type: u16::from_le_bytes([bytes[16], bytes[17]]),
            code: u16::from_le_bytes([bytes[18], bytes[19]]),
            value: i32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]),
        })
    }
}

#[cfg(target_os = "redox")]
impl CheckStatus {
    fn is_success(&self) -> bool {
        matches!(self, Self::Pass(_) | Self::Skip)
    }

    fn render(&self, label: &str) {
        match self {
            Self::Pass(detail) => println!("PASS {label}: {detail}"),
            Self::Fail(detail) => println!("FAIL {label}: {detail}"),
            Self::Timeout(detail) => println!("TIMEOUT {label}: {detail}"),
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
    let mut mouse = false;
    let mut timeout = Duration::from_secs(DEFAULT_TIMEOUT_SECS);
    let mut json = false;

    let mut args = args.into_iter();
    let _program = args.next();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--keyboard" => keyboard = true,
            "--mouse" => mouse = true,
            "--timeout" => {
                let Some(value) = args.next() else {
                    return Err("missing value for --timeout".to_string());
                };
                let secs = value
                    .parse::<u64>()
                    .map_err(|err| format!("invalid timeout '{value}': {err}"))?;
                if secs > MAX_TIMEOUT_SECS {
                    return Err(format!(
                        "timeout '{value}' exceeds maximum of {MAX_TIMEOUT_SECS} seconds"
                    ));
                }
                timeout = Duration::from_secs(secs);
            }
            "--json" => json = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Err(String::new());
            }
            _ => return Err(format!("unsupported argument: {arg}")),
        }
    }

    if !keyboard && !mouse {
        keyboard = true;
        mouse = true;
    }

    Ok(Config {
        keyboard,
        mouse,
        timeout,
        json,
    })
}

fn run(config: &Config) -> Result<bool, String> {
    #[cfg(not(target_os = "redox"))]
    {
        let report = Report::default();
        if config.json {
            let payload = serde_json::to_string(&json!({
                "evdev_scheme": report.evdev_scheme,
                "keyboard_events": report.keyboard_events,
                "mouse_events": report.mouse_events,
            }))
            .map_err(|err| format!("failed to serialize JSON output: {err}"))?;
            eprintln!("evdevd check requires Redox runtime");
            println!("{payload}");
        } else {
            println!("evdevd check requires Redox runtime");
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
    let evdev_scheme_present = fs::metadata("/scheme/evdev").is_ok();
    let event_names = match list_event_names() {
        Ok(names) => names,
        Err(_) => Vec::new(),
    };
    let report = Report {
        evdev_scheme: evdev_scheme_present,
        keyboard_events: false,
        mouse_events: false,
    };
    let metadata = load_event_metadata(&event_names);

    let keyboard_name = select_event_name(&event_names, &metadata, InputKind::Keyboard, None);
    let mouse_name = select_event_name(
        &event_names,
        &metadata,
        InputKind::Mouse,
        keyboard_name.as_deref(),
    );

    let mut report = report;
    let scheme_status = if report.evdev_scheme {
        CheckStatus::Pass(format!(
            "enumerated {} device(s): {}",
            event_names.len(),
            if event_names.is_empty() {
                String::from("none")
            } else {
                event_names.join(", ")
            }
        ))
    } else {
        CheckStatus::Fail("could not enumerate any /scheme/evdev/event* nodes".to_string())
    };

    let keyboard_status = if config.keyboard {
        run_input_check(keyboard_name.as_deref(), EV_KEY, config.timeout, "keyboard")
    } else {
        CheckStatus::Skip
    };
    report.keyboard_events = matches!(keyboard_status, CheckStatus::Pass(_));

    let mouse_status = if config.mouse {
        run_input_check(mouse_name.as_deref(), EV_REL, config.timeout, "mouse")
    } else {
        CheckStatus::Skip
    };
    report.mouse_events = matches!(mouse_status, CheckStatus::Pass(_));

    if config.json {
        let payload = serde_json::to_string(&json!({
            "evdev_scheme": report.evdev_scheme,
            "keyboard_events": report.keyboard_events,
            "mouse_events": report.mouse_events,
        }))
        .map_err(|err| format!("failed to serialize JSON output: {err}"))?;
        println!("{payload}");
    } else {
        scheme_status.render("evdev scheme");
        keyboard_status.render("keyboard events");
        mouse_status.render("mouse events");
    }

    Ok(scheme_status.is_success() && keyboard_status.is_success() && mouse_status.is_success())
}

#[cfg(target_os = "redox")]
fn list_event_names() -> Result<Vec<String>, String> {
    let entries = fs::read_dir("/scheme/evdev")
        .map_err(|err| format!("failed to read /scheme/evdev: {err}"))?;
    let mut names = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| event_index(name).is_some())
        .collect::<Vec<_>>();
    names.sort_by_key(|name| event_index(name).unwrap_or(u32::MAX));
    Ok(names)
}

#[cfg(target_os = "redox")]
fn load_event_metadata(event_names: &[String]) -> Vec<(String, EventMetadata)> {
    let mut metadata = Vec::new();

    for event_name in event_names {
        let path = format!("/scheme/udev/dev/input/{event_name}");
        let info = match read_text_with_limit(&path, MAX_METADATA_BYTES) {
            Ok(info) => info,
            Err(_) => {
                metadata.push((event_name.clone(), EventMetadata::default()));
                continue;
            }
        };
        metadata.push((event_name.clone(), parse_event_metadata(&info)));
    }

    metadata
}

#[cfg(target_os = "redox")]
fn read_text_with_limit(path: &str, max_bytes: usize) -> Result<String, String> {
    let mut file = File::open(path).map_err(|err| format!("failed to open {path}: {err}"))?;
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
fn parse_event_metadata(info: &str) -> EventMetadata {
    let mut metadata = EventMetadata::default();

    for line in info.lines() {
        if let Some(value) = line.strip_prefix("E=ID_INPUT_KEYBOARD=") {
            metadata.keyboard = value.trim() == "1";
        }
        if let Some(value) = line.strip_prefix("E=ID_INPUT_MOUSE=") {
            metadata.mouse = value.trim() == "1";
        }
    }

    metadata
}

#[cfg(any(target_os = "redox", test))]
fn select_event_name(
    event_names: &[String],
    metadata: &[(String, EventMetadata)],
    kind: InputKind,
    exclude: Option<&str>,
) -> Option<String> {
    let mut matching_names = metadata
        .iter()
        .filter_map(|(name, entry)| {
            if exclude == Some(name.as_str()) {
                return None;
            }

            let matches_kind = match kind {
                InputKind::Keyboard => entry.keyboard,
                InputKind::Mouse => entry.mouse,
            };

            matches_kind.then_some(name.clone())
        })
        .collect::<Vec<_>>();
    matching_names.sort_by_key(|name| event_index(name).unwrap_or(u32::MAX));

    if let Some(name) = matching_names.into_iter().next() {
        return Some(name);
    }

    let preferred = match kind {
        InputKind::Keyboard => "event0",
        InputKind::Mouse => "event1",
    };

    if exclude != Some(preferred) && event_names.iter().any(|name| name == preferred) {
        return Some(preferred.to_string());
    }

    None
}

#[cfg(target_os = "redox")]
fn run_input_check(
    event_name: Option<&str>,
    expected_type: u16,
    timeout: Duration,
    label: &str,
) -> CheckStatus {
    let Some(event_name) = event_name else {
        return CheckStatus::Fail(format!("no {label} event device was enumerated"));
    };

    let path = format!("/scheme/evdev/{event_name}");
    let mut file = match open_nonblocking(&path) {
        Ok(file) => file,
        Err(err) => return CheckStatus::Fail(err),
    };

    match wait_for_event(&mut file, expected_type, timeout) {
        Ok(Some(event)) => CheckStatus::Pass(format!(
            "{path} produced type={} code={} value={}",
            event.event_type, event.code, event.value
        )),
        Ok(None) => CheckStatus::Timeout(format!(
            "{path} produced no matching event within {}s",
            timeout.as_secs()
        )),
        Err(err) => CheckStatus::Fail(format!("{path}: {err}")),
    }
}

#[cfg(target_os = "redox")]
fn open_nonblocking(path: &str) -> Result<File, String> {
    OpenOptions::new()
        .read(true)
        .custom_flags(O_NONBLOCK as i32)
        .open(path)
        .map_err(|err| format!("failed to open {path}: {err}"))
}

#[cfg(target_os = "redox")]
fn wait_for_event(
    file: &mut File,
    expected_type: u16,
    timeout: Duration,
) -> Result<Option<InputEvent>, String> {
    let deadline = Instant::now() + timeout;
    let mut raw = [0_u8; CURRENT_EVENT_SIZE * 4];

    while Instant::now() < deadline {
        match file.read(&mut raw) {
            Ok(0) => std::thread::sleep(Duration::from_millis(25)),
            Ok(len) => {
                let events = parse_events_for_expected(&raw[..len], expected_type)?;
                if let Some(event) = events
                    .into_iter()
                    .find(|event| event.event_type == expected_type)
                {
                    return Ok(Some(event));
                }
            }
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(err) => return Err(format!("failed to read event data: {err}")),
        }
    }

    Ok(None)
}

#[cfg(any(target_os = "redox", test))]
fn parse_events_for_expected(bytes: &[u8], expected_type: u16) -> Result<Vec<InputEvent>, String> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }

    let current =
        parse_events_with_layout(bytes, CURRENT_EVENT_SIZE, InputEvent::from_current_bytes);
    let legacy = parse_events_with_layout(bytes, LEGACY_EVENT_SIZE, InputEvent::from_legacy_bytes);

    match (current, legacy) {
        (Ok(current_events), Ok(legacy_events)) => {
            let current_matches = current_events
                .iter()
                .any(|event| event.event_type == expected_type);
            let legacy_matches = legacy_events
                .iter()
                .any(|event| event.event_type == expected_type);

            match (current_matches, legacy_matches) {
                (true, false) => Ok(current_events),
                (false, true) => Ok(legacy_events),
                (true, true) | (false, false) => Ok(current_events),
            }
        }
        (Ok(current_events), Err(_)) => Ok(current_events),
        (Err(_), Ok(legacy_events)) => Ok(legacy_events),
        (Err(current_err), Err(legacy_err)) => Err(format!(
            "failed to decode evdev payload as 24-byte or 16-byte events: {current_err}; {legacy_err}"
        )),
    }
}

#[cfg(any(target_os = "redox", test))]
fn parse_events_with_layout(
    bytes: &[u8],
    event_size: usize,
    decode: fn(&[u8]) -> Result<InputEvent, String>,
) -> Result<Vec<InputEvent>, String> {
    if bytes.len() % event_size != 0 {
        return Err(format!(
            "payload length {} is not divisible by event size {event_size}",
            bytes.len()
        ));
    }

    bytes.chunks_exact(event_size).map(decode).collect()
}

#[cfg(any(target_os = "redox", test))]
fn event_index(name: &str) -> Option<u32> {
    name.strip_prefix("event")?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vec_args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn parse_args_defaults_to_keyboard_and_mouse() {
        let config = parse_args(vec_args(&[PROGRAM])).unwrap();
        assert!(config.keyboard);
        assert!(config.mouse);
        assert_eq!(config.timeout, Duration::from_secs(DEFAULT_TIMEOUT_SECS));
        assert!(!config.json);
    }

    #[test]
    fn parse_args_accepts_targeted_flags() {
        let config = parse_args(vec_args(&[
            PROGRAM,
            "--keyboard",
            "--timeout",
            "9",
            "--json",
        ]))
        .unwrap();
        assert!(config.keyboard);
        assert!(!config.mouse);
        assert_eq!(config.timeout, Duration::from_secs(9));
        assert!(config.json);
    }

    #[test]
    fn parse_args_rejects_invalid_timeout() {
        let err = parse_args(vec_args(&[PROGRAM, "--timeout", "abc"])).unwrap_err();
        assert!(err.contains("invalid timeout"));
    }

    #[test]
    fn parse_args_rejects_timeout_over_limit() {
        let err = parse_args(vec_args(&[PROGRAM, "--timeout", "301"])).unwrap_err();
        assert!(err.contains("exceeds maximum"));
    }

    #[test]
    fn parses_current_input_event_layout() {
        let bytes = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 30, 0, 1, 0, 0, 0,
        ];
        let event = InputEvent::from_current_bytes(&bytes).unwrap();
        assert_eq!(
            event,
            InputEvent {
                event_type: EV_KEY,
                code: 30,
                value: 1,
            }
        );
    }

    #[test]
    fn parses_legacy_input_event_layout() {
        let bytes = [0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 5, 0, 0, 0];
        let event = InputEvent::from_legacy_bytes(&bytes).unwrap();
        assert_eq!(
            event,
            InputEvent {
                event_type: EV_REL,
                code: 0,
                value: 5,
            }
        );
    }

    #[test]
    fn event_index_parses_numeric_suffix() {
        assert_eq!(event_index("event0"), Some(0));
        assert_eq!(event_index("event17"), Some(17));
        assert_eq!(event_index("mouse"), None);
    }

    #[test]
    fn parse_event_metadata_extracts_keyboard_and_mouse_flags() {
        let metadata =
            parse_event_metadata("E=ID_INPUT=1\nE=ID_INPUT_KEYBOARD=1\nE=ID_INPUT_MOUSE=0\n");
        assert!(metadata.keyboard);
        assert!(!metadata.mouse);
    }

    #[test]
    fn select_event_name_prefers_metadata_match() {
        let event_names = vec!["event0".to_string(), "event1".to_string()];
        let metadata = vec![
            (
                "event0".to_string(),
                EventMetadata {
                    keyboard: true,
                    mouse: false,
                },
            ),
            (
                "event1".to_string(),
                EventMetadata {
                    keyboard: false,
                    mouse: true,
                },
            ),
        ];

        assert_eq!(
            select_event_name(&event_names, &metadata, InputKind::Mouse, None),
            Some("event1".to_string())
        );
    }

    #[test]
    fn select_event_name_prefers_keyboard_metadata_match() {
        let event_names = vec!["event0".to_string(), "event1".to_string()];
        let metadata = vec![
            (
                "event0".to_string(),
                EventMetadata {
                    keyboard: true,
                    mouse: false,
                },
            ),
            (
                "event1".to_string(),
                EventMetadata {
                    keyboard: false,
                    mouse: true,
                },
            ),
        ];

        assert_eq!(
            select_event_name(&event_names, &metadata, InputKind::Keyboard, None),
            Some("event0".to_string())
        );
    }

    #[test]
    fn select_event_name_does_not_fallback_to_arbitrary_device() {
        let event_names = vec!["event2".to_string()];
        let metadata = vec![("event2".to_string(), EventMetadata::default())];

        assert_eq!(
            select_event_name(&event_names, &metadata, InputKind::Mouse, None),
            None
        );
    }

    #[test]
    fn parse_events_prefers_legacy_layout_when_only_legacy_matches_expected_type() {
        let bytes = [
            0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 30, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];

        let events = parse_events_for_expected(&bytes, EV_KEY).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].event_type, EV_KEY);
        assert_eq!(events[0].code, 30);
        assert_eq!(events[0].value, 1);
    }
}
