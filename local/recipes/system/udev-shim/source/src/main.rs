mod device_db;
mod scheme;

use std::env;
use std::process;

use log::{error, info, LevelFilter, Metadata, Record};
use redox_scheme::{SignalBehavior, Socket};

use scheme::UdevScheme;

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

fn run() -> Result<(), String> {
    let mut scheme = UdevScheme::new();

    match scheme.scan_pci_devices() {
        Ok(n) => info!("udev-shim: enumerated {} PCI device(s)", n),
        Err(e) => error!("udev-shim: PCI scan failed: {}", e),
    }

    let socket =
        Socket::create("udev").map_err(|e| format!("failed to register udev scheme: {}", e))?;
    info!("udev-shim: registered scheme:udev");

    loop {
        let request = match socket.next_request(SignalBehavior::Restart) {
            Ok(Some(r)) => r,
            Ok(None) => {
                info!("udev-shim: scheme unmounted, exiting");
                break;
            }
            Err(e) => {
                error!("udev-shim: failed to read scheme request: {}", e);
                continue;
            }
        };

        let response = match request.handle_scheme_block_mut(&mut scheme) {
            Ok(r) => r,
            Err(_req) => {
                error!("udev-shim: failed to handle request");
                continue;
            }
        };

        if let Err(e) = socket.write_response(response, SignalBehavior::Restart) {
            error!("udev-shim: failed to write response: {}", e);
        }
    }

    Ok(())
}

fn main() {
    let log_level = match env::var("UDEV_SHIM_LOG").as_deref() {
        Ok("debug") => LevelFilter::Debug,
        Ok("trace") => LevelFilter::Trace,
        _ => LevelFilter::Info,
    };
    let _ = log::set_boxed_logger(Box::new(StderrLogger { level: log_level }));
    log::set_max_level(log_level);

    if let Err(e) = run() {
        error!("udev-shim: fatal error: {}", e);
        process::exit(1);
    }
}
