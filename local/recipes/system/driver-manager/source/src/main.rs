mod config;
mod exec;
mod hotplug;
mod scheme;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use std::{env, fs, process};

use redox_driver_core::device::DeviceId;
use redox_driver_core::driver::ProbeResult;
use redox_driver_core::manager::{DeviceManager, ManagerConfig, ProbeEvent};
use redox_driver_pci::PciBus;
use std::fs::OpenOptions;
use std::io::Write;

use config::DriverConfig;
use scheme::{DriverManagerScheme, notify_bind};

struct StderrLogger;

const BOOT_TIMELINE_PATH: &str = "/tmp/redbear-boot-timeline.json";

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!("[{}] driver-manager: {}", record.level(), record.args());
        }
    }
    fn flush(&self) {}
}

fn run_enumeration(
    manager: &Arc<Mutex<DeviceManager>>,
    scheme: &DriverManagerScheme,
) -> (usize, usize) {
    let enum_start = Instant::now();
    let events = match manager.lock() {
        Ok(mut mgr) => mgr.enumerate(),
        Err(err) => {
            log::error!("failed to enumerate devices: manager lock poisoned: {err}");
            return (0, 0);
        }
    };
    let enum_duration = enum_start.elapsed();

    let mut bound = 0usize;
    let mut deferred = 0usize;

    for event in &events {
        log_timeline(event);
        match event {
            ProbeEvent::ProbeCompleted {
                device,
                driver_name,
                result,
            } => {
                match result {
                    ProbeResult::Bound => {
                        log::info!("bound: {} -> {}", device.path, driver_name);
                        notify_bound_device(scheme, device, driver_name);
                        bound += 1;
                    }
                    ProbeResult::Deferred { reason } => {
                        log::info!("deferred: {} -> {} ({})", device.path, driver_name, reason);
                        deferred += 1;
                    }
                    ProbeResult::Fatal { reason } => {
                        log::error!("fatal: {} -> {} ({})", device.path, driver_name, reason);
                    }
                    _ => {}
                }
            }
            ProbeEvent::BusEnumerated { bus, device_count } => {
                log::info!("bus {} enumerated {} device(s)", bus, device_count);
            }
            ProbeEvent::BusEnumerationFailed { bus, error } => {
                log::error!("bus {} enumeration failed: {:?}", bus, error);
            }
            _ => {}
        }
    }

    log::info!(
        "enumeration complete: {} bound, {} deferred ({}ms total)",
        bound, deferred, enum_duration.as_millis()
    );

    (bound, deferred)
}

fn notify_bound_device(scheme: &DriverManagerScheme, device: &DeviceId, driver_name: &str) {
    if device.bus == "pci" {
        notify_bind(scheme, &device.path, driver_name);
    }
}

fn reset_timeline_log() {
    if let Err(err) = fs::write(BOOT_TIMELINE_PATH, "") {
        log::warn!("failed to reset boot timeline log at {BOOT_TIMELINE_PATH}: {err}");
    }
}

fn log_timeline(event: &ProbeEvent) {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    let entry = match event {
        ProbeEvent::BusEnumerated { bus, device_count } => format!(
            r#"{{"ts":{},"event":"bus_enumerated","bus":"{}","count":{}}}"#,
            timestamp, bus, device_count
        ),
        ProbeEvent::ProbeCompleted {
            device,
            driver_name,
            result,
        } => {
            let status = match result {
                ProbeResult::Bound => "bound",
                ProbeResult::Deferred { .. } => "deferred",
                ProbeResult::Fatal { .. } => "failed",
                ProbeResult::NotSupported => "skipped",
            };
            format!(
                r#"{{"ts":{},"event":"probe","device":"{}","driver":"{}","status":"{}"}}"#,
                timestamp, device.path, driver_name, status
            )
        }
        _ => return,
    };

    match OpenOptions::new()
        .create(true)
        .append(true)
        .open(BOOT_TIMELINE_PATH)
    {
        Ok(mut file) => {
            if let Err(err) = writeln!(file, "{entry}") {
                log::warn!("failed to append boot timeline entry to {BOOT_TIMELINE_PATH}: {err}");
            }
        }
        Err(err) => {
            log::warn!("failed to open boot timeline log at {BOOT_TIMELINE_PATH}: {err}");
        }
    }
}

fn main() {
    log::set_logger(&StderrLogger).ok();
    log::set_max_level(log::LevelFilter::Info);

    let args: Vec<String> = env::args().collect();
    let initfs = args.iter().any(|a| a == "--initfs");
    let hotplug_mode = args.iter().any(|a| a == "--hotplug");

    let config_dir = if initfs {
        "/scheme/initfs/lib/drivers.d"
    } else {
        "/lib/drivers.d"
    };

    let driver_configs = match DriverConfig::load_all(config_dir) {
        Ok(c) => c,
        Err(e) => {
            log::error!("failed to load driver configs: {}", e);
            process::exit(1);
        }
    };

    if driver_configs.is_empty() {
        log::warn!("no driver configs found in {}", config_dir);
        process::exit(0);
    }

    log::info!("loaded {} driver config(s)", driver_configs.len());

    let manager_config = ManagerConfig {
        max_concurrent_probes: 4,
        deferred_retry_ms: 500,
        async_probe: true,
    };

    let manager = Arc::new(Mutex::new(DeviceManager::new(manager_config.clone())));
    let scheme = Arc::new(DriverManagerScheme::new());

    match manager.lock() {
        Ok(mut mgr) => {
            mgr.register_bus(Box::new(PciBus::new()));

            for dc in &driver_configs {
                mgr.register_driver(Box::new(dc.clone()));
            }
        }
        Err(err) => {
            log::error!("failed to configure driver manager: manager lock poisoned: {err}");
            process::exit(1);
        }
    }

    let mgr_clone = Arc::clone(&manager);
    let scheme_clone = Arc::clone(&scheme);

    reset_timeline_log();

    if manager_config.async_probe {
        let handle = thread::spawn(move || {
            let (bound, deferred) = run_enumeration(&mgr_clone, scheme_clone.as_ref());
            log::info!("async enum: {} bound, {} deferred", bound, deferred);
        });
        if handle.join().is_err() {
            log::error!("initial enumeration thread panicked");
            process::exit(1);
        }
    } else {
        let (bound, deferred) = run_enumeration(&manager, scheme.as_ref());
        log::info!("enum complete: {} bound, {} deferred", bound, deferred);
    }

    if let Err(err) = scheme::start_scheme_server(Arc::clone(&scheme)) {
        log::error!("{err}");
        process::exit(1);
    }

    if hotplug_mode {
        log::info!("entering hotplug event loop");
        hotplug::run_hotplug_loop(manager.clone(), scheme.clone(), 2000);
        return;
    }

    let max_retries = 30u32;
    for retry in 1..=max_retries {
        thread::sleep(Duration::from_millis(500));

        let retry_events = match manager.lock() {
            Ok(mut mgr) => mgr.retry_deferred(),
            Err(err) => {
                log::error!("failed to retry deferred probes: manager lock poisoned: {err}");
                process::exit(1);
            }
        };

        let mut remaining = 0;
        let mut newly_bound = 0;

        for event in &retry_events {
            log_timeline(event);
            if let ProbeEvent::ProbeCompleted {
                device,
                driver_name,
                result,
            } = event
            {
                match result {
                    ProbeResult::Bound => {
                        newly_bound += 1;
                        notify_bound_device(scheme.as_ref(), device, driver_name);
                    }
                    ProbeResult::Deferred { .. } => remaining += 1,
                    _ => {}
                }
            }
        }

        if remaining == 0 {
            log::info!("all deferred resolved after {} retries", retry);
            return;
        }

        if newly_bound > 0 {
            log::info!(
                "retry #{}: {} new, {} remaining",
                retry,
                newly_bound,
                remaining
            );
        }
    }

    log::warn!("deferred probe retry limit reached");
    process::exit(0);
}
