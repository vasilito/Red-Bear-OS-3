#[cfg(target_os = "redox")]
use std::collections::BTreeMap;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::sync::{Arc, Mutex};

#[cfg(target_os = "redox")]
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(target_os = "redox")]
use redox_scheme::scheme::SchemeSync;
#[cfg(target_os = "redox")]
use redox_scheme::{CallerCtx, OpenResult};
#[cfg(target_os = "redox")]
use redox_scheme::{
    SignalBehavior, Socket,
    scheme::{SchemeState, register_sync_scheme},
};
#[cfg(target_os = "redox")]
use syscall::Stat;
#[cfg(target_os = "redox")]
use syscall::error::{EACCES, EBADF, EINVAL, EIO, ENOENT, Error, Result};
#[cfg(target_os = "redox")]
use syscall::flag::{EventFlags, MODE_DIR, MODE_FILE, O_ACCMODE, O_RDONLY};
#[cfg(target_os = "redox")]
use syscall::schemev2::NewFdFlags;

#[cfg(target_os = "redox")]
const SCHEME_NAME: &str = "driver-manager";
#[cfg(target_os = "redox")]
const ROOT_ID: usize = 1;
const MAX_EVENT_LINES: usize = 256;
const PARAM_ROOT: &str = "/tmp/redbear-driver-params";

#[cfg(target_os = "redox")]
#[derive(Clone, Debug, Eq, PartialEq)]
enum HandleKind {
    Root,
    Devices,
    Device(String),
    Bound,
    Events,
}

pub struct DriverManagerScheme {
    pub bound_devices: Mutex<HashMap<String, String>>,
    events: Mutex<VecDeque<String>>,
    #[cfg(target_os = "redox")]
    handles: Mutex<BTreeMap<usize, HandleKind>>,
    #[cfg(target_os = "redox")]
    next_id: AtomicUsize,
}

#[cfg(target_os = "redox")]
struct SchemeServer {
    scheme: Arc<DriverManagerScheme>,
}

impl DriverManagerScheme {
    pub fn new() -> Self {
        Self {
            bound_devices: Mutex::new(HashMap::new()),
            events: Mutex::new(VecDeque::new()),
            #[cfg(target_os = "redox")]
            handles: Mutex::new(BTreeMap::new()),
            #[cfg(target_os = "redox")]
            next_id: AtomicUsize::new(ROOT_ID + 1),
        }
    }

    pub fn bound_device_addresses(&self) -> Vec<String> {
        match self.sorted_bound_addresses() {
            Ok(addresses) => addresses,
            Err(err) => {
                log::error!("driver-manager: failed to snapshot bound devices: {err}");
                Vec::new()
            }
        }
    }

    #[cfg(target_os = "redox")]
    fn alloc_handle(&self, kind: HandleKind) -> Result<usize> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut handles = self.handles.lock().map_err(|_| Error::new(EIO))?;
        handles.insert(id, kind);
        Ok(id)
    }

    #[cfg(target_os = "redox")]
    fn handle(&self, id: usize) -> Result<HandleKind> {
        if id == ROOT_ID {
            return Ok(HandleKind::Root);
        }

        let handles = self.handles.lock().map_err(|_| Error::new(EIO))?;
        handles.get(&id).cloned().ok_or(Error::new(EBADF))
    }

    #[cfg(target_os = "redox")]
    fn open_from_root(&self, path: &str) -> Result<HandleKind> {
        let trimmed = path.trim_matches('/');
        if trimmed.is_empty() {
            return Ok(HandleKind::Root);
        }

        let segments = trimmed
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>();

        match segments.as_slice() {
            ["devices"] => Ok(HandleKind::Devices),
            ["bound"] => Ok(HandleKind::Bound),
            ["events"] => Ok(HandleKind::Events),
            ["devices", pci_addr] if Self::valid_pci_addr(pci_addr) => {
                let _ = self.device_status(pci_addr)?;
                Ok(HandleKind::Device((*pci_addr).to_string()))
            }
            _ => Err(Error::new(ENOENT)),
        }
    }

    #[cfg(target_os = "redox")]
    fn open_from_devices(&self, path: &str) -> Result<HandleKind> {
        let trimmed = path.trim_matches('/');
        if trimmed.is_empty() {
            return Ok(HandleKind::Devices);
        }

        if trimmed.contains('/') || !Self::valid_pci_addr(trimmed) {
            return Err(Error::new(ENOENT));
        }

        let _ = self.device_status(trimmed)?;
        Ok(HandleKind::Device(trimmed.to_string()))
    }

    fn sorted_bound_addresses(&self) -> std::result::Result<Vec<String>, String> {
        let bound_devices = self
            .bound_devices
            .lock()
            .map_err(|err| format!("bound_devices lock poisoned: {err}"))?;
        let mut addresses = bound_devices.keys().cloned().collect::<Vec<_>>();
        addresses.sort_unstable();
        Ok(addresses)
    }

    #[cfg(target_os = "redox")]
    fn device_status(&self, pci_addr: &str) -> Result<String> {
        let bound_devices = self.bound_devices.lock().map_err(|_| Error::new(EIO))?;
        let driver_name = bound_devices
            .get(pci_addr)
            .cloned()
            .ok_or(Error::new(ENOENT))?;

        Ok(format!(
            "pci_addr={pci_addr}\ndriver={driver_name}\nenabled=true\n"
        ))
    }

    #[cfg(target_os = "redox")]
    fn events_output(&self) -> Result<String> {
        let events = self.events.lock().map_err(|_| Error::new(EIO))?;
        Ok(events.iter().cloned().collect::<String>())
    }

    #[cfg(target_os = "redox")]
    fn bound_output(&self) -> Result<String> {
        let bound_devices = self.bound_devices.lock().map_err(|_| Error::new(EIO))?;
        let mut entries = bound_devices
            .iter()
            .map(|(pci_addr, driver_name)| format!("{pci_addr} -> {driver_name}"))
            .collect::<Vec<_>>();
        entries.sort_unstable();

        if entries.is_empty() {
            Ok(String::new())
        } else {
            Ok(format!("{}\n", entries.join("\n")))
        }
    }

    #[cfg(target_os = "redox")]
    fn read_handle_string(&self, kind: &HandleKind) -> Result<String> {
        match kind {
            HandleKind::Root => Ok("devices\nevents\n".to_string()),
            HandleKind::Devices => {
                let addresses = self.sorted_bound_addresses().map_err(|err| {
                    log::error!("driver-manager: failed to read bound device list: {err}");
                    Error::new(EIO)
                })?;
                if addresses.is_empty() {
                    Ok(String::new())
                } else {
                    Ok(format!("{}\n", addresses.join("\n")))
                }
            }
            HandleKind::Device(pci_addr) => self.device_status(pci_addr),
            HandleKind::Bound => self.bound_output(),
            HandleKind::Events => self.events_output(),
        }
    }

    #[cfg(target_os = "redox")]
    fn handle_path(&self, kind: &HandleKind) -> String {
        match kind {
            HandleKind::Root => format!("{SCHEME_NAME}:/"),
            HandleKind::Devices => format!("{SCHEME_NAME}:/devices"),
            HandleKind::Device(pci_addr) => format!("{SCHEME_NAME}:/devices/{pci_addr}"),
            HandleKind::Bound => format!("{SCHEME_NAME}:/bound"),
            HandleKind::Events => format!("{SCHEME_NAME}:/events"),
        }
    }

    #[cfg(target_os = "redox")]
    fn handle_mode(&self, kind: &HandleKind) -> u16 {
        match kind {
            HandleKind::Root | HandleKind::Devices => MODE_DIR | 0o755,
            HandleKind::Device(_) | HandleKind::Bound | HandleKind::Events => MODE_FILE | 0o644,
        }
    }

    #[cfg(target_os = "redox")]
    fn valid_pci_addr(value: &str) -> bool {
        !value.is_empty()
            && value
                .chars()
                .all(|ch| ch.is_ascii_hexdigit() || matches!(ch, ':' | '.'))
    }

    fn push_event_line(&self, line: String) {
        match self.events.lock() {
            Ok(mut events) => {
                if events.len() >= MAX_EVENT_LINES {
                    events.pop_front();
                }
                events.push_back(line);
            }
            Err(err) => {
                log::error!("driver-manager: failed to record hotplug event: {err}");
            }
        }
    }
}

#[cfg(target_os = "redox")]
impl SchemeServer {
    fn new(scheme: Arc<DriverManagerScheme>) -> Self {
        Self { scheme }
    }
}

#[cfg(target_os = "redox")]
impl SchemeSync for SchemeServer {
    fn scheme_root(&mut self) -> Result<usize> {
        Ok(ROOT_ID)
    }

    fn openat(
        &mut self,
        dirfd: usize,
        path: &str,
        flags: usize,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<OpenResult> {
        if flags & O_ACCMODE != O_RDONLY {
            return Err(Error::new(EACCES));
        }

        let kind = match self.scheme.handle(dirfd)? {
            HandleKind::Root => self.scheme.open_from_root(path)?,
            HandleKind::Devices => self.scheme.open_from_devices(path)?,
            _ => return Err(Error::new(EACCES)),
        };

        Ok(OpenResult::ThisScheme {
            number: self.scheme.alloc_handle(kind)?,
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
        let kind = self.scheme.handle(id)?;
        let data = self.scheme.read_handle_string(&kind)?;
        let bytes = data.as_bytes();
        let offset = usize::try_from(offset).map_err(|_| Error::new(EINVAL))?;

        if offset >= bytes.len() {
            return Ok(0);
        }

        let count = (bytes.len() - offset).min(buf.len());
        buf[..count].copy_from_slice(&bytes[offset..offset + count]);
        Ok(count)
    }

    fn fstat(&mut self, id: usize, stat: &mut Stat, _ctx: &CallerCtx) -> Result<()> {
        let kind = self.scheme.handle(id)?;
        stat.st_mode = self.scheme.handle_mode(&kind);
        Ok(())
    }

    fn fpath(&mut self, id: usize, buf: &mut [u8], _ctx: &CallerCtx) -> Result<usize> {
        let kind = self.scheme.handle(id)?;
        let path = self.scheme.handle_path(&kind);
        let bytes = path.as_bytes();
        let count = bytes.len().min(buf.len());
        buf[..count].copy_from_slice(&bytes[..count]);
        Ok(count)
    }

    fn fsync(&mut self, id: usize, _ctx: &CallerCtx) -> Result<()> {
        let _ = self.scheme.handle(id)?;
        Ok(())
    }

    fn fcntl(&mut self, id: usize, _cmd: usize, _arg: usize, _ctx: &CallerCtx) -> Result<usize> {
        let _ = self.scheme.handle(id)?;
        Ok(0)
    }

    fn fevent(&mut self, id: usize, _flags: EventFlags, _ctx: &CallerCtx) -> Result<EventFlags> {
        let _ = self.scheme.handle(id)?;
        Ok(EventFlags::empty())
    }

    fn on_close(&mut self, id: usize) {
        if id == ROOT_ID {
            return;
        }

        if let Ok(mut handles) = self.scheme.handles.lock() {
            handles.remove(&id);
        }
    }
}

fn write_driver_param(pci_addr: &str, param: &str, value: &str) -> std::io::Result<()> {
    let dir = format!("{PARAM_ROOT}/{pci_addr}");
    fs::create_dir_all(&dir)?;
    fs::write(format!("{dir}/{param}"), value)
}

pub fn notify_bind(scheme: &DriverManagerScheme, pci_addr: &str, driver_name: &str) {
    match scheme.bound_devices.lock() {
        Ok(mut bound_devices) => {
            bound_devices.insert(pci_addr.to_string(), driver_name.to_string());
        }
        Err(err) => {
            log::error!(
                "driver-manager: failed to update bound device state for {pci_addr}: {err}"
            );
        }
    }

    scheme.push_event_line(format!(
        "action=bind pci_addr={pci_addr} driver={driver_name}\n"
    ));

    if let Err(err) = write_driver_param(pci_addr, "driver", driver_name) {
        log::warn!("driver-manager: failed to write driver param for {pci_addr}: {err}");
    }
    if let Err(err) = write_driver_param(pci_addr, "enabled", "true") {
        log::warn!("driver-manager: failed to write enabled param for {pci_addr}: {err}");
    }
}

pub fn notify_unbind(scheme: &DriverManagerScheme, pci_addr: &str) {
    let previous_driver = match scheme.bound_devices.lock() {
        Ok(mut bound_devices) => bound_devices.remove(pci_addr),
        Err(err) => {
            log::error!(
                "driver-manager: failed to remove bound device state for {pci_addr}: {err}"
            );
            None
        }
    };

    let event_line = if let Some(driver_name) = previous_driver.as_deref() {
        format!("action=unbind pci_addr={pci_addr} driver={driver_name}\n")
    } else {
        format!("action=unbind pci_addr={pci_addr}\n")
    };
    scheme.push_event_line(event_line);

    if let Err(err) = write_driver_param(pci_addr, "driver", "") {
        log::warn!("driver-manager: failed to clear driver param for {pci_addr}: {err}");
    }
    if let Err(err) = write_driver_param(pci_addr, "enabled", "false") {
        log::warn!("driver-manager: failed to write disabled param for {pci_addr}: {err}");
    }
}

#[cfg(target_os = "redox")]
pub fn start_scheme_server(scheme: Arc<DriverManagerScheme>) -> std::result::Result<(), String> {
    let socket = Socket::create()
        .map_err(|err| format!("driver-manager: failed to create scheme socket: {err}"))?;
    let mut server = SchemeServer::new(scheme);

    register_sync_scheme(&socket, SCHEME_NAME, &mut server)
        .map_err(|err| format!("driver-manager: failed to register scheme:{SCHEME_NAME}: {err}"))?;

    log::info!("driver-manager: registered scheme:{SCHEME_NAME}");

    std::thread::Builder::new()
        .name("driver-manager-scheme".to_string())
        .spawn(move || {
            let mut state = SchemeState::new();

            loop {
                let request = match socket.next_request(SignalBehavior::Restart) {
                    Ok(Some(request)) => request,
                    Ok(None) => {
                        log::info!("driver-manager: scheme socket closed, shutting down");
                        break;
                    }
                    Err(err) => {
                        log::error!("driver-manager: failed to read scheme request: {err}");
                        break;
                    }
                };

                if let redox_scheme::RequestKind::Call(request) = request.kind() {
                    let response = request.handle_sync(&mut server, &mut state);
                    if let Err(err) = socket.write_response(response, SignalBehavior::Restart) {
                        log::error!("driver-manager: failed to write scheme response: {err}");
                        break;
                    }
                }
            }
        })
        .map_err(|err| format!("driver-manager: failed to spawn scheme server thread: {err}"))?;

    Ok(())
}

#[cfg(not(target_os = "redox"))]
pub fn start_scheme_server(_scheme: Arc<DriverManagerScheme>) -> std::result::Result<(), String> {
    Ok(())
}
