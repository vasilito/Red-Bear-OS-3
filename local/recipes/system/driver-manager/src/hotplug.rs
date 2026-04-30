use std::thread;
use std::time::Duration;
use std::sync::{Arc, Mutex};

use redox_driver_core::manager::DeviceManager;
use redox_driver_core::manager::ProbeEvent;
use redox_driver_core::driver::ProbeResult;

pub fn run_hotplug_loop(
    manager: Arc<Mutex<DeviceManager>>,
    poll_interval_ms: u64,
) {
    log::info!("hotplug: starting event loop ({} ms poll)", poll_interval_ms);

    loop {
        thread::sleep(Duration::from_millis(poll_interval_ms));

        let events = {
            let mut mgr = manager.lock().unwrap();
            mgr.enumerate()
        };

        for event in &events {
            match event {
                ProbeEvent::ProbeCompleted { device, driver_name, result } => {
                    match result {
                        ProbeResult::Bound => {
                            log::info!("hotplug: bound {} -> {}", device.path, driver_name);
                        }
                        ProbeResult::Deferred { reason } => {
                            log::info!(
                                "hotplug: deferred {} -> {} ({})",
                                device.path,
                                driver_name,
                                reason
                            );
                        }
                        ProbeResult::Fatal { reason } => {
                            log::error!(
                                "hotplug: fatal {} -> {} ({})",
                                device.path,
                                driver_name,
                                reason
                            );
                        }
                        _ => {}
                    }
                }
                ProbeEvent::NoDriverFound { device } => {
                    log::debug!("hotplug: no driver for new device {}", device.path);
                }
                _ => {}
            }
        }

        let retry_events = {
            let mut mgr = manager.lock().unwrap();
            mgr.retry_deferred()
        };

        let mut resolved = 0usize;
        for event in &retry_events {
            if let ProbeEvent::ProbeCompleted { result, .. } = event {
                if *result == ProbeResult::Bound {
                    resolved += 1;
                }
            }
        }

        if resolved > 0 {
            log::info!("hotplug: resolved {} deferred probes", resolved);
        }
    }
}
