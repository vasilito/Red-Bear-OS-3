use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use redox_driver_core::device::DeviceId;
use redox_driver_core::driver::ProbeResult;
use redox_driver_core::manager::DeviceManager;
use redox_driver_core::manager::ProbeEvent;

use crate::scheme::{DriverManagerScheme, notify_bind, notify_unbind};

pub fn run_hotplug_loop(
    manager: Arc<Mutex<DeviceManager>>,
    scheme: Arc<DriverManagerScheme>,
    poll_interval_ms: u64,
) {
    log::info!(
        "hotplug: starting event loop ({} ms poll)",
        poll_interval_ms
    );

    loop {
        thread::sleep(Duration::from_millis(poll_interval_ms));

        let events = match manager.lock() {
            Ok(mut mgr) => mgr.enumerate(),
            Err(err) => {
                log::error!("hotplug: failed to enumerate devices: manager lock poisoned: {err}");
                break;
            }
        };

        let mut seen_pci_devices = BTreeSet::new();
        let mut pci_enumerated = false;

        for event in &events {
            match event {
                ProbeEvent::BusEnumerated { bus, .. } => {
                    if bus == "pci" {
                        pci_enumerated = true;
                    }
                }
                ProbeEvent::BusEnumerationFailed { bus, error } => {
                    log::error!("hotplug: bus {} enumeration failed: {:?}", bus, error);
                }
                ProbeEvent::AlreadyBound {
                    device,
                    driver_name,
                } => {
                    track_pci_device(device, &mut seen_pci_devices);
                    notify_bound_device(scheme.as_ref(), device, driver_name);
                    log::debug!("hotplug: already bound {} -> {}", device.path, driver_name);
                }
                ProbeEvent::ProbeCompleted {
                    device,
                    driver_name,
                    result,
                } => {
                    track_pci_device(device, &mut seen_pci_devices);
                    match result {
                        ProbeResult::Bound => {
                            log::info!("hotplug: bound {} -> {}", device.path, driver_name);
                            notify_bound_device(scheme.as_ref(), device, driver_name);
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
                    track_pci_device(device, &mut seen_pci_devices);
                    log::debug!("hotplug: no driver for new device {}", device.path);
                }
                _ => {}
            }
        }

        if pci_enumerated {
            for pci_addr in scheme.bound_device_addresses() {
                if !seen_pci_devices.contains(&pci_addr) {
                    log::info!("hotplug: removed {}", pci_addr);
                    notify_unbind(scheme.as_ref(), &pci_addr);
                }
            }
        }

        let retry_events = match manager.lock() {
            Ok(mut mgr) => mgr.retry_deferred(),
            Err(err) => {
                log::error!(
                    "hotplug: failed to retry deferred probes: manager lock poisoned: {err}"
                );
                break;
            }
        };

        let mut resolved = 0usize;
        for event in &retry_events {
            if let ProbeEvent::ProbeCompleted {
                device,
                driver_name,
                result,
            } = event
            {
                if *result == ProbeResult::Bound {
                    resolved += 1;
                    notify_bound_device(scheme.as_ref(), device, driver_name);
                }
            }
        }

        if resolved > 0 {
            log::info!("hotplug: resolved {} deferred probes", resolved);
        }
    }
}

fn track_pci_device(device: &DeviceId, seen_pci_devices: &mut BTreeSet<String>) {
    if device.bus == "pci" {
        seen_pci_devices.insert(device.path.clone());
    }
}

fn notify_bound_device(scheme: &DriverManagerScheme, device: &DeviceId, driver_name: &str) {
    if device.bus == "pci" {
        notify_bind(scheme, &device.path, driver_name);
    }
}
