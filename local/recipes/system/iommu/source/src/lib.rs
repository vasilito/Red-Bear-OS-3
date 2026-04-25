//! AMD-Vi-backed scheme:iommu implementation.

pub mod acpi;
pub mod amd_vi;
pub mod command_buffer;
pub mod device_table;
pub mod interrupt;
pub mod mmio;
pub mod page_table;

use std::collections::BTreeMap;

use acpi::{parse_bdf, Bdf};
use amd_vi::AmdViUnit;
use page_table::{DomainPageTables, MappingFlags};
use redox_scheme::SchemeBlockMut;
use syscall::data::Stat;
use syscall::error::{Error, Result, EBADF, EINVAL, EIO, EISDIR, ENODEV, ENOENT};
use syscall::flag::{EventFlags, MODE_DIR, MODE_FILE, SEEK_CUR, SEEK_END, SEEK_SET};

pub const IOMMU_PROTOCOL_VERSION: u16 = 1;

pub mod opcode {
    pub const QUERY: u16 = 0x0000;
    pub const CREATE_DOMAIN: u16 = 0x0001;
    pub const DESTROY_DOMAIN: u16 = 0x0002;
    pub const INIT_UNITS: u16 = 0x0003;
    pub const MAP: u16 = 0x0010;
    pub const UNMAP: u16 = 0x0011;
    pub const TRANSLATE: u16 = 0x0012;
    pub const ASSIGN_DEVICE: u16 = 0x0020;
    pub const UNASSIGN_DEVICE: u16 = 0x0021;
    pub const DRAIN_EVENTS: u16 = 0x0030;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IommuRequest {
    pub opcode: u16,
    pub version: u16,
    pub arg0: u32,
    pub arg1: u64,
    pub arg2: u64,
    pub arg3: u64,
}

impl IommuRequest {
    pub const SIZE: usize = 32;

    pub const fn new(opcode: u16, arg0: u32, arg1: u64, arg2: u64, arg3: u64) -> Self {
        Self {
            opcode,
            version: IOMMU_PROTOCOL_VERSION,
            arg0,
            arg1,
            arg2,
            arg3,
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let header = bytes.get(..Self::SIZE)?;
        Some(Self {
            opcode: u16::from_le_bytes(header.get(0..2)?.try_into().ok()?),
            version: u16::from_le_bytes(header.get(2..4)?.try_into().ok()?),
            arg0: u32::from_le_bytes(header.get(4..8)?.try_into().ok()?),
            arg1: u64::from_le_bytes(header.get(8..16)?.try_into().ok()?),
            arg2: u64::from_le_bytes(header.get(16..24)?.try_into().ok()?),
            arg3: u64::from_le_bytes(header.get(24..32)?.try_into().ok()?),
        })
    }

    pub fn to_bytes(self) -> [u8; Self::SIZE] {
        let mut bytes = [0u8; Self::SIZE];
        bytes[0..2].copy_from_slice(&self.opcode.to_le_bytes());
        bytes[2..4].copy_from_slice(&self.version.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.arg0.to_le_bytes());
        bytes[8..16].copy_from_slice(&self.arg1.to_le_bytes());
        bytes[16..24].copy_from_slice(&self.arg2.to_le_bytes());
        bytes[24..32].copy_from_slice(&self.arg3.to_le_bytes());
        bytes
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IommuResponse {
    pub status: i32,
    pub kind: u16,
    pub version: u16,
    pub arg0: u32,
    pub arg1: u64,
    pub arg2: u64,
    pub arg3: u64,
}

impl IommuResponse {
    pub const SIZE: usize = 36;

    pub const fn success(kind: u16, arg0: u32, arg1: u64, arg2: u64, arg3: u64) -> Self {
        Self {
            status: 0,
            kind,
            version: IOMMU_PROTOCOL_VERSION,
            arg0,
            arg1,
            arg2,
            arg3,
        }
    }

    pub const fn error(kind: u16, errno: i32) -> Self {
        Self {
            status: -errno,
            kind,
            version: IOMMU_PROTOCOL_VERSION,
            arg0: 0,
            arg1: 0,
            arg2: 0,
            arg3: 0,
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let header = bytes.get(..Self::SIZE)?;
        Some(Self {
            status: i32::from_le_bytes(header.get(0..4)?.try_into().ok()?),
            kind: u16::from_le_bytes(header.get(4..6)?.try_into().ok()?),
            version: u16::from_le_bytes(header.get(6..8)?.try_into().ok()?),
            arg0: u32::from_le_bytes(header.get(8..12)?.try_into().ok()?),
            arg1: u64::from_le_bytes(header.get(12..20)?.try_into().ok()?),
            arg2: u64::from_le_bytes(header.get(20..28)?.try_into().ok()?),
            arg3: u64::from_le_bytes(header.get(28..36)?.try_into().ok()?),
        })
    }

    pub fn to_bytes(self) -> [u8; Self::SIZE] {
        let mut bytes = [0u8; Self::SIZE];
        bytes[0..4].copy_from_slice(&self.status.to_le_bytes());
        bytes[4..6].copy_from_slice(&self.kind.to_le_bytes());
        bytes[6..8].copy_from_slice(&self.version.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.arg0.to_le_bytes());
        bytes[12..20].copy_from_slice(&self.arg1.to_le_bytes());
        bytes[20..28].copy_from_slice(&self.arg2.to_le_bytes());
        bytes[28..36].copy_from_slice(&self.arg3.to_le_bytes());
        bytes
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HandleKind {
    Root,
    Control,
    Domain(u16),
    Device(Bdf),
}

#[derive(Clone, Debug)]
struct Handle {
    kind: HandleKind,
    offset: usize,
    response: Vec<u8>,
}

pub struct IommuScheme {
    units: Vec<AmdViUnit>,
    next_id: usize,
    handles: BTreeMap<usize, Handle>,
    domains: BTreeMap<u16, DomainPageTables>,
    device_assignments: BTreeMap<Bdf, (u16, usize)>,
}

impl IommuScheme {
    pub fn new() -> Self {
        Self::with_units(Vec::new())
    }

    pub fn with_units(units: Vec<AmdViUnit>) -> Self {
        Self {
            units,
            next_id: 0,
            handles: BTreeMap::new(),
            domains: BTreeMap::new(),
            device_assignments: BTreeMap::new(),
        }
    }

    pub fn unit_count(&self) -> usize {
        self.units.len()
    }

    fn insert_handle(&mut self, kind: HandleKind) -> usize {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.handles.insert(
            id,
            Handle {
                kind,
                offset: 0,
                response: Vec::new(),
            },
        );
        id
    }

    fn ensure_domain_exists(&mut self, domain_id: u16) -> core::result::Result<(), i32> {
        if self.domains.contains_key(&domain_id) {
            return Ok(());
        }

        let domain = DomainPageTables::new(domain_id).map_err(|_| EIO as i32)?;
        self.domains.insert(domain_id, domain);
        Ok(())
    }

    fn next_domain_id(&self) -> Option<u16> {
        (1..u16::MAX).find(|domain_id| !self.domains.contains_key(domain_id))
    }

    fn ensure_unit_initialized(&mut self, unit_index: usize) -> core::result::Result<(), i32> {
        let Some(unit) = self.units.get_mut(unit_index) else {
            return Err(ENODEV as i32);
        };

        if unit.initialized() {
            return Ok(());
        }

        unit.init().map_err(|err| {
            log::error!(
                "iommu: failed to initialize unit {} at MMIO {:#x}: {}",
                unit_index,
                unit.info().mmio_base,
                err
            );
            EIO as i32
        })
    }

    fn root_listing(&self) -> Vec<u8> {
        let mut listing = String::from("control\n");
        for (index, unit) in self.units.iter().enumerate() {
            let state = if unit.initialized() {
                "initialized"
            } else {
                "detected"
            };
            listing.push_str(&format!(
                "unit/{index} {} mmio={:#x} state={}\n",
                unit.info().iommu_bdf,
                unit.info().mmio_base,
                state
            ));
        }
        for domain_id in self.domains.keys() {
            listing.push_str(&format!("domain/{domain_id}\n"));
        }
        for bdf in self.device_assignments.keys() {
            listing.push_str(&format!("device/{bdf}\n"));
        }
        listing.into_bytes()
    }

    fn parse_domain_id(path: &str) -> Option<u16> {
        let trimmed = path.trim();
        trimmed
            .strip_prefix("0x")
            .and_then(|hex| u16::from_str_radix(hex, 16).ok())
            .or_else(|| trimmed.parse::<u16>().ok())
            .or_else(|| u16::from_str_radix(trimmed, 16).ok())
    }

    fn map_flags(bits: u32) -> MappingFlags {
        let flags = MappingFlags {
            readable: bits & 0x1 != 0,
            writable: bits & 0x2 != 0,
            executable: bits & 0x4 != 0,
            force_coherent: bits & 0x8 != 0,
            user: bits & 0x10 != 0,
        };

        if !flags.readable
            && !flags.writable
            && !flags.executable
            && !flags.force_coherent
            && !flags.user
        {
            MappingFlags::read_write()
        } else {
            flags
        }
    }

    fn choose_unit_for_device(
        &self,
        bdf: Bdf,
        requested_unit: Option<usize>,
    ) -> core::result::Result<usize, i32> {
        if let Some(index) = requested_unit {
            let Some(unit) = self.units.get(index) else {
                return Err(ENODEV as i32);
            };
            if unit.handles_device(bdf) {
                return Ok(index);
            }
            return Err(ENODEV as i32);
        }

        self.units
            .iter()
            .position(|unit| unit.handles_device(bdf))
            .ok_or(ENODEV as i32)
    }

    fn dispatch_request(&mut self, kind: HandleKind, request: IommuRequest) -> IommuResponse {
        if request.version != IOMMU_PROTOCOL_VERSION {
            return IommuResponse::error(request.opcode, EINVAL as i32);
        }

        match kind {
            HandleKind::Root => IommuResponse::error(request.opcode, EISDIR as i32),
            HandleKind::Control => self.handle_control_request(request),
            HandleKind::Domain(domain_id) => self.handle_domain_request(domain_id, request),
            HandleKind::Device(bdf) => self.handle_device_request(bdf, request),
        }
    }

    fn handle_control_request(&mut self, request: IommuRequest) -> IommuResponse {
        match request.opcode {
            opcode::QUERY => IommuResponse::success(
                request.opcode,
                self.units.len() as u32,
                self.domains.len() as u64,
                self.device_assignments.len() as u64,
                self.units.iter().filter(|unit| unit.initialized()).count() as u64,
            ),
            opcode::INIT_UNITS => {
                let requested_index = if request.arg0 == u32::MAX {
                    None
                } else {
                    Some(request.arg0 as usize)
                };

                let mut initialized_now = 0u32;
                let mut attempted = 0u64;
                for index in 0..self.units.len() {
                    if requested_index.is_some() && requested_index != Some(index) {
                        continue;
                    }

                    attempted += 1;
                    let was_initialized = self
                        .units
                        .get(index)
                        .map(|unit| unit.initialized())
                        .unwrap_or(false);

                    if let Err(errno) = self.ensure_unit_initialized(index) {
                        return IommuResponse::error(request.opcode, errno);
                    }

                    if !was_initialized {
                        initialized_now = initialized_now.saturating_add(1);
                    }
                }

                let initialized_total =
                    self.units.iter().filter(|unit| unit.initialized()).count() as u64;

                IommuResponse::success(
                    request.opcode,
                    initialized_now,
                    attempted,
                    initialized_total,
                    requested_index
                        .map(|index| index as u64)
                        .unwrap_or(u64::MAX),
                )
            }
            opcode::CREATE_DOMAIN => {
                let domain_id = if request.arg0 == 0 {
                    match self.next_domain_id() {
                        Some(domain_id) => domain_id,
                        None => return IommuResponse::error(request.opcode, EIO as i32),
                    }
                } else {
                    request.arg0 as u16
                };

                if let Err(errno) = self.ensure_domain_exists(domain_id) {
                    return IommuResponse::error(request.opcode, errno);
                }
                let Some(domain) = self.domains.get(&domain_id) else {
                    return IommuResponse::error(request.opcode, EIO as i32);
                };
                IommuResponse::success(
                    request.opcode,
                    domain_id as u32,
                    domain.root_address(),
                    domain.levels() as u64,
                    domain.mapping_count() as u64,
                )
            }
            opcode::DESTROY_DOMAIN => {
                let domain_id = request.arg0 as u16;
                if self
                    .device_assignments
                    .values()
                    .any(|(assigned_domain, _)| *assigned_domain == domain_id)
                {
                    return IommuResponse::error(request.opcode, EINVAL as i32);
                }
                if self.domains.remove(&domain_id).is_none() {
                    return IommuResponse::error(request.opcode, ENOENT as i32);
                }
                IommuResponse::success(request.opcode, domain_id as u32, 0, 0, 0)
            }
            opcode::DRAIN_EVENTS => {
                let requested_index = if request.arg0 == u32::MAX {
                    None
                } else {
                    Some(request.arg0 as usize)
                };

                let mut count = 0u32;
                let mut first_code = 0u64;
                let mut first_device = 0u64;
                let mut first_address = 0u64;

                for (index, unit) in self.units.iter_mut().enumerate() {
                    if requested_index.is_some() && requested_index != Some(index) {
                        continue;
                    }
                    if !unit.initialized() {
                        continue;
                    }
                    match unit.drain_events() {
                        Ok(events) => {
                            if let Some(event) = events.first() {
                                if count == 0 {
                                    first_code = u64::from(event.event_code);
                                    first_device = u64::from(event.device_id.raw());
                                    first_address = event.address;
                                }
                                count = count.saturating_add(events.len() as u32);
                            }
                        }
                        Err(_) => return IommuResponse::error(request.opcode, EIO as i32),
                    }
                }

                IommuResponse::success(
                    request.opcode,
                    count,
                    first_code,
                    first_device,
                    first_address,
                )
            }
            _ => IommuResponse::error(request.opcode, EINVAL as i32),
        }
    }

    fn handle_domain_request(&mut self, domain_id: u16, request: IommuRequest) -> IommuResponse {
        if let Err(errno) = self.ensure_domain_exists(domain_id) {
            return IommuResponse::error(request.opcode, errno);
        }

        match request.opcode {
            opcode::QUERY => {
                let Some(domain) = self.domains.get(&domain_id) else {
                    return IommuResponse::error(request.opcode, ENOENT as i32);
                };
                IommuResponse::success(
                    request.opcode,
                    domain_id as u32,
                    domain.root_address(),
                    domain.levels() as u64,
                    domain.mapping_count() as u64,
                )
            }
            opcode::MAP => {
                let flags = Self::map_flags(request.arg0);
                let preferred_iova = if request.arg3 == 0 {
                    None
                } else {
                    Some(request.arg3)
                };
                let Some(domain) = self.domains.get_mut(&domain_id) else {
                    return IommuResponse::error(request.opcode, ENOENT as i32);
                };
                match domain.map_range(request.arg1, request.arg2, flags, preferred_iova) {
                    Ok(iova) => IommuResponse::success(
                        request.opcode,
                        domain_id as u32,
                        iova,
                        request.arg2,
                        0,
                    ),
                    Err(_) => IommuResponse::error(request.opcode, EIO as i32),
                }
            }
            opcode::UNMAP => {
                let Some(domain) = self.domains.get_mut(&domain_id) else {
                    return IommuResponse::error(request.opcode, ENOENT as i32);
                };
                match domain.unmap_range(request.arg1) {
                    Ok(size) => IommuResponse::success(
                        request.opcode,
                        domain_id as u32,
                        request.arg1,
                        size,
                        0,
                    ),
                    Err(_) => IommuResponse::error(request.opcode, ENOENT as i32),
                }
            }
            opcode::TRANSLATE => {
                let iova = request.arg1;
                let Some(domain) = self.domains.get(&domain_id) else {
                    return IommuResponse::error(request.opcode, ENOENT as i32);
                };
                match domain.translate(iova) {
                    Some(phys) => IommuResponse::success(
                        request.opcode,
                        domain_id as u32,
                        iova,
                        phys,
                        0,
                    ),
                    None => IommuResponse::error(request.opcode, ENOENT as i32),
                }
            }
            _ => IommuResponse::error(request.opcode, EINVAL as i32),
        }
    }

    fn handle_device_request(&mut self, bdf: Bdf, request: IommuRequest) -> IommuResponse {
        match request.opcode {
            opcode::QUERY => {
                let (domain_id, unit_index) = self
                    .device_assignments
                    .get(&bdf)
                    .copied()
                    .unwrap_or((0, usize::MAX));
                IommuResponse::success(
                    request.opcode,
                    domain_id as u32,
                    if unit_index == usize::MAX {
                        u64::MAX
                    } else {
                        unit_index as u64
                    },
                    u64::from(bdf.raw()),
                    0,
                )
            }
            opcode::ASSIGN_DEVICE => {
                let domain_id = request.arg0 as u16;
                if let Err(errno) = self.ensure_domain_exists(domain_id) {
                    return IommuResponse::error(request.opcode, errno);
                }

                let requested_unit = if request.arg1 == u64::MAX {
                    None
                } else {
                    Some(request.arg1 as usize)
                };
                let unit_index = match self.choose_unit_for_device(bdf, requested_unit) {
                    Ok(index) => index,
                    Err(errno) => return IommuResponse::error(request.opcode, errno),
                };

                if let Err(errno) = self.ensure_unit_initialized(unit_index) {
                    return IommuResponse::error(request.opcode, errno);
                }

                let Some(domain) = self.domains.get(&domain_id) else {
                    return IommuResponse::error(request.opcode, ENOENT as i32);
                };

                let Some(unit) = self.units.get_mut(unit_index) else {
                    return IommuResponse::error(request.opcode, ENODEV as i32);
                };

                match unit.assign_device(bdf, domain) {
                    Ok(()) => {
                        self.device_assignments.insert(bdf, (domain_id, unit_index));
                        IommuResponse::success(
                            request.opcode,
                            domain_id as u32,
                            unit_index as u64,
                            u64::from(bdf.raw()),
                            0,
                        )
                    }
                    Err(_) => IommuResponse::error(request.opcode, EIO as i32),
                }
            }
            opcode::UNASSIGN_DEVICE => {
                let Some((domain_id, unit_index)) = self.device_assignments.remove(&bdf) else {
                    return IommuResponse::error(request.opcode, ENOENT as i32);
                };

                let unit = self.units.get_mut(unit_index);
                if let Some(unit) = unit {
                    if unit.initialized() {
                        if let Err(err) = unit.unassign_device(bdf) {
                            log::error!(
                                "iommu: failed to invalidate DTE for {bdf} on unit {unit_index}: {err}"
                            );
                            return IommuResponse::error(request.opcode, EIO as i32);
                        }
                    }
                }

                IommuResponse::success(
                    request.opcode,
                    domain_id as u32,
                    u64::from(bdf.raw()),
                    unit_index as u64,
                    0,
                )
            }
            _ => IommuResponse::error(request.opcode, EINVAL as i32),
        }
    }
}

impl Default for IommuScheme {
    fn default() -> Self {
        Self::new()
    }
}

impl SchemeBlockMut for IommuScheme {
    fn open(&mut self, path: &str, _flags: usize, _uid: u32, _gid: u32) -> Result<Option<usize>> {
        let cleaned = path.trim_matches('/');

        let kind = if cleaned.is_empty() {
            HandleKind::Root
        } else if cleaned == "control" {
            HandleKind::Control
        } else if let Some(rest) = cleaned.strip_prefix("domain/") {
            let domain_id = Self::parse_domain_id(rest).ok_or(Error::new(ENOENT))?;
            self.ensure_domain_exists(domain_id).map_err(Error::new)?;
            HandleKind::Domain(domain_id)
        } else if let Some(rest) = cleaned.strip_prefix("device/") {
            let bdf = parse_bdf(rest).ok_or(Error::new(ENOENT))?;
            HandleKind::Device(bdf)
        } else {
            return Err(Error::new(ENOENT));
        };

        Ok(Some(self.insert_handle(kind)))
    }

    fn read(&mut self, id: usize, buf: &mut [u8]) -> Result<Option<usize>> {
        let (kind, offset, response) = {
            let handle = self.handles.get(&id).ok_or(Error::new(EBADF))?;
            (handle.kind, handle.offset, handle.response.clone())
        };

        let content = match kind {
            HandleKind::Root => self.root_listing(),
            _ => response,
        };

        if offset >= content.len() {
            return Ok(Some(0));
        }

        let to_copy = (content.len() - offset).min(buf.len());
        buf[..to_copy].copy_from_slice(&content[offset..offset + to_copy]);

        let handle = self.handles.get_mut(&id).ok_or(Error::new(EBADF))?;
        handle.offset = offset + to_copy;
        Ok(Some(to_copy))
    }

    fn write(&mut self, id: usize, buf: &[u8]) -> Result<Option<usize>> {
        let kind = self
            .handles
            .get(&id)
            .map(|handle| handle.kind)
            .ok_or(Error::new(EBADF))?;
        if kind == HandleKind::Root {
            return Err(Error::new(EISDIR));
        }

        let response = match IommuRequest::from_bytes(buf) {
            Some(request) => self.dispatch_request(kind, request),
            None => IommuResponse::error(0, EINVAL as i32),
        };

        let handle = self.handles.get_mut(&id).ok_or(Error::new(EBADF))?;
        handle.response = response.to_bytes().to_vec();
        handle.offset = 0;
        Ok(Some(buf.len()))
    }

    fn seek(&mut self, id: usize, pos: isize, whence: usize) -> Result<Option<isize>> {
        let (kind, current_offset, response_len) = {
            let handle = self.handles.get(&id).ok_or(Error::new(EBADF))?;
            (handle.kind, handle.offset, handle.response.len())
        };

        let content_len = match kind {
            HandleKind::Root => self.root_listing().len(),
            _ => response_len,
        };

        let new_offset = match whence {
            SEEK_SET => pos,
            SEEK_CUR => current_offset as isize + pos,
            SEEK_END => content_len as isize + pos,
            _ => return Err(Error::new(EINVAL)),
        };
        if new_offset < 0 {
            return Err(Error::new(EINVAL));
        }

        let handle = self.handles.get_mut(&id).ok_or(Error::new(EBADF))?;
        handle.offset = new_offset as usize;
        Ok(Some(new_offset))
    }

    fn fpath(&mut self, id: usize, buf: &mut [u8]) -> Result<Option<usize>> {
        let kind = self
            .handles
            .get(&id)
            .map(|handle| handle.kind)
            .ok_or(Error::new(EBADF))?;
        let path = match kind {
            HandleKind::Root => "iommu:".to_string(),
            HandleKind::Control => "iommu:control".to_string(),
            HandleKind::Domain(domain_id) => format!("iommu:domain/{domain_id}"),
            HandleKind::Device(bdf) => format!("iommu:device/{bdf}"),
        };
        let bytes = path.as_bytes();
        let to_copy = bytes.len().min(buf.len());
        buf[..to_copy].copy_from_slice(&bytes[..to_copy]);
        Ok(Some(to_copy))
    }

    fn fstat(&mut self, id: usize, stat: &mut Stat) -> Result<Option<usize>> {
        let kind = self
            .handles
            .get(&id)
            .map(|handle| handle.kind)
            .ok_or(Error::new(EBADF))?;
        match kind {
            HandleKind::Root => {
                stat.st_mode = MODE_DIR | 0o555;
                stat.st_size = self.root_listing().len() as u64;
            }
            HandleKind::Control => {
                stat.st_mode = MODE_FILE | 0o666;
                stat.st_size = IommuResponse::SIZE as u64;
            }
            HandleKind::Domain(domain_id) => {
                stat.st_mode = MODE_FILE | 0o666;
                stat.st_size = self
                    .domains
                    .get(&domain_id)
                    .map(|domain| domain.mapping_count() as u64)
                    .unwrap_or(0);
            }
            HandleKind::Device(_) => {
                stat.st_mode = MODE_FILE | 0o666;
                stat.st_size = 0;
            }
        }
        stat.st_blksize = 4096;
        stat.st_blocks = stat.st_size.div_ceil(512);
        Ok(Some(0))
    }

    fn fevent(&mut self, id: usize, _flags: EventFlags) -> Result<Option<EventFlags>> {
        let _ = self.handles.get(&id).ok_or(Error::new(EBADF))?;
        Ok(Some(EventFlags::empty()))
    }

    fn close(&mut self, id: usize) -> Result<Option<usize>> {
        if self.handles.remove(&id).is_none() {
            return Err(Error::new(EBADF));
        }
        Ok(Some(0))
    }
}

#[cfg(all(test, not(target_os = "redox")))]
mod host_redox_stubs {
    use core::ptr;

    use syscall::error::{EINVAL, ENOSYS};

    fn error_result(errno: i32) -> usize {
        usize::wrapping_neg(errno as usize)
    }

    #[no_mangle]
    pub extern "C" fn redox_open_v1(
        _path_base: *const u8,
        _path_len: usize,
        _flags: u32,
        _mode: u16,
    ) -> usize {
        error_result(ENOSYS)
    }

    #[no_mangle]
    pub extern "C" fn redox_openat_v1(
        _fd: usize,
        _buf: *const u8,
        _path_len: usize,
        _flags: u32,
        _fcntl_flags: u32,
    ) -> usize {
        error_result(ENOSYS)
    }

    #[no_mangle]
    pub extern "C" fn redox_close_v1(_fd: usize) -> usize {
        0
    }

    #[no_mangle]
    pub extern "C" fn redox_mmap_v1(
        _addr: *mut (),
        _unaligned_len: usize,
        _prot: u32,
        _flags: u32,
        _fd: usize,
        _offset: u64,
    ) -> usize {
        error_result(ENOSYS)
    }

    #[no_mangle]
    pub extern "C" fn redox_munmap_v1(_addr: *mut (), _unaligned_len: usize) -> usize {
        0
    }

    #[no_mangle]
    pub extern "C" fn redox_sys_call_v0(
        _fd: usize,
        _payload: *mut u8,
        _payload_len: usize,
        _flags: usize,
        _metadata: *const u64,
        _metadata_len: usize,
    ) -> usize {
        error_result(ENOSYS)
    }

    #[no_mangle]
    pub extern "C" fn redox_strerror_v1(dst: *mut u8, dst_len: *mut usize, _error: u32) -> usize {
        if dst.is_null() || dst_len.is_null() {
            return error_result(EINVAL);
        }

        let message = b"host test stub";
        unsafe {
            let writable = *dst_len;
            let count = writable.min(message.len());
            ptr::copy_nonoverlapping(message.as_ptr(), dst, count);
            *dst_len = count;
        }
        0
    }
}

#[cfg(test)]
mod tests {
    use super::{opcode, IommuRequest, IommuResponse, IommuScheme};
    use crate::page_table::PAGE_SIZE;
    use redox_scheme::SchemeBlockMut;

    fn read_response(scheme: &mut IommuScheme, id: usize) -> IommuResponse {
        let mut bytes = [0u8; IommuResponse::SIZE];
        let count = scheme
            .read(id, &mut bytes)
            .unwrap_or_else(|err| panic!("read failed: {err}"))
            .unwrap_or_else(|| panic!("expected response bytes"));
        IommuResponse::from_bytes(&bytes[..count])
            .unwrap_or_else(|| panic!("invalid response bytes"))
    }

    #[test]
    fn request_round_trip_serialization() {
        let request = IommuRequest::new(opcode::MAP, 7, 0x1000, 0x2000, 0x3000);
        let encoded = request.to_bytes();
        let decoded = IommuRequest::from_bytes(&encoded)
            .unwrap_or_else(|| panic!("failed to deserialize request"));
        assert_eq!(decoded, request);
    }

    #[test]
    fn root_lists_control_endpoint() {
        let mut scheme = IommuScheme::new();
        let root = scheme
            .open("", 0, 0, 0)
            .unwrap_or_else(|err| panic!("open root failed: {err}"))
            .unwrap_or_else(|| panic!("root open returned no handle"));

        let mut bytes = [0u8; 128];
        let count = scheme
            .read(root, &mut bytes)
            .unwrap_or_else(|err| panic!("read root failed: {err}"))
            .unwrap_or_else(|| panic!("expected root bytes"));
        let listing = String::from_utf8_lossy(&bytes[..count]);
        assert!(listing.contains("control"));
    }

    #[test]
    fn control_can_create_and_query_domain() {
        let mut scheme = IommuScheme::new();
        let control = scheme
            .open("control", 0, 0, 0)
            .unwrap_or_else(|err| panic!("open control failed: {err}"))
            .unwrap_or_else(|| panic!("control open returned no handle"));

        let request = IommuRequest::new(opcode::CREATE_DOMAIN, 7, 0, 0, 0);
        scheme
            .write(control, &request.to_bytes())
            .unwrap_or_else(|err| panic!("create domain write failed: {err}"));
        let response = read_response(&mut scheme, control);

        assert_eq!(response.status, 0);
        assert_eq!(response.arg0, 7);
        assert_ne!(response.arg1, 0);

        let query = IommuRequest::new(opcode::QUERY, 0, 0, 0, 0);
        scheme
            .write(control, &query.to_bytes())
            .unwrap_or_else(|err| panic!("control query failed: {err}"));
        let query_response = read_response(&mut scheme, control);
        assert_eq!(query_response.status, 0);
        assert_eq!(query_response.arg0, 0);
        assert_eq!(query_response.arg1, 1);
    }

    #[test]
    fn init_units_on_empty_scheme_is_a_noop_success() {
        let mut scheme = IommuScheme::new();
        let control = scheme
            .open("control", 0, 0, 0)
            .unwrap_or_else(|err| panic!("open control failed: {err}"))
            .unwrap_or_else(|| panic!("control open returned no handle"));

        let request = IommuRequest::new(opcode::INIT_UNITS, u32::MAX, 0, 0, 0);
        scheme
            .write(control, &request.to_bytes())
            .unwrap_or_else(|err| panic!("init units write failed: {err}"));
        let response = read_response(&mut scheme, control);

        assert_eq!(response.status, 0);
        assert_eq!(response.arg0, 0);
        assert_eq!(response.arg1, 0);
        assert_eq!(response.arg2, 0);
    }

    #[test]
    fn domain_handle_can_map_pages() {
        let mut scheme = IommuScheme::new();
        let domain = scheme
            .open("domain/5", 0, 0, 0)
            .unwrap_or_else(|err| panic!("open domain failed: {err}"))
            .unwrap_or_else(|| panic!("domain open returned no handle"));

        let map = IommuRequest::new(opcode::MAP, 0x3, 0x4000_0000, PAGE_SIZE * 2, 0);
        scheme
            .write(domain, &map.to_bytes())
            .unwrap_or_else(|err| panic!("domain map write failed: {err}"));
        let response = read_response(&mut scheme, domain);
        assert_eq!(response.status, 0);
        assert_eq!(response.arg0, 5);
        assert_ne!(response.arg1, 0);

        let unmap = IommuRequest::new(opcode::UNMAP, 0, response.arg1, 0, 0);
        scheme
            .write(domain, &unmap.to_bytes())
            .unwrap_or_else(|err| panic!("domain unmap write failed: {err}"));
        let unmap_response = read_response(&mut scheme, domain);
        assert_eq!(unmap_response.status, 0);
        assert_eq!(unmap_response.arg2, PAGE_SIZE * 2);
    }

    #[test]
    fn assigning_without_detected_units_returns_error_response() {
        let mut scheme = IommuScheme::new();
        let device = scheme
            .open("device/00:14.0", 0, 0, 0)
            .unwrap_or_else(|err| panic!("open device failed: {err}"))
            .unwrap_or_else(|| panic!("device open returned no handle"));

        let assign = IommuRequest::new(opcode::ASSIGN_DEVICE, 1, u64::MAX, 0, 0);
        scheme
            .write(device, &assign.to_bytes())
            .unwrap_or_else(|err| panic!("device assign write failed: {err}"));
        let response = read_response(&mut scheme, device);
        assert!(response.status < 0);
    }
}
