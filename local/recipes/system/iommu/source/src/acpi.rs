use std::error::Error as StdError;
use std::fmt;

const ACPI_HEADER_BYTES: usize = 36;
const IVRS_HEADER_BYTES: usize = ACPI_HEADER_BYTES + 4;
const IVHD_HEADER_BYTES: usize = 0x18;

const IVHD_TYPE_10: u8 = 0x10;
const IVHD_TYPE_11: u8 = 0x11;
const IVMD_TYPE_20: u8 = 0x20;
const IVMD_TYPE_21: u8 = 0x21;

const IVHD_ALL: u8 = 0x00;
const IVHD_SEL: u8 = 0x01;
const IVHD_SOR: u8 = 0x02;
const IVHD_EOR: u8 = 0x03;
const IVHD_PAD4: u8 = 0x42;
const IVHD_PAD8: u8 = 0x43;
const IVHD_VAR: u8 = 0x44;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Bdf(pub u16);

impl Bdf {
    pub const fn new(bus: u8, device: u8, function: u8) -> Self {
        Self(((bus as u16) << 8) | (((device as u16) & 0x1F) << 3) | ((function as u16) & 0x7))
    }

    pub const fn raw(self) -> u16 {
        self.0
    }

    pub const fn bus(self) -> u8 {
        (self.0 >> 8) as u8
    }

    pub const fn device(self) -> u8 {
        ((self.0 >> 3) & 0x1F) as u8
    }

    pub const fn function(self) -> u8 {
        (self.0 & 0x7) as u8
    }
}

impl fmt::Display for Bdf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02x}:{:02x}.{}",
            self.bus(),
            self.device(),
            self.function()
        )
    }
}

pub fn parse_bdf(text: &str) -> Option<Bdf> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(raw) = trimmed.strip_prefix("0x") {
        return u16::from_str_radix(raw, 16).ok().map(Bdf);
    }

    if trimmed.contains('.') {
        let (head, function) = trimmed.rsplit_once('.')?;
        let function = u8::from_str_radix(function, 16)
            .or_else(|_| function.parse::<u8>())
            .ok()?;

        let parts: Vec<&str> = head.split(':').collect();
        let (bus, device) = match parts.as_slice() {
            [bus, device] => (*bus, *device),
            [_, bus, device] => (*bus, *device),
            _ => return None,
        };

        let bus = u8::from_str_radix(bus, 16).ok()?;
        let device = u8::from_str_radix(device, 16).ok()?;
        return Some(Bdf::new(bus, device, function));
    }

    u16::from_str_radix(trimmed, 16).ok().map(Bdf)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IvhdEntry {
    All { flags: u8 },
    Select { bdf: Bdf, flags: u8 },
    StartRange { bdf: Bdf, flags: u8 },
    EndRange { bdf: Bdf },
    Padding { kind: u8, length: usize },
    Variable { kind: u8, payload: Vec<u8> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IommuUnitInfo {
    pub entry_type: u8,
    pub flags: u8,
    pub length: u16,
    pub iommu_bdf: Bdf,
    pub capability_offset: u16,
    pub mmio_base: u64,
    pub pci_segment_group: u16,
    pub iommu_info: u16,
    pub iommu_efr: u32,
    pub device_entries: Vec<IvhdEntry>,
}

impl IommuUnitInfo {
    pub fn unit_id(&self) -> u8 {
        ((self.iommu_info >> 6) & 0x7F) as u8
    }

    pub fn msi_number(&self) -> u8 {
        (self.iommu_info & 0x3F) as u8
    }

    pub fn handles_device(&self, bdf: Bdf) -> bool {
        let mut all = false;
        let mut range_start: Option<u16> = None;

        for entry in &self.device_entries {
            match *entry {
                IvhdEntry::All { .. } => all = true,
                IvhdEntry::Select { bdf: selected, .. } if selected == bdf => return true,
                IvhdEntry::StartRange { bdf: start, .. } => range_start = Some(start.raw()),
                IvhdEntry::EndRange { bdf: end } => {
                    if let Some(start) = range_start.take() {
                        let raw = bdf.raw();
                        if (start..=end.raw()).contains(&raw) {
                            return true;
                        }
                    }
                }
                _ => {}
            }
        }

        all
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IvrsInfo {
    pub revision: u8,
    pub iv_info: u32,
    pub units: Vec<IommuUnitInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IvrsError {
    TooShort,
    InvalidSignature([u8; 4]),
    InvalidLength(u32),
    InvalidChecksum,
    TruncatedEntry { offset: usize },
    InvalidEntryLength { offset: usize, length: usize },
    InvalidIvhdLength { offset: usize, length: usize },
    InvalidVariableLength { offset: usize, length: usize },
}

impl fmt::Display for IvrsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort => write!(f, "IVRS table is shorter than the ACPI header"),
            Self::InvalidSignature(sig) => write!(
                f,
                "invalid IVRS signature {:?}",
                String::from_utf8_lossy(sig)
            ),
            Self::InvalidLength(length) => write!(f, "invalid IVRS table length {length}"),
            Self::InvalidChecksum => write!(f, "IVRS checksum validation failed"),
            Self::TruncatedEntry { offset } => {
                write!(f, "truncated IVRS entry at offset {offset:#x}")
            }
            Self::InvalidEntryLength { offset, length } => {
                write!(
                    f,
                    "invalid IVRS entry length {length} at offset {offset:#x}"
                )
            }
            Self::InvalidIvhdLength { offset, length } => {
                write!(
                    f,
                    "invalid IVHD entry length {length} at offset {offset:#x}"
                )
            }
            Self::InvalidVariableLength { offset, length } => {
                write!(
                    f,
                    "invalid IVHD variable-length entry {length} at offset {offset:#x}"
                )
            }
        }
    }
}

impl StdError for IvrsError {}

pub fn parse_ivrs(bytes: &[u8]) -> Result<IvrsInfo, IvrsError> {
    if bytes.len() < IVRS_HEADER_BYTES {
        return Err(IvrsError::TooShort);
    }

    let signature = bytes[0..4].try_into().map_err(|_| IvrsError::TooShort)?;
    if signature != *b"IVRS" {
        return Err(IvrsError::InvalidSignature(signature));
    }

    let length = read_u32(bytes, 4).ok_or(IvrsError::TooShort)?;
    if length < IVRS_HEADER_BYTES as u32 {
        return Err(IvrsError::InvalidLength(length));
    }
    if bytes.len() < length as usize {
        return Err(IvrsError::TooShort);
    }

    let table = &bytes[..length as usize];
    if table.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte)) != 0 {
        return Err(IvrsError::InvalidChecksum);
    }

    let revision = table[8];
    let iv_info = read_u32(table, ACPI_HEADER_BYTES).ok_or(IvrsError::TooShort)?;

    let mut units = Vec::new();
    let mut offset = IVRS_HEADER_BYTES;
    while offset < table.len() {
        if offset + 4 > table.len() {
            return Err(IvrsError::TruncatedEntry { offset });
        }

        let entry_type = table[offset];
        let entry_length =
            read_u16(table, offset + 2).ok_or(IvrsError::TruncatedEntry { offset })? as usize;

        if entry_length < 4 {
            return Err(IvrsError::InvalidEntryLength {
                offset,
                length: entry_length,
            });
        }
        if offset + entry_length > table.len() {
            return Err(IvrsError::TruncatedEntry { offset });
        }

        let entry = &table[offset..offset + entry_length];
        if matches!(entry_type, IVHD_TYPE_10 | IVHD_TYPE_11) {
            units.push(parse_ivhd(entry, offset)?);
        }

        if matches!(entry_type, IVMD_TYPE_20 | IVMD_TYPE_21) {
            offset += entry_length;
            continue;
        }

        offset += entry_length;
    }

    Ok(IvrsInfo {
        revision,
        iv_info,
        units,
    })
}

fn parse_ivhd(entry: &[u8], table_offset: usize) -> Result<IommuUnitInfo, IvrsError> {
    if entry.len() < IVHD_HEADER_BYTES {
        return Err(IvrsError::InvalidIvhdLength {
            offset: table_offset,
            length: entry.len(),
        });
    }

    let mut device_entries = Vec::new();
    let mut offset = IVHD_HEADER_BYTES;
    while offset < entry.len() {
        let kind = entry[offset];
        match kind {
            IVHD_ALL => {
                ensure_remaining(entry, offset, 4, table_offset)?;
                device_entries.push(IvhdEntry::All {
                    flags: entry[offset + 1],
                });
                offset += 4;
            }
            IVHD_SEL => {
                ensure_remaining(entry, offset, 4, table_offset)?;
                device_entries.push(IvhdEntry::Select {
                    bdf: Bdf(
                        read_u16(entry, offset + 2).ok_or(IvrsError::TruncatedEntry {
                            offset: table_offset + offset,
                        })?,
                    ),
                    flags: entry[offset + 1],
                });
                offset += 4;
            }
            IVHD_SOR => {
                ensure_remaining(entry, offset, 4, table_offset)?;
                device_entries.push(IvhdEntry::StartRange {
                    bdf: Bdf(
                        read_u16(entry, offset + 2).ok_or(IvrsError::TruncatedEntry {
                            offset: table_offset + offset,
                        })?,
                    ),
                    flags: entry[offset + 1],
                });
                offset += 4;
            }
            IVHD_EOR => {
                ensure_remaining(entry, offset, 4, table_offset)?;
                device_entries.push(IvhdEntry::EndRange {
                    bdf: Bdf(
                        read_u16(entry, offset + 2).ok_or(IvrsError::TruncatedEntry {
                            offset: table_offset + offset,
                        })?,
                    ),
                });
                offset += 4;
            }
            IVHD_PAD4 => {
                ensure_remaining(entry, offset, 8, table_offset)?;
                device_entries.push(IvhdEntry::Padding { kind, length: 8 });
                offset += 8;
            }
            IVHD_PAD8 => {
                ensure_remaining(entry, offset, 12, table_offset)?;
                device_entries.push(IvhdEntry::Padding { kind, length: 12 });
                offset += 12;
            }
            IVHD_VAR => {
                ensure_remaining(entry, offset, 2, table_offset)?;
                let variable_length = entry[offset + 1] as usize;
                if variable_length < 2 {
                    return Err(IvrsError::InvalidVariableLength {
                        offset: table_offset + offset,
                        length: variable_length,
                    });
                }
                ensure_remaining(entry, offset, variable_length, table_offset)?;
                device_entries.push(IvhdEntry::Variable {
                    kind,
                    payload: entry[offset + 2..offset + variable_length].to_vec(),
                });
                offset += variable_length;
            }
            _ => {
                ensure_remaining(entry, offset, 4, table_offset)?;
                device_entries.push(IvhdEntry::Variable {
                    kind,
                    payload: entry[offset + 1..offset + 4].to_vec(),
                });
                offset += 4;
            }
        }
    }

    Ok(IommuUnitInfo {
        entry_type: entry[0],
        flags: entry[1],
        length: read_u16(entry, 2).ok_or(IvrsError::TruncatedEntry {
            offset: table_offset,
        })?,
        iommu_bdf: Bdf(read_u16(entry, 4).ok_or(IvrsError::TruncatedEntry {
            offset: table_offset,
        })?),
        capability_offset: read_u16(entry, 6).ok_or(IvrsError::TruncatedEntry {
            offset: table_offset,
        })?,
        mmio_base: read_u64(entry, 8).ok_or(IvrsError::TruncatedEntry {
            offset: table_offset,
        })?,
        pci_segment_group: read_u16(entry, 16).ok_or(IvrsError::TruncatedEntry {
            offset: table_offset,
        })?,
        iommu_info: read_u16(entry, 18).ok_or(IvrsError::TruncatedEntry {
            offset: table_offset,
        })?,
        iommu_efr: read_u32(entry, 20).ok_or(IvrsError::TruncatedEntry {
            offset: table_offset,
        })?,
        device_entries,
    })
}

fn ensure_remaining(
    entry: &[u8],
    offset: usize,
    length: usize,
    table_offset: usize,
) -> Result<(), IvrsError> {
    if offset + length > entry.len() {
        return Err(IvrsError::TruncatedEntry {
            offset: table_offset + offset,
        });
    }
    Ok(())
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    bytes
        .get(offset..offset + 2)?
        .try_into()
        .ok()
        .map(u16::from_le_bytes)
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    bytes
        .get(offset..offset + 4)?
        .try_into()
        .ok()
        .map(u32::from_le_bytes)
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    bytes
        .get(offset..offset + 8)?
        .try_into()
        .ok()
        .map(u64::from_le_bytes)
}

#[cfg(test)]
mod tests {
    use super::{parse_bdf, parse_ivrs, Bdf, IommuUnitInfo, IvhdEntry, IVRS_HEADER_BYTES};

    fn build_ivrs(units: &[Vec<u8>]) -> Vec<u8> {
        let length = (IVRS_HEADER_BYTES + units.iter().map(Vec::len).sum::<usize>()) as u32;
        let mut bytes = vec![0u8; length as usize];

        bytes[0..4].copy_from_slice(b"IVRS");
        bytes[4..8].copy_from_slice(&length.to_le_bytes());
        bytes[8] = 3;
        bytes[10..16].copy_from_slice(b"RDBEAR");
        bytes[16..24].copy_from_slice(b"AMDVI   ");
        bytes[36..40].copy_from_slice(&0x0123_4567u32.to_le_bytes());

        let mut offset = IVRS_HEADER_BYTES;
        for unit in units {
            bytes[offset..offset + unit.len()].copy_from_slice(unit);
            offset += unit.len();
        }

        let checksum =
            (!bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte))).wrapping_add(1);
        bytes[9] = checksum;
        bytes
    }

    fn build_ivhd(mmio_base: u64, iommu_bdf: Bdf, entries: &[u8]) -> Vec<u8> {
        let length = (0x18 + entries.len()) as u16;
        let mut bytes = vec![0u8; length as usize];
        bytes[0] = 0x11;
        bytes[1] = 0xA0;
        bytes[2..4].copy_from_slice(&length.to_le_bytes());
        bytes[4..6].copy_from_slice(&iommu_bdf.raw().to_le_bytes());
        bytes[6..8].copy_from_slice(&0x0040u16.to_le_bytes());
        bytes[8..16].copy_from_slice(&mmio_base.to_le_bytes());
        bytes[16..18].copy_from_slice(&0u16.to_le_bytes());
        bytes[18..20].copy_from_slice(&0x01c2u16.to_le_bytes());
        bytes[20..24].copy_from_slice(&0x00aa_5500u32.to_le_bytes());
        bytes[24..].copy_from_slice(entries);
        bytes
    }

    #[test]
    fn parses_bdf_text_forms() {
        assert_eq!(parse_bdf("00:14.0"), Some(Bdf::new(0x00, 0x14, 0x0)));
        assert_eq!(parse_bdf("0000:02:00.1"), Some(Bdf::new(0x02, 0x00, 0x1)));
        assert_eq!(parse_bdf("0x1234"), Some(Bdf(0x1234)));
        assert_eq!(parse_bdf("zz:zz.z"), None);
    }

    #[test]
    fn parses_ivrs_with_multiple_units() {
        let unit0_entries = [
            0x01, 0x11, 0x08, 0x00, // select 00:01.0
            0x02, 0x22, 0x10, 0x00, // start range 00:02.0
            0x03, 0x00, 0x17, 0x00, // end range 00:02.7
        ];
        let unit1_entries = [0x00, 0x00, 0x00, 0x00];

        let table = build_ivrs(&[
            build_ivhd(0xfee0_0000, Bdf::new(0, 0x18, 2), &unit0_entries),
            build_ivhd(0xfee1_0000, Bdf::new(0, 0x18, 3), &unit1_entries),
        ]);

        let parsed = parse_ivrs(&table).unwrap_or_else(|err| panic!("IVRS parse failed: {err}"));
        assert_eq!(parsed.units.len(), 2);
        assert_eq!(parsed.units[0].mmio_base, 0xfee0_0000);
        assert_eq!(parsed.units[1].iommu_bdf, Bdf::new(0, 0x18, 3));

        let unit = &parsed.units[0];
        assert!(unit.handles_device(Bdf::new(0, 1, 0)));
        assert!(unit.handles_device(Bdf::new(0, 2, 3)));
        assert!(!unit.handles_device(Bdf::new(0, 3, 0)));
        assert_eq!(unit.unit_id(), 7);
        assert_eq!(unit.msi_number(), 2);
    }

    #[test]
    fn all_entry_covers_entire_bus_space() {
        let unit = IommuUnitInfo {
            entry_type: 0x11,
            flags: 0,
            length: 0x1c,
            iommu_bdf: Bdf::new(0, 0x18, 2),
            capability_offset: 0x40,
            mmio_base: 0xfee0_0000,
            pci_segment_group: 0,
            iommu_info: 0,
            iommu_efr: 0,
            device_entries: vec![IvhdEntry::All { flags: 0 }],
        };

        assert!(unit.handles_device(Bdf::new(0x80, 0x1f, 7)));
    }
}
