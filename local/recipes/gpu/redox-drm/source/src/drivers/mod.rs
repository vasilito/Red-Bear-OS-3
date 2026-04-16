pub mod amd;
pub mod intel;
pub mod interrupt;

use std::collections::HashMap;
use std::sync::Arc;

use log::info;
use redox_driver_sys::pci::{PciDevice, PciDeviceInfo, PCI_VENDOR_ID_AMD, PCI_VENDOR_ID_INTEL};
use redox_driver_sys::quirks::PciQuirkFlags;

use crate::driver::{DriverError, GpuDriver, Result};

pub struct DriverRegistry;

impl DriverRegistry {
    pub fn probe(
        info: PciDeviceInfo,
        firmware: HashMap<String, Vec<u8>>,
    ) -> Result<Arc<dyn GpuDriver>> {
        let full = if info.bars.is_empty() {
            let mut device = PciDevice::open_location(&info.location)
                .map_err(|e| DriverError::Pci(format!("open PCI device failed: {e}")))?;
            device
                .full_info()
                .map_err(|e| DriverError::Pci(format!("read PCI device info failed: {e}")))?
        } else {
            info
        };

        let quirks = full.quirks();
        if !quirks.is_empty() {
            info!(
                "redox-drm: quirks for {:#06x}:{:#06x}: {:?}",
                full.vendor_id, full.device_id, quirks
            );
        }

        if quirks.contains(PciQuirkFlags::DISABLE_ACCEL) {
            return Err(DriverError::Pci(format!(
                "device {:#06x}:{:#06x} at {} has DISABLE_ACCEL quirk — skipping probe",
                full.vendor_id, full.device_id, full.location
            )));
        }

        match full.vendor_id {
            PCI_VENDOR_ID_AMD => {
                let driver = amd::AmdDriver::new(full, firmware)?;
                Ok(Arc::new(driver))
            }
            PCI_VENDOR_ID_INTEL => {
                let driver = intel::IntelDriver::new(full, firmware)?;
                Ok(Arc::new(driver))
            }
            _ => Err(DriverError::Pci(format!(
                "unsupported GPU vendor {:#06x} at {}",
                full.vendor_id, full.location
            ))),
        }
    }
}
