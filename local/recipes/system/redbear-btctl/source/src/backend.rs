use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::bond_store::{validate_adapter_name, BondRecord, BondStore};

const STATUS_FRESHNESS_SECS: u64 = 90;
const EXPERIMENTAL_WORKLOAD: &str = "battery-sensor-battery-level-read";
const EXPERIMENTAL_PERIPHERAL_CLASS: &str = "ble-battery-sensor";
const EXPERIMENTAL_CHARACTERISTIC: &str = "battery-level";
const EXPERIMENTAL_SERVICE_UUID: &str = "0000180f-0000-1000-8000-00805f9b34fb";
const EXPERIMENTAL_CHAR_UUID: &str = "00002a19-0000-1000-8000-00805f9b34fb";
const EXPERIMENTAL_VALUE_HEX: &str = "57";
const EXPERIMENTAL_VALUE_PERCENT: u8 = 87;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdapterStatus {
    ExplicitStartupRequired,
    AdapterVisible,
    Scanning,
    Failed,
}

impl AdapterStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AdapterStatus::ExplicitStartupRequired => "explicit-startup-required",
            AdapterStatus::AdapterVisible => "adapter-visible",
            AdapterStatus::Scanning => "scanning",
            AdapterStatus::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct AdapterState {
    pub status: String,
    pub transport_status: String,
    pub last_error: String,
    pub scan_results: Vec<String>,
    pub connected_bond_ids: Vec<String>,
    pub connect_result: String,
    pub disconnect_result: String,
    pub read_char_result: String,
    pub bond_store_path: String,
    pub bonds: Vec<BondRecord>,
}

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

pub fn connection_state_lines(connected_bond_ids: &[String]) -> Vec<String> {
    vec![
        format!(
            "connection_state={}",
            if connected_bond_ids.is_empty() {
                "stub-disconnected"
            } else {
                "stub-connected"
            }
        ),
        format!("connected_bond_count={}", connected_bond_ids.len()),
        format!("connected_bond_ids={}", connected_bond_ids.join(",")),
        format!(
            "note=stub-control-only-no-real-link-layer-beyond-experimental-{}",
            EXPERIMENTAL_WORKLOAD
        ),
    ]
}

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

pub trait Backend {
    fn adapters(&self) -> Vec<String>;
    fn capabilities(&self) -> Vec<String>;
    fn initial_status(&self, adapter: &str) -> AdapterStatus;
    fn transport_status(&self, adapter: &str) -> String;
    fn default_scan_results(&self, adapter: &str) -> Vec<String>;
    fn connected_bond_ids(&self, adapter: &str) -> Result<Vec<String>, String>;
    fn connect_result(&self, adapter: &str) -> Result<String, String>;
    fn disconnect_result(&self, adapter: &str) -> Result<String, String>;
    fn read_char_result(&self, adapter: &str) -> Result<String, String>;
    fn status(&self, adapter: &str) -> Result<AdapterStatus, String>;
    fn scan(&mut self, adapter: &str) -> Result<Vec<String>, String>;
    fn connect(&mut self, adapter: &str, bond_id: &str) -> Result<(), String>;
    fn disconnect(&mut self, adapter: &str, bond_id: &str) -> Result<(), String>;
    fn read_char(
        &mut self,
        adapter: &str,
        bond_id: &str,
        service_uuid: &str,
        char_uuid: &str,
    ) -> Result<(), String>;
    fn bond_store_path(&self, adapter: &str) -> Result<String, String>;
    fn load_bonds(&self, adapter: &str) -> Result<Vec<BondRecord>, String>;
    fn add_stub_bond(
        &mut self,
        adapter: &str,
        bond_id: &str,
        alias: Option<&str>,
    ) -> Result<BondRecord, String>;
    fn remove_bond(&mut self, adapter: &str, bond_id: &str) -> Result<bool, String>;
}

pub struct StubBackend {
    adapters: Vec<String>,
    scan_results: Vec<String>,
    transport_status_file: PathBuf,
    bond_store: BondStore,
    runtime_state: BTreeMap<String, AdapterRuntimeState>,
}

impl StubBackend {
    pub fn from_env() -> Self {
        let adapters = parse_list(
            env::var("REDBEAR_BTCTL_STUB_ADAPTERS").ok().as_deref(),
            &["hci0"],
        );
        let seeded_connected_bond_ids = parse_list(
            env::var("REDBEAR_BTCTL_STUB_CONNECTED_BOND_IDS")
                .ok()
                .as_deref(),
            &[],
        );
        for adapter in &adapters {
            if validate_adapter_name(adapter).is_err() {
                panic!("invalid Bluetooth adapter name in REDBEAR_BTCTL_STUB_ADAPTERS: {adapter}");
            }
        }

        Self {
            runtime_state: adapters
                .iter()
                .cloned()
                .map(|adapter| {
                    let mut state = AdapterRuntimeState::new();
                    state.connected_bond_ids = seeded_connected_bond_ids.iter().cloned().collect();
                    (adapter, state)
                })
                .collect(),
            adapters,
            scan_results: parse_list(
                env::var("REDBEAR_BTCTL_STUB_SCAN_RESULTS").ok().as_deref(),
                &["demo-beacon", "demo-sensor"],
            ),
            transport_status_file: env::var_os("REDBEAR_BTCTL_TRANSPORT_STATUS_FILE")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/var/run/redbear-btusb/status")),
            bond_store: BondStore::from_env(),
        }
    }

    #[cfg(test)]
    pub fn new_for_test(
        adapters: Vec<String>,
        scan_results: Vec<String>,
        transport_status_file: PathBuf,
        bond_store_root: PathBuf,
    ) -> Self {
        for adapter in &adapters {
            assert!(
                validate_adapter_name(adapter).is_ok(),
                "invalid test adapter name: {adapter}"
            );
        }

        Self {
            runtime_state: adapters
                .iter()
                .cloned()
                .map(|adapter| (adapter, AdapterRuntimeState::new()))
                .collect(),
            adapters,
            scan_results,
            transport_status_file,
            bond_store: BondStore::new(bond_store_root),
        }
    }

    fn knows_adapter(&self, adapter: &str) -> bool {
        self.adapters.iter().any(|candidate| candidate == adapter)
    }

    fn runtime_visible(&self) -> bool {
        fs::read_to_string(&self.transport_status_file)
            .map(|content| transport_status_is_runtime_visible(&content))
            .unwrap_or(false)
    }

    fn read_transport_status(path: &Path) -> Option<String> {
        let content = fs::read_to_string(path).ok()?;
        let parts = content
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>();
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" "))
        }
    }

    fn ensure_adapter(&self, adapter: &str) -> Result<(), String> {
        if self.knows_adapter(adapter) {
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
}

fn current_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn transport_status_is_runtime_visible(content: &str) -> bool {
    let runtime_visible = content
        .lines()
        .map(str::trim)
        .any(|line| line == "runtime_visibility=runtime-visible");

    let updated_at = content.lines().find_map(|line| {
        line.trim()
            .strip_prefix("updated_at_epoch=")
            .and_then(|value| value.parse::<u64>().ok())
    });

    runtime_visible
        && updated_at
            .map(|timestamp| {
                current_epoch_seconds().saturating_sub(timestamp) <= STATUS_FRESHNESS_SECS
            })
            .unwrap_or(false)
}

impl Backend for StubBackend {
    fn adapters(&self) -> Vec<String> {
        self.adapters.clone()
    }

    fn capabilities(&self) -> Vec<String> {
        vec![
            "backend=stub".to_string(),
            "transport=usb".to_string(),
            "startup=explicit".to_string(),
            "mode=ble-first".to_string(),
            "scan=true".to_string(),
            format!("workload={}", EXPERIMENTAL_WORKLOAD),
            "read_char=true".to_string(),
            "write_char=false".to_string(),
            "notify=false".to_string(),
            "bond_store=stub-cli".to_string(),
            "scheme=btctl".to_string(),
            format!("status_file={}", self.transport_status_file.display()),
            format!("bond_store_root={}", self.bond_store.root().display()),
        ]
    }

    fn initial_status(&self, adapter: &str) -> AdapterStatus {
        if !self.knows_adapter(adapter) {
            AdapterStatus::Failed
        } else if self.runtime_visible() {
            AdapterStatus::AdapterVisible
        } else {
            AdapterStatus::ExplicitStartupRequired
        }
    }

    fn transport_status(&self, adapter: &str) -> String {
        if !self.knows_adapter(adapter) {
            return "transport=unknown-adapter".to_string();
        }

        if self.runtime_visible() {
            return Self::read_transport_status(&self.transport_status_file).unwrap_or_else(|| {
                format!(
                    "transport=usb startup=explicit runtime_visibility=installed-only status_file={}",
                    self.transport_status_file.display()
                )
            });
        }

        format!(
            "transport=usb startup=explicit runtime_visibility=installed-only status_file={}",
            self.transport_status_file.display()
        )
    }

    fn default_scan_results(&self, _adapter: &str) -> Vec<String> {
        Vec::new()
    }

    fn connected_bond_ids(&self, adapter: &str) -> Result<Vec<String>, String> {
        self.ensure_adapter(adapter)?;
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
        if !self.runtime_visible() {
            return Err(
                "transport not runtime-visible; start redbear-btusb explicitly".to_string(),
            );
        }
        Ok(self.scan_results.clone())
    }

    fn connect(&mut self, adapter: &str, bond_id: &str) -> Result<(), String> {
        self.ensure_adapter(adapter)?;

        if !self.runtime_visible() {
            let state = self.runtime_state_mut(adapter)?;
            state.last_connect_result =
                format!("connect_result=rejected-transport-not-runtime-visible bond_id={bond_id}");
            return Err(
                "transport not runtime-visible; start redbear-btusb explicitly".to_string(),
            );
        }

        if !self.bond_exists(adapter, bond_id)? {
            let state = self.runtime_state_mut(adapter)?;
            state.last_connect_result =
                format!("connect_result=rejected-missing-bond bond_id={bond_id}");
            return Err("bond record not found; add a stub bond record first".to_string());
        }

        let state = self.runtime_state_mut(adapter)?;
        let outcome = if state.connected_bond_ids.insert(bond_id.to_string()) {
            "connected"
        } else {
            "already-connected"
        };
        state.last_connect_result =
            format!("connect_result=stub-connected bond_id={bond_id} state={outcome}");
        Ok(())
    }

    fn disconnect(&mut self, adapter: &str, bond_id: &str) -> Result<(), String> {
        self.ensure_adapter(adapter)?;

        if !self.runtime_visible() {
            let state = self.runtime_state_mut(adapter)?;
            state.last_disconnect_result = format!(
                "disconnect_result=rejected-transport-not-runtime-visible bond_id={bond_id}"
            );
            return Err(
                "transport not runtime-visible; start redbear-btusb explicitly".to_string(),
            );
        }

        if !self.bond_exists(adapter, bond_id)? {
            let state = self.runtime_state_mut(adapter)?;
            state.last_disconnect_result =
                format!("disconnect_result=rejected-missing-bond bond_id={bond_id}");
            return Err("bond record not found; add a stub bond record first".to_string());
        }

        let state = self.runtime_state_mut(adapter)?;
        let outcome = if state.connected_bond_ids.remove(bond_id) {
            "disconnected"
        } else {
            "already-disconnected"
        };
        state.last_disconnect_result =
            format!("disconnect_result=stub-disconnected bond_id={bond_id} state={outcome}");
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

        if !self.runtime_visible() {
            let state = self.runtime_state_mut(adapter)?;
            state.last_read_char_result = rejected_read_char_result(
                "rejected-transport-not-runtime-visible",
                bond_id,
                service_uuid,
                char_uuid,
            );
            return Err(
                "transport not runtime-visible; start redbear-btusb explicitly".to_string(),
            );
        }

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

        self.runtime_state_mut(adapter)?.last_read_char_result = success_read_char_result(bond_id);
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
                    "disconnect_result=stub-disconnected bond_id={bond_id} state=removed-with-bond"
                );
            }
        }

        Ok(removed)
    }
}

fn parse_list(raw: Option<&str>, default: &[&str]) -> Vec<String> {
    raw.map(|value| {
        value
            .split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>()
    })
    .filter(|entries| !entries.is_empty())
    .unwrap_or_else(|| default.iter().map(|entry| (*entry).to_string()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("{name}-{stamp}"))
    }

    #[test]
    fn stub_status_requires_explicit_transport_startup() {
        let backend = StubBackend::new_for_test(
            vec!["hci0".to_string()],
            vec!["demo-beacon".to_string()],
            temp_path("rbos-btctl-missing-transport"),
            temp_path("rbos-btctl-bond-store-missing"),
        );
        assert_eq!(
            backend.initial_status("hci0"),
            AdapterStatus::ExplicitStartupRequired
        );
        assert!(backend
            .transport_status("hci0")
            .contains("runtime_visibility=installed-only"));
    }

    #[test]
    fn stub_scan_uses_transport_status_file() {
        let status_path = temp_path("rbos-btctl-transport-present");
        fs::write(
            &status_path,
            &format!(
                "transport=usb\nstartup=explicit\nupdated_at_epoch={}\nruntime_visibility=runtime-visible\n",
                current_epoch_seconds()
            ),
        )
        .unwrap();

        let mut backend = StubBackend::new_for_test(
            vec!["hci0".to_string()],
            vec!["demo-beacon".to_string(), "demo-sensor".to_string()],
            status_path.clone(),
            temp_path("rbos-btctl-bond-store-visible"),
        );

        assert_eq!(
            backend.status("hci0").unwrap(),
            AdapterStatus::AdapterVisible
        );
        assert_eq!(
            backend.scan("hci0").unwrap(),
            vec!["demo-beacon".to_string(), "demo-sensor".to_string()]
        );

        fs::remove_file(status_path).unwrap();
    }

    #[test]
    fn stale_transport_status_requires_explicit_startup() {
        let status_path = temp_path("rbos-btctl-transport-stale");
        fs::write(
            &status_path,
            "transport=usb\nstartup=explicit\nupdated_at_epoch=1\nruntime_visibility=runtime-visible\n",
        )
        .unwrap();

        let backend = StubBackend::new_for_test(
            vec!["hci0".to_string()],
            vec!["demo-beacon".to_string()],
            status_path.clone(),
            temp_path("rbos-btctl-bond-store-stale"),
        );

        assert_eq!(
            backend.initial_status("hci0"),
            AdapterStatus::ExplicitStartupRequired
        );

        fs::remove_file(status_path).unwrap();
    }

    #[test]
    fn connect_requires_runtime_visible_transport() {
        let bond_store_root = temp_path("rbos-btctl-connect-missing-transport-bonds");
        let mut backend = StubBackend::new_for_test(
            vec!["hci0".to_string()],
            vec!["demo-beacon".to_string()],
            temp_path("rbos-btctl-connect-missing-transport"),
            bond_store_root.clone(),
        );

        backend
            .add_stub_bond("hci0", "AA:BB:CC:DD:EE:FF", Some("demo-sensor"))
            .unwrap();

        let err = backend.connect("hci0", "AA:BB:CC:DD:EE:FF").unwrap_err();
        assert!(err.contains("start redbear-btusb explicitly"));
        assert_eq!(
            backend.connected_bond_ids("hci0").unwrap(),
            Vec::<String>::new()
        );
        assert_eq!(
            backend.connect_result("hci0").unwrap(),
            "connect_result=rejected-transport-not-runtime-visible bond_id=AA:BB:CC:DD:EE:FF"
        );

        fs::remove_dir_all(bond_store_root).unwrap();
    }

    #[test]
    fn read_char_requires_connected_bond_and_exact_workload_uuid_pair() {
        let status_path = temp_path("rbos-btctl-read-char-visible");
        fs::write(
            &status_path,
            &format!(
                "transport=usb\nstartup=explicit\nupdated_at_epoch={}\nruntime_visibility=runtime-visible\n",
                current_epoch_seconds()
            ),
        )
        .unwrap();
        let bond_store_root = temp_path("rbos-btctl-read-char-visible-bonds");
        let mut backend = StubBackend::new_for_test(
            vec!["hci0".to_string()],
            vec!["demo-beacon".to_string()],
            status_path.clone(),
            bond_store_root.clone(),
        );

        backend
            .add_stub_bond("hci0", "AA:BB:CC:DD:EE:FF", Some("demo-battery-sensor"))
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
        assert!(backend
            .read_char_result("hci0")
            .unwrap()
            .contains("read_char_result=rejected-not-connected"));

        backend.connect("hci0", "AA:BB:CC:DD:EE:FF").unwrap();

        let unsupported = backend
            .read_char(
                "hci0",
                "AA:BB:CC:DD:EE:FF",
                EXPERIMENTAL_SERVICE_UUID,
                "00002a1a-0000-1000-8000-00805f9b34fb",
            )
            .unwrap_err();
        assert!(unsupported.contains("only the experimental"));
        assert!(backend
            .read_char_result("hci0")
            .unwrap()
            .contains("read_char_result=rejected-unsupported-characteristic"));

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
        assert!(result.contains(&format!("workload={}", EXPERIMENTAL_WORKLOAD)));
        assert!(result.contains("bond_id=AA:BB:CC:DD:EE:FF"));
        assert!(result.contains("access=read-only"));
        assert!(result.contains("value_percent=87"));

        fs::remove_file(status_path).unwrap();
        fs::remove_dir_all(bond_store_root).unwrap();
    }

    #[test]
    fn connect_and_disconnect_track_stub_connection_state() {
        let status_path = temp_path("rbos-btctl-connect-visible");
        fs::write(
            &status_path,
            &format!(
                "transport=usb\nstartup=explicit\nupdated_at_epoch={}\nruntime_visibility=runtime-visible\n",
                current_epoch_seconds()
            ),
        )
        .unwrap();
        let bond_store_root = temp_path("rbos-btctl-connect-visible-bonds");
        let mut backend = StubBackend::new_for_test(
            vec!["hci0".to_string()],
            vec!["demo-beacon".to_string()],
            status_path.clone(),
            bond_store_root.clone(),
        );

        backend
            .add_stub_bond("hci0", "AA:BB:CC:DD:EE:FF", Some("demo-sensor"))
            .unwrap();
        backend.connect("hci0", "AA:BB:CC:DD:EE:FF").unwrap();
        backend.connect("hci0", "AA:BB:CC:DD:EE:FF").unwrap();

        assert_eq!(
            backend.connected_bond_ids("hci0").unwrap(),
            vec!["AA:BB:CC:DD:EE:FF".to_string()]
        );
        assert_eq!(
            backend.connect_result("hci0").unwrap(),
            "connect_result=stub-connected bond_id=AA:BB:CC:DD:EE:FF state=already-connected"
        );

        backend.disconnect("hci0", "AA:BB:CC:DD:EE:FF").unwrap();
        backend.disconnect("hci0", "AA:BB:CC:DD:EE:FF").unwrap();

        assert_eq!(
            backend.connected_bond_ids("hci0").unwrap(),
            Vec::<String>::new()
        );
        assert_eq!(
            backend.disconnect_result("hci0").unwrap(),
            "disconnect_result=stub-disconnected bond_id=AA:BB:CC:DD:EE:FF state=already-disconnected"
        );

        fs::remove_file(status_path).unwrap();
        fs::remove_dir_all(bond_store_root).unwrap();
    }

    #[test]
    fn invalid_adapter_names_are_rejected_in_test_backend() {
        let result = std::panic::catch_unwind(|| {
            StubBackend::new_for_test(
                vec!["../escape".to_string()],
                vec!["demo-beacon".to_string()],
                temp_path("rbos-btctl-invalid-adapter-status"),
                temp_path("rbos-btctl-invalid-adapter-bonds"),
            )
        });

        assert!(result.is_err());

        let dot_result = std::panic::catch_unwind(|| {
            StubBackend::new_for_test(
                vec!["..".to_string()],
                vec!["demo-beacon".to_string()],
                temp_path("rbos-btctl-dotdot-status"),
                temp_path("rbos-btctl-dotdot-bonds"),
            )
        });

        assert!(dot_result.is_err());
    }

    #[test]
    fn stub_bond_store_persists_across_backend_instances() {
        let bond_store_root = temp_path("rbos-btctl-bond-store-persist");
        let mut writer = StubBackend::new_for_test(
            vec!["hci0".to_string()],
            vec!["demo-beacon".to_string()],
            temp_path("rbos-btctl-transport-unused"),
            bond_store_root.clone(),
        );

        let record = writer
            .add_stub_bond("hci0", "AA:BB:CC:DD:EE:FF", Some("demo-sensor"))
            .unwrap();
        assert_eq!(record.source, "stub-cli");

        let reader = StubBackend::new_for_test(
            vec!["hci0".to_string()],
            vec!["demo-beacon".to_string()],
            temp_path("rbos-btctl-transport-unused-reader"),
            bond_store_root.clone(),
        );

        let bonds = reader.load_bonds("hci0").unwrap();
        assert_eq!(bonds.len(), 1);
        assert_eq!(bonds[0].bond_id, "AA:BB:CC:DD:EE:FF");
        assert_eq!(bonds[0].alias.as_deref(), Some("demo-sensor"));
        assert_eq!(
            reader.bond_store_path("hci0").unwrap(),
            bond_store_root
                .join("hci0")
                .join("bonds")
                .display()
                .to_string()
        );

        fs::remove_dir_all(bond_store_root).unwrap();
    }
}
