use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;

use redox_scheme::scheme::SchemeSync;
use redox_scheme::{CallerCtx, OpenResult};
use syscall::error::{Error, Result, EACCES, EBADF, ENOENT, EROFS};
use syscall::flag::{EventFlags, MODE_DIR, MODE_FILE};
use syscall::schemev2::NewFdFlags;

use crate::device_db::{
    classify_pci_device, format_device_info, format_uevent_info, DeviceInfo, InputKind, Subsystem,
};

const SCHEME_ROOT_ID: usize = 1;

enum HandleKind {
    Root,
    Devices,
    Device(usize),
    Dev,
    DevInputDir,
    DevInput(usize, File),
    DevInputMice,
    DevDriDir,
    DevDri(usize),
    DevLinks,
    LinksInputDir,
    LinksInputByPathDir,
    LinksDriDir,
    LinksDriByPathDir,
    Link(usize),
    Uevent,
}

impl Clone for HandleKind {
    fn clone(&self) -> Self {
        match self {
            Self::Root => Self::Root,
            Self::Devices => Self::Devices,
            Self::Device(idx) => Self::Device(*idx),
            Self::Dev => Self::Dev,
            Self::DevInputDir => Self::DevInputDir,
            Self::DevInput(idx, file) => Self::DevInput(
                *idx,
                file.try_clone().expect("udev-shim: clone dev input fd"),
            ),
            Self::DevInputMice => Self::DevInputMice,
            Self::DevDriDir => Self::DevDriDir,
            Self::DevDri(idx) => Self::DevDri(*idx),
            Self::DevLinks => Self::DevLinks,
            Self::LinksInputDir => Self::LinksInputDir,
            Self::LinksInputByPathDir => Self::LinksInputByPathDir,
            Self::LinksDriDir => Self::LinksDriDir,
            Self::LinksDriByPathDir => Self::LinksDriByPathDir,
            Self::Link(idx) => Self::Link(*idx),
            Self::Uevent => Self::Uevent,
        }
    }
}

pub struct UdevScheme {
    next_id: usize,
    handles: BTreeMap<usize, HandleKind>,
    devices: Vec<DeviceInfo>,
}

impl UdevScheme {
    pub fn new() -> Self {
        Self {
            next_id: SCHEME_ROOT_ID + 1,
            handles: BTreeMap::new(),
            devices: Vec::new(),
        }
    }

    pub fn scan_pci_devices(&mut self) -> Result<usize> {
        self.devices.clear();

        let mut pci_slots = Vec::new();
        match std::fs::read_dir("/scheme/pci") {
            Ok(dir) => {
                for entry in dir {
                    let entry = match entry {
                        Ok(entry) => entry,
                        Err(_) => continue,
                    };

                    let name = match entry.file_name().to_str() {
                        Some(name) => name.to_string(),
                        None => continue,
                    };

                    if let Some(slot) = parse_pci_slot(&name) {
                        pci_slots.push(slot);
                    }
                }
            }
            Err(err) => {
                log::warn!("udev-shim: failed to read /scheme/pci: {err}");
            }
        }

        pci_slots.sort_unstable();
        for (bus, dev, func) in pci_slots {
            self.devices.push(classify_pci_device(bus, dev, func));
        }

        if scheme_registered("input") {
            self.devices.push(DeviceInfo::new_platform_input(
                "Redox Keyboard Input",
                "/devices/platform/keyboard0",
                InputKind::Keyboard,
                "",
                "",
            ));
        }

        if scheme_registered("pointer") || scheme_registered("mouse") {
            self.devices.push(DeviceInfo::new_platform_input(
                "Redox Mouse Input",
                "/devices/platform/mouse0",
                InputKind::Mouse,
                "",
                "",
            ));
        }

        self.assign_virtual_nodes();

        Ok(self.devices.len())
    }

    fn assign_virtual_nodes(&mut self) {
        for dev in &mut self.devices {
            if dev.subsystem == Subsystem::Gpu || (dev.subsystem == Subsystem::Input && !dev.is_pci)
            {
                dev.set_node_metadata("", "", Vec::new());
            } else {
                dev.symlinks.clear();
            }
        }

        let mut gpu_indices: Vec<usize> = self
            .devices
            .iter()
            .enumerate()
            .filter_map(|(idx, dev)| (dev.subsystem == Subsystem::Gpu).then_some(idx))
            .collect();
        gpu_indices.sort_by_key(|idx| {
            let dev = &self.devices[*idx];
            (gpu_priority(dev), dev.bus, dev.dev, dev.func)
        });

        for (card_idx, device_idx) in gpu_indices.into_iter().enumerate() {
            let devnode = format!("/dev/dri/card{card_idx}");
            let scheme_target = format!("drm/card{card_idx}");
            let symlink = format!(
                "/links/dri/by-path/{}-card",
                self.devices[device_idx].id_path()
            );
            self.devices[device_idx].set_node_metadata(devnode, scheme_target, vec![symlink]);
        }

        let mut input_indices: Vec<usize> = self
            .devices
            .iter()
            .enumerate()
            .filter_map(|(idx, dev)| {
                (dev.subsystem == Subsystem::Input && !dev.is_pci).then_some(idx)
            })
            .collect();
        input_indices.sort_by_key(|idx| {
            let dev = &self.devices[*idx];
            (input_priority(dev), dev.devpath.clone())
        });

        for (event_idx, device_idx) in input_indices.into_iter().enumerate() {
            let devnode = format!("/dev/input/event{event_idx}");
            let scheme_target = format!("evdev/event{event_idx}");
            let suffix = match self.devices[device_idx].input_kind {
                Some(InputKind::Keyboard) => "event-kbd",
                Some(InputKind::Mouse) => "event-mouse",
                Some(InputKind::Generic) | None => "event",
            };
            let symlink = format!(
                "/links/input/by-path/{}-{}",
                self.devices[device_idx].id_path(),
                suffix
            );
            self.devices[device_idx].set_node_metadata(devnode, scheme_target, vec![symlink]);
        }
    }

    fn find_device_by_devnode(&self, devnode: &str) -> Option<usize> {
        self.devices
            .iter()
            .enumerate()
            .find_map(|(idx, dev)| (dev.devnode == devnode).then_some(idx))
    }

    fn find_device_by_link(&self, prefix: &str, tail: &str) -> Option<usize> {
        let expected = format!("{prefix}{tail}");
        self.devices.iter().enumerate().find_map(|(idx, dev)| {
            dev.symlinks
                .iter()
                .any(|link| link == &expected)
                .then_some(idx)
        })
    }

    fn mouse_device_index(&self) -> Option<usize> {
        self.devices.iter().enumerate().find_map(|(idx, dev)| {
            (dev.is_input_mouse() && !dev.scheme_target.is_empty()).then_some(idx)
        })
    }

    fn input_event_indices(&self) -> Vec<usize> {
        self.devices
            .iter()
            .enumerate()
            .filter_map(|(idx, dev)| {
                (dev.devnode.starts_with("/dev/input/event") && !dev.scheme_target.is_empty())
                    .then_some(idx)
            })
            .collect()
    }

    fn dri_card_indices(&self) -> Vec<usize> {
        self.devices
            .iter()
            .enumerate()
            .filter_map(|(idx, dev)| {
                (dev.devnode.starts_with("/dev/dri/card") && !dev.scheme_target.is_empty())
                    .then_some(idx)
            })
            .collect()
    }

    fn directory_listing<I>(&self, entries: I) -> String
    where
        I: IntoIterator<Item = String>,
    {
        let mut listing = String::new();
        for entry in entries {
            listing.push_str(&entry);
            listing.push('\n');
        }
        listing
    }

    fn link_listing(&self, prefix: &str) -> String {
        self.directory_listing(
            self.devices
                .iter()
                .flat_map(|dev| dev.symlinks.iter())
                .filter_map(|link| {
                    link.strip_prefix(prefix).and_then(|tail| {
                        (!tail.is_empty() && !tail.contains('/')).then(|| tail.to_string())
                    })
                }),
        )
    }

    fn uevent_content(&self) -> String {
        let mut content = String::new();
        for (idx, dev) in self.devices.iter().enumerate() {
            if idx > 0 {
                content.push('\n');
            }
            content.push_str(&format_uevent_info(dev));
        }
        content
    }

    fn content_for_handle(&self, kind: &HandleKind) -> Result<String> {
        match kind {
            HandleKind::Root => Ok(self.directory_listing(
                ["devices", "dev", "links", "uevent"]
                    .into_iter()
                    .map(String::from),
            )),
            HandleKind::Devices => {
                Ok(self.directory_listing((0..self.devices.len()).map(|idx| idx.to_string())))
            }
            HandleKind::Device(idx) => self
                .devices
                .get(*idx)
                .map(format_device_info)
                .ok_or_else(|| Error::new(ENOENT)),
            HandleKind::Dev => {
                Ok(self.directory_listing(["input", "dri"].into_iter().map(String::from)))
            }
            HandleKind::DevInputDir => {
                let mut entries: Vec<String> = self
                    .input_event_indices()
                    .into_iter()
                    .filter_map(|idx| basename(&self.devices[idx].devnode))
                    .collect();
                if self.mouse_device_index().is_some() {
                    entries.push("mice".to_string());
                }
                Ok(self.directory_listing(entries))
            }
            HandleKind::DevInput(idx, _) => {
                let dev = self.devices.get(*idx).ok_or_else(|| Error::new(ENOENT))?;
                if dev.scheme_target.is_empty() {
                    return Err(Error::new(ENOENT));
                }
                let mut info = format_device_info(dev);
                info.push_str(&format!("SCHEME_TARGET={}\n", dev.scheme_target));
                Ok(info)
            }
            HandleKind::DevDri(idx) => {
                let dev = self.devices.get(*idx).ok_or_else(|| Error::new(ENOENT))?;
                if dev.scheme_target.is_empty() {
                    return Err(Error::new(ENOENT));
                }
                let mut info = format_device_info(dev);
                info.push_str(&format!("SCHEME_TARGET={}\n", dev.scheme_target));
                Ok(info)
            }
            HandleKind::Link(idx) => {
                let dev = self.devices.get(*idx).ok_or_else(|| Error::new(ENOENT))?;
                if dev.scheme_target.is_empty() {
                    return Err(Error::new(ENOENT));
                }
                let mut info = format_device_info(dev);
                info.push_str(&format!("SCHEME_TARGET={}\n", dev.scheme_target));
                Ok(info)
            }
            HandleKind::DevInputMice => {
                let idx = self
                    .mouse_device_index()
                    .ok_or_else(|| Error::new(ENOENT))?;
                let dev = &self.devices[idx];
                let mut info = format_device_info(dev);
                info.push_str(&format!("SCHEME_TARGET={}\n", dev.scheme_target));
                Ok(info)
            }
            HandleKind::DevDriDir => Ok(self.directory_listing(
                self.dri_card_indices()
                    .into_iter()
                    .filter_map(|idx| basename(&self.devices[idx].devnode)),
            )),
            HandleKind::DevLinks => {
                Ok(self.directory_listing(["input", "dri"].into_iter().map(String::from)))
            }
            HandleKind::LinksInputDir => {
                Ok(self.directory_listing(["by-path"].into_iter().map(String::from)))
            }
            HandleKind::LinksInputByPathDir => Ok(self.link_listing("/links/input/by-path/")),
            HandleKind::LinksDriDir => {
                Ok(self.directory_listing(["by-path"].into_iter().map(String::from)))
            }
            HandleKind::LinksDriByPathDir => Ok(self.link_listing("/links/dri/by-path/")),
            HandleKind::Uevent => Ok(self.uevent_content()),
        }
    }

    fn is_directory(kind: &HandleKind) -> bool {
        matches!(
            kind,
            HandleKind::Root
                | HandleKind::Devices
                | HandleKind::Dev
                | HandleKind::DevInputDir
                | HandleKind::DevDriDir
                | HandleKind::DevLinks
                | HandleKind::LinksInputDir
                | HandleKind::LinksInputByPathDir
                | HandleKind::LinksDriDir
                | HandleKind::LinksDriByPathDir
        )
    }

    fn kind_for_id(&self, id: usize) -> Result<HandleKind> {
        if id == SCHEME_ROOT_ID {
            return Ok(HandleKind::Root);
        }

        self.handles
            .get(&id)
            .cloned()
            .ok_or_else(|| Error::new(EBADF))
    }

    fn kind_for_path(&self, path: &str) -> Result<HandleKind> {
        let cleaned = path.trim_matches('/');

        match cleaned {
            "" => Ok(HandleKind::Root),
            "devices" => Ok(HandleKind::Devices),
            "dev" => Ok(HandleKind::Dev),
            "dev/input" => Ok(HandleKind::DevInputDir),
            "dev/input/mice" => {
                if self.mouse_device_index().is_none() {
                    return Err(Error::new(ENOENT));
                }
                Ok(HandleKind::DevInputMice)
            }
            "dev/dri" => Ok(HandleKind::DevDriDir),
            "links" => Ok(HandleKind::DevLinks),
            "links/input" => Ok(HandleKind::LinksInputDir),
            "links/input/by-path" => Ok(HandleKind::LinksInputByPathDir),
            "links/dri" => Ok(HandleKind::LinksDriDir),
            "links/dri/by-path" => Ok(HandleKind::LinksDriByPathDir),
            "uevent" => Ok(HandleKind::Uevent),
            _ => {
                if let Some(rest) = cleaned.strip_prefix("devices/") {
                    let idx = rest.parse::<usize>().map_err(|_| Error::new(ENOENT))?;
                    if idx >= self.devices.len() {
                        return Err(Error::new(ENOENT));
                    }
                    Ok(HandleKind::Device(idx))
                } else if let Some(rest) = cleaned.strip_prefix("dev/input/") {
                    let devnode = format!("/dev/input/{rest}");
                    let idx = self
                        .find_device_by_devnode(&devnode)
                        .ok_or_else(|| Error::new(ENOENT))?;
                    let dev = self.devices.get(idx).ok_or_else(|| Error::new(ENOENT))?;
                    if dev.scheme_target.is_empty() {
                        return Err(Error::new(ENOENT));
                    }
                    let file = File::open(format!("/scheme/{}", dev.scheme_target))
                        .map_err(|_| Error::new(ENOENT))?;
                    Ok(HandleKind::DevInput(idx, file))
                } else if let Some(rest) = cleaned.strip_prefix("dev/dri/") {
                    let devnode = format!("/dev/dri/{rest}");
                    let idx = self
                        .find_device_by_devnode(&devnode)
                        .ok_or_else(|| Error::new(ENOENT))?;
                    Ok(HandleKind::DevDri(idx))
                } else if let Some(rest) = cleaned.strip_prefix("links/input/by-path/") {
                    let idx = self
                        .find_device_by_link("/links/input/by-path/", rest)
                        .ok_or_else(|| Error::new(ENOENT))?;
                    Ok(HandleKind::Link(idx))
                } else if let Some(rest) = cleaned.strip_prefix("links/dri/by-path/") {
                    let idx = self
                        .find_device_by_link("/links/dri/by-path/", rest)
                        .ok_or_else(|| Error::new(ENOENT))?;
                    Ok(HandleKind::Link(idx))
                } else {
                    Err(Error::new(ENOENT))
                }
            }
        }
    }

    fn allocate_handle(&mut self, kind: HandleKind) -> usize {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.handles.insert(id, kind);
        id
    }

    fn path_for_handle(&self, kind: &HandleKind) -> Result<String> {
        match kind {
            HandleKind::Root => Ok("/scheme/udev".to_string()),
            HandleKind::Devices => Ok("/scheme/udev/devices".to_string()),
            HandleKind::Device(idx) => {
                if *idx >= self.devices.len() {
                    return Err(Error::new(ENOENT));
                }
                Ok(format!("/scheme/udev/devices/{idx}"))
            }
            HandleKind::Dev => Ok("/scheme/udev/dev".to_string()),
            HandleKind::DevInputDir => Ok("/scheme/udev/dev/input".to_string()),
            HandleKind::DevInput(idx, _) => self
                .devices
                .get(*idx)
                .filter(|dev| !dev.devnode.is_empty())
                .map(|dev| format!("/scheme/udev{}", dev.devnode))
                .ok_or_else(|| Error::new(ENOENT)),
            HandleKind::DevInputMice => Ok("/scheme/udev/dev/input/mice".to_string()),
            HandleKind::DevDriDir => Ok("/scheme/udev/dev/dri".to_string()),
            HandleKind::DevDri(idx) => self
                .devices
                .get(*idx)
                .filter(|dev| !dev.devnode.is_empty())
                .map(|dev| format!("/scheme/udev{}", dev.devnode))
                .ok_or_else(|| Error::new(ENOENT)),
            HandleKind::DevLinks => Ok("/scheme/udev/links".to_string()),
            HandleKind::LinksInputDir => Ok("/scheme/udev/links/input".to_string()),
            HandleKind::LinksInputByPathDir => Ok("/scheme/udev/links/input/by-path".to_string()),
            HandleKind::LinksDriDir => Ok("/scheme/udev/links/dri".to_string()),
            HandleKind::LinksDriByPathDir => Ok("/scheme/udev/links/dri/by-path".to_string()),
            HandleKind::Link(idx) => self
                .devices
                .get(*idx)
                .and_then(|dev| dev.symlinks.first())
                .map(|link| format!("/scheme/udev{link}"))
                .ok_or_else(|| Error::new(ENOENT)),
            HandleKind::Uevent => Ok("/scheme/udev/uevent".to_string()),
        }
    }
}

impl SchemeSync for UdevScheme {
    fn scheme_root(&mut self) -> Result<usize> {
        Ok(SCHEME_ROOT_ID)
    }

    fn openat(
        &mut self,
        dirfd: usize,
        path: &str,
        _flags: usize,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<OpenResult> {
        if dirfd != SCHEME_ROOT_ID {
            return Err(Error::new(EACCES));
        }

        let kind = self.kind_for_path(path)?;
        let id = if matches!(kind, HandleKind::Root) {
            SCHEME_ROOT_ID
        } else {
            self.allocate_handle(kind)
        };

        Ok(OpenResult::ThisScheme {
            number: id,
            flags: NewFdFlags::empty(),
        })
    }

    fn read(
        &mut self,
        id: usize,
        buf: &mut [u8],
        offset: u64,
        _flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let kind = self.kind_for_id(id)?;
        match kind {
            HandleKind::DevInput(_, mut file) => file.read(buf).map_err(|_| Error::new(ENOENT)),
            _ => {
                let content = self.content_for_handle(&kind)?;
                let bytes = content.as_bytes();

                if offset >= bytes.len() as u64 {
                    return Ok(0);
                }

                let start = offset as usize;
                let remaining = &bytes[start..];
                let to_copy = remaining.len().min(buf.len());
                buf[..to_copy].copy_from_slice(&remaining[..to_copy]);
                Ok(to_copy)
            }
        }
    }

    fn write(
        &mut self,
        id: usize,
        _buf: &[u8],
        _offset: u64,
        _flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let _kind = self.kind_for_id(id)?;
        Err(Error::new(EROFS))
    }

    fn fpath(&mut self, id: usize, buf: &mut [u8], _ctx: &CallerCtx) -> Result<usize> {
        let kind = self.kind_for_id(id)?;
        let path = self.path_for_handle(&kind)?;
        let bytes = path.as_bytes();
        let to_copy = bytes.len().min(buf.len());
        buf[..to_copy].copy_from_slice(&bytes[..to_copy]);
        Ok(to_copy)
    }

    fn fstat(&mut self, id: usize, stat: &mut syscall::Stat, _ctx: &CallerCtx) -> Result<()> {
        let kind = self.kind_for_id(id)?;
        let size = match kind {
            HandleKind::DevInput(_, _) => 0,
            _ => self.content_for_handle(&kind)?.len() as u64,
        };

        stat.st_mode = if Self::is_directory(&kind) {
            MODE_DIR | 0o555
        } else {
            MODE_FILE | 0o444
        };
        stat.st_size = size;
        stat.st_blocks = size.div_ceil(512);
        stat.st_blksize = 4096;
        stat.st_nlink = 1;

        Ok(())
    }

    fn fsync(&mut self, id: usize, _ctx: &CallerCtx) -> Result<()> {
        let _kind = self.kind_for_id(id)?;
        Ok(())
    }

    fn fcntl(&mut self, id: usize, _cmd: usize, _arg: usize, _ctx: &CallerCtx) -> Result<usize> {
        let _kind = self.kind_for_id(id)?;
        Ok(0)
    }

    fn fsize(&mut self, id: usize, _ctx: &CallerCtx) -> Result<u64> {
        let kind = self.kind_for_id(id)?;
        Ok(match kind {
            HandleKind::DevInput(_, _) => 0,
            _ => self.content_for_handle(&kind)?.len() as u64,
        })
    }

    fn ftruncate(&mut self, id: usize, _len: u64, _ctx: &CallerCtx) -> Result<()> {
        let _kind = self.kind_for_id(id)?;
        Err(Error::new(EROFS))
    }

    fn fevent(&mut self, id: usize, _flags: EventFlags, _ctx: &CallerCtx) -> Result<EventFlags> {
        let _kind = self.kind_for_id(id)?;
        Ok(EventFlags::empty())
    }

    fn on_close(&mut self, id: usize) {
        if id != SCHEME_ROOT_ID {
            self.handles.remove(&id);
        }
    }
}

fn path_exists(path: &str) -> bool {
    std::fs::metadata(path).is_ok()
}

fn scheme_registered(name: &str) -> bool {
    std::fs::read_dir("/scheme")
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .any(|entry| entry == name)
}

fn parse_pci_slot(name: &str) -> Option<(u8, u8, u8)> {
    let mut parts = name.split('.');
    let bus = parts.next()?.parse::<u8>().ok()?;
    let dev = parts.next()?.parse::<u8>().ok()?;
    let func = parts.next()?.parse::<u8>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((bus, dev, func))
}

fn basename(path: &str) -> Option<String> {
    path.rsplit('/').next().and_then(|part| {
        if part.is_empty() {
            None
        } else {
            Some(part.to_string())
        }
    })
}

fn gpu_priority(dev: &DeviceInfo) -> u8 {
    match dev.vendor_id {
        0x1002 => 0,
        0x8086 => 1,
        _ => 2,
    }
}

fn input_priority(dev: &DeviceInfo) -> u8 {
    if dev.is_input_keyboard() {
        0
    } else {
        match dev.input_kind {
            Some(InputKind::Mouse) => 1,
            Some(InputKind::Generic) | None => 2,
            Some(InputKind::Keyboard) => 0,
        }
    }
}
