pub mod amd;
pub mod intel;

use std::collections::HashMap;
use std::sync::Arc;

use redox_driver_sys::pci::{PciDevice, PciDeviceInfo, PCI_VENDOR_ID_AMD, PCI_VENDOR_ID_INTEL};

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
