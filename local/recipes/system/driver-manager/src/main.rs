mod config;
mod exec;
mod hotplug;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use std::{process, env};

use redox_driver_core::manager::{DeviceManager, ManagerConfig, ProbeEvent};
use redox_driver_core::driver::ProbeResult;
use redox_driver_pci::PciBus;

use config::DriverConfig;

struct StderrLogger;

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
) -> (usize, usize) {
    let events = {
        let mut mgr = manager.lock().unwrap();
        mgr.enumerate()
    };

    let mut bound = 0usize;
    let mut deferred = 0usize;
    let mut durations: Vec<u128> = Vec::new();

    for event in &events {
        match event {
            ProbeEvent::ProbeCompleted { device, driver_name, result } => {
                let start = Instant::now();
                match result {
                    ProbeResult::Bound => {
                        let duration = start.elapsed();
                        durations.push(duration.as_millis());
                        log::info!("probed: {} -> {} ({}ms)", device.path, driver_name, duration.as_millis());
                        log::info!("bound: {} -> {}", device.path, driver_name);
                        bound += 1;
                    }
                    ProbeResult::Deferred { reason } => {
                        let duration = start.elapsed();
                        log::info!("probed: {} -> {} ({}ms)", device.path, driver_name, duration.as_millis());
                        log::info!("deferred: {} -> {} ({})", device.path, driver_name, reason);
                        deferred += 1;
                    }
                    ProbeResult::Fatal { reason } => {
                        let duration = start.elapsed();
                        log::info!("probed: {} -> {} ({}ms)", device.path, driver_name, duration.as_millis());
                        log::error!("fatal: {} -> {} ({})", device.path, driver_name, reason);
                    }
                    _ => {}
                }
            }
            ProbeEvent::BusEnumerated { bus, device_count } => {
                log::info!("bus {} enumerated {} device(s)", bus, device_count);
            }
            _ => {}
        }
    }

    if !durations.is_empty() {
        let sum: u128 = durations.iter().sum();
        let avg = sum / durations.len() as u128;
        let max = *durations.iter().max().unwrap_or(&0);
        log::info!("probe summary: {} drivers, avg {}ms, max {}ms", durations.len(), avg, max);
    }

    (bound, deferred)
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

    {
        let mut mgr = manager.lock().unwrap();
        mgr.register_bus(Box::new(PciBus::new()));

        for dc in &driver_configs {
            mgr.register_driver(Box::new(dc.clone()));
        }
    }

    let mgr_clone = Arc::clone(&manager);

    if manager_config.async_probe {
        let handle = thread::spawn(move || {
            let (bound, deferred) = run_enumeration(&mgr_clone);
            log::info!("async enum: {} bound, {} deferred", bound, deferred);
        });
        let _ = handle.join();
    } else {
        let (bound, deferred) = run_enumeration(&manager);
        log::info!("enum complete: {} bound, {} deferred", bound, deferred);
    }

    if hotplug_mode {
        log::info!("entering hotplug event loop");
        hotplug::run_hotplug_loop(manager.clone(), 2000);
        return;
    }

    let max_retries = 30u32;
    for retry in 1..=max_retries {
        thread::sleep(Duration::from_millis(500));

        let retry_events = {
            let mut mgr = manager.lock().unwrap();
            mgr.retry_deferred()
        };

        let mut remaining = 0;
        let mut newly_bound = 0;

        for event in &retry_events {
            if let ProbeEvent::ProbeCompleted { result, .. } = event {
                match result {
                    ProbeResult::Bound => newly_bound += 1,
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
            log::info!("retry #{}: {} new, {} remaining", retry, newly_bound, remaining);
        }
    }

    log::warn!("deferred probe retry limit reached");
    process::exit(0);
}
