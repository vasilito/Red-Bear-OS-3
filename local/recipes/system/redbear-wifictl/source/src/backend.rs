use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::process::Command;

#[cfg(test)]
pub(crate) static TEST_ENV_LOCK: std::sync::LazyLock<std::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(()));

#[cfg(target_os = "redox")]
use redox_driver_sys::pci::PciDevice;
#[cfg(target_os = "redox")]
use redox_driver_sys::pci::PciLocation;

#[derive(Clone, Debug)]
struct ParsedPciLocation {
    segment: u16,
    bus: u8,
    device: u8,
    function: u8,
}

impl std::fmt::Display for ParsedPciLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:04x}--{:02x}--{:02x}.{}",
            self.segment, self.bus, self.device, self.function
        )
    }
}

#[cfg(target_os = "redox")]
impl From<ParsedPciLocation> for PciLocation {
    fn from(value: ParsedPciLocation) -> Self {
        Self {
            segment: value.segment,
            bus: value.bus,
            device: value.device,
            function: value.function,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WifiStatus {
    Down,
    DeviceDetected,
    FirmwareReady,
    Scanning,
    Associating,
    Connected,
    Failed,
}

impl WifiStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            WifiStatus::Down => "down",
            WifiStatus::DeviceDetected => "device-detected",
            WifiStatus::FirmwareReady => "firmware-ready",
            WifiStatus::Scanning => "scanning",
            WifiStatus::Associating => "associating",
            WifiStatus::Connected => "connected",
            WifiStatus::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct InterfaceState {
    pub ssid: String,
    pub security: String,
    pub key: String,
    pub status: String,
    pub link_state: String,
    pub firmware_status: String,
    pub transport_status: String,
    pub transport_init_status: String,
    pub activation_status: String,
    pub connect_result: String,
    pub disconnect_result: String,
    pub last_error: String,
    pub scan_results: Vec<String>,
}

pub trait Backend {
    fn interfaces(&self) -> Vec<String>;
    fn capabilities(&self) -> Vec<String>;
    #[allow(dead_code)]
    fn initial_status(&self, interface: &str) -> WifiStatus;
    fn initial_link_state(&self, interface: &str) -> String;
    #[allow(dead_code)]
    fn default_scan_results(&self, interface: &str) -> Vec<String>;
    fn scan(&mut self, interface: &str) -> Result<Vec<String>, String>;
    fn firmware_status(&self, interface: &str) -> String;
    fn transport_status(&self, interface: &str) -> String;
    fn prepare(&mut self, interface: &str) -> Result<WifiStatus, String>;
    fn transport_probe(&mut self, interface: &str) -> Result<String, String>;
    fn init_transport(&mut self, interface: &str) -> Result<String, String>;
    fn activate(&mut self, interface: &str) -> Result<String, String>;
    fn connect_result(&self, interface: &str) -> String;
    fn disconnect_result(&self, interface: &str) -> String;
    fn retry(&mut self, interface: &str) -> Result<WifiStatus, String>;
    #[allow(dead_code)]
    fn connect(&mut self, interface: &str, state: &InterfaceState) -> Result<WifiStatus, String>;
    #[allow(dead_code)]
    fn disconnect(&mut self, interface: &str) -> Result<WifiStatus, String>;
}

#[derive(Clone, Debug)]
struct IntelInterface {
    name: String,
    location: String,
    config_path: PathBuf,
    device_id: u16,
    subsystem_id: u16,
    firmware_family: &'static str,
    transport_status: String,
    ucode_candidates: Vec<String>,
    selected_ucode: Option<String>,
    pnvm_candidate: Option<String>,
    pnvm_found: Option<String>,
    prepared: bool,
    transport_initialized: bool,
    activated: bool,
    connect_result: String,
    disconnect_result: String,
}

pub struct StubBackend {
    interfaces: Vec<String>,
}

pub struct NoDeviceBackend;

impl StubBackend {
    pub fn from_env() -> Self {
        let interfaces = env::var("REDBEAR_WIFICTL_STUB_INTERFACES")
            .ok()
            .map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| vec!["wlan0".to_string()]);
        Self { interfaces }
    }
}

impl NoDeviceBackend {
    pub fn new() -> Self {
        Self
    }
}

impl Backend for StubBackend {
    fn interfaces(&self) -> Vec<String> {
        self.interfaces.clone()
    }

    fn capabilities(&self) -> Vec<String> {
        vec![
            "backend=stub".to_string(),
            "security=open,wpa2-psk".to_string(),
            "scan=true".to_string(),
            "connect=true".to_string(),
        ]
    }

    fn initial_status(&self, _interface: &str) -> WifiStatus {
        WifiStatus::DeviceDetected
    }

    fn default_scan_results(&self, _interface: &str) -> Vec<String> {
        vec!["demo-ssid".to_string(), "demo-open".to_string()]
    }

    fn scan(&mut self, _interface: &str) -> Result<Vec<String>, String> {
        Ok(vec!["demo-ssid".to_string(), "demo-open".to_string()])
    }

    fn firmware_status(&self, _interface: &str) -> String {
        "firmware=stub".to_string()
    }

    fn transport_status(&self, _interface: &str) -> String {
        "transport=stub".to_string()
    }

    fn initial_link_state(&self, _interface: &str) -> String {
        "link=down".to_string()
    }

    fn init_transport(&mut self, _interface: &str) -> Result<String, String> {
        Ok("transport_init=stub".to_string())
    }

    fn activate(&mut self, _interface: &str) -> Result<String, String> {
        Ok("activation=stub".to_string())
    }

    fn connect_result(&self, _interface: &str) -> String {
        "connect=stub".to_string()
    }

    fn disconnect_result(&self, _interface: &str) -> String {
        "disconnect=stub".to_string()
    }

    fn retry(&mut self, _interface: &str) -> Result<WifiStatus, String> {
        Ok(WifiStatus::DeviceDetected)
    }

    fn prepare(&mut self, _interface: &str) -> Result<WifiStatus, String> {
        Ok(WifiStatus::FirmwareReady)
    }

    fn transport_probe(&mut self, _interface: &str) -> Result<String, String> {
        Ok("transport=stub mmio_probe=host-skipped".to_string())
    }

    fn connect(&mut self, _interface: &str, state: &InterfaceState) -> Result<WifiStatus, String> {
        if state.ssid.is_empty() {
            return Err("missing SSID".to_string());
        }
        match state.security.as_str() {
            "open" => Ok(WifiStatus::Connected),
            "wpa2-psk" if !state.key.is_empty() => Ok(WifiStatus::Connected),
            "wpa2-psk" => Err("missing key".to_string()),
            other => Err(format!("unsupported security {other}")),
        }
    }

    fn disconnect(&mut self, _interface: &str) -> Result<WifiStatus, String> {
        Ok(WifiStatus::DeviceDetected)
    }
}

impl Backend for NoDeviceBackend {
    fn interfaces(&self) -> Vec<String> {
        Vec::new()
    }

    fn capabilities(&self) -> Vec<String> {
        vec![
            "backend=no-device".to_string(),
            "target=intel-not-detected".to_string(),
            "scan=false".to_string(),
            "connect=false".to_string(),
        ]
    }

    fn initial_status(&self, _interface: &str) -> WifiStatus {
        WifiStatus::Down
    }

    fn default_scan_results(&self, _interface: &str) -> Vec<String> {
        Vec::new()
    }

    fn scan(&mut self, _interface: &str) -> Result<Vec<String>, String> {
        Err("no Intel Wi-Fi device detected".to_string())
    }

    fn firmware_status(&self, _interface: &str) -> String {
        "firmware=no-device".to_string()
    }

    fn transport_status(&self, _interface: &str) -> String {
        "transport=no-device".to_string()
    }

    fn initial_link_state(&self, _interface: &str) -> String {
        "link=no-device".to_string()
    }

    fn init_transport(&mut self, _interface: &str) -> Result<String, String> {
        Err("no Intel Wi-Fi device detected".to_string())
    }

    fn activate(&mut self, _interface: &str) -> Result<String, String> {
        Err("no Intel Wi-Fi device detected".to_string())
    }

    fn connect_result(&self, _interface: &str) -> String {
        "connect=no-device".to_string()
    }

    fn disconnect_result(&self, _interface: &str) -> String {
        "disconnect=no-device".to_string()
    }

    fn retry(&mut self, _interface: &str) -> Result<WifiStatus, String> {
        Err("no Intel Wi-Fi device detected".to_string())
    }

    fn prepare(&mut self, _interface: &str) -> Result<WifiStatus, String> {
        Err("no Intel Wi-Fi device detected".to_string())
    }

    fn transport_probe(&mut self, _interface: &str) -> Result<String, String> {
        Err("no Intel Wi-Fi device detected".to_string())
    }

    fn connect(&mut self, _interface: &str, _state: &InterfaceState) -> Result<WifiStatus, String> {
        Err("no Intel Wi-Fi device detected".to_string())
    }

    fn disconnect(&mut self, _interface: &str) -> Result<WifiStatus, String> {
        Err("no Intel Wi-Fi device detected".to_string())
    }
}

pub struct IntelBackend {
    pci_root: PathBuf,
    firmware_root: PathBuf,
    firmware_scheme_root: PathBuf,
    interfaces: Vec<IntelInterface>,
}

impl IntelBackend {
    pub fn from_env() -> Self {
        let pci_root = env::var_os("REDBEAR_WIFICTL_PCI_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/scheme/pci"));
        let firmware_root = env::var_os("REDBEAR_WIFICTL_FIRMWARE_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/lib/firmware"));
        let firmware_scheme_root = env::var_os("REDBEAR_WIFICTL_FIRMWARE_SCHEME_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/scheme/firmware"));
        let interfaces = detect_intel_wifi_interfaces(&pci_root, &firmware_root);
        Self {
            pci_root,
            firmware_root,
            firmware_scheme_root,
            interfaces,
        }
    }

    fn read_firmware_blob(&self, name: &str) -> Result<Vec<u8>, String> {
        let scheme_path = self.firmware_scheme_root.join(name);
        if let Ok(bytes) = fs::read(&scheme_path) {
            return Ok(bytes);
        }

        let fs_path = self.firmware_root.join(name);
        fs::read(&fs_path).map_err(|err| {
            format!(
                "failed to read firmware {} from {} or {}: {err}",
                name,
                scheme_path.display(),
                fs_path.display()
            )
        })
    }

    fn driver_command() -> PathBuf {
        env::var_os("REDBEAR_IWLWIFI_CMD")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/usr/lib/drivers/redbear-iwlwifi"))
    }

    fn run_driver_action(
        &self,
        action: &str,
        iface: &IntelInterface,
    ) -> Result<Vec<String>, String> {
        self.run_driver_action_with_args(action, iface, &[])
    }

    fn run_driver_action_with_args(
        &self,
        action: &str,
        iface: &IntelInterface,
        extra_args: &[&str],
    ) -> Result<Vec<String>, String> {
        let cmd = Self::driver_command();
        if !cmd.exists() {
            return Err(format!("driver command {} not found", cmd.display()));
        }

        let mut command = Command::new(&cmd);
        command
            .arg(action)
            .arg(iface.location.to_string())
            .args(extra_args)
            .env("REDBEAR_IWLWIFI_PCI_ROOT", &self.pci_root)
            .env("REDBEAR_IWLWIFI_FIRMWARE_ROOT", &self.firmware_root);

        let output = command
            .output()
            .map_err(|err| format!("failed to run {} {}: {err}", cmd.display(), action))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(if stderr.is_empty() {
                format!(
                    "{} {} failed with status {}",
                    cmd.display(),
                    action,
                    output.status
                )
            } else {
                stderr
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect())
    }

    fn line_value(lines: &[String], key: &str) -> Option<String> {
        lines
            .iter()
            .find_map(|line| line.strip_prefix(&format!("{key}=")).map(str::to_string))
    }
}

impl Backend for IntelBackend {
    fn interfaces(&self) -> Vec<String> {
        self.interfaces
            .iter()
            .map(|iface| iface.name.clone())
            .collect()
    }

    fn capabilities(&self) -> Vec<String> {
        let mut capabilities = vec![
            "backend=intel".to_string(),
            "target=arrow-lake-and-older".to_string(),
            "security=open,wpa2-psk".to_string(),
            format!("pci-root={}", self.pci_root.display()),
            format!("firmware-root={}", self.firmware_root.display()),
            format!(
                "firmware-scheme-root={}",
                self.firmware_scheme_root.display()
            ),
            "transport=iwlwifi-class".to_string(),
        ];
        for iface in &self.interfaces {
            let candidate_list = iface.ucode_candidates.join(",");
            let found = iface
                .selected_ucode
                .clone()
                .unwrap_or_else(|| "missing".to_string());
            let pnvm = iface
                .pnvm_found
                .clone()
                .or_else(|| iface.pnvm_candidate.clone())
                .unwrap_or_else(|| "none".to_string());
            capabilities.push(format!(
                "iface={} device={:04x} subsystem={:04x} family={} transport={} prepared={} activated={} ucode_candidates={} ucode_selected={} pnvm={}",
                iface.name,
                iface.device_id,
                iface.subsystem_id,
                iface.firmware_family,
                iface.transport_status,
                if iface.prepared { "yes" } else { "no" },
                if iface.activated { "yes" } else { "no" },
                candidate_list,
                found,
                pnvm
            ));
        }
        capabilities
    }

    fn initial_status(&self, interface: &str) -> WifiStatus {
        if self.interfaces.is_empty() {
            WifiStatus::Down
        } else if self
            .interfaces
            .iter()
            .find(|candidate| candidate.name == interface)
            .map(|candidate| candidate.prepared)
            .unwrap_or(false)
        {
            WifiStatus::FirmwareReady
        } else {
            WifiStatus::DeviceDetected
        }
    }

    fn default_scan_results(&self, _interface: &str) -> Vec<String> {
        Vec::new()
    }

    fn scan(&mut self, interface: &str) -> Result<Vec<String>, String> {
        let Some(pos) = self
            .interfaces
            .iter()
            .position(|candidate| candidate.name == interface)
        else {
            return Err("unknown wireless interface".to_string());
        };
        let candidate = &self.interfaces[pos];
        if !candidate.prepared {
            return Err("firmware not prepared; run prepare first".to_string());
        }
        if !candidate.transport_initialized {
            return Err("transport not initialized; run init-transport first".to_string());
        }
        if !candidate.activated {
            return Err("NIC not activated; run activate-nic first".to_string());
        }
        let scan_lines = self.run_driver_action("--scan", candidate)?;
        let mut results = scan_lines
            .iter()
            .filter_map(|line| line.strip_prefix("scan_result=").map(str::to_string))
            .collect::<Vec<_>>();
        if results.is_empty() {
            results = vec!["driver-scan-not-implemented".to_string()];
        }
        Ok(results)
    }

    fn firmware_status(&self, interface: &str) -> String {
        let Some(candidate) = self
            .interfaces
            .iter()
            .find(|candidate| candidate.name == interface)
        else {
            return "firmware=unknown-interface".to_string();
        };
        match &candidate.selected_ucode {
            Some(found) => format!(
                "firmware=present family={} prepared={} selected={} pnvm={} candidates={}",
                candidate.firmware_family,
                if candidate.prepared { "yes" } else { "no" },
                found,
                candidate
                    .pnvm_found
                    .clone()
                    .or_else(|| candidate.pnvm_candidate.clone())
                    .unwrap_or_else(|| "none".to_string()),
                candidate.ucode_candidates.join(",")
            ),
            None => format!(
                "firmware=missing family={} prepared={} candidates={} pnvm={}",
                candidate.firmware_family,
                if candidate.prepared { "yes" } else { "no" },
                candidate.ucode_candidates.join(","),
                candidate
                    .pnvm_candidate
                    .clone()
                    .unwrap_or_else(|| "none".to_string())
            ),
        }
    }

    fn transport_status(&self, interface: &str) -> String {
        self.interfaces
            .iter()
            .find(|candidate| candidate.name == interface)
            .map(|candidate| candidate.transport_status.clone())
            .unwrap_or_else(|| "transport=unknown-interface".to_string())
    }

    fn initial_link_state(&self, interface: &str) -> String {
        if self
            .interfaces
            .iter()
            .any(|candidate| candidate.name == interface)
        {
            "link=down".to_string()
        } else {
            "link=unknown-interface".to_string()
        }
    }

    fn init_transport(&mut self, interface: &str) -> Result<String, String> {
        let Some(pos) = self
            .interfaces
            .iter()
            .position(|candidate| candidate.name == interface)
        else {
            return Err("unknown wireless interface".to_string());
        };
        let candidate = &self.interfaces[pos];
        if !candidate.prepared {
            return Err("firmware not prepared; run prepare first".to_string());
        }
        let transport_lines = self
            .run_driver_action("--init-transport", candidate)
            .or_else(|_| {
                init_transport_action(&candidate.config_path, candidate.firmware_family)
                    .map(|v| vec![v])
            })?;
        let transport_status = Self::line_value(&transport_lines, "transport_status")
            .or_else(|| {
                Self::line_value(&transport_lines, "status")
                    .map(|status| format!("transport_status={status}"))
            })
            .unwrap_or_else(|| format!("transport_status={}", candidate.transport_status));
        self.interfaces[pos].transport_status = transport_status.clone();
        self.interfaces[pos].transport_initialized = true;
        Ok("transport_init=ok".to_string())
    }

    fn activate(&mut self, interface: &str) -> Result<String, String> {
        let Some(pos) = self
            .interfaces
            .iter()
            .position(|candidate| candidate.name == interface)
        else {
            return Err("unknown wireless interface".to_string());
        };
        let candidate = &self.interfaces[pos];
        if !candidate.transport_initialized {
            return Err("transport not initialized; run init-transport first".to_string());
        }
        let activation_lines = self
            .run_driver_action("--activate-nic", candidate)
            .or_else(|_| activate_nic_action(&candidate.config_path).map(|v| vec![v]))?;
        let activation_status = Self::line_value(&activation_lines, "activation_status")
            .or_else(|| Self::line_value(&activation_lines, "activation"))
            .unwrap_or_else(|| "activation=ok".to_string());
        self.interfaces[pos].transport_status =
            Self::line_value(&activation_lines, "transport_status").unwrap_or_else(|| {
                transport_status_after_prepare(&candidate.config_path, candidate.firmware_family)
                    .unwrap_or_else(|_| candidate.transport_status.clone())
            });
        self.interfaces[pos].activated = true;
        Ok(activation_status)
    }

    fn connect_result(&self, interface: &str) -> String {
        self.interfaces
            .iter()
            .find(|candidate| candidate.name == interface)
            .map(|candidate| candidate.connect_result.clone())
            .unwrap_or_else(|| "connect=unknown-interface".to_string())
    }

    fn disconnect_result(&self, interface: &str) -> String {
        self.interfaces
            .iter()
            .find(|candidate| candidate.name == interface)
            .map(|candidate| candidate.disconnect_result.clone())
            .unwrap_or_else(|| "disconnect=unknown-interface".to_string())
    }

    fn prepare(&mut self, interface: &str) -> Result<WifiStatus, String> {
        let Some(pos) = self
            .interfaces
            .iter()
            .position(|candidate| candidate.name == interface)
        else {
            return Err("unknown wireless interface".to_string());
        };
        let candidate = &self.interfaces[pos];
        let firmware_family = candidate.firmware_family;
        let config_path = candidate.config_path.clone();
        let prepare_lines = self
            .run_driver_action("--prepare", candidate)
            .or_else(|_| {
                let Some(ucode) = candidate.selected_ucode.clone() else {
                    return Err(format!(
                        "missing firmware for {} (expected one of: {})",
                        firmware_family,
                        candidate.ucode_candidates.join(", ")
                    ));
                };
                let pnvm_required = candidate.pnvm_candidate.clone();
                let pnvm_found = candidate.pnvm_found.clone();
                if let Some(pnvm) = pnvm_required.as_ref() {
                    if pnvm_found.is_none() {
                        return Err(format!(
                            "missing pnvm for {} (expected {})",
                            firmware_family, pnvm
                        ));
                    }
                }
                let _ = self.read_firmware_blob(&ucode)?;
                if let Some(pnvm) = pnvm_found.as_ref() {
                    let _ = self.read_firmware_blob(pnvm)?;
                }
                program_transport_bits(&config_path)?;
                let transport_status =
                    transport_status_after_prepare(&config_path, firmware_family)?;
                Ok(vec![
                    format!("status={}", WifiStatus::FirmwareReady.as_str()),
                    format!("transport_status={transport_status}"),
                ])
            })?;
        let transport_status =
            Self::line_value(&prepare_lines, "transport_status").unwrap_or_else(|| {
                transport_status_after_prepare(&config_path, firmware_family)
                    .unwrap_or_else(|_| candidate.transport_status.clone())
            });
        self.interfaces[pos].prepared = true;
        self.interfaces[pos].transport_status = transport_status;
        Ok(WifiStatus::FirmwareReady)
    }

    fn transport_probe(&mut self, interface: &str) -> Result<String, String> {
        let Some(pos) = self
            .interfaces
            .iter()
            .position(|candidate| candidate.name == interface)
        else {
            return Err("unknown wireless interface".to_string());
        };
        let candidate = &self.interfaces[pos];
        self.run_driver_action("--transport-probe", candidate)
            .ok()
            .and_then(|lines| Self::line_value(&lines, "transport_status"))
            .map(Ok)
            .unwrap_or_else(|| {
                transport_status_after_prepare(&candidate.config_path, candidate.firmware_family)
            })
    }

    fn connect(&mut self, interface: &str, state: &InterfaceState) -> Result<WifiStatus, String> {
        let Some(pos) = self
            .interfaces
            .iter()
            .position(|candidate| candidate.name == interface)
        else {
            return Err("unknown wireless interface".to_string());
        };
        let candidate = &self.interfaces[pos];
        if !candidate.prepared {
            return Err("firmware not prepared; run prepare first".to_string());
        }
        if !candidate.transport_initialized {
            return Err("transport not initialized; run init-transport first".to_string());
        }
        if !candidate.activated {
            return Err("NIC not activated; run activate-nic first".to_string());
        }
        if candidate.selected_ucode.is_none() {
            return Err(format!(
                "missing firmware for {} (expected one of: {})",
                candidate.firmware_family,
                candidate.ucode_candidates.join(", ")
            ));
        }
        if state.ssid.is_empty() {
            return Err("missing SSID".to_string());
        }
        let security = if state.security.is_empty() {
            "open"
        } else {
            state.security.as_str()
        };
        if security == "wpa2-psk" && state.key.is_empty() {
            return Err("missing key".to_string());
        }

        let connect_lines = self.run_driver_action_with_args(
            "--connect",
            candidate,
            &[state.ssid.as_str(), security, state.key.as_str()],
        )?;
        self.interfaces[pos].connect_result = Self::line_value(&connect_lines, "connect_result")
            .unwrap_or_else(|| format!("connect_result=ssid={} security={security}", state.ssid));
        if Self::line_value(&connect_lines, "status").as_deref() == Some("associated") {
            Ok(WifiStatus::Connected)
        } else {
            Ok(WifiStatus::Associating)
        }
    }

    fn disconnect(&mut self, interface: &str) -> Result<WifiStatus, String> {
        let Some(pos) = self
            .interfaces
            .iter()
            .position(|candidate| candidate.name == interface)
        else {
            return Err("unknown wireless interface".to_string());
        };
        let candidate = &self.interfaces[pos];
        if !candidate.activated {
            return Err("NIC not activated; run activate-nic first".to_string());
        }
        let lines = self.run_driver_action("--disconnect", candidate)?;
        self.interfaces[pos].disconnect_result = Self::line_value(&lines, "disconnect_result")
            .unwrap_or_else(|| "disconnect_result=ok".to_string());
        Ok(WifiStatus::DeviceDetected)
    }

    fn retry(&mut self, interface: &str) -> Result<WifiStatus, String> {
        let Some(candidate) = self
            .interfaces
            .iter()
            .find(|candidate| candidate.name == interface)
        else {
            return Err("unknown wireless interface".to_string());
        };
        if !candidate.prepared {
            return Err("firmware not prepared; run prepare first".to_string());
        }
        if !candidate.transport_initialized {
            return Err("transport not initialized; run init-transport first".to_string());
        }
        let retry_lines = self.run_driver_action("--retry", candidate)?;
        if Self::line_value(&retry_lines, "status").as_deref() == Some("device-detected") {
            Ok(WifiStatus::DeviceDetected)
        } else {
            Ok(WifiStatus::Failed)
        }
    }
}

fn detect_intel_wifi_interfaces(
    pci_root: &PathBuf,
    firmware_root: &PathBuf,
) -> Vec<IntelInterface> {
    let mut devices = BTreeMap::new();
    let Ok(entries) = fs::read_dir(pci_root) else {
        return Vec::new();
    };

    for entry in entries.flatten() {
        let Ok(config) = fs::read(entry.path().join("config")) else {
            continue;
        };
        if config.len() < 48 {
            continue;
        }
        let vendor_id = u16::from_le_bytes([config[0x00], config[0x01]]);
        let device_id = u16::from_le_bytes([config[0x02], config[0x03]]);
        let class_code = config[0x0B];
        let subclass = config[0x0A];
        let subsystem_id = u16::from_le_bytes([config[0x2E], config[0x2F]]);
        if vendor_id == 0x8086 && class_code == 0x02 && subclass == 0x80 {
            let Ok(location) = parse_location_from_config_path(&entry.path().join("config")) else {
                continue;
            };
            let idx = devices.len();
            let (firmware_family, ucode_candidates, pnvm_candidate) =
                intel_firmware_candidates(device_id, subsystem_id);
            let selected_ucode = ucode_candidates
                .iter()
                .find(|candidate| firmware_root.join(candidate).exists())
                .cloned();
            let pnvm_found = pnvm_candidate
                .as_ref()
                .filter(|candidate| firmware_root.join(candidate).exists())
                .cloned();
            devices.insert(
                format!("wlan{idx}"),
                IntelInterface {
                    name: format!("wlan{idx}"),
                    location: location.to_string(),
                    config_path: entry.path().join("config"),
                    device_id,
                    subsystem_id,
                    firmware_family,
                    transport_status: transport_status_from_config(&config),
                    ucode_candidates,
                    selected_ucode,
                    pnvm_candidate,
                    pnvm_found,
                    prepared: false,
                    transport_initialized: false,
                    activated: false,
                    connect_result: "connect=not-run".to_string(),
                    disconnect_result: "disconnect=not-run".to_string(),
                },
            );
        }
    }

    devices.into_values().collect()
}

fn intel_firmware_candidates(
    device_id: u16,
    subsystem_id: u16,
) -> (&'static str, Vec<String>, Option<String>) {
    let (stems, pnvm): (Vec<&'static str>, Option<&'static str>) = match (device_id, subsystem_id) {
        (0x7740, 0x4090) => (
            vec![
                "iwlwifi-bz-b0-gf-a0-92.ucode",
                "iwlwifi-bz-b0-gf-a0-94.ucode",
                "iwlwifi-bz-b0-gf-a0-100.ucode",
            ],
            Some("iwlwifi-bz-b0-gf-a0.pnvm"),
        ),
        (0x7740, _) => (
            vec![
                "iwlwifi-bz-b0-fm-c0-92.ucode",
                "iwlwifi-bz-b0-fm-c0-94.ucode",
                "iwlwifi-bz-b0-fm-c0-100.ucode",
            ],
            Some("iwlwifi-bz-b0-fm-c0.pnvm"),
        ),
        (0x2725, _) => (
            vec![
                "iwlwifi-ty-a0-gf-a0-59.ucode",
                "iwlwifi-ty-a0-gf-a0-84.ucode",
            ],
            Some("iwlwifi-ty-a0-gf-a0.pnvm"),
        ),
        (0x7af0, 0x4090) => (
            vec![
                "iwlwifi-so-a0-gf-a0-64.ucode",
                "iwlwifi-so-a0-gf-a0-66.ucode",
            ],
            Some("iwlwifi-so-a0-gf-a0.pnvm"),
        ),
        (0x7af0, 0x4070) => (
            vec!["iwlwifi-so-a0-hr-b0-64.ucode"],
            Some("iwlwifi-so-a0-hr-b0.pnvm"),
        ),
        (0x7af0, 0x0aaa) | (0x7af0, 0x0030) => (
            vec![
                "iwlwifi-so-a0-jf-b0-64.ucode",
                "iwlwifi-9000-pu-b0-jf-b0-46.ucode",
            ],
            Some("iwlwifi-so-a0-jf-b0.pnvm"),
        ),
        _ => (vec!["iwlwifi-unknown"], None),
    };

    let family = match (device_id, subsystem_id) {
        (0x7740, _) => "intel-bz-arrow-lake",
        (0x2725, _) => "intel-ax210",
        (0x7af0, 0x4090) => "intel-ax211",
        (0x7af0, 0x4070) => "intel-ax201",
        (0x7af0, 0x0aaa) | (0x7af0, 0x0030) => "intel-9462-9560",
        _ => "intel-unknown",
    };

    (
        family,
        stems.into_iter().map(str::to_string).collect(),
        pnvm.map(str::to_string),
    )
}

fn transport_status_from_config(config: &[u8]) -> String {
    let command = u16::from_le_bytes([config[0x04], config[0x05]]);
    let bar0 = u32::from_le_bytes([config[0x10], config[0x11], config[0x12], config[0x13]]);
    let irq_pin = config[0x3D];

    let memory_enabled = (command & 0x2) != 0;
    let bus_master = (command & 0x4) != 0;
    let bar_present = bar0 != 0;
    let irq_present = irq_pin != 0;

    format!(
        "transport=pci memory_enabled={} bus_master={} bar0_present={} irq_pin_present={}",
        if memory_enabled { "yes" } else { "no" },
        if bus_master { "yes" } else { "no" },
        if bar_present { "yes" } else { "no" },
        if irq_present { "yes" } else { "no" }
    )
}

#[cfg(target_os = "redox")]
const IWL_CSR_HW_IF_CONFIG_REG: usize = 0x000;
#[cfg(target_os = "redox")]
const IWL_CSR_RESET: usize = 0x020;
#[cfg(target_os = "redox")]
const IWL_CSR_GP_CNTRL: usize = 0x024;
#[cfg(target_os = "redox")]
const IWL_CSR_HW_REV: usize = 0x028;
#[cfg(target_os = "redox")]
const IWL_CSR_GP_CNTRL_REG_FLAG_MAC_ACCESS_REQ: u32 = 0x00000008;
#[cfg(target_os = "redox")]
const IWL_CSR_GP_CNTRL_REG_FLAG_BZ_MAC_ACCESS_REQ: u32 = 0x00200000;
#[cfg(target_os = "redox")]
const IWL_CSR_GP_CNTRL_REG_FLAG_MAC_CLOCK_READY: u32 = 0x00000001;
#[cfg(target_os = "redox")]
const IWL_CSR_GP_CNTRL_REG_FLAG_INIT_DONE: u32 = 0x00000004;
#[cfg(target_os = "redox")]
const IWL_CSR_HW_IF_CONFIG_REG_BIT_NIC_READY: u32 = 0x00000004;
#[cfg(target_os = "redox")]
const IWL_CSR_GP_CNTRL_REG_FLAG_SW_RESET_BZ: u32 = 0x80000000;
#[cfg(target_os = "redox")]
const IWL_CSR_RESET_REG_FLAG_SW_RESET: u32 = 0x00000080;

fn parse_location_from_config_path(config_path: &PathBuf) -> Result<ParsedPciLocation, String> {
    let parent = config_path
        .parent()
        .ok_or_else(|| format!("missing PCI parent for {}", config_path.display()))?;
    let name = parent
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid PCI path {}", parent.display()))?;

    let parts: Vec<&str> = name.splitn(3, "--").collect();
    if parts.len() != 3 {
        return Err(format!("invalid PCI scheme entry {name}"));
    }
    let segment =
        u16::from_str_radix(parts[0], 16).map_err(|_| format!("invalid segment in {name}"))?;
    let bus = u8::from_str_radix(parts[1], 16).map_err(|_| format!("invalid bus in {name}"))?;
    let dev_func: Vec<&str> = parts[2].splitn(2, '.').collect();
    if dev_func.len() != 2 {
        return Err(format!("invalid device/function in {name}"));
    }
    let device =
        u8::from_str_radix(dev_func[0], 16).map_err(|_| format!("invalid device in {name}"))?;
    let function =
        u8::from_str_radix(dev_func[1], 16).map_err(|_| format!("invalid function in {name}"))?;

    Ok(ParsedPciLocation {
        segment,
        bus,
        device,
        function,
    })
}

#[cfg(target_os = "redox")]
fn transport_status_after_prepare(config_path: &PathBuf, family: &str) -> Result<String, String> {
    let location: PciLocation = parse_location_from_config_path(config_path)?.into();
    let mut pci = PciDevice::open_location(&location)
        .map_err(|err| format!("failed to reopen PCI device {location}: {err}"))?;
    let info = pci
        .full_info()
        .map_err(|err| format!("failed to read PCI info for {location}: {err}"))?;
    let bar = info
        .find_memory_bar(0)
        .ok_or_else(|| format!("no memory BAR0 for {location}"))?;
    let (addr, size) = bar
        .memory_info()
        .ok_or_else(|| format!("invalid BAR0 mapping info for {location}"))?;
    let mmio = pci
        .map_bar(0, addr, size)
        .map_err(|err| format!("failed to map BAR0 for {location}: {err}"))?;
    let reg0 = mmio.read32(0);
    let hw_rev = mmio.read32(IWL_CSR_HW_REV);
    let gp_before = mmio.read32(IWL_CSR_GP_CNTRL);
    let access_req = if family.starts_with("intel-bz-") {
        IWL_CSR_GP_CNTRL_REG_FLAG_BZ_MAC_ACCESS_REQ
    } else {
        IWL_CSR_GP_CNTRL_REG_FLAG_MAC_ACCESS_REQ
    };
    mmio.write32(IWL_CSR_GP_CNTRL, gp_before | access_req);
    let gp_after = mmio.read32(IWL_CSR_GP_CNTRL);
    let hw_if = mmio.read32(IWL_CSR_HW_IF_CONFIG_REG);
    let mac_clock = (gp_after & IWL_CSR_GP_CNTRL_REG_FLAG_MAC_CLOCK_READY) != 0;
    let nic_ready = (hw_if & IWL_CSR_HW_IF_CONFIG_REG_BIT_NIC_READY) != 0;
    Ok(format!(
        "{} mmio_probe=ok reg0=0x{reg0:08x} hw_rev=0x{hw_rev:08x} mac_access_req={} mac_clock_ready={} nic_ready={}",
        read_transport_status(config_path)?,
        if family.starts_with("intel-bz-") { "bz" } else { "legacy" },
        if mac_clock { "yes" } else { "no" },
        if nic_ready { "yes" } else { "no" }
    ))
}

#[cfg(not(target_os = "redox"))]
fn transport_status_after_prepare(config_path: &PathBuf, _family: &str) -> Result<String, String> {
    Ok(format!(
        "{} mmio_probe=host-skipped",
        read_transport_status(config_path)?
    ))
}

#[cfg(target_os = "redox")]
fn init_transport_action(config_path: &PathBuf, family: &str) -> Result<String, String> {
    let location: PciLocation = parse_location_from_config_path(config_path)?.into();
    let mut pci = PciDevice::open_location(&location)
        .map_err(|err| format!("failed to reopen PCI device {location}: {err}"))?;
    let info = pci
        .full_info()
        .map_err(|err| format!("failed to read PCI info for {location}: {err}"))?;
    let bar = info
        .find_memory_bar(0)
        .ok_or_else(|| format!("no memory BAR0 for {location}"))?;
    let (addr, size) = bar
        .memory_info()
        .ok_or_else(|| format!("invalid BAR0 mapping info for {location}"))?;
    let mmio = pci
        .map_bar(0, addr, size)
        .map_err(|err| format!("failed to map BAR0 for {location}: {err}"))?;

    let gp_before = mmio.read32(IWL_CSR_GP_CNTRL);
    let access_req = if family.starts_with("intel-bz-") {
        IWL_CSR_GP_CNTRL_REG_FLAG_BZ_MAC_ACCESS_REQ
    } else {
        IWL_CSR_GP_CNTRL_REG_FLAG_MAC_ACCESS_REQ
    };
    mmio.write32(IWL_CSR_GP_CNTRL, gp_before | access_req);
    if family.starts_with("intel-bz-") {
        let gp_reset = mmio.read32(IWL_CSR_GP_CNTRL);
        mmio.write32(
            IWL_CSR_GP_CNTRL,
            gp_reset | IWL_CSR_GP_CNTRL_REG_FLAG_SW_RESET_BZ,
        );
    } else {
        let reset_before = mmio.read32(IWL_CSR_RESET);
        mmio.write32(
            IWL_CSR_RESET,
            reset_before | IWL_CSR_RESET_REG_FLAG_SW_RESET,
        );
    }
    let hw_if_before = mmio.read32(IWL_CSR_HW_IF_CONFIG_REG);
    mmio.write32(
        IWL_CSR_HW_IF_CONFIG_REG,
        hw_if_before | IWL_CSR_HW_IF_CONFIG_REG_BIT_NIC_READY,
    );
    let hw_if_after = mmio.read32(IWL_CSR_HW_IF_CONFIG_REG);
    let nic_ready = (hw_if_after & IWL_CSR_HW_IF_CONFIG_REG_BIT_NIC_READY) != 0;
    let base = transport_status_after_prepare(config_path, family)?;
    Ok(format!(
        "{} init_hw_if_before=0x{hw_if_before:08x} init_hw_if_after=0x{hw_if_after:08x} nic_ready_write={} reset_method={}",
        base,
        if nic_ready { "yes" } else { "no" },
        if family.starts_with("intel-bz-") { "gp-cntrl-sw-reset" } else { "csr-reset-sw-reset" }
    ))
}

#[cfg(not(target_os = "redox"))]
fn init_transport_action(config_path: &PathBuf, family: &str) -> Result<String, String> {
    Ok(transport_status_after_prepare(config_path, family)?)
}

#[cfg(target_os = "redox")]
fn activate_nic_action(config_path: &PathBuf) -> Result<String, String> {
    let location: PciLocation = parse_location_from_config_path(config_path)?.into();
    let mut pci = PciDevice::open_location(&location)
        .map_err(|err| format!("failed to reopen PCI device {location}: {err}"))?;
    let info = pci
        .full_info()
        .map_err(|err| format!("failed to read PCI info for {location}: {err}"))?;
    let bar = info
        .find_memory_bar(0)
        .ok_or_else(|| format!("no memory BAR0 for {location}"))?;
    let (addr, size) = bar
        .memory_info()
        .ok_or_else(|| format!("invalid BAR0 mapping info for {location}"))?;
    let mmio = pci
        .map_bar(0, addr, size)
        .map_err(|err| format!("failed to map BAR0 for {location}: {err}"))?;

    let gp_before = mmio.read32(IWL_CSR_GP_CNTRL);
    mmio.write32(
        IWL_CSR_GP_CNTRL,
        gp_before | IWL_CSR_GP_CNTRL_REG_FLAG_INIT_DONE,
    );
    let gp_after = mmio.read32(IWL_CSR_GP_CNTRL);
    let init_done = (gp_after & IWL_CSR_GP_CNTRL_REG_FLAG_INIT_DONE) != 0;
    let mac_clock = (gp_after & IWL_CSR_GP_CNTRL_REG_FLAG_MAC_CLOCK_READY) != 0;

    Ok(format!(
        "activation=ok init_done={} mac_clock_ready={}",
        if init_done { "yes" } else { "no" },
        if mac_clock { "yes" } else { "no" }
    ))
}

#[cfg(not(target_os = "redox"))]
fn activate_nic_action(_config_path: &PathBuf) -> Result<String, String> {
    Ok("activation=host-skipped".to_string())
}

fn read_transport_status(config_path: &PathBuf) -> Result<String, String> {
    let config = fs::read(config_path)
        .map_err(|err| format!("failed to read PCI config {}: {err}", config_path.display()))?;
    if config.len() < 64 {
        return Err(format!(
            "PCI config too small at {}: expected at least 64 bytes",
            config_path.display()
        ));
    }
    Ok(transport_status_from_config(&config))
}

fn program_transport_bits(config_path: &PathBuf) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(config_path)
        .map_err(|err| format!("failed to open PCI config {}: {err}", config_path.display()))?;

    let mut command = [0u8; 2];
    file.seek(SeekFrom::Start(0x04))
        .map_err(|err| format!("failed to seek PCI command register: {err}"))?;
    file.read_exact(&mut command)
        .map_err(|err| format!("failed to read PCI command register: {err}"))?;

    let mut value = u16::from_le_bytes(command);
    value |= 0x0002; // memory space
    value |= 0x0004; // bus master

    file.seek(SeekFrom::Start(0x04))
        .map_err(|err| format!("failed to seek PCI command register for write: {err}"))?;
    file.write_all(&value.to_le_bytes())
        .map_err(|err| format!("failed to write PCI command register: {err}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!("{prefix}-{stamp}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn stub_backend_connects_with_wpa2() {
        let mut backend = StubBackend::from_env();
        let state = InterfaceState {
            ssid: "demo".to_string(),
            security: "wpa2-psk".to_string(),
            key: "secret".to_string(),
            ..Default::default()
        };
        assert_eq!(
            backend.connect("wlan0", &state).unwrap(),
            WifiStatus::Connected
        );
    }

    #[test]
    fn no_device_backend_exposes_no_interfaces() {
        let backend = NoDeviceBackend::new();
        assert!(backend.interfaces().is_empty());
        assert_eq!(backend.initial_status("wlan0"), WifiStatus::Down);
        assert_eq!(backend.initial_link_state("wlan0"), "link=no-device");
        assert!(backend
            .capabilities()
            .iter()
            .any(|line| line == "backend=no-device"));
    }

    #[test]
    fn intel_backend_detects_wifi_controller() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let pci = temp_root("rbos-wifictl-pci");
        let firmware = temp_root("rbos-wifictl-fw");
        let slot = pci.join("0000--00--14.3");
        fs::create_dir_all(&slot).unwrap();
        let mut cfg = vec![0u8; 64];
        cfg[0x00] = 0x86;
        cfg[0x01] = 0x80;
        cfg[0x02] = 0x40;
        cfg[0x03] = 0x77;
        cfg[0x0A] = 0x80;
        cfg[0x0B] = 0x02;
        cfg[0x10] = 0x01;
        cfg[0x2E] = 0x90;
        cfg[0x2F] = 0x40;
        cfg[0x3D] = 0x01;
        fs::write(slot.join("config"), cfg).unwrap();
        fs::write(firmware.join("iwlwifi-bz-b0-gf-a0-92.ucode"), []).unwrap();

        unsafe {
            env::set_var("REDBEAR_WIFICTL_PCI_ROOT", &pci);
            env::set_var("REDBEAR_WIFICTL_FIRMWARE_ROOT", &firmware);
            env::remove_var("REDBEAR_IWLWIFI_CMD");
        }
        fs::write(firmware.join("iwlwifi-bz-b0-gf-a0.pnvm"), []).unwrap();
        let mut backend = IntelBackend::from_env();
        assert_eq!(backend.interfaces(), vec!["wlan0".to_string()]);
        assert_eq!(backend.initial_status("wlan0"), WifiStatus::DeviceDetected);
        assert_eq!(backend.prepare("wlan0").unwrap(), WifiStatus::FirmwareReady);
        assert!(backend
            .capabilities()
            .iter()
            .any(|line| line.contains("ucode_selected=iwlwifi-bz-b0-gf-a0-92.ucode")));
        assert!(backend
            .transport_status("wlan0")
            .contains("memory_enabled=yes"));
        assert!(backend.firmware_status("wlan0").contains("prepared=yes"));
        assert!(backend
            .transport_status("wlan0")
            .contains("memory_enabled=yes"));
        assert!(backend.transport_status("wlan0").contains("bus_master=yes"));
    }

    #[test]
    fn intel_backend_transport_probe_does_not_use_init_transport_action() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let pci = temp_root("rbos-wifictl-pci-probe");
        let firmware = temp_root("rbos-wifictl-fw-probe");
        let slot = pci.join("0000--00--14.3");
        fs::create_dir_all(&slot).unwrap();
        let mut cfg = vec![0u8; 64];
        cfg[0x00] = 0x86;
        cfg[0x01] = 0x80;
        cfg[0x02] = 0x40;
        cfg[0x03] = 0x77;
        cfg[0x0A] = 0x80;
        cfg[0x0B] = 0x02;
        cfg[0x10] = 0x01;
        cfg[0x2E] = 0x90;
        cfg[0x2F] = 0x40;
        cfg[0x3D] = 0x01;
        fs::write(slot.join("config"), cfg).unwrap();
        fs::write(firmware.join("iwlwifi-bz-b0-gf-a0-92.ucode"), []).unwrap();
        fs::write(firmware.join("iwlwifi-bz-b0-gf-a0.pnvm"), []).unwrap();

        let driver = temp_root("rbos-wifictl-driver").join("redbear-iwlwifi-mock.sh");
        fs::write(
            &driver,
            r##"#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  --transport-probe)
    printf 'transport_status=transport=mock-probe-only\n'
    ;;
  --init-transport)
    printf 'transport_status=transport=mock-init-path\n'
    ;;
  *)
    printf 'status=unexpected-action\n'
    ;;
esac
"##,
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&driver).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&driver, perms).unwrap();
        }

        let old_cmd = env::var_os("REDBEAR_IWLWIFI_CMD");
        unsafe {
            env::set_var("REDBEAR_WIFICTL_PCI_ROOT", &pci);
            env::set_var("REDBEAR_WIFICTL_FIRMWARE_ROOT", &firmware);
            env::set_var("REDBEAR_IWLWIFI_CMD", &driver);
        }

        let mut backend = IntelBackend::from_env();
        let transport_status = backend.transport_probe("wlan0").unwrap();
        assert_eq!(transport_status, "transport=mock-probe-only");

        unsafe {
            if let Some(old_cmd) = old_cmd {
                env::set_var("REDBEAR_IWLWIFI_CMD", old_cmd);
            } else {
                env::remove_var("REDBEAR_IWLWIFI_CMD");
            }
        }
    }

    #[test]
    fn intel_backend_connect_uses_driver_connect_action() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let pci = temp_root("rbos-wifictl-pci-connect");
        let firmware = temp_root("rbos-wifictl-fw-connect");
        let slot = pci.join("0000--00--14.3");
        fs::create_dir_all(&slot).unwrap();
        let mut cfg = vec![0u8; 64];
        cfg[0x00] = 0x86;
        cfg[0x01] = 0x80;
        cfg[0x02] = 0x40;
        cfg[0x03] = 0x77;
        cfg[0x0A] = 0x80;
        cfg[0x0B] = 0x02;
        cfg[0x10] = 0x01;
        cfg[0x2E] = 0x90;
        cfg[0x2F] = 0x40;
        cfg[0x3D] = 0x01;
        fs::write(slot.join("config"), cfg).unwrap();
        fs::write(firmware.join("iwlwifi-bz-b0-gf-a0-92.ucode"), []).unwrap();
        fs::write(firmware.join("iwlwifi-bz-b0-gf-a0.pnvm"), []).unwrap();

        let driver = temp_root("rbos-wifictl-driver-connect").join("redbear-iwlwifi-mock.sh");
        fs::write(
            &driver,
            r##"#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  --prepare)
    printf 'status=firmware-ready\n'
    printf 'transport_status=transport=prepared\n'
    ;;
  --init-transport)
    printf 'transport_status=transport=init\n'
    ;;
  --activate-nic)
    printf 'activation=ok\n'
    printf 'transport_status=transport=active\n'
    ;;
  --connect)
    printf 'status=associated\n'
    printf 'connect_result=mock-associated ssid=%s security=%s\n' "${3:-}" "${4:-}"
    ;;
  *)
    printf 'status=unexpected-action\n'
    ;;
esac
"##,
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&driver).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&driver, perms).unwrap();
        }

        let old_cmd = env::var_os("REDBEAR_IWLWIFI_CMD");
        unsafe {
            env::set_var("REDBEAR_WIFICTL_PCI_ROOT", &pci);
            env::set_var("REDBEAR_WIFICTL_FIRMWARE_ROOT", &firmware);
            env::set_var("REDBEAR_IWLWIFI_CMD", &driver);
        }

        let mut backend = IntelBackend::from_env();
        assert_eq!(backend.prepare("wlan0").unwrap(), WifiStatus::FirmwareReady);
        assert_eq!(
            backend.init_transport("wlan0").unwrap(),
            "transport_init=ok"
        );
        assert_eq!(backend.activate("wlan0").unwrap(), "ok");

        let state = InterfaceState {
            ssid: "demo".to_string(),
            security: "wpa2-psk".to_string(),
            key: "secret".to_string(),
            ..Default::default()
        };
        assert_eq!(
            backend.connect("wlan0", &state).unwrap(),
            WifiStatus::Connected
        );

        unsafe {
            if let Some(old_cmd) = old_cmd {
                env::set_var("REDBEAR_IWLWIFI_CMD", old_cmd);
            } else {
                env::remove_var("REDBEAR_IWLWIFI_CMD");
            }
        }
    }
}
