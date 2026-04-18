use std::io::{Read, Write};

use log::{info, warn};
use redox_driver_sys::irq::{IrqHandle, MsixTable, MsixVector};
use redox_driver_sys::pci::{PciDevice, PciDeviceInfo, PCI_CAP_ID_MSI, PCI_CAP_ID_MSIX};
use redox_driver_sys::quirks::PciQuirkFlags;

use crate::driver::{DriverError, Result};

pub enum InterruptHandle {
    Msix {
        vector: MsixVector,
        table: MsixTable,
        cap_offset: u8,
    },
    Msi {
        handle: IrqHandle,
        irq: u32,
    },
    Legacy {
        handle: IrqHandle,
        irq: u32,
    },
}

fn force_legacy_irq(quirks: PciQuirkFlags) -> bool {
    quirks.contains(PciQuirkFlags::FORCE_LEGACY_IRQ)
}

impl InterruptHandle {
    pub fn setup(device_info: &PciDeviceInfo, pci_device: &mut PciDevice) -> Result<Self> {
        let quirks = device_info.quirks();

        if force_legacy_irq(quirks) {
            info!(
                "redox-drm: forcing legacy IRQ for {} (FORCE_LEGACY_IRQ quirk)",
                device_info.location
            );
            return Self::try_legacy(device_info);
        }

        if !quirks.contains(PciQuirkFlags::NO_MSIX) {
            if let Ok(Some(handle)) = Self::try_msix(device_info, pci_device) {
                return Ok(handle);
            }
        } else {
            info!(
                "redox-drm: skipping MSI-X for {} (NO_MSIX quirk)",
                device_info.location
            );
        }

        if !quirks.contains(PciQuirkFlags::NO_MSI) {
            if let Ok(Some(handle)) = Self::try_msi(device_info, pci_device) {
                return Ok(handle);
            }
        } else {
            info!(
                "redox-drm: skipping MSI for {} (NO_MSI quirk)",
                device_info.location
            );
        }

        Self::try_legacy(device_info)
    }

    fn try_msix(device_info: &PciDeviceInfo, pci_device: &mut PciDevice) -> Result<Option<Self>> {
        let msix_cap = match device_info.find_capability(PCI_CAP_ID_MSIX) {
            Some(cap) => cap,
            None => return Ok(None),
        };

        let msix_info = match pci_device.parse_msix(msix_cap.offset) {
            Ok(info) => info,
            Err(e) => {
                warn!(
                    "redox-drm: MSI-X capability parse failed for {}: {e}",
                    device_info.location
                );
                return Ok(None);
            }
        };

        let table = match MsixTable::map(device_info, &msix_info) {
            Ok(t) => t,
            Err(e) => {
                warn!(
                    "redox-drm: MSI-X table map failed for {}: {e}",
                    device_info.location
                );
                return Ok(None);
            }
        };

        table.mask_all();

        if let Err(e) = pci_device.enable_msix(msix_cap.offset) {
            warn!(
                "redox-drm: MSI-X enable failed for {}: {e}",
                device_info.location
            );
            return Ok(None);
        }

        let vector = match table.request_vector(0) {
            Ok(v) => v,
            Err(e) => {
                warn!(
                    "redox-drm: MSI-X vector allocation failed for {}: {e}",
                    device_info.location
                );
                let _ = pci_device.disable_msix(msix_cap.offset);
                return Ok(None);
            }
        };

        info!(
            "redox-drm: MSI-X enabled for {} vector {} irq {}",
            device_info.location, vector.index, vector.irq
        );

        Ok(Some(InterruptHandle::Msix {
            vector,
            table,
            cap_offset: msix_cap.offset,
        }))
    }

    fn try_msi(device_info: &PciDeviceInfo, _pci_device: &mut PciDevice) -> Result<Option<Self>> {
        let msi_cap = match device_info.find_capability(PCI_CAP_ID_MSI) {
            Some(cap) => cap,
            None => return Ok(None),
        };

        let irq = device_info.irq.ok_or_else(|| {
            DriverError::Io(format!("no IRQ for MSI on {}", device_info.location))
        })?;

        let handle = match IrqHandle::request(irq) {
            Ok(h) => h,
            Err(e) => {
                warn!(
                    "redox-drm: MSI IRQ request failed for {}: {e}",
                    device_info.location
                );
                return Ok(None);
            }
        };

        info!(
            "redox-drm: MSI enabled for {} cap_offset={:#x} irq {}",
            device_info.location, msi_cap.offset, irq
        );

        Ok(Some(InterruptHandle::Msi { handle, irq }))
    }

    fn try_legacy(device_info: &PciDeviceInfo) -> Result<Self> {
        let irq = device_info
            .irq
            .ok_or_else(|| DriverError::Io(format!("no IRQ for {}", device_info.location)))?;

        let handle = IrqHandle::request(irq).map_err(|e| DriverError::Io(e.to_string()))?;
        info!(
            "redox-drm: using legacy IRQ {irq} for {}",
            device_info.location
        );

        Ok(InterruptHandle::Legacy { handle, irq })
    }

    pub fn try_wait(&mut self) -> Result<bool> {
        match self {
            InterruptHandle::Msix { vector, .. } => {
                let mut buf = [0u8; 8];
                match vector.fd.read(&mut buf) {
                    Ok(n) if n > 0 => Ok(true),
                    Ok(_) => Ok(false),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
                    Err(e) => Err(DriverError::Io(e.to_string())),
                }
            }
            InterruptHandle::Msi { handle, .. } | InterruptHandle::Legacy { handle, .. } => handle
                .try_wait()
                .map(|ev| ev.is_some())
                .map_err(|e| DriverError::Io(e.to_string())),
        }
    }

    pub fn eoi(&mut self) -> Result<()> {
        match self {
            InterruptHandle::Msix { vector, .. } => {
                let mut buf = [0u8; 8];
                vector
                    .fd
                    .read_exact(&mut buf)
                    .map_err(|e| DriverError::Io(e.to_string()))?;
                vector
                    .fd
                    .write_all(&buf)
                    .map_err(|e| DriverError::Io(e.to_string()))
            }
            InterruptHandle::Msi { handle, .. } | InterruptHandle::Legacy { handle, .. } => {
                let _ = handle.wait().map_err(|e| DriverError::Io(e.to_string()))?;
                Ok(())
            }
        }
    }

    pub fn irq(&self) -> u32 {
        match self {
            InterruptHandle::Msix { vector, .. } => vector.irq,
            InterruptHandle::Msi { irq, .. } | InterruptHandle::Legacy { irq, .. } => *irq,
        }
    }

    pub fn mode_name(&self) -> &'static str {
        match self {
            InterruptHandle::Msix { .. } => "MSI-X",
            InterruptHandle::Msi { .. } => "MSI",
            InterruptHandle::Legacy { .. } => "legacy INTx",
        }
    }

    pub fn is_msix(&self) -> bool {
        matches!(self, InterruptHandle::Msix { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::force_legacy_irq;
    use redox_driver_sys::quirks::PciQuirkFlags;

    #[test]
    fn force_legacy_irq_only_triggers_on_quirk() {
        assert!(!force_legacy_irq(PciQuirkFlags::empty()));
        assert!(!force_legacy_irq(PciQuirkFlags::NO_MSI));
        assert!(force_legacy_irq(PciQuirkFlags::FORCE_LEGACY_IRQ));
        assert!(force_legacy_irq(
            PciQuirkFlags::FORCE_LEGACY_IRQ | PciQuirkFlags::NO_MSIX
        ));
    }
}
