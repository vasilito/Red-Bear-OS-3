use alloc::vec::Vec;

use crate::device::DeviceInfo;

/// Priority type used to order driver probes.
pub type MatchPriority = i32;

/// A single entry in a driver's match table.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct DriverMatch {
    /// Optional vendor identifier match.
    pub vendor: Option<u16>,
    /// Optional device identifier match.
    pub device: Option<u16>,
    /// Optional class-code match.
    pub class: Option<u8>,
    /// Optional subclass-code match.
    pub subclass: Option<u8>,
    /// Optional programming-interface match.
    pub prog_if: Option<u8>,
    /// Optional subsystem vendor match.
    pub subsystem_vendor: Option<u16>,
    /// Optional subsystem device match.
    pub subsystem_device: Option<u16>,
}

impl DriverMatch {
    /// Checks whether this match entry matches the provided device information.
    pub fn matches(&self, info: &DeviceInfo) -> bool {
        self.vendor.map_or(true, |v| info.vendor == Some(v))
            && self.device.map_or(true, |d| info.device == Some(d))
            && self.class.map_or(true, |c| info.class == Some(c))
            && self.subclass.map_or(true, |s| info.subclass == Some(s))
            && self.prog_if.map_or(true, |p| info.prog_if == Some(p))
            && self
                .subsystem_vendor
                .map_or(true, |v| info.subsystem_vendor == Some(v))
            && self
                .subsystem_device
                .map_or(true, |d| info.subsystem_device == Some(d))
    }
}

/// Collection wrapper for a driver's match entries.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct MatchTable {
    entries: Vec<DriverMatch>,
}

impl MatchTable {
    /// Creates a new match table from the provided entries.
    pub fn new(entries: Vec<DriverMatch>) -> Self {
        Self { entries }
    }

    /// Returns the underlying immutable slice of match entries.
    pub fn entries(&self) -> &[DriverMatch] {
        self.entries.as_slice()
    }

    /// Returns `true` if any entry in the table matches the provided device.
    pub fn matches(&self, info: &DeviceInfo) -> bool {
        self.entries.iter().any(|entry| entry.matches(info))
    }
}

impl From<Vec<DriverMatch>> for MatchTable {
    fn from(entries: Vec<DriverMatch>) -> Self {
        Self::new(entries)
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::String;

    use super::DriverMatch;
    use crate::device::{DeviceId, DeviceInfo};

    fn sample_device() -> DeviceInfo {
        DeviceInfo {
            id: DeviceId {
                bus: String::from("pci"),
                path: String::from("0000:00:02.0"),
            },
            vendor: Some(0x8086),
            device: Some(0x1234),
            class: Some(0x03),
            subclass: Some(0x00),
            prog_if: Some(0x00),
            revision: Some(0x01),
            subsystem_vendor: Some(0x8086),
            subsystem_device: Some(0xabcd),
            raw_path: String::from("/scheme/pci/00.02.0"),
            description: Some(String::from("Display controller")),
        }
    }

    #[test]
    fn driver_match_accepts_exact_match() {
        let info = sample_device();
        let driver_match = DriverMatch {
            vendor: Some(0x8086),
            device: Some(0x1234),
            class: Some(0x03),
            subclass: Some(0x00),
            prog_if: Some(0x00),
            subsystem_vendor: Some(0x8086),
            subsystem_device: Some(0xabcd),
        };

        assert!(driver_match.matches(&info));
    }

    #[test]
    fn driver_match_supports_wildcards() {
        let info = sample_device();
        let driver_match = DriverMatch {
            vendor: Some(0x8086),
            device: None,
            class: Some(0x03),
            subclass: None,
            prog_if: None,
            subsystem_vendor: None,
            subsystem_device: None,
        };

        assert!(driver_match.matches(&info));
    }

    #[test]
    fn driver_match_rejects_mismatch() {
        let info = sample_device();
        let driver_match = DriverMatch {
            vendor: Some(0x10ec),
            device: None,
            class: None,
            subclass: None,
            prog_if: None,
            subsystem_vendor: None,
            subsystem_device: None,
        };

        assert!(!driver_match.matches(&info));
    }
}
