// thermald — ACPI thermal zone manager
// Reads thermal zone data from /scheme/acpi/thermal/
// Provides /scheme/thermal for temperature queries

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use log::{error, info, warn, LevelFilter, Metadata, Record};

#[cfg(target_os = "redox")]
use redox_scheme::{
    scheme::{SchemeState, SchemeSync},
    CallerCtx, OpenResult, SignalBehavior, Socket,
};
#[cfg(target_os = "redox")]
use syscall::flag::{MODE_DIR, MODE_FILE};
#[cfg(target_os = "redox")]
use syscall::schemev2::NewFdFlags;
#[cfg(target_os = "redox")]
use syscall::{
    error::{Error as SysError, Result as SysResult, EBADF, EINVAL, ENOENT},
    Stat,
};

const ACPI_THERMAL_ROOT: &str = "/scheme/acpi/thermal";
const ACPI_SLEEP_PATH: &str = "/scheme/acpi/sleep";
const CPUFREQ_GOVERNOR_PATHS: [&str; 2] = ["/scheme/cpufreq/governor", "/scheme/cpufreq/control/governor"];
const THERMAL_POLL_INTERVAL: Duration = Duration::from_secs(2);
const PASSIVE_HYSTERESIS_C: f64 = 2.0;
const ACTIVE_MARGIN_C: f64 = 5.0;

struct StderrLogger {
    level: LevelFilter,
}

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            let _ = writeln!(io::stderr().lock(), "[{}] thermald: {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

#[derive(Clone, Debug)]
pub struct ThermalZone {
    name: String,
    temperature: f64,
    passive_threshold: Option<f64>,
    critical_threshold: Option<f64>,
    tc1: Option<f64>,
    tc2: Option<f64>,
}

#[derive(Clone, Debug)]
struct ZoneRuntime {
    zone: ThermalZone,
    source_dir: PathBuf,
    last_temperature: Option<f64>,
    passive_cooling: bool,
    active_cooling: bool,
}

#[derive(Clone, Debug, Default)]
struct ThermalState {
    zones: Vec<ZoneRuntime>,
    passive_governor_engaged: bool,
}

impl ZoneRuntime {
    #[cfg(target_os = "redox")]
    fn status_line(&self) -> &'static str {
        match (self.active_cooling, self.passive_cooling) {
            (true, _) => "active",
            (false, true) => "passive",
            (false, false) => "normal",
        }
    }

    #[cfg(target_os = "redox")]
    fn summary(&self) -> String {
        format!(
            "name={}\ntemperature_c={:.1}\npassive_threshold_c={}\ncritical_threshold_c={}\ntc1={}\ntc2={}\nstate={}\n",
            self.zone.name,
            self.zone.temperature,
            format_option(self.zone.passive_threshold),
            format_option(self.zone.critical_threshold),
            format_option(self.zone.tc1),
            format_option(self.zone.tc2),
            self.status_line(),
        )
    }
}

fn init_logging(level: LevelFilter) {
    if log::set_boxed_logger(Box::new(StderrLogger { level })).is_err() {
        return;
    }

    log::set_max_level(level);
}

#[cfg(target_os = "redox")]
fn format_option(value: Option<f64>) -> String {
    match value {
        Some(number) => format!("{number:.1}"),
        None => "na".to_string(),
    }
}

fn read_trimmed(path: impl AsRef<Path>) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_scalar(text: &str) -> Option<f64> {
    for token in text.split(|character: char| {
        character.is_whitespace() || matches!(character, ',' | ';' | ':' | '=' | '[' | ']' | '(' | ')')
    }) {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }

        if let Some(hex) = token.strip_prefix("0x").or_else(|| token.strip_prefix("0X")) {
            if let Ok(value) = u64::from_str_radix(hex, 16) {
                return Some(value as f64);
            }
        }

        if let Ok(value) = token.parse::<f64>() {
            return Some(value);
        }
    }

    None
}

fn read_scalar(dir: &Path, names: &[&str]) -> Option<f64> {
    for name in names {
        let path = dir.join(name);
        let Some(value) = read_trimmed(&path) else {
            continue;
        };
        if let Some(parsed) = parse_scalar(&value) {
            return Some(parsed);
        }
    }

    None
}

fn normalize_temperature_celsius(raw: f64) -> f64 {
    if raw >= 2_000.0 {
        (raw / 10.0) - 273.15
    } else if raw >= 200.0 {
        raw - 273.15
    } else {
        raw
    }
}

fn zone_name_for_entry(entry: &fs::DirEntry) -> Option<String> {
    entry.file_name().into_string().ok()
}

fn discover_zone_dirs() -> Vec<(String, PathBuf)> {
    let mut zones = Vec::new();
    let Ok(entries) = fs::read_dir(ACPI_THERMAL_ROOT) else {
        return zones;
    };

    for entry in entries.filter_map(Result::ok) {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        if !file_type.is_dir() {
            continue;
        }

        let Some(name) = zone_name_for_entry(&entry) else {
            continue;
        };

        zones.push((name, entry.path()));
    }

    zones.sort_by(|left, right| left.0.cmp(&right.0));
    zones
}

fn read_zone_runtime(name: String, dir: PathBuf, previous: Option<&ZoneRuntime>) -> Option<ZoneRuntime> {
    let temperature = normalize_temperature_celsius(read_scalar(&dir, &["_TMP", "tmp", "temperature"])?);
    let passive_threshold =
        read_scalar(&dir, &["_PSV", "psv", "passive_threshold"]).map(normalize_temperature_celsius);
    let critical_threshold =
        read_scalar(&dir, &["_CRT", "crt", "critical_threshold"]).map(normalize_temperature_celsius);
    let tc1 = read_scalar(&dir, &["_TC1", "tc1"]);
    let tc2 = read_scalar(&dir, &["_TC2", "tc2"]);

    Some(ZoneRuntime {
        zone: ThermalZone {
            name,
            temperature,
            passive_threshold,
            critical_threshold,
            tc1,
            tc2,
        },
        source_dir: dir,
        last_temperature: previous.map(|zone| zone.zone.temperature),
        passive_cooling: previous.is_some_and(|zone| zone.passive_cooling),
        active_cooling: previous.is_some_and(|zone| zone.active_cooling),
    })
}

fn refresh_zones(previous: &[ZoneRuntime]) -> Vec<ZoneRuntime> {
    let previous_by_name: BTreeMap<&str, &ZoneRuntime> = previous
        .iter()
        .map(|zone| (zone.zone.name.as_str(), zone))
        .collect();

    let mut refreshed = Vec::new();
    for (name, dir) in discover_zone_dirs() {
        let previous_zone = previous_by_name.get(name.as_str()).copied();
        if let Some(zone) = read_zone_runtime(name, dir, previous_zone) {
            refreshed.push(zone);
        }
    }

    refreshed
}

fn cpufreq_governor_path() -> Option<&'static str> {
    CPUFREQ_GOVERNOR_PATHS
        .iter()
        .copied()
        .find(|candidate| Path::new(candidate).exists())
}

fn set_cpufreq_governor(governor: &str) -> io::Result<bool> {
    let Some(path) = cpufreq_governor_path() else {
        return Ok(false);
    };

    fs::write(path, format!("{governor}\n"))?;
    Ok(true)
}

fn write_scp_policy(dir: &Path, active: bool) -> io::Result<bool> {
    let policy = if active { "0\n" } else { "1\n" };

    for candidate in ["_SCP", "scp", "cooling_policy"] {
        let path = dir.join(candidate);
        if !path.exists() {
            continue;
        }

        fs::write(path, policy)?;
        return Ok(true);
    }

    Ok(false)
}

fn should_request_active_cooling(zone: &ZoneRuntime) -> bool {
    let Some(passive_threshold) = zone.zone.passive_threshold else {
        return false;
    };

    if zone.zone.temperature < passive_threshold {
        return false;
    }

    if zone
        .zone
        .critical_threshold
        .is_some_and(|critical| zone.zone.temperature >= critical - ACTIVE_MARGIN_C)
    {
        return true;
    }

    let Some(previous_temperature) = zone.last_temperature else {
        return zone.zone.temperature >= passive_threshold + ACTIVE_MARGIN_C;
    };

    let slope = zone.zone.temperature - previous_temperature;
    let tc1 = zone.zone.tc1.unwrap_or(1.0);
    let tc2 = zone.zone.tc2.unwrap_or(1.0);
    let weighted_trend = (slope * tc1) + ((zone.zone.temperature - passive_threshold).max(0.0) * tc2);

    weighted_trend >= 1.0 || zone.zone.temperature >= passive_threshold + ACTIVE_MARGIN_C
}

fn write_acpi_sleep_request() -> io::Result<bool> {
    if !Path::new(ACPI_SLEEP_PATH).exists() {
        return Ok(false);
    }

    let mut last_error = None;
    for request in ["S5\n", "5\n", "shutdown\n"] {
        match fs::write(ACPI_SLEEP_PATH, request) {
            Ok(()) => return Ok(true),
            Err(error) => last_error = Some(error),
        }
    }

    if let Some(error) = last_error {
        Err(error)
    } else {
        Ok(false)
    }
}

fn try_shutdown_command(argv: &[&str]) -> io::Result<bool> {
    if argv.is_empty() {
        return Ok(false);
    }

    let status = Command::new(argv[0]).args(&argv[1..]).status()?;
    Ok(status.success())
}

fn emergency_shutdown(zone: &ZoneRuntime) -> ! {
    error!(
        "CRITICAL: zone {} at {:.1}°C (limit {:.1}°C)",
        zone.zone.name,
        zone.zone.temperature,
        zone.zone.critical_threshold.unwrap_or(zone.zone.temperature),
    );
    error!("initiating emergency shutdown");

    match write_acpi_sleep_request() {
        Ok(true) => error!("requested ACPI S5 through {ACPI_SLEEP_PATH}"),
        Ok(false) => warn!("{ACPI_SLEEP_PATH} is unavailable; falling back to shutdown commands"),
        Err(error) => warn!("failed to request ACPI S5 through {ACPI_SLEEP_PATH}: {error}"),
    }

    for argv in [
        &["/usr/bin/shutdown"][..],
        &["shutdown"][..],
        &["poweroff"][..],
    ] {
        match try_shutdown_command(argv) {
            Ok(true) => error!("shutdown command {:?} completed successfully", argv),
            Ok(false) => warn!("shutdown command {:?} returned a failure status", argv),
            Err(error) => warn!("failed to execute shutdown command {:?}: {}", argv, error),
        }
    }

    process::exit(1);
}

fn update_policy(shared: &Arc<RwLock<ThermalState>>) {
    let previous_state = match shared.as_ref().read() {
        Ok(state) => state.clone(),
        Err(error) => {
            warn!("state lock poisoned while reading thermal state: {error}");
            ThermalState::default()
        }
    };

    let mut zones = refresh_zones(&previous_state.zones);
    let mut passive_needed = false;

    for zone in &mut zones {
        if let Some(critical_threshold) = zone.zone.critical_threshold {
            if zone.zone.temperature >= critical_threshold {
                emergency_shutdown(zone);
            }
        }

        if let Some(passive_threshold) = zone.zone.passive_threshold {
            if zone.zone.temperature >= passive_threshold {
                passive_needed = true;
                if !zone.passive_cooling {
                    warn!(
                        "zone {} at {:.1}°C (passive limit {:.1}°C) — requesting powersave governor",
                        zone.zone.name,
                        zone.zone.temperature,
                        passive_threshold,
                    );
                }
                zone.passive_cooling = true;
            }
            if zone.passive_cooling
                && zone.zone.temperature <= passive_threshold - PASSIVE_HYSTERESIS_C
            {
                info!(
                    "zone {} cooled to {:.1}°C; passive throttling no longer required",
                    zone.zone.name,
                    zone.zone.temperature,
                );
                zone.passive_cooling = false;
            }
        } else {
            zone.passive_cooling = false;
        }

        let active_needed = should_request_active_cooling(zone);
        if active_needed != zone.active_cooling {
            match write_scp_policy(&zone.source_dir, active_needed) {
                Ok(true) => {
                    let mode = if active_needed { "active" } else { "passive" };
                    info!("zone {} switched ACPI cooling policy to {mode}", zone.zone.name);
                }
                Ok(false) => {
                    if active_needed {
                        warn!(
                            "zone {} needs active cooling, but no writable _SCP policy surface is available",
                            zone.zone.name,
                        );
                    }
                }
                Err(error) => warn!(
                    "zone {}: failed to update ACPI cooling policy: {}",
                    zone.zone.name,
                    error,
                ),
            }
            zone.active_cooling = active_needed;
        }
    }

    if passive_needed != previous_state.passive_governor_engaged {
        let target_governor = if passive_needed { "powersave" } else { "ondemand" };
        match set_cpufreq_governor(target_governor) {
            Ok(true) => info!("requested cpufreq governor {target_governor}"),
            Ok(false) => warn!(
                "cpufreq control surface is unavailable; passive cooling could not set governor {target_governor}"
            ),
            Err(error) => warn!("failed to set cpufreq governor {target_governor}: {error}"),
        }
    }

    match shared.as_ref().write() {
        Ok(mut state) => {
            state.zones = zones;
            state.passive_governor_engaged = passive_needed;
        }
        Err(error) => {
            warn!("state lock poisoned while writing thermal state: {error}");
        }
    }
}

fn monitor_loop(shared: Arc<RwLock<ThermalState>>) -> ! {
    let mut warned_missing_surface = false;

    loop {
        if !Path::new(ACPI_THERMAL_ROOT).exists() {
            if !warned_missing_surface {
                warn!(
                    "{} is unavailable; thermald will keep polling and serve an empty thermal surface",
                    ACPI_THERMAL_ROOT,
                );
                warned_missing_surface = true;
            }
        } else {
            warned_missing_surface = false;
        }

        update_policy(&shared);
        thread::sleep(THERMAL_POLL_INTERVAL);
    }
}

#[cfg(target_os = "redox")]
const SCHEME_ROOT_ID: usize = 1;

#[cfg(target_os = "redox")]
#[derive(Clone, Debug)]
enum HandleKind {
    Root,
    ZonesDir,
    ZoneDir(String),
    Summary,
    Temperature(String),
    PassiveThreshold(String),
    CriticalThreshold(String),
    Tc1(String),
    Tc2(String),
    Status(String),
}

#[cfg(target_os = "redox")]
struct ThermalScheme {
    shared: Arc<RwLock<ThermalState>>,
    next_id: usize,
    handles: BTreeMap<usize, HandleKind>,
}

#[cfg(target_os = "redox")]
impl ThermalScheme {
    fn new(shared: Arc<RwLock<ThermalState>>) -> Self {
        Self {
            shared,
            next_id: SCHEME_ROOT_ID + 1,
            handles: BTreeMap::new(),
        }
    }

    fn alloc_handle(&mut self, kind: HandleKind) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.handles.insert(id, kind);
        id
    }

    fn handle(&self, id: usize) -> SysResult<&HandleKind> {
        self.handles.get(&id).ok_or(SysError::new(EBADF))
    }

    fn zones(&self) -> Vec<ZoneRuntime> {
        match self.shared.read() {
            Ok(state) => state.zones.clone(),
            Err(_) => Vec::new(),
        }
    }

    fn zone(&self, name: &str) -> Option<ZoneRuntime> {
        self.zones().into_iter().find(|zone| zone.zone.name == name)
    }

    fn read_file(&self, kind: &HandleKind) -> Option<String> {
        match kind {
            HandleKind::Summary => {
                let zones = self.zones();
                let mut out = String::new();
                for zone in zones {
                    out.push_str(&zone.summary());
                    out.push('\n');
                }
                Some(out)
            }
            HandleKind::Temperature(name) => self
                .zone(name)
                .map(|zone| format!("{:.1}\n", zone.zone.temperature)),
            HandleKind::PassiveThreshold(name) => self
                .zone(name)
                .map(|zone| format!("{}\n", format_option(zone.zone.passive_threshold))),
            HandleKind::CriticalThreshold(name) => self
                .zone(name)
                .map(|zone| format!("{}\n", format_option(zone.zone.critical_threshold))),
            HandleKind::Tc1(name) => self.zone(name).map(|zone| format!("{}\n", format_option(zone.zone.tc1))),
            HandleKind::Tc2(name) => self.zone(name).map(|zone| format!("{}\n", format_option(zone.zone.tc2))),
            HandleKind::Status(name) => self.zone(name).map(|zone| format!("{}\n", zone.status_line())),
            _ => None,
        }
    }

    fn is_dir(kind: &HandleKind) -> bool {
        matches!(kind, HandleKind::Root | HandleKind::ZonesDir | HandleKind::ZoneDir(_))
    }

    fn resolve_zone_component(name: &str, tail: &[&str]) -> SysResult<HandleKind> {
        match tail {
            [] => Ok(HandleKind::ZoneDir(name.to_string())),
            ["temperature"] => Ok(HandleKind::Temperature(name.to_string())),
            ["passive-threshold"] => Ok(HandleKind::PassiveThreshold(name.to_string())),
            ["critical-threshold"] => Ok(HandleKind::CriticalThreshold(name.to_string())),
            ["tc1"] => Ok(HandleKind::Tc1(name.to_string())),
            ["tc2"] => Ok(HandleKind::Tc2(name.to_string())),
            ["status"] => Ok(HandleKind::Status(name.to_string())),
            _ => Err(SysError::new(ENOENT)),
        }
    }

    fn resolve_from_root(&self, path: &str) -> SysResult<HandleKind> {
        let trimmed = path.trim_matches('/');
        if trimmed.is_empty() {
            return Ok(HandleKind::Root);
        }

        let parts: Vec<&str> = trimmed.split('/').filter(|part| !part.is_empty()).collect();
        match parts.as_slice() {
            ["zones"] => Ok(HandleKind::ZonesDir),
            ["summary"] => Ok(HandleKind::Summary),
            ["zones", zone_name, tail @ ..] => {
                if self.zone(zone_name).is_none() {
                    return Err(SysError::new(ENOENT));
                }

                Self::resolve_zone_component(zone_name, tail)
            }
            _ => Err(SysError::new(ENOENT)),
        }
    }

    fn resolve_from_handle(&self, handle: &HandleKind, path: &str) -> SysResult<HandleKind> {
        let trimmed = path.trim_matches('/');
        match handle {
            HandleKind::Root => self.resolve_from_root(trimmed),
            HandleKind::ZonesDir => {
                if trimmed.is_empty() {
                    Ok(HandleKind::ZonesDir)
                } else if self.zone(trimmed).is_some() {
                    Ok(HandleKind::ZoneDir(trimmed.to_string()))
                } else {
                    Err(SysError::new(ENOENT))
                }
            }
            HandleKind::ZoneDir(name) => {
                if self.zone(name).is_none() {
                    return Err(SysError::new(ENOENT));
                }

                if trimmed.is_empty() {
                    Ok(HandleKind::ZoneDir(name.clone()))
                } else {
                    let tail: Vec<&str> = trimmed.split('/').filter(|part| !part.is_empty()).collect();
                    Self::resolve_zone_component(name, &tail)
                }
            }
            _ => Err(SysError::new(EINVAL)),
        }
    }
}

#[cfg(target_os = "redox")]
impl SchemeSync for ThermalScheme {
    fn scheme_root(&mut self) -> SysResult<usize> {
        Ok(SCHEME_ROOT_ID)
    }

    fn openat(
        &mut self,
        dirfd: usize,
        path: &str,
        _flags: usize,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> SysResult<OpenResult> {
        let kind = if dirfd == SCHEME_ROOT_ID {
            self.resolve_from_root(path)?
        } else {
            let parent = self.handle(dirfd)?.clone();
            self.resolve_from_handle(&parent, path)?
        };

        Ok(OpenResult::ThisScheme {
            number: self.alloc_handle(kind),
            flags: NewFdFlags::POSITIONED,
        })
    }

    fn fstat(&mut self, id: usize, stat: &mut Stat, _ctx: &CallerCtx) -> SysResult<()> {
        let kind = if id == SCHEME_ROOT_ID {
            HandleKind::Root
        } else {
            self.handle(id)?.clone()
        };
        stat.st_mode = if Self::is_dir(&kind) { MODE_DIR } else { MODE_FILE };
        stat.st_size = match self.read_file(&kind) {
            Some(content) => match u64::try_from(content.len()) {
                Ok(size) => size,
                Err(_) => u64::MAX,
            },
            None => 0,
        };
        Ok(())
    }

    fn read(
        &mut self,
        id: usize,
        buf: &mut [u8],
        offset: u64,
        _flags: u32,
        _ctx: &CallerCtx,
    ) -> SysResult<usize> {
        let kind = self.handle(id)?.clone();
        if Self::is_dir(&kind) {
            return Err(SysError::new(EINVAL));
        }

        let Some(content) = self.read_file(&kind) else {
            return Err(SysError::new(ENOENT));
        };

        let bytes = content.as_bytes();
        let Ok(offset) = usize::try_from(offset) else {
            return Err(SysError::new(EINVAL));
        };

        if offset >= bytes.len() {
            return Ok(0);
        }

        let count = (bytes.len() - offset).min(buf.len());
        buf[..count].copy_from_slice(&bytes[offset..offset + count]);
        Ok(count)
    }

    fn on_close(&mut self, id: usize) {
        self.handles.remove(&id);
    }
}

#[cfg(target_os = "redox")]
fn run_scheme(shared: Arc<RwLock<ThermalState>>) {
    let socket = match Socket::create() {
        Ok(socket) => socket,
        Err(error) => {
            error!("failed to create scheme:thermal socket: {error}");
            return;
        }
    };

    let mut scheme = ThermalScheme::new(shared);
    let mut state = SchemeState::new();

    match libredox::call::setrens(0, 0) {
        Ok(_) => info!("/scheme/thermal ready"),
        Err(error) => {
            error!("failed to enter null namespace for scheme:thermal: {error}");
            return;
        }
    }

    loop {
        let request = match socket.next_request(SignalBehavior::Restart) {
            Ok(Some(request)) => request,
            Ok(None) => {
                warn!("scheme:thermal socket closed; stopping thermal scheme server");
                break;
            }
            Err(error) => {
                error!("failed to read scheme:thermal request: {error}");
                break;
            }
        };

        if let redox_scheme::RequestKind::Call(request) = request.kind() {
            let response = request.handle_sync(&mut scheme, &mut state);
            if let Err(error) = socket.write_response(response, SignalBehavior::Restart) {
                error!("failed to write scheme:thermal response: {error}");
                break;
            }
        }
    }
}

#[cfg(not(target_os = "redox"))]
fn run_scheme(_shared: Arc<RwLock<ThermalState>>) {
    info!("host build: scheme:thermal serving is disabled outside Redox");
}

fn main() {
    let level = match std::env::var("THERMALD_LOG").as_deref() {
        Ok("debug") => LevelFilter::Debug,
        Ok("trace") => LevelFilter::Trace,
        Ok("warn") => LevelFilter::Warn,
        Ok("error") => LevelFilter::Error,
        _ => LevelFilter::Info,
    };
    init_logging(level);

    info!("thermal management daemon starting");

    let shared = Arc::new(RwLock::new(ThermalState::default()));
    update_policy(&shared);

    let initial_zone_count = match shared.as_ref().read() {
        Ok(state) => state.zones.len(),
        Err(_) => 0,
    };
    info!("{} thermal zone(s) found", initial_zone_count);

    let scheme_shared = Arc::clone(&shared);
    let _scheme_thread = thread::spawn(move || run_scheme(scheme_shared));

    monitor_loop(shared);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_temperature() {
        // 0xBB8 = 3000 (in tenths of Kelvin) = 26.85°C
        let val: u32 = 0xBB8;
        let celsius = (val as f64 - 2731.5) / 10.0;
        assert!((celsius - 26.85).abs() < 0.1);
    }

    #[test]
    fn parse_decimal_temperature() {
        let val: u32 = 3000; // 300.0K = 26.85°C
        let celsius = (val as f64 - 2731.5) / 10.0;
        assert!((celsius - 26.85).abs() < 0.1);
    }

    #[test]
    fn detect_critical_exceeds_threshold() {
        let zone = ThermalZone {
            name: "TZ00".into(),
            temperature: 100.0,
            passive_threshold: Some(80.0),
            critical_threshold: Some(95.0),
            tc1: None,
            tc2: None,
        };
        assert!(zone.temperature >= zone.critical_threshold.unwrap());
    }

    #[test]
    fn no_critical_when_below_threshold() {
        let zone = ThermalZone {
            name: "TZ00".into(),
            temperature: 50.0,
            passive_threshold: Some(80.0),
            critical_threshold: Some(95.0),
            tc1: None,
            tc2: None,
        };
        assert!(zone.temperature < zone.critical_threshold.unwrap());
    }
}
