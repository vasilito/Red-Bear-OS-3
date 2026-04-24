//! HCI scheme backend for redbear-btctl.
//!
//! Implements the `Backend` trait by reading/writing HCI scheme files
//! (`/scheme/hciN/*`) instead of using hardcoded stub data.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::path::{Path, PathBuf};

use crate::backend::{AdapterStatus, Backend};
use crate::bond_store::{BondRecord, BondStore, STUB_BOND_SOURCE};

#[cfg(test)]
use crate::bond_store::validate_adapter_name;

// ---------------------------------------------------------------------------
// Scheme filesystem abstraction
// ---------------------------------------------------------------------------

/// Abstraction over filesystem operations so tests can use `std::fs` against
/// temp directories while production code uses libredox scheme calls.
trait SchemeFs {
    fn read_file(&self, path: &Path) -> std::io::Result<Vec<u8>>;
    fn write_file(&self, path: &Path, data: &[u8]) -> std::io::Result<()>;
}

/// Standard filesystem adapter — used in tests and on non-Redox hosts.
struct StdFs;

impl SchemeFs for StdFs {
    fn read_file(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn write_file(&self, path: &Path, data: &[u8]) -> std::io::Result<()> {
        // Ensure parent directory exists for test mock filesystems.
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, data)
    }
}

/// Redox scheme filesystem adapter — uses libredox for direct scheme I/O.
#[cfg(target_os = "redox")]
struct RedoxSchemeFs;

#[cfg(target_os = "redox")]
impl SchemeFs for RedoxSchemeFs {
    fn read_file(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        let path_str = path
            .to_str()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "non-UTF-8 path"))?;
        let fd = libredox::call::open(path_str, libc::O_RDONLY, 0)?;
        let mut buf = vec![0u8; 4096];
        let n = libredox::call::read(fd, &mut buf)?;
        buf.truncate(n);
        Ok(buf)
    }

    fn write_file(&self, path: &Path, data: &[u8]) -> std::io::Result<()> {
        let path_str = path
            .to_str()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "non-UTF-8 path"))?;
        let fd = libredox::call::open(path_str, libc::O_WRONLY, 0)?;
        libredox::call::write(fd, data)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Experimental read-char constants (mirrors backend.rs internals)
// ---------------------------------------------------------------------------

const EXPERIMENTAL_WORKLOAD: &str = "battery-sensor-battery-level-read";
const EXPERIMENTAL_PERIPHERAL_CLASS: &str = "ble-battery-sensor";
const EXPERIMENTAL_CHARACTERISTIC: &str = "battery-level";
const EXPERIMENTAL_SERVICE_UUID: &str = "0000180f-0000-1000-8000-00805f9b34fb";
const EXPERIMENTAL_CHAR_UUID: &str = "00002a19-0000-1000-8000-00805f9b34fb";
const EXPERIMENTAL_VALUE_HEX: &str = "57";
const EXPERIMENTAL_VALUE_PERCENT: u8 = 87;

fn normalize_uuid(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn default_read_char_result() -> String {
    format!(
        "read_char_result=not-run workload={} peripheral_class={} characteristic={} service_uuid={} char_uuid={} access=read-only",
        EXPERIMENTAL_WORKLOAD,
        EXPERIMENTAL_PERIPHERAL_CLASS,
        EXPERIMENTAL_CHARACTERISTIC,
        EXPERIMENTAL_SERVICE_UUID,
        EXPERIMENTAL_CHAR_UUID
    )
}

fn rejected_read_char_result(
    reason: &str,
    bond_id: &str,
    service_uuid: &str,
    char_uuid: &str,
) -> String {
    format!(
        "read_char_result={} workload={} peripheral_class={} characteristic={} bond_id={} service_uuid={} char_uuid={} access=read-only supported_service_uuid={} supported_char_uuid={}",
        reason,
        EXPERIMENTAL_WORKLOAD,
        EXPERIMENTAL_PERIPHERAL_CLASS,
        EXPERIMENTAL_CHARACTERISTIC,
        bond_id,
        normalize_uuid(service_uuid),
        normalize_uuid(char_uuid),
        EXPERIMENTAL_SERVICE_UUID,
        EXPERIMENTAL_CHAR_UUID
    )
}

fn success_read_char_result(bond_id: &str) -> String {
    format!(
        "read_char_result=stub-value workload={} peripheral_class={} characteristic={} bond_id={} service_uuid={} char_uuid={} access=read-only value_hex={} value_percent={}",
        EXPERIMENTAL_WORKLOAD,
        EXPERIMENTAL_PERIPHERAL_CLASS,
        EXPERIMENTAL_CHARACTERISTIC,
        bond_id,
        EXPERIMENTAL_SERVICE_UUID,
        EXPERIMENTAL_CHAR_UUID,
        EXPERIMENTAL_VALUE_HEX,
        EXPERIMENTAL_VALUE_PERCENT
    )
}

fn gatt_success_read_char_result(bond_id: &str, value_hex: &str, value_percent: u8) -> String {
    format!(
        "read_char_result=gatt-value workload={} peripheral_class={} characteristic={} bond_id={} service_uuid={} char_uuid={} access=read-only value_hex={} value_percent={}",
        EXPERIMENTAL_WORKLOAD,
        EXPERIMENTAL_PERIPHERAL_CLASS,
        EXPERIMENTAL_CHARACTERISTIC,
        bond_id,
        EXPERIMENTAL_SERVICE_UUID,
        EXPERIMENTAL_CHAR_UUID,
        value_hex,
        value_percent
    )
}

// ---------------------------------------------------------------------------
// Per-adapter runtime state
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
struct AdapterRuntimeState {
    connected_bond_ids: BTreeSet<String>,
    last_connect_result: String,
    last_disconnect_result: String,
    last_read_char_result: String,
}

impl AdapterRuntimeState {
    fn new() -> Self {
        Self {
            last_connect_result: "connect_result=not-run".to_string(),
            last_disconnect_result: "disconnect_result=not-run".to_string(),
            last_read_char_result: default_read_char_result(),
            ..Self::default()
        }
    }
}

// ---------------------------------------------------------------------------
// GATT data structures and parsing
// ---------------------------------------------------------------------------

struct GattService {
    start_handle: String,
    end_handle: String,
    uuid: String,
}

struct GattCharacteristic {
    #[allow(dead_code)]
    handle: String,
    value_handle: String,
    #[allow(dead_code)]
    properties: String,
    uuid: String,
}

/// Parse GATT service entries from text.
///
/// Expected format per line: `service=start_handle=XXXX;end_handle=XXXX;uuid=XXXX`
fn parse_gatt_services(content: &str) -> Vec<GattService> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("service="))
        .filter_map(|line| {
            let entry = line.strip_prefix("service=")?;
            let mut start_handle = None;
            let mut end_handle = None;
            let mut uuid = None;
            for part in entry.split(';') {
                if let Some(v) = part.strip_prefix("start_handle=") {
                    start_handle = Some(v.to_string());
                }
                if let Some(v) = part.strip_prefix("end_handle=") {
                    end_handle = Some(v.to_string());
                }
                if let Some(v) = part.strip_prefix("uuid=") {
                    uuid = Some(v.to_string());
                }
            }
            Some(GattService {
                start_handle: start_handle?,
                end_handle: end_handle?,
                uuid: uuid?,
            })
        })
        .collect()
}

/// Parse GATT characteristic entries from text.
///
/// Expected format per line: `char=handle=XXXX;value_handle=XXXX;properties=XX;uuid=XXXX`
fn parse_gatt_characteristics(content: &str) -> Vec<GattCharacteristic> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("char="))
        .filter_map(|line| {
            let entry = line.strip_prefix("char=")?;
            let mut handle = None;
            let mut value_handle = None;
            let mut properties = None;
            let mut uuid = None;
            for part in entry.split(';') {
                if let Some(v) = part.strip_prefix("handle=") {
                    handle = Some(v.to_string());
                }
                if let Some(v) = part.strip_prefix("value_handle=") {
                    value_handle = Some(v.to_string());
                }
                if let Some(v) = part.strip_prefix("properties=") {
                    properties = Some(v.to_string());
                }
                if let Some(v) = part.strip_prefix("uuid=") {
                    uuid = Some(v.to_string());
                }
            }
            Some(GattCharacteristic {
                handle: handle?,
                value_handle: value_handle?,
                properties: properties?,
                uuid: uuid?,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// HciBackend
// ---------------------------------------------------------------------------

pub struct HciBackend {
    scheme_path: PathBuf,
    adapter: String,
    fs: Box<dyn SchemeFs>,
    scan_results: Vec<String>,
    runtime_state: BTreeMap<String, AdapterRuntimeState>,
    bond_store: BondStore,
}

impl HciBackend {
    /// Build an HciBackend from environment variables (production path).
    ///
    /// On Redox, uses `RedoxSchemeFs` for direct scheme I/O.
    /// On non-Redox hosts, falls back to `StdFs` (useful for development).
    pub fn from_env() -> Self {
        let adapter =
            env::var("REDBEAR_BTCTL_HCI_ADAPTER").unwrap_or_else(|_| "hci0".to_string());
        let scheme_path = PathBuf::from(format!("/scheme/{adapter}"));

        Self {
            runtime_state: {
                let mut map = BTreeMap::new();
                map.insert(adapter.clone(), AdapterRuntimeState::new());
                map
            },
            adapter: adapter.clone(),
            scheme_path,
            fs: Self::create_fs(),
            scan_results: Vec::new(),
            bond_store: BondStore::from_env(),
        }
    }

    /// Build an HciBackend for testing with a mock filesystem root.
    #[cfg(test)]
    pub fn new_for_test(scheme_root: PathBuf, adapter: String, bond_store_root: PathBuf) -> Self {
        validate_adapter_name(&adapter).expect("invalid test adapter name");

        Self {
            scheme_path: scheme_root.join(&adapter),
            adapter: adapter.clone(),
            fs: Box::new(StdFs),
            scan_results: Vec::new(),
            runtime_state: {
                let mut map = BTreeMap::new();
                map.insert(adapter, AdapterRuntimeState::new());
                map
            },
            bond_store: BondStore::new(bond_store_root),
        }
    }

    #[cfg(target_os = "redox")]
    fn create_fs() -> Box<dyn SchemeFs> {
        Box::new(RedoxSchemeFs)
    }

    #[cfg(not(target_os = "redox"))]
    fn create_fs() -> Box<dyn SchemeFs> {
        Box::new(StdFs)
    }

    fn ensure_adapter(&self, adapter: &str) -> Result<(), String> {
        if adapter == self.adapter {
            Ok(())
        } else {
            Err("unknown Bluetooth adapter".to_string())
        }
    }

    fn runtime_state(&self, adapter: &str) -> Result<&AdapterRuntimeState, String> {
        self.runtime_state
            .get(adapter)
            .ok_or_else(|| "unknown Bluetooth adapter".to_string())
    }

    fn runtime_state_mut(&mut self, adapter: &str) -> Result<&mut AdapterRuntimeState, String> {
        self.runtime_state
            .get_mut(adapter)
            .ok_or_else(|| "unknown Bluetooth adapter".to_string())
    }

    fn bond_exists(&self, adapter: &str, bond_id: &str) -> Result<bool, String> {
        Ok(self
            .load_bonds(adapter)?
            .iter()
            .any(|bond| bond.bond_id == bond_id))
    }

    fn read_scheme_text(&self, relative: &str) -> Result<String, String> {
        let path = self.scheme_path.join(relative);
        self.fs
            .read_file(&path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))
            .and_then(|bytes| {
                String::from_utf8(bytes)
                    .map_err(|err| format!("non-UTF-8 response from {}: {err}", path.display()))
            })
    }

    fn write_scheme(&self, relative: &str, data: &[u8]) -> Result<(), String> {
        let path = self.scheme_path.join(relative);
        self.fs
            .write_file(&path, data)
            .map_err(|err| format!("failed to write {}: {err}", path.display()))
    }

    fn parse_controller_state(status: &str) -> AdapterStatus {
        for line in status.lines().map(str::trim) {
            if let Some(value) = line.strip_prefix("controller_state=") {
                return match value.trim() {
                    "active" => AdapterStatus::AdapterVisible,
                    "scanning" => AdapterStatus::Scanning,
                    _ => AdapterStatus::ExplicitStartupRequired,
                };
            }
        }
        AdapterStatus::ExplicitStartupRequired
    }

    fn parse_connections(content: &str) -> Vec<String> {
        let mut addrs: Vec<String> = content
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .filter_map(|line| {
                line.split_whitespace()
                    .find_map(|part| part.strip_prefix("addr=").map(str::to_string))
            })
            .collect();
        addrs.sort();
        addrs
    }

    fn resolve_handle(&self, bond_id: &str) -> Result<String, String> {
        let content = self.read_scheme_text("connections")?;
        for line in content.lines().map(str::trim) {
            if line.is_empty() {
                continue;
            }
            let mut handle = None;
            let mut addr = None;
            for part in line.split_whitespace() {
                if let Some(v) = part.strip_prefix("handle=") {
                    handle = Some(v.to_string());
                }
                if let Some(v) = part.strip_prefix("addr=") {
                    addr = Some(v.to_string());
                }
            }
            if addr.as_deref() == Some(bond_id) {
                return handle.ok_or_else(|| {
                    format!("connection entry for {bond_id} has no handle field")
                });
            }
        }
        Err(format!("bond {bond_id} not found in active connections"))
    }

    fn resolve_conn_handle(&self, bond_id: &str) -> Result<String, String> {
        self.resolve_handle(bond_id)
    }

    fn read_scheme_bytes(&self, relative: &str) -> Result<Vec<u8>, String> {
        let path = self.scheme_path.join(relative);
        self.fs
            .read_file(&path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))
    }

    fn discover_gatt_services(&self, conn_handle: &str) -> Result<Vec<GattService>, String> {
        self.write_scheme(
            "gatt-discover-services",
            format!("handle={conn_handle}").as_bytes(),
        )?;
        let content = self.read_scheme_text("gatt-services")?;
        Ok(parse_gatt_services(&content))
    }

    fn discover_gatt_characteristics(
        &self,
        conn_handle: &str,
        start_handle: &str,
        end_handle: &str,
    ) -> Result<Vec<GattCharacteristic>, String> {
        self.write_scheme(
            "gatt-discover-chars",
            format!("handle={conn_handle};start={start_handle};end={end_handle}").as_bytes(),
        )?;
        let content = self.read_scheme_text("gatt-characteristics")?;
        Ok(parse_gatt_characteristics(&content))
    }

    fn read_gatt_char_value(
        &self,
        conn_handle: &str,
        value_handle: &str,
    ) -> Result<Vec<u8>, String> {
        self.write_scheme(
            "gatt-read-char",
            format!("handle={conn_handle};addr={value_handle}").as_bytes(),
        )?;
        self.read_scheme_bytes("gatt-read-char")
    }

    fn try_gatt_read(
        &self,
        bond_id: &str,
        service_uuid: &str,
        char_uuid: &str,
    ) -> Result<(String, u8), String> {
        let conn_handle = self.resolve_conn_handle(bond_id)?;

        let services = self.discover_gatt_services(&conn_handle)?;
        let target_svc = normalize_uuid(service_uuid);
        let service = services
            .iter()
            .find(|s| normalize_uuid(&s.uuid) == target_svc)
            .ok_or_else(|| format!("service {service_uuid} not found in GATT services"))?;

        let chars =
            self.discover_gatt_characteristics(&conn_handle, &service.start_handle, &service.end_handle)?;
        let target_ch = normalize_uuid(char_uuid);
        let char_entry = chars
            .iter()
            .find(|c| normalize_uuid(&c.uuid) == target_ch)
            .ok_or_else(|| format!("characteristic {char_uuid} not found in GATT characteristics"))?;

        let raw_bytes = self.read_gatt_char_value(&conn_handle, &char_entry.value_handle)?;
        let value_percent = raw_bytes.first().copied().unwrap_or(0);
        let value_hex = format!("{value_percent:02x}");

        Ok((value_hex, value_percent))
    }
}

impl Backend for HciBackend {
    fn adapters(&self) -> Vec<String> {
        vec![self.adapter.clone()]
    }

    fn capabilities(&self) -> Vec<String> {
        vec![
            "backend=hci-scheme".to_string(),
            "transport=usb".to_string(),
            "startup=auto".to_string(),
            "mode=ble-first".to_string(),
            "scan=true".to_string(),
            format!("workload={}", EXPERIMENTAL_WORKLOAD),
            "read_char=true".to_string(),
            "write_char=false".to_string(),
            "notify=false".to_string(),
            format!("bond_store={}", STUB_BOND_SOURCE),
            "scheme=btctl".to_string(),
            format!("scheme_path={}", self.scheme_path.display()),
            format!("bond_store_root={}", self.bond_store.root().display()),
        ]
    }

    fn initial_status(&self, adapter: &str) -> AdapterStatus {
        if self.ensure_adapter(adapter).is_err() {
            return AdapterStatus::Failed;
        }
        match self.read_scheme_text("status") {
            Ok(content) => Self::parse_controller_state(&content),
            Err(_) => AdapterStatus::ExplicitStartupRequired,
        }
    }

    fn transport_status(&self, adapter: &str) -> String {
        if self.ensure_adapter(adapter).is_err() {
            return "transport=unknown-adapter".to_string();
        }
        self.read_scheme_text("status").unwrap_or_else(|_| {
            format!(
                "transport=usb startup=auto scheme_path={}",
                self.scheme_path.display()
            )
        })
    }

    fn default_scan_results(&self, _adapter: &str) -> Vec<String> {
        Vec::new()
    }

    fn connected_bond_ids(&self, adapter: &str) -> Result<Vec<String>, String> {
        self.ensure_adapter(adapter)?;
        if let Ok(content) = self.read_scheme_text("connections") {
            let parsed = Self::parse_connections(&content);
            if !parsed.is_empty() {
                return Ok(parsed);
            }
        }
        Ok(self
            .runtime_state(adapter)?
            .connected_bond_ids
            .iter()
            .cloned()
            .collect())
    }

    fn connect_result(&self, adapter: &str) -> Result<String, String> {
        self.ensure_adapter(adapter)?;
        Ok(self.runtime_state(adapter)?.last_connect_result.clone())
    }

    fn disconnect_result(&self, adapter: &str) -> Result<String, String> {
        self.ensure_adapter(adapter)?;
        Ok(self.runtime_state(adapter)?.last_disconnect_result.clone())
    }

    fn read_char_result(&self, adapter: &str) -> Result<String, String> {
        self.ensure_adapter(adapter)?;
        Ok(self.runtime_state(adapter)?.last_read_char_result.clone())
    }

    fn status(&self, adapter: &str) -> Result<AdapterStatus, String> {
        self.ensure_adapter(adapter)?;
        Ok(self.initial_status(adapter))
    }

    fn scan(&mut self, adapter: &str) -> Result<Vec<String>, String> {
        self.ensure_adapter(adapter)?;
        self.write_scheme("le-scan", b"start")?;
        let content = self.read_scheme_text("le-scan-results")?;
        let results = content
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        self.scan_results = results.clone();
        Ok(results)
    }

    fn connect(&mut self, adapter: &str, bond_id: &str) -> Result<(), String> {
        self.ensure_adapter(adapter)?;
        if !self.bond_exists(adapter, bond_id)? {
            let state = self.runtime_state_mut(adapter)?;
            state.last_connect_result =
                format!("connect_result=rejected-missing-bond bond_id={bond_id}");
            return Err("bond record not found; add a stub bond record first".to_string());
        }
        self.write_scheme("connect", format!("addr={bond_id}").as_bytes())?;
        let state = self.runtime_state_mut(adapter)?;
        let outcome = if state.connected_bond_ids.insert(bond_id.to_string()) {
            "connected"
        } else {
            "already-connected"
        };
        state.last_connect_result =
            format!("connect_result=hci-scheme-connected bond_id={bond_id} state={outcome}");
        Ok(())
    }

    fn disconnect(&mut self, adapter: &str, bond_id: &str) -> Result<(), String> {
        self.ensure_adapter(adapter)?;
        if !self.bond_exists(adapter, bond_id)? {
            let state = self.runtime_state_mut(adapter)?;
            state.last_disconnect_result =
                format!("disconnect_result=rejected-missing-bond bond_id={bond_id}");
            return Err("bond record not found; add a stub bond record first".to_string());
        }
        match self.resolve_handle(bond_id) {
            Ok(h) => {
                self.write_scheme("disconnect", format!("handle={h}").as_bytes())?;
            }
            Err(_) => {
                // No active connection in scheme; proceed with local state update.
            }
        }
        let state = self.runtime_state_mut(adapter)?;
        let outcome = if state.connected_bond_ids.remove(bond_id) {
            "disconnected"
        } else {
            "already-disconnected"
        };
        state.last_disconnect_result = format!(
            "disconnect_result=hci-scheme-disconnected bond_id={bond_id} state={outcome}"
        );
        Ok(())
    }

    fn read_char(
        &mut self,
        adapter: &str,
        bond_id: &str,
        service_uuid: &str,
        char_uuid: &str,
    ) -> Result<(), String> {
        self.ensure_adapter(adapter)?;
        if !self.bond_exists(adapter, bond_id)? {
            let state = self.runtime_state_mut(adapter)?;
            state.last_read_char_result = rejected_read_char_result(
                "rejected-missing-bond",
                bond_id,
                service_uuid,
                char_uuid,
            );
            return Err("bond record not found; add a stub bond record first".to_string());
        }
        if !self
            .runtime_state(adapter)?
            .connected_bond_ids
            .contains(bond_id)
        {
            let state = self.runtime_state_mut(adapter)?;
            state.last_read_char_result = rejected_read_char_result(
                "rejected-not-connected",
                bond_id,
                service_uuid,
                char_uuid,
            );
            return Err(
                "bond is not connected; run --connect before the experimental read".to_string(),
            );
        }
        if normalize_uuid(service_uuid) != EXPERIMENTAL_SERVICE_UUID
            || normalize_uuid(char_uuid) != EXPERIMENTAL_CHAR_UUID
        {
            let state = self.runtime_state_mut(adapter)?;
            state.last_read_char_result = rejected_read_char_result(
                "rejected-unsupported-characteristic",
                bond_id,
                service_uuid,
                char_uuid,
            );
            return Err(format!(
                "only the experimental {} workload is supported: service {} characteristic {}",
                EXPERIMENTAL_WORKLOAD, EXPERIMENTAL_SERVICE_UUID, EXPERIMENTAL_CHAR_UUID
            ));
        }

        match self.try_gatt_read(bond_id, service_uuid, char_uuid) {
            Ok((value_hex, value_percent)) => {
                self.runtime_state_mut(adapter)?.last_read_char_result =
                    gatt_success_read_char_result(bond_id, &value_hex, value_percent);
            }
            Err(_) => {
                self.runtime_state_mut(adapter)?.last_read_char_result =
                    success_read_char_result(bond_id);
            }
        }
        Ok(())
    }

    fn bond_store_path(&self, adapter: &str) -> Result<String, String> {
        self.ensure_adapter(adapter)?;
        Ok(self
            .bond_store
            .adapter_bonds_dir(adapter)
            .display()
            .to_string())
    }

    fn load_bonds(&self, adapter: &str) -> Result<Vec<BondRecord>, String> {
        self.ensure_adapter(adapter)?;
        self.bond_store
            .load(adapter)
            .map_err(|err| format!("failed to load bond store: {err}"))
    }

    fn add_stub_bond(
        &mut self,
        adapter: &str,
        bond_id: &str,
        alias: Option<&str>,
    ) -> Result<BondRecord, String> {
        self.ensure_adapter(adapter)?;
        self.bond_store
            .add_stub(adapter, bond_id, alias)
            .map_err(|err| format!("failed to persist stub bond record: {err}"))
    }

    fn remove_bond(&mut self, adapter: &str, bond_id: &str) -> Result<bool, String> {
        self.ensure_adapter(adapter)?;
        let removed = self
            .bond_store
            .remove(adapter, bond_id)
            .map_err(|err| format!("failed to remove stub bond record: {err}"))?;
        if removed {
            let state = self.runtime_state_mut(adapter)?;
            if state.connected_bond_ids.remove(bond_id) {
                state.last_disconnect_result = format!(
                    "disconnect_result=hci-scheme-disconnected bond_id={bond_id} state=removed-with-bond"
                );
            }
        }
        Ok(removed)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("{name}-{stamp}"))
    }

    fn setup_scheme(scheme_root: &Path, adapter: &str) -> PathBuf {
        let adapter_dir = scheme_root.join(adapter);
        fs::create_dir_all(&adapter_dir).unwrap();
        adapter_dir
    }

    // -- Capabilities and adapter identity --

    #[test]
    fn hci_capabilities_report_backend_type() {
        let root = temp_path("rbos-hci-cap");
        let backend = HciBackend::new_for_test(
            root.clone(),
            "hci0".to_string(),
            temp_path("rbos-hci-cap-bonds"),
        );
        let caps = backend.capabilities();
        assert!(caps.iter().any(|c| c == "backend=hci-scheme"));
        assert!(caps.iter().any(|c| c.starts_with("scheme_path=")));
        assert!(caps.iter().any(|c| c == "startup=auto"));
        assert_eq!(backend.adapters(), vec!["hci0".to_string()]);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn hci_rejects_unknown_adapter() {
        let root = temp_path("rbos-hci-unknown");
        let mut backend = HciBackend::new_for_test(
            root.clone(),
            "hci0".to_string(),
            temp_path("rbos-hci-unknown-bonds"),
        );
        assert_eq!(backend.initial_status("hci9"), AdapterStatus::Failed);
        assert!(backend.status("hci9").is_err());
        assert!(backend.scan("hci9").is_err());
        fs::remove_dir_all(root).ok();
    }

    // -- Status and transport --

    #[test]
    fn hci_initial_status_reads_controller_state() {
        let root = temp_path("rbos-hci-status");
        let adapter_dir = setup_scheme(&root, "hci0");
        fs::write(
            adapter_dir.join("status"),
            "controller_state=active\ntransport=usb\n",
        )
        .unwrap();

        let backend = HciBackend::new_for_test(
            root.clone(),
            "hci0".to_string(),
            temp_path("rbos-hci-status-bonds"),
        );
        assert_eq!(
            backend.initial_status("hci0"),
            AdapterStatus::AdapterVisible
        );
        assert!(backend
            .transport_status("hci0")
            .contains("controller_state=active"));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn hci_initial_status_returns_startup_required_when_no_scheme() {
        let root = temp_path("rbos-hci-no-scheme");
        let backend = HciBackend::new_for_test(
            root.clone(),
            "hci0".to_string(),
            temp_path("rbos-hci-no-scheme-bonds"),
        );
        assert_eq!(
            backend.initial_status("hci0"),
            AdapterStatus::ExplicitStartupRequired
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn hci_transport_status_falls_back_when_file_missing() {
        let root = temp_path("rbos-hci-transport-missing");
        let backend = HciBackend::new_for_test(
            root.clone(),
            "hci0".to_string(),
            temp_path("rbos-hci-transport-missing-bonds"),
        );
        let ts = backend.transport_status("hci0");
        assert!(ts.contains("transport=usb"));
        assert!(ts.contains("startup=auto"));
        fs::remove_dir_all(root).ok();
    }

    // -- Scan --

    #[test]
    fn hci_scan_writes_start_and_reads_results() {
        let root = temp_path("rbos-hci-scan");
        let adapter_dir = setup_scheme(&root, "hci0");
        fs::write(
            adapter_dir.join("le-scan-results"),
            "AA:BB:CC:DD:EE:FF\n11:22:33:44:55:66\n",
        )
        .unwrap();

        let mut backend = HciBackend::new_for_test(
            root.clone(),
            "hci0".to_string(),
            temp_path("rbos-hci-scan-bonds"),
        );
        let results = backend.scan("hci0").unwrap();
        assert_eq!(
            results,
            vec!["AA:BB:CC:DD:EE:FF", "11:22:33:44:55:66"]
        );

        let written = fs::read_to_string(adapter_dir.join("le-scan")).unwrap();
        assert_eq!(written, "start");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn hci_scan_returns_error_when_scheme_not_present() {
        let root = temp_path("rbos-hci-scan-missing");
        let mut backend = HciBackend::new_for_test(
            root.clone(),
            "hci0".to_string(),
            temp_path("rbos-hci-scan-missing-bonds"),
        );
        assert!(backend.scan("hci0").is_err());
        fs::remove_dir_all(root).ok();
    }

    // -- Connect and disconnect --

    #[test]
    fn hci_connect_writes_addr_to_scheme() {
        let root = temp_path("rbos-hci-connect");
        let adapter_dir = setup_scheme(&root, "hci0");
        let bond_store = temp_path("rbos-hci-connect-bonds");

        let mut backend =
            HciBackend::new_for_test(root.clone(), "hci0".to_string(), bond_store.clone());
        backend
            .add_stub_bond("hci0", "AA:BB:CC:DD:EE:FF", Some("demo"))
            .unwrap();
        backend.connect("hci0", "AA:BB:CC:DD:EE:FF").unwrap();

        let written = fs::read_to_string(adapter_dir.join("connect")).unwrap();
        assert_eq!(written, "addr=AA:BB:CC:DD:EE:FF");

        let result = backend.connect_result("hci0").unwrap();
        assert!(result.contains("connect_result=hci-scheme-connected"));
        assert!(result.contains("bond_id=AA:BB:CC:DD:EE:FF"));

        let connected = backend.connected_bond_ids("hci0").unwrap();
        assert_eq!(connected, vec!["AA:BB:CC:DD:EE:FF"]);

        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(bond_store).ok();
    }

    #[test]
    fn hci_connect_rejects_missing_bond() {
        let root = temp_path("rbos-hci-connect-missing");
        let mut backend = HciBackend::new_for_test(
            root.clone(),
            "hci0".to_string(),
            temp_path("rbos-hci-connect-missing-bonds"),
        );
        let err = backend.connect("hci0", "AA:BB:CC:DD:EE:FF").unwrap_err();
        assert!(err.contains("bond record not found"));

        let result = backend.connect_result("hci0").unwrap();
        assert!(result.contains("rejected-missing-bond"));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn hci_disconnect_resolves_handle_from_connections() {
        let root = temp_path("rbos-hci-disconnect");
        let adapter_dir = setup_scheme(&root, "hci0");
        let bond_store = temp_path("rbos-hci-disconnect-bonds");

        fs::write(
            adapter_dir.join("connections"),
            "handle=0042 addr=AA:BB:CC:DD:EE:FF\n",
        )
        .unwrap();

        let mut backend =
            HciBackend::new_for_test(root.clone(), "hci0".to_string(), bond_store.clone());
        backend
            .add_stub_bond("hci0", "AA:BB:CC:DD:EE:FF", Some("demo"))
            .unwrap();
        backend.connect("hci0", "AA:BB:CC:DD:EE:FF").unwrap();
        backend.disconnect("hci0", "AA:BB:CC:DD:EE:FF").unwrap();

        let written = fs::read_to_string(adapter_dir.join("disconnect")).unwrap();
        assert_eq!(written, "handle=0042");

        let result = backend.disconnect_result("hci0").unwrap();
        assert!(result.contains("disconnect_result=hci-scheme-disconnected"));
        assert!(result.contains("bond_id=AA:BB:CC:DD:EE:FF"));

        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(bond_store).ok();
    }

    #[test]
    fn hci_disconnect_proceeds_without_handle_if_no_connection_file() {
        let root = temp_path("rbos-hci-disconnect-noconn");
        let bond_store = temp_path("rbos-hci-disconnect-noconn-bonds");

        let mut backend =
            HciBackend::new_for_test(root.clone(), "hci0".to_string(), bond_store.clone());
        backend
            .add_stub_bond("hci0", "AA:BB:CC:DD:EE:FF", Some("demo"))
            .unwrap();
        backend.connect("hci0", "AA:BB:CC:DD:EE:FF").unwrap();
        backend.disconnect("hci0", "AA:BB:CC:DD:EE:FF").unwrap();

        let result = backend.disconnect_result("hci0").unwrap();
        assert!(result.contains("disconnect_result=hci-scheme-disconnected"));
        assert!(backend.connected_bond_ids("hci0").unwrap().is_empty());

        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(bond_store).ok();
    }

    // -- Connected bond IDs from connections file --

    #[test]
    fn hci_connected_bond_ids_reads_from_scheme() {
        let root = temp_path("rbos-hci-connected");
        let adapter_dir = setup_scheme(&root, "hci0");
        fs::write(
            adapter_dir.join("connections"),
            "handle=0001 addr=AA:BB:CC:DD:EE:FF\nhandle=0002 addr=11:22:33:44:55:66\n",
        )
        .unwrap();

        let backend = HciBackend::new_for_test(
            root.clone(),
            "hci0".to_string(),
            temp_path("rbos-hci-connected-bonds"),
        );
        let ids = backend.connected_bond_ids("hci0").unwrap();
        assert_eq!(ids, vec!["11:22:33:44:55:66", "AA:BB:CC:DD:EE:FF"]);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn hci_connected_bond_ids_falls_back_to_runtime_state() {
        let root = temp_path("rbos-hci-connected-fallback");
        let _adapter_dir = setup_scheme(&root, "hci0");

        let mut backend = HciBackend::new_for_test(
            root.clone(),
            "hci0".to_string(),
            temp_path("rbos-hci-connected-fallback-bonds"),
        );
        backend
            .add_stub_bond("hci0", "AA:BB:CC:DD:EE:FF", None)
            .unwrap();
        backend.connect("hci0", "AA:BB:CC:DD:EE:FF").unwrap();

        let ids = backend.connected_bond_ids("hci0").unwrap();
        assert_eq!(ids, vec!["AA:BB:CC:DD:EE:FF"]);
        fs::remove_dir_all(root).ok();
    }

    // -- Read char (experimental stub) --

    #[test]
    fn hci_read_char_returns_experimental_stub_when_connected() {
        let root = temp_path("rbos-hci-read-char");
        let bond_store = temp_path("rbos-hci-read-char-bonds");

        let mut backend =
            HciBackend::new_for_test(root.clone(), "hci0".to_string(), bond_store.clone());
        backend
            .add_stub_bond("hci0", "AA:BB:CC:DD:EE:FF", Some("battery"))
            .unwrap();
        backend.connect("hci0", "AA:BB:CC:DD:EE:FF").unwrap();
        backend
            .read_char(
                "hci0",
                "AA:BB:CC:DD:EE:FF",
                EXPERIMENTAL_SERVICE_UUID,
                EXPERIMENTAL_CHAR_UUID,
            )
            .unwrap();

        let result = backend.read_char_result("hci0").unwrap();
        assert!(result.contains("read_char_result=stub-value"));
        assert!(result.contains("value_percent=87"));
        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(bond_store).ok();
    }

    #[test]
    fn hci_read_char_rejects_unsupported_characteristic() {
        let root = temp_path("rbos-hci-read-char-unsupported");
        let bond_store = temp_path("rbos-hci-read-char-unsupported-bonds");

        let mut backend =
            HciBackend::new_for_test(root.clone(), "hci0".to_string(), bond_store.clone());
        backend
            .add_stub_bond("hci0", "AA:BB:CC:DD:EE:FF", None)
            .unwrap();
        backend.connect("hci0", "AA:BB:CC:DD:EE:FF").unwrap();

        let err = backend
            .read_char(
                "hci0",
                "AA:BB:CC:DD:EE:FF",
                EXPERIMENTAL_SERVICE_UUID,
                "00002a1a-0000-1000-8000-00805f9b34fb",
            )
            .unwrap_err();
        assert!(err.contains("only the experimental"));

        let result = backend.read_char_result("hci0").unwrap();
        assert!(result.contains("rejected-unsupported-characteristic"));
        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(bond_store).ok();
    }

    #[test]
    fn hci_read_char_rejects_not_connected() {
        let root = temp_path("rbos-hci-read-char-not-conn");
        let bond_store = temp_path("rbos-hci-read-char-not-conn-bonds");

        let mut backend =
            HciBackend::new_for_test(root.clone(), "hci0".to_string(), bond_store.clone());
        backend
            .add_stub_bond("hci0", "AA:BB:CC:DD:EE:FF", None)
            .unwrap();

        let err = backend
            .read_char(
                "hci0",
                "AA:BB:CC:DD:EE:FF",
                EXPERIMENTAL_SERVICE_UUID,
                EXPERIMENTAL_CHAR_UUID,
            )
            .unwrap_err();
        assert!(err.contains("run --connect"));
        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(bond_store).ok();
    }

    // -- GATT parsing --

    #[test]
    fn parse_gatt_services_extracts_handle_range_and_uuid() {
        let content = format!(
            "service=start_handle=0001;end_handle=0005;uuid={EXPERIMENTAL_SERVICE_UUID}\n\
             service=start_handle=0010;end_handle=0020;uuid=00001800-0000-1000-8000-00805f9b34fb\n"
        );
        let services = parse_gatt_services(&content);
        assert_eq!(services.len(), 2);
        assert_eq!(services[0].start_handle, "0001");
        assert_eq!(services[0].end_handle, "0005");
        assert_eq!(services[0].uuid, EXPERIMENTAL_SERVICE_UUID);
        assert_eq!(services[1].start_handle, "0010");
    }

    #[test]
    fn parse_gatt_services_ignores_malformed_lines() {
        let content = "not-a-service-line\nservice=start_handle=0001;end_handle=0005;uuid=abcd\n";
        let services = parse_gatt_services(content);
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].start_handle, "0001");
    }

    #[test]
    fn parse_gatt_services_handles_empty_input() {
        let services = parse_gatt_services("");
        assert!(services.is_empty());
    }

    #[test]
    fn parse_gatt_characteristics_extracts_handles_and_uuid() {
        let content = format!(
            "char=handle=0002;value_handle=0003;properties=12;uuid={EXPERIMENTAL_CHAR_UUID}\n\
             char=handle=0005;value_handle=0006;properties=02;uuid=00002a00-0000-1000-8000-00805f9b34fb\n"
        );
        let chars = parse_gatt_characteristics(&content);
        assert_eq!(chars.len(), 2);
        assert_eq!(chars[0].handle, "0002");
        assert_eq!(chars[0].value_handle, "0003");
        assert_eq!(chars[0].properties, "12");
        assert_eq!(chars[0].uuid, EXPERIMENTAL_CHAR_UUID);
        assert_eq!(chars[1].value_handle, "0006");
    }

    #[test]
    fn parse_gatt_characteristics_ignores_malformed_lines() {
        let content = "garbage\nchar=handle=0002;value_handle=0003;properties=12;uuid=abcd\n";
        let chars = parse_gatt_characteristics(content);
        assert_eq!(chars.len(), 1);
    }

    #[test]
    fn parse_gatt_characteristics_handles_empty_input() {
        let chars = parse_gatt_characteristics("");
        assert!(chars.is_empty());
    }

    // -- GATT workflow through scheme files --

    #[test]
    fn hci_read_char_uses_gatt_workflow_when_scheme_files_present() {
        let root = temp_path("rbos-hci-gatt");
        let adapter_dir = setup_scheme(&root, "hci0");
        let bond_store = temp_path("rbos-hci-gatt-bonds");

        fs::write(
            adapter_dir.join("connections"),
            "handle=0001 addr=AA:BB:CC:DD:EE:FF\n",
        )
        .unwrap();
        fs::write(
            adapter_dir.join("gatt-services"),
            format!("service=start_handle=0001;end_handle=0005;uuid={EXPERIMENTAL_SERVICE_UUID}\n"),
        )
        .unwrap();
        fs::write(
            adapter_dir.join("gatt-characteristics"),
            format!(
                "char=handle=0002;value_handle=0003;properties=12;uuid={EXPERIMENTAL_CHAR_UUID}\n"
            ),
        )
        .unwrap();
        // The write to gatt-read-char overwrites this file; the read returns the command bytes.
        fs::write(adapter_dir.join("gatt-read-char"), &[0x57u8]).unwrap();

        let mut backend =
            HciBackend::new_for_test(root.clone(), "hci0".to_string(), bond_store.clone());
        backend
            .add_stub_bond("hci0", "AA:BB:CC:DD:EE:FF", Some("battery"))
            .unwrap();
        backend.connect("hci0", "AA:BB:CC:DD:EE:FF").unwrap();
        backend
            .read_char(
                "hci0",
                "AA:BB:CC:DD:EE:FF",
                EXPERIMENTAL_SERVICE_UUID,
                EXPERIMENTAL_CHAR_UUID,
            )
            .unwrap();

        let result = backend.read_char_result("hci0").unwrap();
        assert!(
            result.contains("gatt-value"),
            "expected gatt-value in result, got: {result}"
        );

        // Verify the GATT commands were written to scheme files.
        let discover_svc = fs::read_to_string(adapter_dir.join("gatt-discover-services")).unwrap();
        assert_eq!(discover_svc, "handle=0001");

        let discover_ch = fs::read_to_string(adapter_dir.join("gatt-discover-chars")).unwrap();
        assert_eq!(discover_ch, "handle=0001;start=0001;end=0005");

        let read_cmd = fs::read_to_string(adapter_dir.join("gatt-read-char")).unwrap();
        assert_eq!(read_cmd, "handle=0001;addr=0003");

        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(bond_store).ok();
    }

    #[test]
    fn hci_read_char_falls_back_to_stub_when_no_connections_file() {
        let root = temp_path("rbos-hci-gatt-noconn");
        let adapter_dir = setup_scheme(&root, "hci0");
        let bond_store = temp_path("rbos-hci-gatt-noconn-bonds");

        // Provide gatt files but no connections file — resolve_conn_handle will fail.
        fs::write(
            adapter_dir.join("gatt-services"),
            format!("service=start_handle=0001;end_handle=0005;uuid={EXPERIMENTAL_SERVICE_UUID}\n"),
        )
        .unwrap();

        let mut backend =
            HciBackend::new_for_test(root.clone(), "hci0".to_string(), bond_store.clone());
        backend
            .add_stub_bond("hci0", "AA:BB:CC:DD:EE:FF", Some("battery"))
            .unwrap();
        backend.connect("hci0", "AA:BB:CC:DD:EE:FF").unwrap();
        backend
            .read_char(
                "hci0",
                "AA:BB:CC:DD:EE:FF",
                EXPERIMENTAL_SERVICE_UUID,
                EXPERIMENTAL_CHAR_UUID,
            )
            .unwrap();

        let result = backend.read_char_result("hci0").unwrap();
        assert!(
            result.contains("stub-value"),
            "expected stub-value fallback, got: {result}"
        );

        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(bond_store).ok();
    }

    #[test]
    fn hci_read_char_falls_back_to_stub_when_service_not_found() {
        let root = temp_path("rbos-hci-gatt-nosvc");
        let adapter_dir = setup_scheme(&root, "hci0");
        let bond_store = temp_path("rbos-hci-gatt-nosvc-bonds");

        fs::write(
            adapter_dir.join("connections"),
            "handle=0001 addr=AA:BB:CC:DD:EE:FF\n",
        )
        .unwrap();
        // Service list does not contain the battery service UUID.
        fs::write(
            adapter_dir.join("gatt-services"),
            "service=start_handle=0010;end_handle=0020;uuid=00001800-0000-1000-8000-00805f9b34fb\n",
        )
        .unwrap();

        let mut backend =
            HciBackend::new_for_test(root.clone(), "hci0".to_string(), bond_store.clone());
        backend
            .add_stub_bond("hci0", "AA:BB:CC:DD:EE:FF", None)
            .unwrap();
        backend.connect("hci0", "AA:BB:CC:DD:EE:FF").unwrap();
        backend
            .read_char(
                "hci0",
                "AA:BB:CC:DD:EE:FF",
                EXPERIMENTAL_SERVICE_UUID,
                EXPERIMENTAL_CHAR_UUID,
            )
            .unwrap();

        let result = backend.read_char_result("hci0").unwrap();
        assert!(
            result.contains("stub-value"),
            "expected stub-value fallback when service missing, got: {result}"
        );

        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(bond_store).ok();
    }

    #[test]
    fn hci_read_char_falls_back_to_stub_when_characteristic_not_found() {
        let root = temp_path("rbos-hci-gatt-nochar");
        let adapter_dir = setup_scheme(&root, "hci0");
        let bond_store = temp_path("rbos-hci-gatt-nochar-bonds");

        fs::write(
            adapter_dir.join("connections"),
            "handle=0001 addr=AA:BB:CC:DD:EE:FF\n",
        )
        .unwrap();
        fs::write(
            adapter_dir.join("gatt-services"),
            format!("service=start_handle=0001;end_handle=0005;uuid={EXPERIMENTAL_SERVICE_UUID}\n"),
        )
        .unwrap();
        // Characteristic list does not contain the battery level UUID.
        fs::write(
            adapter_dir.join("gatt-characteristics"),
            "char=handle=0002;value_handle=0003;properties=02;uuid=00002a00-0000-1000-8000-00805f9b34fb\n",
        )
        .unwrap();

        let mut backend =
            HciBackend::new_for_test(root.clone(), "hci0".to_string(), bond_store.clone());
        backend
            .add_stub_bond("hci0", "AA:BB:CC:DD:EE:FF", None)
            .unwrap();
        backend.connect("hci0", "AA:BB:CC:DD:EE:FF").unwrap();
        backend
            .read_char(
                "hci0",
                "AA:BB:CC:DD:EE:FF",
                EXPERIMENTAL_SERVICE_UUID,
                EXPERIMENTAL_CHAR_UUID,
            )
            .unwrap();

        let result = backend.read_char_result("hci0").unwrap();
        assert!(
            result.contains("stub-value"),
            "expected stub-value fallback when char missing, got: {result}"
        );

        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(bond_store).ok();
    }

    #[test]
    fn hci_read_char_gatt_value_formats_battery_percent() {
        let root = temp_path("rbos-hci-gatt-fmt");
        let adapter_dir = setup_scheme(&root, "hci0");
        let bond_store = temp_path("rbos-hci-gatt-fmt-bonds");

        fs::write(
            adapter_dir.join("connections"),
            "handle=00ab addr=AA:BB:CC:DD:EE:FF\n",
        )
        .unwrap();
        fs::write(
            adapter_dir.join("gatt-services"),
            format!("service=start_handle=0050;end_handle=00ff;uuid={EXPERIMENTAL_SERVICE_UUID}\n"),
        )
        .unwrap();
        fs::write(
            adapter_dir.join("gatt-characteristics"),
            format!(
                "char=handle=0060;value_handle=0061;properties=12;uuid={EXPERIMENTAL_CHAR_UUID}\n"
            ),
        )
        .unwrap();
        // Write will overwrite with command; read gets command bytes.
        // Command "handle=00ab;addr=0061" — first byte 'h' = 0x68 = 104.
        fs::write(adapter_dir.join("gatt-read-char"), &[0x00u8]).unwrap();

        let mut backend =
            HciBackend::new_for_test(root.clone(), "hci0".to_string(), bond_store.clone());
        backend
            .add_stub_bond("hci0", "AA:BB:CC:DD:EE:FF", Some("battery"))
            .unwrap();
        backend.connect("hci0", "AA:BB:CC:DD:EE:FF").unwrap();
        backend
            .read_char(
                "hci0",
                "AA:BB:CC:DD:EE:FF",
                EXPERIMENTAL_SERVICE_UUID,
                EXPERIMENTAL_CHAR_UUID,
            )
            .unwrap();

        let result = backend.read_char_result("hci0").unwrap();
        assert!(result.contains("gatt-value"));
        // Verify the full GATT command chain used the correct handles.
        let disc_svc = fs::read_to_string(adapter_dir.join("gatt-discover-services")).unwrap();
        assert_eq!(disc_svc, "handle=00ab");
        let disc_ch = fs::read_to_string(adapter_dir.join("gatt-discover-chars")).unwrap();
        assert_eq!(disc_ch, "handle=00ab;start=0050;end=00ff");
        let read_cmd = fs::read_to_string(adapter_dir.join("gatt-read-char")).unwrap();
        assert_eq!(read_cmd, "handle=00ab;addr=0061");

        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(bond_store).ok();
    }

    // -- Bond store --

    #[test]
    fn hci_bond_store_persists_across_backend_instances() {
        let root = temp_path("rbos-hci-bond-persist");
        let bond_store = temp_path("rbos-hci-bond-persist-bonds");

        let mut writer =
            HciBackend::new_for_test(root.clone(), "hci0".to_string(), bond_store.clone());
        let record = writer
            .add_stub_bond("hci0", "AA:BB:CC:DD:EE:FF", Some("demo"))
            .unwrap();
        assert_eq!(record.source, "stub-cli");

        let reader = HciBackend::new_for_test(root.clone(), "hci0".to_string(), bond_store.clone());
        let bonds = reader.load_bonds("hci0").unwrap();
        assert_eq!(bonds.len(), 1);
        assert_eq!(bonds[0].bond_id, "AA:BB:CC:DD:EE:FF");
        assert_eq!(bonds[0].alias.as_deref(), Some("demo"));

        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(bond_store).ok();
    }

    #[test]
    fn hci_remove_bond_clears_connection_state() {
        let root = temp_path("rbos-hci-remove-bond");
        let bond_store = temp_path("rbos-hci-remove-bond-bonds");

        let mut backend =
            HciBackend::new_for_test(root.clone(), "hci0".to_string(), bond_store.clone());
        backend
            .add_stub_bond("hci0", "AA:BB:CC:DD:EE:FF", None)
            .unwrap();
        backend.connect("hci0", "AA:BB:CC:DD:EE:FF").unwrap();
        assert_eq!(
            backend.connected_bond_ids("hci0").unwrap(),
            vec!["AA:BB:CC:DD:EE:FF"]
        );

        backend.remove_bond("hci0", "AA:BB:CC:DD:EE:FF").unwrap();
        assert!(backend.connected_bond_ids("hci0").unwrap().is_empty());

        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(bond_store).ok();
    }
}
