use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::bus::{Bus, BusError};
use crate::device::{BoundDevice, DeviceId, DeviceInfo};
use crate::driver::{Driver, ProbeResult};

/// Event emitted by the device manager during discovery or deferred-probe processing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProbeEvent {
    /// A bus finished enumeration and reported the number of discovered devices.
    BusEnumerated {
        /// Bus name returned by the [`Bus`] implementation.
        bus: String,
        /// Number of devices returned by the bus.
        device_count: usize,
    },
    /// A bus failed to enumerate devices.
    BusEnumerationFailed {
        /// Bus name returned by the [`Bus`] implementation.
        bus: String,
        /// Error returned by the bus.
        error: BusError,
    },
    /// The manager skipped probing because the device is already bound.
    AlreadyBound {
        /// Identifier of the device that was skipped.
        device: DeviceId,
        /// Driver that already owns the device.
        driver_name: String,
    },
    /// A driver completed a probe attempt for a device.
    ProbeCompleted {
        /// Identifier of the probed device.
        device: DeviceId,
        /// Driver that performed the probe.
        driver_name: String,
        /// Result returned by the driver's probe method.
        result: ProbeResult,
    },
    /// No registered driver had a matching table entry for the device.
    NoDriverFound {
        /// Identifier of the unmatched device.
        device: DeviceId,
    },
    /// A deferred probe referenced a driver that is no longer registered.
    MissingDriver {
        /// Identifier of the affected device.
        device: DeviceId,
        /// Driver name that could not be found.
        driver_name: String,
    },
}

/// Configuration for the central [`DeviceManager`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagerConfig {
    /// Maximum number of probes the manager should allow concurrently.
    ///
    /// The current implementation probes synchronously and stores this as policy metadata for
    /// future async or threaded executors.
    pub max_concurrent_probes: usize,
    /// Interval, in milliseconds, between deferred-probe retries.
    pub deferred_retry_ms: u64,
    /// Whether the manager should prefer asynchronous probing when an executor is available.
    pub async_probe: bool,
}

/// Central device manager that orchestrates device discovery and driver binding.
pub struct DeviceManager {
    buses: Vec<Box<dyn Bus>>,
    drivers: Vec<Box<dyn Driver>>,
    bound_devices: BTreeMap<DeviceId, BoundDevice>,
    deferred_queue: Vec<(DeviceInfo, String)>,
    config: ManagerConfig,
}

impl DeviceManager {
    /// Creates a new device manager with the provided policy configuration.
    pub fn new(config: ManagerConfig) -> Self {
        Self {
            buses: Vec::new(),
            drivers: Vec::new(),
            bound_devices: BTreeMap::new(),
            deferred_queue: Vec::new(),
            config,
        }
    }

    /// Registers a bus that will be included in future enumeration cycles.
    pub fn register_bus(&mut self, bus: Box<dyn Bus>) {
        self.buses.push(bus);
    }

    /// Registers a driver and reorders drivers so higher-priority probes run first.
    pub fn register_driver(&mut self, driver: Box<dyn Driver>) {
        self.drivers.push(driver);
        self.drivers
            .sort_by(|left, right| right.priority().cmp(&left.priority()));
    }

    /// Runs a full enumeration cycle across all registered buses.
    pub fn enumerate(&mut self) -> Vec<ProbeEvent> {
        let _probe_budget = self.config.max_concurrent_probes.max(1);
        let _async_probe = self.config.async_probe;

        let mut events = Vec::new();

        for bus_index in 0..self.buses.len() {
            let (bus_name, enumeration) = {
                let bus = &self.buses[bus_index];
                (bus.name().to_string(), bus.enumerate_devices())
            };

            match enumeration {
                Ok(devices) => {
                    events.push(ProbeEvent::BusEnumerated {
                        bus: bus_name,
                        device_count: devices.len(),
                    });

                    for info in devices {
                        if let Some(bound) = self.bound_devices.get(&info.id) {
                            events.push(ProbeEvent::AlreadyBound {
                                device: info.id.clone(),
                                driver_name: bound.driver_name.clone(),
                            });
                            continue;
                        }

                        self.probe_device(info, &mut events);
                    }
                }
                Err(error) => events.push(ProbeEvent::BusEnumerationFailed {
                    bus: bus_name,
                    error,
                }),
            }
        }

        events
    }

    /// Retries all deferred probe attempts in registration order.
    pub fn retry_deferred(&mut self) -> Vec<ProbeEvent> {
        let _retry_interval_ms = self.config.deferred_retry_ms;

        let mut events = Vec::new();
        let deferred = core::mem::take(&mut self.deferred_queue);

        for (info, driver_name) in deferred {
            if let Some(bound) = self.bound_devices.get(&info.id) {
                events.push(ProbeEvent::AlreadyBound {
                    device: info.id.clone(),
                    driver_name: bound.driver_name.clone(),
                });
                continue;
            }

            let Some(driver_index) = self
                .drivers
                .iter()
                .position(|driver| driver.name() == driver_name)
            else {
                events.push(ProbeEvent::MissingDriver {
                    device: info.id.clone(),
                    driver_name,
                });
                continue;
            };

            let (probe_driver_name, result) = {
                let driver = &self.drivers[driver_index];
                (driver.name().to_string(), driver.probe(&info))
            };

            match &result {
                ProbeResult::Bound => {
                    self.bound_devices.insert(
                        info.id.clone(),
                        BoundDevice {
                            info: info.clone(),
                            driver_name: probe_driver_name.clone(),
                            parameters: BTreeMap::new(),
                        },
                    );
                }
                ProbeResult::Deferred { .. } => {
                    self.enqueue_deferred(info.clone(), probe_driver_name.clone());
                }
                ProbeResult::NotSupported | ProbeResult::Fatal { .. } => {}
            }

            events.push(ProbeEvent::ProbeCompleted {
                device: info.id.clone(),
                driver_name: probe_driver_name,
                result,
            });
        }

        events
    }

    fn probe_device(&mut self, info: DeviceInfo, events: &mut Vec<ProbeEvent>) {
        let mut matched = false;

        for driver_index in 0..self.drivers.len() {
            let is_match = {
                let driver = &self.drivers[driver_index];
                driver
                    .match_table()
                    .iter()
                    .any(|driver_match| driver_match.matches(&info))
            };

            if !is_match {
                continue;
            }

            matched = true;
            let (driver_name, result) = {
                let driver = &self.drivers[driver_index];
                (driver.name().to_string(), driver.probe(&info))
            };

            match &result {
                ProbeResult::Bound => {
                    self.bound_devices.insert(
                        info.id.clone(),
                        BoundDevice {
                            info: info.clone(),
                            driver_name: driver_name.clone(),
                            parameters: BTreeMap::new(),
                        },
                    );

                    events.push(ProbeEvent::ProbeCompleted {
                        device: info.id.clone(),
                        driver_name,
                        result,
                    });
                    return;
                }
                ProbeResult::Deferred { .. } => {
                    self.enqueue_deferred(info.clone(), driver_name.clone());
                    events.push(ProbeEvent::ProbeCompleted {
                        device: info.id.clone(),
                        driver_name,
                        result,
                    });
                    return;
                }
                ProbeResult::Fatal { .. } => {
                    events.push(ProbeEvent::ProbeCompleted {
                        device: info.id.clone(),
                        driver_name,
                        result,
                    });
                    return;
                }
                ProbeResult::NotSupported => {
                    events.push(ProbeEvent::ProbeCompleted {
                        device: info.id.clone(),
                        driver_name,
                        result,
                    });
                }
            }
        }

        if !matched {
            events.push(ProbeEvent::NoDriverFound { device: info.id });
        }
    }

    fn enqueue_deferred(&mut self, info: DeviceInfo, driver_name: String) {
        let already_queued = self.deferred_queue.iter().any(|(queued_info, queued_driver)| {
            queued_info.id == info.id && queued_driver == &driver_name
        });

        if !already_queued {
            self.deferred_queue.push((info, driver_name));
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;

    use super::{DeviceManager, ManagerConfig};
    use crate::bus::{Bus, BusError};
    use crate::device::{DeviceId, DeviceInfo};
    use crate::driver::{Driver, DriverError, ProbeResult};
    use crate::r#match::DriverMatch;

    struct MockBus {
        name: &'static str,
        devices: Vec<DeviceInfo>,
    }

    impl Bus for MockBus {
        fn name(&self) -> &str {
            self.name
        }

        fn enumerate_devices(&self) -> Result<Vec<DeviceInfo>, BusError> {
            Ok(self.devices.clone())
        }
    }

    struct MockDriver {
        name: &'static str,
        description: &'static str,
        priority: i32,
        matches: Vec<DriverMatch>,
    }

    impl Driver for MockDriver {
        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            self.description
        }

        fn priority(&self) -> i32 {
            self.priority
        }

        fn match_table(&self) -> &[DriverMatch] {
            self.matches.as_slice()
        }

        fn probe(&self, _info: &DeviceInfo) -> ProbeResult {
            ProbeResult::NotSupported
        }

        fn remove(&self, _info: &DeviceInfo) -> Result<(), DriverError> {
            Ok(())
        }
    }

    fn config() -> ManagerConfig {
        ManagerConfig {
            max_concurrent_probes: 4,
            deferred_retry_ms: 250,
            async_probe: false,
        }
    }

    #[test]
    fn register_bus_and_driver_store_entries() {
        let mut manager = DeviceManager::new(config());

        manager.register_bus(Box::new(MockBus {
            name: "pci",
            devices: Vec::new(),
        }));
        manager.register_driver(Box::new(MockDriver {
            name: "low",
            description: "low-priority driver",
            priority: 10,
            matches: vec![DriverMatch {
                vendor: Some(0x1234),
                device: None,
                class: None,
                subclass: None,
                prog_if: None,
                subsystem_vendor: None,
                subsystem_device: None,
            }],
        }));
        manager.register_driver(Box::new(MockDriver {
            name: "high",
            description: "high-priority driver",
            priority: 100,
            matches: vec![DriverMatch {
                vendor: Some(0x1234),
                device: Some(0x5678),
                class: None,
                subclass: None,
                prog_if: None,
                subsystem_vendor: None,
                subsystem_device: None,
            }],
        }));

        assert_eq!(manager.buses.len(), 1);
        assert_eq!(manager.drivers.len(), 2);
        assert_eq!(manager.drivers[0].name(), "high");
        assert_eq!(manager.drivers[1].name(), "low");
    }

    #[test]
    fn enumerate_reports_registered_bus() {
        let mut manager = DeviceManager::new(config());

        manager.register_bus(Box::new(MockBus {
            name: "pci",
            devices: vec![DeviceInfo {
                id: DeviceId {
                    bus: String::from("pci"),
                    path: String::from("0000:00:1f.2"),
                },
                vendor: Some(0x8086),
                device: Some(0x2922),
                class: Some(0x01),
                subclass: Some(0x06),
                prog_if: Some(0x01),
                revision: Some(0x02),
                subsystem_vendor: None,
                subsystem_device: None,
                raw_path: String::from("/scheme/pci/00.1f.2"),
                description: Some(String::from("AHCI controller")),
            }],
        }));

        let events = manager.enumerate();

        assert!(events.iter().any(|event| matches!(
            event,
            super::ProbeEvent::BusEnumerated { bus, device_count }
            if bus == "pci" && *device_count == 1
        )));
    }
}
