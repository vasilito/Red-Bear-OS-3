use std::io::{Read, Seek, SeekFrom, Write};

use crate::{DriverError, Result};

pub const PCI_VENDOR_ID_AMD: u16 = 0x1002;
pub const PCI_VENDOR_ID_INTEL: u16 = 0x8086;
pub const PCI_VENDOR_ID_NVIDIA: u16 = 0x10DE;

pub const PCI_CLASS_DISPLAY: u8 = 0x03;
pub const PCI_CLASS_DISPLAY_VGA: u8 = 0x00;
pub const PCI_CLASS_DISPLAY_3D: u8 = 0x02;

pub const PCI_HEADER_TYPE_NORMAL: u8 = 0x00;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PciLocation {
    pub segment: u16,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

impl PciLocation {
    pub fn scheme_path(&self) -> String {
        format!(
            "/scheme/pci/{:04x}--{:02x}--{:02x}.{}",
            self.segment, self.bus, self.device, self.function
        )
    }

    pub fn bdf(&self) -> u32 {
        ((self.bus as u32) << 16)
            | ((self.device as u32) & 0x1F) << 11
            | ((self.function as u32) & 0x07) << 8
    }

    pub fn from_bdf(bdf: u32) -> Self {
        PciLocation {
            segment: 0,
            bus: ((bdf >> 16) & 0xFF) as u8,
            device: ((bdf >> 11) & 0x1F) as u8,
            function: ((bdf >> 8) & 0x07) as u8,
        }
    }
}

impl std::fmt::Display for PciLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:04x}:{:02x}:{:02x}.{}",
            self.segment, self.bus, self.device, self.function
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PciBarInfo {
    pub index: usize,
    pub kind: PciBarKind,
    pub addr: u64,
    pub size: u64,
    pub prefetchable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PciBarKind {
    Memory32,
    Memory64,
    Io,
    None,
}

impl PciBarInfo {
    pub fn is_memory(&self) -> bool {
        matches!(self.kind, PciBarKind::Memory32 | PciBarKind::Memory64)
    }

    pub fn is_io(&self) -> bool {
        self.kind == PciBarKind::Io
    }

    pub fn memory_info(&self) -> Option<(u64, usize)> {
        if self.is_memory() && self.addr != 0 && self.size != 0 {
            Some((self.addr, self.size as usize))
        } else {
            None
        }
    }

    pub fn io_port(&self) -> Option<u16> {
        if self.is_io() && self.addr != 0 {
            Some(self.addr as u16)
        } else {
            None
        }
    }
}

pub const PCI_CMD_IO_SPACE: u16 = 0x0001;
pub const PCI_CMD_MEMORY_SPACE: u16 = 0x0002;
pub const PCI_CMD_BUS_MASTER: u16 = 0x0004;
pub const PCI_CMD_MEM_WRITE_INVALIDATE: u16 = 0x0010;
pub const PCI_CMD_PARITY_ERROR_RESPONSE: u16 = 0x0040;
pub const PCI_CMD_SERR_ENABLE: u16 = 0x0100;
pub const PCI_CMD_INTX_DISABLE: u16 = 0x0400;

#[derive(Clone, Debug)]
pub struct PciCapability {
    pub id: u8,
    pub offset: u8,
    pub vendor_cap_id: Option<u8>,
}

pub const PCI_CAP_ID_MSI: u8 = 0x05;
pub const PCI_CAP_ID_MSIX: u8 = 0x11;
pub const PCI_CAP_ID_PCIE: u8 = 0x10;
pub const PCI_CAP_ID_POWER: u8 = 0x01;
pub const PCI_CAP_ID_VNDR: u8 = 0x09;

#[derive(Clone, Debug)]
pub struct MsixCapability {
    pub table_bar: u8,
    pub table_offset: u32,
    pub pba_bar: u8,
    pub pba_offset: u32,
    pub table_size: u16,
    pub masked: bool,
}

#[derive(Clone, Debug)]
pub struct PciDeviceInfo {
    pub location: PciLocation,
    pub vendor_id: u16,
    pub device_id: u16,
    pub revision: u8,
    pub class_code: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub header_type: u8,
    pub irq: Option<u32>,
    pub bars: Vec<PciBarInfo>,
    pub capabilities: Vec<PciCapability>,
}

impl PciDeviceInfo {
    pub fn is_gpu(&self) -> bool {
        self.class_code == PCI_CLASS_DISPLAY
    }

    pub fn is_amd_gpu(&self) -> bool {
        self.class_code == PCI_CLASS_DISPLAY && self.vendor_id == PCI_VENDOR_ID_AMD
    }

    pub fn is_intel_gpu(&self) -> bool {
        self.class_code == PCI_CLASS_DISPLAY && self.vendor_id == PCI_VENDOR_ID_INTEL
    }

    pub fn find_capability(&self, id: u8) -> Option<&PciCapability> {
        self.capabilities.iter().find(|c| c.id == id)
    }

    pub fn find_msix(&self) -> Option<MsixCapability> {
        self.find_capability(PCI_CAP_ID_MSIX).and_then(|cap| {
            let mut dev = PciDevice::from_info(self).ok()?;
            dev.parse_msix(cap.offset).ok()
        })
    }

    pub fn find_memory_bar(&self, index: usize) -> Option<&PciBarInfo> {
        self.bars.iter().find(|b| b.index == index && b.is_memory())
    }
}

pub struct PciDevice {
    location: PciLocation,
    config_fd: std::fs::File,
}

impl PciDevice {
    pub fn open(segment: u16, bus: u8, device: u8, function: u8) -> Result<Self> {
        let loc = PciLocation {
            segment,
            bus,
            device,
            function,
        };
        Self::open_location(&loc)
    }

    pub fn open_location(loc: &PciLocation) -> Result<Self> {
        let config_path = format!("{}/config", loc.scheme_path());
        let fd = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&config_path)
            .map_err(|e| {
                DriverError::Pci(format!("cannot open PCI config at {}: {}", config_path, e))
            })?;
        Ok(PciDevice {
            location: *loc,
            config_fd: fd,
        })
    }

    pub fn from_info(info: &PciDeviceInfo) -> Result<Self> {
        Self::open_location(&info.location)
    }

    pub fn location(&self) -> &PciLocation {
        &self.location
    }

    pub fn read_config_dword(&mut self, offset: u64) -> Result<u32> {
        self.config_fd.seek(SeekFrom::Start(offset))?;
        let mut buf = [0u8; 4];
        self.config_fd.read_exact(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    pub fn read_config_word(&mut self, offset: u64) -> Result<u16> {
        self.config_fd.seek(SeekFrom::Start(offset))?;
        let mut buf = [0u8; 2];
        self.config_fd.read_exact(&mut buf)?;
        Ok(u16::from_le_bytes(buf))
    }

    pub fn read_config_byte(&mut self, offset: u64) -> Result<u8> {
        self.config_fd.seek(SeekFrom::Start(offset))?;
        let mut buf = [0u8; 1];
        self.config_fd.read_exact(&mut buf)?;
        Ok(buf[0])
    }

    pub fn write_config_dword(&mut self, offset: u64, val: u32) -> Result<()> {
        self.config_fd.seek(SeekFrom::Start(offset))?;
        self.config_fd.write_all(&val.to_le_bytes())?;
        Ok(())
    }

    pub fn write_config_word(&mut self, offset: u64, val: u16) -> Result<()> {
        self.config_fd.seek(SeekFrom::Start(offset))?;
        self.config_fd.write_all(&val.to_le_bytes())?;
        Ok(())
    }

    pub fn write_config_byte(&mut self, offset: u64, val: u8) -> Result<()> {
        self.config_fd.seek(SeekFrom::Start(offset))?;
        self.config_fd.write_all(&[val])?;
        Ok(())
    }

    pub fn vendor_id(&mut self) -> Result<u16> {
        self.read_config_word(0x00)
    }

    pub fn device_id(&mut self) -> Result<u16> {
        self.read_config_word(0x02)
    }

    pub fn command(&mut self) -> Result<u16> {
        self.read_config_word(0x04)
    }

    pub fn set_command(&mut self, flags: u16) -> Result<()> {
        self.write_config_word(0x04, flags)
    }

    pub fn enable_device(&mut self) -> Result<()> {
        let mut cmd = self.command()?;
        cmd |= PCI_CMD_IO_SPACE | PCI_CMD_MEMORY_SPACE | PCI_CMD_BUS_MASTER;
        self.set_command(cmd)
    }

    pub fn set_bus_master(&mut self, enable: bool) -> Result<()> {
        let mut cmd = self.command()?;
        if enable {
            cmd |= 0x0004;
        } else {
            cmd &= !0x0004;
        }
        self.set_command(cmd)
    }

    pub fn set_intx_disable(&mut self, disable: bool) -> Result<()> {
        let mut cmd = self.command()?;
        if disable {
            cmd |= 0x0400;
        } else {
            cmd &= !0x0400;
        }
        self.set_command(cmd)
    }

    pub fn status(&mut self) -> Result<u16> {
        self.read_config_word(0x06)
    }

    pub fn revision(&mut self) -> Result<u8> {
        self.read_config_byte(0x08)
    }

    pub fn class_code(&mut self) -> Result<u8> {
        self.read_config_byte(0x0B)
    }

    pub fn subclass(&mut self) -> Result<u8> {
        self.read_config_byte(0x0A)
    }

    pub fn prog_if(&mut self) -> Result<u8> {
        self.read_config_byte(0x09)
    }

    pub fn header_type(&mut self) -> Result<u8> {
        let ht = self.read_config_byte(0x0E)?;
        Ok(ht & 0x7F)
    }

    pub fn is_multi_function(&mut self) -> Result<bool> {
        let ht = self.read_config_byte(0x0E)?;
        Ok(ht & 0x80 != 0)
    }

    pub fn irq_line(&mut self) -> Result<u8> {
        self.read_config_byte(0x3C)
    }

    pub fn irq_pin(&mut self) -> Result<u8> {
        self.read_config_byte(0x3D)
    }

    pub fn full_info(&mut self) -> Result<PciDeviceInfo> {
        let vendor_id = self.vendor_id()?;
        let device_id = self.device_id()?;
        let revision = self.revision()?;
        let prog_if = self.prog_if()?;
        let subclass = self.subclass()?;
        let class_code = self.class_code()?;
        let header_type = self.header_type()?;
        let irq_byte = self.irq_line()?;
        let bars = if header_type == PCI_HEADER_TYPE_NORMAL {
            self.parse_bars()?
        } else {
            Vec::new()
        };
        let capabilities = if header_type == PCI_HEADER_TYPE_NORMAL {
            self.parse_capabilities()?
        } else {
            Vec::new()
        };

        Ok(PciDeviceInfo {
            location: self.location,
            vendor_id,
            device_id,
            revision,
            class_code,
            subclass,
            prog_if,
            header_type,
            irq: if irq_byte != 0 && irq_byte != 0xFF {
                Some(irq_byte as u32)
            } else {
                None
            },
            bars,
            capabilities,
        })
    }

    pub fn parse_bars(&mut self) -> Result<Vec<PciBarInfo>> {
        let mut bars = Vec::with_capacity(6);
        let mut bar_idx = 0usize;
        let mut config_offset = 0x10u64;

        while bar_idx < 6 && config_offset <= 0x24 {
            let val_lo = self.read_config_dword(config_offset)?;

            if val_lo == 0 {
                bars.push(PciBarInfo {
                    index: bar_idx,
                    kind: PciBarKind::None,
                    addr: 0,
                    size: 0,
                    prefetchable: false,
                });
                bar_idx += 1;
                config_offset += 4;
                continue;
            }

            let is_io = (val_lo & 0x01) != 0;

            if is_io {
                let addr = (val_lo & 0xFFFFFFFC) as u64;
                let size = self.probe_bar_size(config_offset)?;
                bars.push(PciBarInfo {
                    index: bar_idx,
                    kind: PciBarKind::Io,
                    addr,
                    size,
                    prefetchable: false,
                });
                bar_idx += 1;
                config_offset += 4;
            } else {
                let is_64bit = ((val_lo >> 2) & 0x01) != 0;
                let prefetchable = ((val_lo >> 3) & 0x01) != 0;

                let addr_lo = (val_lo & 0xFFFFFFF0) as u64;
                let (addr, size) = if is_64bit {
                    let val_hi = self.read_config_dword(config_offset + 4)?;
                    let full_addr = addr_lo | ((val_hi as u64) << 32);
                    let full_size = self.probe_bar64_size(config_offset)?;
                    bars.push(PciBarInfo {
                        index: bar_idx,
                        kind: PciBarKind::Memory64,
                        addr: full_addr,
                        size: full_size,
                        prefetchable,
                    });
                    bar_idx += 2;
                    config_offset += 8;
                    continue;
                } else {
                    let sz = self.probe_bar_size(config_offset)?;
                    (addr_lo, sz)
                };

                bars.push(PciBarInfo {
                    index: bar_idx,
                    kind: PciBarKind::Memory32,
                    addr,
                    size,
                    prefetchable,
                });
                bar_idx += 1;
                config_offset += 4;
            }
        }

        Ok(bars)
    }

    fn probe_bar_size(&mut self, offset: u64) -> Result<u64> {
        let original = self.read_config_dword(offset)?;
        self.write_config_dword(offset, 0xFFFFFFFF)?;
        let inverted = self.read_config_dword(offset)?;
        self.write_config_dword(offset, original)?;

        let is_io = (original & 0x01) != 0;
        let mask = if is_io { 0xFFFFFFFC } else { 0xFFFFFFF0 };

        let size_val = !(inverted & mask) & mask;
        if size_val == 0 {
            return Ok(0);
        }
        Ok(size_val as u64)
    }

    fn probe_bar64_size(&mut self, offset: u64) -> Result<u64> {
        let original_lo = self.read_config_dword(offset)?;
        let original_hi = self.read_config_dword(offset + 4)?;

        self.write_config_dword(offset, 0xFFFFFFFF)?;
        self.write_config_dword(offset + 4, 0xFFFFFFFF)?;

        let inverted_lo = self.read_config_dword(offset)?;
        let inverted_hi = self.read_config_dword(offset + 4)?;

        self.write_config_dword(offset, original_lo)?;
        self.write_config_dword(offset + 4, original_hi)?;

        let lo = !(inverted_lo & 0xFFFFFFF0) & 0xFFFFFFF0;
        let hi = !inverted_hi;

        if lo == 0 && hi == 0 {
            return Ok(0);
        }

        let size = ((hi as u64) << 32) | (lo as u64);
        Ok(size)
    }

    pub fn parse_capabilities(&mut self) -> Result<Vec<PciCapability>> {
        let status = self.status()?;
        if status & 0x0010 == 0 {
            return Ok(Vec::new());
        }

        let mut caps = Vec::new();
        let mut cap_ptr = self.read_config_byte(0x34)? as u64;

        let mut visited = 0u8;
        while cap_ptr >= 0x40 && visited < 48 {
            let cap_id = self.read_config_byte(cap_ptr)?;
            let next_ptr = self.read_config_byte(cap_ptr + 1)? as u64;

            if cap_id == 0 {
                break;
            }

            let vendor_cap_id = if cap_id == PCI_CAP_ID_VNDR {
                self.read_config_byte(cap_ptr + 2).ok()
            } else {
                None
            };

            caps.push(PciCapability {
                id: cap_id,
                offset: cap_ptr as u8,
                vendor_cap_id,
            });

            if next_ptr == 0 || next_ptr <= cap_ptr {
                break;
            }
            cap_ptr = next_ptr;
            visited += 1;
        }

        Ok(caps)
    }

    pub fn parse_msix(&mut self, cap_offset: u8) -> Result<MsixCapability> {
        let msg_ctrl = self.read_config_word(cap_offset as u64 + 2)?;
        let table_raw = self.read_config_dword(cap_offset as u64 + 4)?;
        let pba_raw = self.read_config_dword(cap_offset as u64 + 8)?;

        let table_bar = (table_raw & 0x07) as u8;
        let table_offset = table_raw & 0xFFFFFFF8;
        let pba_bar = (pba_raw & 0x07) as u8;
        let pba_offset = pba_raw & 0xFFFFFFF8;
        let table_size = (msg_ctrl & 0x07FF) + 1;
        let masked = (msg_ctrl & 0x8000) != 0;

        Ok(MsixCapability {
            table_bar,
            table_offset,
            pba_bar,
            pba_offset,
            table_size,
            masked,
        })
    }

    pub fn enable_msix(&mut self, cap_offset: u8) -> Result<()> {
        let msg_ctrl = self.read_config_word(cap_offset as u64 + 2)?;
        let new_ctrl = msg_ctrl | 0x8000;
        self.write_config_word(cap_offset as u64 + 2, new_ctrl)?;
        Ok(())
    }

    pub fn disable_msix(&mut self, cap_offset: u8) -> Result<()> {
        let msg_ctrl = self.read_config_word(cap_offset as u64 + 2)?;
        let new_ctrl = msg_ctrl & !0x8000;
        self.write_config_word(cap_offset as u64 + 2, new_ctrl)?;
        Ok(())
    }

    pub fn map_bar(
        &mut self,
        _bar_index: usize,
        phys_addr: u64,
        size: usize,
    ) -> Result<crate::memory::MmioRegion> {
        crate::memory::MmioRegion::map(
            phys_addr,
            size,
            crate::memory::CacheType::DeviceMemory,
            crate::memory::MmioProt::READ_WRITE,
        )
    }
}

impl std::io::Write for PciDevice {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.config_fd.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.config_fd.flush()
    }
}

pub fn enumerate_pci_class(class: u8) -> Result<Vec<PciDeviceInfo>> {
    let entries = std::fs::read_dir("/scheme/pci")?;
    let mut devices = Vec::new();

    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = match name.to_str() {
            Some(s) => s,
            None => continue,
        };

        // pcid scheme entries use format: segment--bus--device.function
        let location = match parse_scheme_entry(name_str) {
            Some(loc) => loc,
            None => continue,
        };

        let config_path = format!("{}/config", location.scheme_path());
        if let Ok(data) = std::fs::read(&config_path) {
            if data.len() < 64 {
                continue;
            }
            let class_code = data[0x0b];
            if class_code != class {
                continue;
            }
            let vendor_id = u16::from_le_bytes([data[0x00], data[0x01]]);
            let device_id = u16::from_le_bytes([data[0x02], data[0x03]]);
            let subclass = data[0x0a];
            let prog_if = data[0x09];
            let revision = data[0x08];
            let header_type = data[0x0e] & 0x7F;
            let irq_line = data[0x3c];

            devices.push(PciDeviceInfo {
                location,
                vendor_id,
                device_id,
                revision,
                class_code,
                subclass,
                prog_if,
                header_type,
                irq: if irq_line != 0 && irq_line != 0xff {
                    Some(irq_line as u32)
                } else {
                    None
                },
                bars: Vec::new(),
                capabilities: Vec::new(),
            });
        }
    }

    log::debug!(
        "PCI enumeration for class {class:#04x}: found {} devices",
        devices.len()
    );
    Ok(devices)
}

fn parse_scheme_entry(name: &str) -> Option<PciLocation> {
    let parts: Vec<&str> = name.splitn(3, "--").collect();
    if parts.len() != 3 {
        return None;
    }
    let segment = u16::from_str_radix(parts[0], 16).ok()?;
    let bus = u8::from_str_radix(parts[1], 16).ok()?;
    let dev_func: Vec<&str> = parts[2].splitn(2, '.').collect();
    if dev_func.len() != 2 {
        return None;
    }
    let device = u8::from_str_radix(dev_func[0], 16).ok()?;
    let function = u8::from_str_radix(dev_func[1], 16).ok()?;
    Some(PciLocation {
        segment,
        bus,
        device,
        function,
    })
}

pub fn find_amd_gpus() -> Result<Vec<PciDeviceInfo>> {
    let mut all = enumerate_pci_class(PCI_CLASS_DISPLAY)?;
    all.retain(|d| d.is_amd_gpu());
    Ok(all)
}

pub fn find_intel_gpus() -> Result<Vec<PciDeviceInfo>> {
    let mut all = enumerate_pci_class(PCI_CLASS_DISPLAY)?;
    all.retain(|d| d.is_intel_gpu());
    Ok(all)
}
