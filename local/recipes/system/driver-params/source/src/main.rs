use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};

use log::{LevelFilter, Metadata, Record};
use redox_scheme::scheme::SchemeSync;
use redox_scheme::{CallerCtx, OpenResult};
#[cfg(target_os = "redox")]
use redox_scheme::scheme::SchemeState;
#[cfg(target_os = "redox")]
use redox_scheme::{SignalBehavior, Socket};
use syscall::error::{Error, Result, EACCES, EBADF, EINVAL, ENOENT, EROFS};
use syscall::flag::{EventFlags, MODE_DIR, MODE_FILE, O_ACCMODE, O_RDONLY};
use syscall::schemev2::NewFdFlags;
use syscall::Stat;

const SCHEME_NAME: &str = "driver-params";
const SCHEME_ROOT_ID: usize = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParamKind {
    Bool,
    Integer,
    Text,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParamEntry {
    kind: ParamKind,
    value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum HandleKind {
    Root,
    DriverDir(String),
    Parameter {
        driver_name: String,
        parameter_name: String,
    },
}

struct DriverParamsScheme {
    drivers: HashMap<String, HashMap<String, ParamEntry>>,
    handles: HashMap<usize, HandleKind>,
    next_id: usize,
    bound_path: PathBuf,
}

impl ParamKind {
    fn infer(value: &str) -> Self {
        if matches!(value, "true" | "false") {
            Self::Bool
        } else if value.parse::<i64>().is_ok() {
            Self::Integer
        } else {
            Self::Text
        }
    }

    fn normalize(self, value: &str) -> Result<String> {
        match self {
            Self::Bool => match value {
                "true" | "false" => Ok(value.to_string()),
                _ => Err(Error::new(EINVAL)),
            },
            Self::Integer => value
                .parse::<i64>()
                .map(|parsed| parsed.to_string())
                .map_err(|_| Error::new(EINVAL)),
            Self::Text => Ok(value.to_string()),
        }
    }
}

impl DriverParamsScheme {
    fn new(bound_path: PathBuf) -> Self {
        Self {
            drivers: HashMap::new(),
            handles: HashMap::new(),
            next_id: SCHEME_ROOT_ID + 1,
            bound_path,
        }
    }

    fn alloc_handle(&mut self, kind: HandleKind) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.handles.insert(id, kind);
        id
    }

    fn handle(&self, id: usize) -> Result<&HandleKind> {
        self.handles.get(&id).ok_or(Error::new(EBADF))
    }

    fn read_driver_manager_bound(&mut self) {
        match fs::read_to_string(&self.bound_path) {
            Ok(contents) => {
                for driver_name in Self::discover_driver_names(&contents) {
                    self.drivers
                        .entry(driver_name)
                        .or_default()
                        .entry("enabled".to_string())
                        .or_insert_with(|| ParamEntry {
                            kind: ParamKind::Bool,
                            value: "true".to_string(),
                        });
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => log::warn!(
                "driver-params: failed to read {}: {err}",
                self.bound_path.display()
            ),
        }
    }

    fn discover_driver_names(contents: &str) -> Vec<String> {
        let mut discovered = Vec::new();

        for line in contents.lines().map(str::trim).filter(|line| !line.is_empty()) {
            if let Some(driver_name) = Self::extract_driver_name(line) {
                if !discovered.iter().any(|existing| existing == &driver_name) {
                    discovered.push(driver_name);
                }
            }
        }

        discovered
    }

    fn extract_driver_name(line: &str) -> Option<String> {
        for key in ["driver", "name"] {
            if let Some(value) = Self::extract_assignment_value(line, key) {
                return Some(value);
            }
        }

        if let Some((_, tail)) = line.rsplit_once("->") {
            let candidate = tail.split_whitespace().next().unwrap_or_default();
            if Self::valid_component(candidate) {
                return Some(candidate.to_string());
            }
        }

        if Self::valid_component(line) {
            return Some(line.to_string());
        }

        None
    }

    fn extract_assignment_value(line: &str, key: &str) -> Option<String> {
        let needle = format!("{key}=");
        let start = line.find(&needle)? + needle.len();
        let tail = &line[start..];
        let candidate = tail
            .split(|ch: char| ch.is_whitespace() || ch == ',' || ch == ';')
            .next()
            .unwrap_or_default()
            .trim_matches('"');

        Self::valid_component(candidate).then(|| candidate.to_string())
    }

    fn valid_component(value: &str) -> bool {
        !value.is_empty()
            && value
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    }

    fn sorted_driver_names(&self) -> Vec<String> {
        let mut drivers = self.drivers.keys().cloned().collect::<Vec<_>>();
        drivers.sort_unstable();
        drivers
    }

    fn sorted_parameter_names(&self, driver_name: &str) -> Result<Vec<String>> {
        let mut names = self
            .drivers
            .get(driver_name)
            .ok_or(Error::new(ENOENT))?
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        names.sort_unstable();
        Ok(names)
    }

    fn list_output(values: &[String]) -> String {
        if values.is_empty() {
            String::new()
        } else {
            format!("{}\n", values.join("\n"))
        }
    }

    fn read_handle_string(&mut self, kind: &HandleKind) -> Result<String> {
        self.read_driver_manager_bound();

        match kind {
            HandleKind::Root => Ok(Self::list_output(&self.sorted_driver_names())),
            HandleKind::DriverDir(driver_name) => {
                Ok(Self::list_output(&self.sorted_parameter_names(driver_name)?))
            }
            HandleKind::Parameter {
                driver_name,
                parameter_name,
            } => {
                let entry = self
                    .drivers
                    .get(driver_name)
                    .and_then(|parameters| parameters.get(parameter_name))
                    .ok_or(Error::new(ENOENT))?;
                Ok(format!("{}\n", entry.value))
            }
        }
    }

    fn parameter_handle(
        &mut self,
        driver_name: &str,
        parameter_name: &str,
        write_intent: bool,
    ) -> Result<HandleKind> {
        if !Self::valid_component(driver_name) || !Self::valid_component(parameter_name) {
            return Err(Error::new(ENOENT));
        }

        if !self.drivers.contains_key(driver_name) {
            if !write_intent {
                return Err(Error::new(ENOENT));
            }

            self.drivers.insert(driver_name.to_string(), HashMap::new());
        }

        if !self
            .drivers
            .get(driver_name)
            .and_then(|parameters| parameters.get(parameter_name))
            .is_some()
            && !write_intent
        {
            return Err(Error::new(ENOENT));
        }

        Ok(HandleKind::Parameter {
            driver_name: driver_name.to_string(),
            parameter_name: parameter_name.to_string(),
        })
    }

    fn open_from_root(&mut self, path: &str, write_intent: bool) -> Result<HandleKind> {
        let trimmed = path.trim_matches('/');
        if trimmed.is_empty() {
            return Ok(HandleKind::Root);
        }

        let segments = trimmed
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>();

        match segments.as_slice() {
            [driver_name] => {
                if !Self::valid_component(driver_name) {
                    return Err(Error::new(ENOENT));
                }
                if self.drivers.contains_key(*driver_name) {
                    Ok(HandleKind::DriverDir((*driver_name).to_string()))
                } else {
                    Err(Error::new(ENOENT))
                }
            }
            [driver_name, parameter_name] => {
                self.parameter_handle(driver_name, parameter_name, write_intent)
            }
            _ => Err(Error::new(ENOENT)),
        }
    }

    fn open_from_driver(
        &mut self,
        driver_name: &str,
        path: &str,
        write_intent: bool,
    ) -> Result<HandleKind> {
        let trimmed = path.trim_matches('/');
        if trimmed.is_empty() {
            return Ok(HandleKind::DriverDir(driver_name.to_string()));
        }

        if trimmed.contains('/') {
            return Err(Error::new(ENOENT));
        }

        self.parameter_handle(driver_name, trimmed, write_intent)
    }

    fn parse_write_value(buf: &[u8]) -> Result<String> {
        let value = std::str::from_utf8(buf)
            .map_err(|_| Error::new(EINVAL))?
            .trim()
            .to_string();

        if value.is_empty() {
            return Err(Error::new(EINVAL));
        }

        Ok(value)
    }
}

impl SchemeSync for DriverParamsScheme {
    fn scheme_root(&mut self) -> Result<usize> {
        Ok(SCHEME_ROOT_ID)
    }

    fn openat(
        &mut self,
        dirfd: usize,
        path: &str,
        flags: usize,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<OpenResult> {
        self.read_driver_manager_bound();

        let write_intent = flags & O_ACCMODE != O_RDONLY;
        let kind = if dirfd == SCHEME_ROOT_ID {
            self.open_from_root(path, write_intent)?
        } else {
            match self.handle(dirfd)?.clone() {
                HandleKind::DriverDir(driver_name) => {
                    self.open_from_driver(&driver_name, path, write_intent)?
                }
                _ => return Err(Error::new(EACCES)),
            }
        };

        Ok(OpenResult::ThisScheme {
            number: self.alloc_handle(kind),
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
        let kind = self.handle(id)?.clone();
        let data = self.read_handle_string(&kind)?;
        let bytes = data.as_bytes();
        let offset = usize::try_from(offset).map_err(|_| Error::new(EINVAL))?;

        if offset >= bytes.len() {
            return Ok(0);
        }

        let count = (bytes.len() - offset).min(buf.len());
        buf[..count].copy_from_slice(&bytes[offset..offset + count]);
        Ok(count)
    }

    fn write(
        &mut self,
        id: usize,
        buf: &[u8],
        _offset: u64,
        _flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let value = Self::parse_write_value(buf)?;

        match self.handle(id)?.clone() {
            HandleKind::Parameter {
                driver_name,
                parameter_name,
            } => {
                let parameters = self.drivers.entry(driver_name).or_default();

                match parameters.get_mut(&parameter_name) {
                    Some(entry) => {
                        entry.value = entry.kind.normalize(&value)?;
                    }
                    None => {
                        let kind = ParamKind::infer(&value);
                        let normalized = kind.normalize(&value)?;
                        parameters.insert(
                            parameter_name,
                            ParamEntry {
                                kind,
                                value: normalized,
                            },
                        );
                    }
                }

                Ok(buf.len())
            }
            _ => Err(Error::new(EROFS)),
        }
    }

    fn fstat(&mut self, id: usize, stat: &mut Stat, _ctx: &CallerCtx) -> Result<()> {
        self.read_driver_manager_bound();

        stat.st_mode = match self.handle(id)? {
            HandleKind::Root | HandleKind::DriverDir(_) => MODE_DIR | 0o755,
            HandleKind::Parameter { .. } => MODE_FILE | 0o644,
        };

        Ok(())
    }

    fn fpath(&mut self, id: usize, buf: &mut [u8], _ctx: &CallerCtx) -> Result<usize> {
        let path = match self.handle(id)? {
            HandleKind::Root => format!("{SCHEME_NAME}:/"),
            HandleKind::DriverDir(driver_name) => format!("{SCHEME_NAME}:/{driver_name}"),
            HandleKind::Parameter {
                driver_name,
                parameter_name,
            } => {
                format!("{SCHEME_NAME}:/{driver_name}/{parameter_name}")
            }
        };

        let bytes = path.as_bytes();
        let count = bytes.len().min(buf.len());
        buf[..count].copy_from_slice(&bytes[..count]);
        Ok(count)
    }

    fn fsync(&mut self, id: usize, _ctx: &CallerCtx) -> Result<()> {
        let _ = self.handle(id)?;
        Ok(())
    }

    fn fcntl(&mut self, id: usize, _cmd: usize, _arg: usize, _ctx: &CallerCtx) -> Result<usize> {
        let _ = self.handle(id)?;
        Ok(0)
    }

    fn fevent(&mut self, id: usize, _flags: EventFlags, _ctx: &CallerCtx) -> Result<EventFlags> {
        let _ = self.handle(id)?;
        Ok(EventFlags::empty())
    }

    fn on_close(&mut self, id: usize) {
        if id != SCHEME_ROOT_ID {
            self.handles.remove(&id);
        }
    }
}

struct StderrLogger {
    default_level: LevelFilter,
}

static LOGGER_LEVEL: AtomicUsize = AtomicUsize::new(3);

const STDERR_LOGGER: StderrLogger = StderrLogger {
    default_level: LevelFilter::Info,
};

fn level_filter_to_usize(level: LevelFilter) -> usize {
    match level {
        LevelFilter::Off => 0,
        LevelFilter::Error => 1,
        LevelFilter::Warn => 2,
        LevelFilter::Info => 3,
        LevelFilter::Debug => 4,
        LevelFilter::Trace => 5,
    }
}

fn usize_to_level_filter(level: usize, fallback: LevelFilter) -> LevelFilter {
    match level {
        0 => LevelFilter::Off,
        1 => LevelFilter::Error,
        2 => LevelFilter::Warn,
        3 => LevelFilter::Info,
        4 => LevelFilter::Debug,
        5 => LevelFilter::Trace,
        _ => fallback,
    }
}

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level()
            <= usize_to_level_filter(
                LOGGER_LEVEL.load(Ordering::Relaxed),
                self.default_level,
            )
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            eprintln!("[{}] {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

fn init_logging(level: LevelFilter) {
    LOGGER_LEVEL.store(level_filter_to_usize(level), Ordering::Relaxed);
    let _ = log::set_logger(&STDERR_LOGGER);
    log::set_max_level(level);
}

fn log_level_from_env() -> LevelFilter {
    match env::var("DRIVER_PARAMS_LOG").as_deref() {
        Ok("trace") => LevelFilter::Trace,
        Ok("debug") => LevelFilter::Debug,
        Ok("warn") => LevelFilter::Warn,
        Ok("error") => LevelFilter::Error,
        _ => LevelFilter::Info,
    }
}

fn bound_path_from_env() -> PathBuf {
    env::var("DRIVER_PARAMS_BOUND_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/scheme/driver-manager/bound"))
}

#[cfg(target_os = "redox")]
fn init_notify_fd() -> std::result::Result<i32, String> {
    let raw = env::var("INIT_NOTIFY")
        .map_err(|_| "driver-params: INIT_NOTIFY not set".to_string())?;
    raw.parse::<i32>()
        .map_err(|_| "driver-params: INIT_NOTIFY is not a valid fd".to_string())
}

#[cfg(target_os = "redox")]
fn notify_scheme_ready(
    notify_fd: i32,
    socket: &Socket,
    scheme: &mut DriverParamsScheme,
) -> std::result::Result<(), String> {
    let cap_id = scheme
        .scheme_root()
        .map_err(|err| format!("driver-params: scheme_root failed: {err}"))?;
    let cap_fd = socket
        .create_this_scheme_fd(0, cap_id, 0, 0)
        .map_err(|err| format!("driver-params: create_this_scheme_fd failed: {err}"))?;

    syscall::call_wo(
        notify_fd as usize,
        &libredox::Fd::new(cap_fd).into_raw().to_ne_bytes(),
        syscall::CallFlags::FD,
        &[],
    )
    .map_err(|err| format!("driver-params: failed to notify init that scheme is ready: {err}"))?;
    Ok(())
}

#[cfg(target_os = "redox")]
fn run_daemon(bound_path: PathBuf) -> std::result::Result<(), String> {
    let notify_fd = init_notify_fd()?;
    let socket = Socket::create()
        .map_err(|err| format!("driver-params: failed to create scheme socket: {err}"))?;
    let mut state = SchemeState::new();
    let mut scheme = DriverParamsScheme::new(bound_path);
    scheme.read_driver_manager_bound();

    notify_scheme_ready(notify_fd, &socket, &mut scheme)?;

    libredox::call::setrens(0, 0)
        .map_err(|err| format!("driver-params: failed to enter null namespace: {err}"))?;

    log::info!("driver-params: registered scheme:{SCHEME_NAME}");

    loop {
        let request = socket
            .next_request(SignalBehavior::Restart)
            .map_err(|err| format!("driver-params: failed to read scheme request: {err}"))?;

        let Some(request) = request else {
            return Ok(());
        };

        if let redox_scheme::RequestKind::Call(request) = request.kind() {
            let response = request.handle_sync(&mut scheme, &mut state);
            socket
                .write_response(response, SignalBehavior::Restart)
                .map_err(|err| format!("driver-params: failed to write response: {err}"))?;
        }
    }
}

fn host_probe(bound_path: &Path) {
    let mut scheme = DriverParamsScheme::new(bound_path.to_path_buf());
    scheme.read_driver_manager_bound();

    for driver_name in scheme.sorted_driver_names() {
        println!("{driver_name}");
    }
}

fn main() {
    init_logging(log_level_from_env());

    let bound_path = bound_path_from_env();

    if env::args().nth(1).as_deref() == Some("--probe") {
        host_probe(&bound_path);
        return;
    }

    #[cfg(not(target_os = "redox"))]
    {
        log::error!(
            "driver-params: daemon mode is only supported on Redox; use --probe on host"
        );
        process::exit(1);
    }

    #[cfg(target_os = "redox")]
    {
        if let Err(err) = run_daemon(bound_path) {
            log::error!("{err}");
            process::exit(1);
        }
    }
}
