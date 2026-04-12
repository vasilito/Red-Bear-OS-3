mod blob;
mod scheme;

use std::env;
use std::path::PathBuf;
use std::process;

use log::{error, info, LevelFilter, Metadata, Record};
use redox_scheme::{SignalBehavior, Socket};

use blob::FirmwareRegistry;
use scheme::FirmwareScheme;

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

fn init_logging(level: LevelFilter) {
    if log::set_boxed_logger(Box::new(StderrLogger { level })).is_err() {
        return;
    }
    log::set_max_level(level);
}

fn default_firmware_dir() -> PathBuf {
    PathBuf::from("/usr/firmware/")
}

fn run() -> Result<(), String> {
    let firmware_dir = env::var("FIRMWARE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_firmware_dir());

    info!(
        "firmware-loader: starting with directory {}",
        firmware_dir.display()
    );

    let registry = FirmwareRegistry::new(&firmware_dir)
        .map_err(|e| format!("failed to initialize firmware registry: {e}"))?;

    let socket = Socket::create("firmware")
        .map_err(|e| format!("failed to register firmware scheme: {e}"))?;
    info!("firmware-loader: registered scheme:firmware");

    let mut firmware_scheme = FirmwareScheme::new(registry);

    loop {
        let request = match socket.next_request(SignalBehavior::Restart) {
            Ok(Some(request)) => request,
            Ok(None) => {
                info!("firmware-loader: scheme unmounted, exiting");
                break;
            }
            Err(e) => {
                error!("firmware-loader: failed to read scheme request: {}", e);
                continue;
            }
        };

        let response = match request.handle_scheme_block_mut(&mut firmware_scheme) {
            Ok(response) => response,
            Err(_request) => {
                error!("firmware-loader: failed to handle request");
                continue;
            }
        };

        if let Err(e) = socket.write_response(response, SignalBehavior::Restart) {
            error!("firmware-loader: failed to write response: {}", e);
        }
    }

    Ok(())
}

fn main() {
    let log_level = match env::var("FIRMWARE_LOADER_LOG").as_deref() {
        Ok("debug") => LevelFilter::Debug,
        Ok("trace") => LevelFilter::Trace,
        Ok("warn") => LevelFilter::Warn,
        Ok("error") => LevelFilter::Error,
        _ => LevelFilter::Info,
    };

    init_logging(log_level);

    if let Err(e) = run() {
        error!("firmware-loader: fatal error: {}", e);
        process::exit(1);
    }
}
