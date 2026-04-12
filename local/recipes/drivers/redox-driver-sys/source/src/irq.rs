use std::fs::File;
use std::io::{ErrorKind, Read};

#[cfg(target_os = "redox")]
use std::fs::OpenOptions;
#[cfg(target_os = "redox")]
use std::io::Write;

use crate::memory::{CacheType, MmioProt, MmioRegion};
use crate::pci::{MsixCapability, PciDevice, PciDeviceInfo};
use crate::{DriverError, Result};

const MSIX_ENTRY_SIZE: usize = 16;
const MSIX_VECTOR_CTRL_OFFSET: usize = 12;
const MSIX_MASK_BIT: u32 = 1;
#[cfg(target_os = "redox")]
const X86_MSI_ADDRESS_BASE: u64 = 0x0000_0000_FEE0_0000;

pub struct IrqHandle {
    fd: File,
    irq: u32,
}

#[derive(Debug)]
pub struct IrqEvent {
    pub irq: u32,
}

pub struct MsixTable {
    pub base: MmioRegion,
    pub pba: MmioRegion,
    pub table_size: u16,
    pub bar_addr: u64,
}

pub struct MsixVector {
    pub index: u16,
    pub irq: u32,
    pub fd: File,
}

impl IrqHandle {
    #[cfg(target_os = "redox")]
    pub fn request(irq: u32) -> Result<Self> {
        let path = format!("/scheme/irq/{irq}");
        let fd = File::open(&path).map_err(|e| {
            log::warn!("failed to open IRQ {irq} at {path}: {e}");
            e
        })?;
        log::debug!("IRQ {irq} acquired via {path}");
        Ok(Self { fd, irq })
    }

    #[cfg(not(target_os = "redox"))]
    pub fn request(irq: u32) -> Result<Self> {
        Err(DriverError::Irq(format!(
            "IRQ {irq} is only available on target_os=redox"
        )))
    }

    pub fn wait(&mut self) -> Result<IrqEvent> {
        let mut buf = [0u8; 8];
        self.fd.read_exact(&mut buf)?;
        Ok(IrqEvent { irq: self.irq })
    }

    pub fn try_wait(&mut self) -> Result<Option<IrqEvent>> {
        let mut buf = [0u8; 8];

        loop {
            match self.fd.read(&mut buf) {
                Ok(0) => return Ok(None),
                Ok(_) => return Ok(Some(IrqEvent { irq: self.irq })),
                Err(err) if err.kind() == ErrorKind::WouldBlock => return Ok(None),
                Err(err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(err) => return Err(err.into()),
            }
        }
    }

    #[cfg(target_os = "redox")]
    pub fn set_affinity(&self, cpu_mask: u64) -> Result<()> {
        let path = format!("/scheme/irq/{}/affinity", self.irq);
        let mut fd = OpenOptions::new().write(true).open(&path).map_err(|err| {
            DriverError::Irq(format!("failed to open IRQ affinity control {path}: {err}"))
        })?;
        fd.write_all(&cpu_mask.to_le_bytes())?;
        Ok(())
    }

    #[cfg(not(target_os = "redox"))]
    pub fn set_affinity(&self, _cpu_mask: u64) -> Result<()> {
        Err(DriverError::Irq(
            "IRQ affinity control is only available on target_os=redox".into(),
        ))
    }

    pub fn irq(&self) -> u32 {
        self.irq
    }
}

impl MsixTable {
    pub fn map(device_info: &PciDeviceInfo, cap: &MsixCapability) -> Result<Self> {
        let table_bar = lookup_msix_bar(device_info, cap.table_bar, "table")?;
        let pba_bar = lookup_msix_bar(device_info, cap.pba_bar, "PBA")?;

        let table_len = usize::from(cap.table_size) * MSIX_ENTRY_SIZE;
        let pba_len = usize::from(cap.table_size).div_ceil(64) * core::mem::size_of::<u64>();

        let table_phys =
            checked_bar_window(table_bar.addr, table_bar.size, cap.table_offset, table_len)?;
        let pba_phys = checked_bar_window(pba_bar.addr, pba_bar.size, cap.pba_offset, pba_len)?;

        let base = MmioRegion::map(
            table_phys,
            table_len,
            CacheType::DeviceMemory,
            MmioProt::READ_WRITE,
        )?;
        let pba = MmioRegion::map(
            pba_phys,
            pba_len,
            CacheType::DeviceMemory,
            MmioProt::READ_WRITE,
        )?;

        Ok(Self {
            base,
            pba,
            table_size: cap.table_size,
            bar_addr: table_bar.addr,
        })
    }

    pub fn mask_all(&self) {
        for index in 0..self.table_size {
            self.mask_vector(index);
        }
    }

    pub fn enable(&mut self, pci_device: &mut PciDevice, cap_offset: u8) -> Result<()> {
        pci_device.enable_msix(cap_offset)
    }

    #[cfg(target_os = "redox")]
    pub fn request_vector(&self, index: u16) -> Result<MsixVector> {
        let cpu_id = read_bsp_cpu_id()?;
        let (irq, fd) = allocate_irq_vector(cpu_id)?;
        self.program_x86_message(index, cpu_id, irq)?;
        self.unmask_vector(index);
        Ok(MsixVector { fd, index, irq })
    }

    #[cfg(not(target_os = "redox"))]
    pub fn request_vector(&self, index: u16) -> Result<MsixVector> {
        Err(DriverError::Irq(format!(
            "MSI-X vector {index} allocation is only available on target_os=redox"
        )))
    }

    pub fn mask_vector(&self, index: u16) {
        if let Ok(offset) = self.entry_offset(index) {
            self.base
                .write32(offset + MSIX_VECTOR_CTRL_OFFSET, MSIX_MASK_BIT);
        }
    }

    pub fn unmask_vector(&self, index: u16) {
        if let Ok(offset) = self.entry_offset(index) {
            self.base.write32(offset + MSIX_VECTOR_CTRL_OFFSET, 0);
        }
    }

    pub fn is_pending(&self, index: u16) -> bool {
        if index >= self.table_size {
            return false;
        }

        let word_index = usize::from(index / 64) * core::mem::size_of::<u64>();
        let bit = u32::from(index % 64);
        (self.pba.read64(word_index) & (1u64 << bit)) != 0
    }

    fn entry_offset(&self, index: u16) -> Result<usize> {
        if index >= self.table_size {
            return Err(DriverError::Irq(format!(
                "MSI-X vector index {index} is outside table size {}",
                self.table_size
            )));
        }
        Ok(usize::from(index) * MSIX_ENTRY_SIZE)
    }

    #[cfg(target_os = "redox")]
    fn program_x86_message(&self, index: u16, cpu_id: u8, irq: u32) -> Result<()> {
        let offset = self.entry_offset(index)?;
        let vector = irq
            .checked_add(32)
            .ok_or_else(|| DriverError::Irq(format!("IRQ {irq} overflowed x86 vector space")))?;
        let vector = u8::try_from(vector).map_err(|_| {
            DriverError::Irq(format!("IRQ {irq} does not fit in an x86 MSI-X vector"))
        })?;
        let message_addr = X86_MSI_ADDRESS_BASE | (u64::from(cpu_id) << 12);

        self.base.write32(offset, message_addr as u32);
        self.base.write32(offset + 4, (message_addr >> 32) as u32);
        self.base.write32(offset + 8, u32::from(vector));
        Ok(())
    }
}

fn lookup_msix_bar<'a>(
    device_info: &'a PciDeviceInfo,
    bar_index: u8,
    label: &str,
) -> Result<&'a crate::pci::PciBarInfo> {
    device_info
        .find_memory_bar(bar_index as usize)
        .ok_or_else(|| DriverError::CapabilityNotFound(format!("MSI-X {label} BAR {}", bar_index)))
}

fn checked_bar_window(bar_addr: u64, bar_size: u64, offset: u32, len: usize) -> Result<u64> {
    let len_u64 = u64::try_from(len)
        .map_err(|_| DriverError::InvalidParam("MSI-X BAR window length overflow"))?;
    let start = bar_addr
        .checked_add(u64::from(offset))
        .ok_or(DriverError::InvalidParam("MSI-X BAR address overflow"))?;
    let end = u64::from(offset)
        .checked_add(len_u64)
        .ok_or(DriverError::InvalidParam("MSI-X BAR range overflow"))?;

    if end > bar_size {
        return Err(DriverError::Irq(format!(
            "MSI-X BAR window offset {:#x} len {:#x} exceeds BAR size {:#x}",
            offset, len, bar_size
        )));
    }

    Ok(start)
}

#[cfg(target_os = "redox")]
fn read_bsp_cpu_id() -> Result<u8> {
    let mut fd = File::open("/scheme/irq/bsp")
        .map_err(|err| DriverError::Irq(format!("failed to open /scheme/irq/bsp: {err}")))?;
    let mut buf = [0u8; 8];
    let bytes_read = fd.read(&mut buf)?;

    let raw = match bytes_read {
        8 => u64::from_le_bytes(buf),
        4 => u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as u64,
        _ => {
            return Err(DriverError::Irq(format!(
                "unexpected /scheme/irq/bsp payload size {bytes_read}"
            )))
        }
    };

    u8::try_from(raw).map_err(|_| DriverError::Irq(format!("BSP CPU id {raw} does not fit in u8")))
}

#[cfg(target_os = "redox")]
fn allocate_irq_vector(cpu_id: u8) -> Result<(u32, File)> {
    let dir = format!("/scheme/irq/cpu-{cpu_id:02x}");
    let entries = std::fs::read_dir(&dir).map_err(|err| {
        DriverError::Irq(format!("failed to enumerate IRQ vectors in {dir}: {err}"))
    })?;

    let mut candidates = Vec::new();
    for entry in entries {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Ok(irq) = name.parse::<u32>() else {
            continue;
        };
        candidates.push(irq);
    }
    candidates.sort_unstable();

    for irq in candidates {
        let path = format!("{dir}/{irq}");
        match OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(fd) => return Ok((irq, fd)),
            Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
            Err(err) if err.kind() == ErrorKind::NotFound => continue,
            Err(err) => {
                return Err(DriverError::Irq(format!(
                    "failed to allocate MSI-X IRQ vector via {path}: {err}"
                )))
            }
        }
    }

    Err(DriverError::Irq(format!(
        "no free IRQ vectors available in {dir}"
    )))
}
