use std::fs::File;
use std::io::Read;
use std::process;
use std::time::{Duration, Instant};

use redbear_hwutils::parse_args;

const PROGRAM: &str = "redbear-evtest";
const USAGE: &str = "Usage: redbear-evtest\n\nRead the first keyboard event from the udev-backed evdev consumer path and print it.";
const EVENT_SIZE: usize = 24;
const EV_KEY: u16 = 0x01;

#[derive(Clone, Copy, Debug)]
struct InputEvent {
    event_type: u16,
    code: u16,
    value: i32,
}

impl InputEvent {
    fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != EVENT_SIZE {
            return Err(format!("expected {EVENT_SIZE} bytes, got {}", bytes.len()));
        }

        Ok(Self {
            event_type: u16::from_le_bytes([bytes[16], bytes[17]]),
            code: u16::from_le_bytes([bytes[18], bytes[19]]),
            value: i32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]),
        })
    }
}

fn run() -> Result<(), String> {
    parse_args(PROGRAM, USAGE, std::env::args()).map_err(|err| {
        if err.is_empty() {
            process::exit(0);
        }
        err
    })?;

    let candidate_paths = ["/scheme/udev/dev/input/event0", "/scheme/evdev/event0"];
    let mut last_error = String::new();

    for event_path in candidate_paths {
        let mut file = match File::open(event_path) {
            Ok(file) => file,
            Err(err) => {
                last_error = format!("failed to open {event_path}: {err}");
                continue;
            }
        };

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut raw = [0_u8; EVENT_SIZE];

        while Instant::now() < deadline {
            file.read_exact(&mut raw)
                .map_err(|err| format!("failed to read evdev event from {event_path}: {err}"))?;
            let event = InputEvent::from_bytes(&raw)?;
            if event.event_type == EV_KEY {
                println!("SOURCE={event_path}");
                println!("EV_KEY code={} value={}", event.code, event.value);
                return Ok(());
            }
        }

        last_error = format!("timed out waiting for a key event on {event_path}");
    }

    Err(last_error)
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{PROGRAM}: {err}");
        process::exit(1);
    }
}
