mod device;
mod scheme;
mod translate;
mod types;

use std::env;
use std::fs::File;
use std::io::Read;
use std::process;

use log::{error, info, LevelFilter, Metadata, Record};
use redox_scheme::{SignalBehavior, Socket};

use scheme::EvdevScheme;

struct StderrLogger {
    level: LevelFilter,
}

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }
    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            eprintln!("[{}] {}", record.level(), record.args());
        }
    }
    fn flush(&self) {}
}

fn read_input_events(scheme: &mut EvdevScheme) -> Result<(), String> {
    let mut input_file =
        File::open("/scheme/input").map_err(|e| format!("failed to open /scheme/input: {}", e))?;

    let mut buf = [0u8; 256];
    match input_file.read(&mut buf) {
        Ok(n) if n > 0 => {
            let data = &buf[..n];
            for &byte in data {
                let pressed = (byte & 0x80) == 0;
                let key = byte & 0x7F;
                scheme.feed_keyboard_event(key, pressed);
            }
        }
        Ok(_) => {}
        Err(e) => {
            error!("evdevd: failed to read input: {}", e);
        }
    }
    Ok(())
}

fn run() -> Result<(), String> {
    let mut scheme = EvdevScheme::new();

    let socket =
        Socket::create("evdev").map_err(|e| format!("failed to register evdev scheme: {}", e))?;
    info!("evdevd: registered scheme:evdev");

    loop {
        let request = match socket.next_request(SignalBehavior::Restart) {
            Ok(Some(r)) => r,
            Ok(None) => {
                info!("evdevd: scheme unmounted, exiting");
                break;
            }
            Err(e) => {
                error!("evdevd: failed to read scheme request: {}", e);
                continue;
            }
        };

        let response = match request.handle_scheme_block_mut(&mut scheme) {
            Ok(r) => r,
            Err(_req) => {
                error!("evdevd: failed to handle request");
                continue;
            }
        };

        if let Err(e) = socket.write_response(response, SignalBehavior::Restart) {
            error!("evdevd: failed to write response: {}", e);
        }

        let _ = read_input_events(&mut scheme);
    }

    Ok(())
}

fn main() {
    let log_level = match env::var("EVDEVD_LOG").as_deref() {
        Ok("debug") => LevelFilter::Debug,
        Ok("trace") => LevelFilter::Trace,
        _ => LevelFilter::Info,
    };
    let _ = log::set_boxed_logger(Box::new(StderrLogger { level: log_level }));
    log::set_max_level(log_level);

    if let Err(e) = run() {
        error!("evdevd: fatal error: {}", e);
        process::exit(1);
    }
}
