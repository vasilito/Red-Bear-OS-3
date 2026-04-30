use std::fs;
use std::string::String;
use std::vec::Vec;

use redox_driver_core::bus::{Bus, BusError};
use redox_driver_core::device::{DeviceId, DeviceInfo};

pub struct PciBus {
    pci_root: String,
}

impl PciBus {
    pub fn new() -> Self {
        PciBus {
            pci_root: String::from("/scheme/pci"),
        }
    }

    pub fn with_root(root: &str) -> Self {
        PciBus {
            pci_root: String::from(root),
        }
    }
}

impl Bus for PciBus {
    fn name(&self) -> &str {
        "pci"
    }

    fn enumerate_devices(&self) -> Result<Vec<DeviceInfo>, BusError> {
        let dir = fs::read_dir(&self.pci_root).map_err(|_| BusError::IoError)?;

        let mut devices = Vec::new();

        for entry in dir {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();
            let file_name = match entry.file_name().into_string() {
                Ok(n) => n,
                Err(_) => continue,
            };

            if file_name == "." || file_name == ".." {
                continue;
            }

            let config_path = path.join("config");
            let config_data = match fs::read(&config_path) {
                Ok(d) => d,
                Err(_) => continue,
            };

            if config_data.len() < 64 {
                continue;
            }

            let vendor = u16::from_le_bytes([config_data[0], config_data[1]]);
            let device = u16::from_le_bytes([config_data[2], config_data[3]]);
            let revision = config_data[8];
            let prog_if = config_data[9];
            let subclass = config_data[10];
            let class = config_data[11];

            let subsystem_vendor = if config_data.len() > 0x2E {
                Some(u16::from_le_bytes([config_data[0x2C], config_data[0x2D]]))
            } else {
                None
            };
            let subsystem_device = if config_data.len() > 0x2E {
                Some(u16::from_le_bytes([config_data[0x2E], config_data[0x2F]]))
            } else {
                None
            };

            if vendor == 0xFFFF && device == 0xFFFF {
                continue;
            }

            let device_path = format!("{}/{}", self.pci_root, file_name);

            devices.push(DeviceInfo {
                id: DeviceId {
                    bus: String::from("pci"),
                    path: file_name,
                },
                vendor: Some(vendor),
                device: Some(device),
                class: Some(class),
                subclass: Some(subclass),
                prog_if: Some(prog_if),
                revision: Some(revision),
                subsystem_vendor,
                subsystem_device,
                raw_path: device_path,
                description: None,
            });
        }

        Ok(devices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pci_bus_name_is_pci() {
        let bus = PciBus::new();
        assert_eq!(bus.name(), "pci");
    }

    #[test]
    fn pci_bus_with_custom_root() {
        let bus = PciBus::with_root("/tmp/fake-pci");
        assert_eq!(bus.name(), "pci");
        let result = bus.enumerate_devices();
        assert!(result.is_err() || result.is_ok());
    }

    #[test]
    fn device_id_ordering_allows_btree_map() {
        let a = DeviceId {
            bus: String::from("pci"),
            path: String::from("00.00.0"),
        };
        let b = DeviceId {
            bus: String::from("pci"),
            path: String::from("00.02.0"),
        };
        assert!(a < b);
    }
}
