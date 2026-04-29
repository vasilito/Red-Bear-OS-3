use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::process;
use std::time::{Duration, Instant};

use orbclient::{K_A, KeyEvent};
use redbear_hwutils::parse_args;

const PROGRAM: &str = "redbear-input-inject";
const USAGE: &str = "Usage: redbear-input-inject\n\nInject a synthetic 'A' key press/release through /scheme/input/producer and verify the first evdev consumer event.";
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

fn open_consumer() -> Result<(File, &'static str), String> {
    for path in ["/scheme/udev/dev/input/event0", "/scheme/evdev/event0"] {
        if let Ok(file) = File::open(path) {
            return Ok((file, path));
        }
    }
    Err("failed to open an evdev consumer path".to_string())
}

fn run() -> Result<(), String> {
    parse_args(PROGRAM, USAGE, std::env::args()).map_err(|err| {
        if err.is_empty() {
            process::exit(0);
        }
        err
    })?;

    let (mut consumer, consumer_path) = open_consumer()?;

    let mut producer = OpenOptions::new()
        .write(true)
        .open("/scheme/input/producer")
        .map_err(|err| format!("failed to open /scheme/input/producer: {err}"))?;

    producer
        .write_all(
            &KeyEvent {
                character: 'a',
                scancode: K_A,
                pressed: true,
            }
            .to_event(),
        )
        .map_err(|err| format!("failed to inject key press: {err}"))?;
    producer
        .write_all(
            &KeyEvent {
                character: 'a',
                scancode: K_A,
                pressed: false,
            }
            .to_event(),
        )
        .map_err(|err| format!("failed to inject key release: {err}"))?;

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut raw = [0_u8; EVENT_SIZE];
    while Instant::now() < deadline {
        consumer
            .read_exact(&mut raw)
            .map_err(|err| format!("failed to read evdev event from {consumer_path}: {err}"))?;
        let event = InputEvent::from_bytes(&raw)?;
        if event.event_type == EV_KEY {
            println!("Injected synthetic key event: A");
            println!("SOURCE={consumer_path}");
            println!("EV_KEY code={} value={}", event.code, event.value);
            return Ok(());
        }
    }

    Err(format!(
        "timed out waiting for an evdev consumer event on {consumer_path}"
    ))
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{PROGRAM}: {err}");
        process::exit(1);
    }
}
