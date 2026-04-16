use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SecurityKind {
    Open,
    Wpa2Psk,
}

impl SecurityKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Wpa2Psk => "wpa2-psk",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::Open => Self::Wpa2Psk,
            Self::Wpa2Psk => Self::Open,
        }
    }

    pub fn previous(&self) -> Self {
        self.next()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IpMode {
    Bounded,
    Dhcp,
    Static,
}

impl IpMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bounded => "bounded",
            Self::Dhcp => "dhcp",
            Self::Static => "static",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::Bounded => Self::Dhcp,
            Self::Dhcp => Self::Static,
            Self::Static => Self::Bounded,
        }
    }

    pub fn previous(&self) -> Self {
        match self {
            Self::Bounded => Self::Static,
            Self::Dhcp => Self::Bounded,
            Self::Static => Self::Dhcp,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    pub description: String,
    pub interface: String,
    pub ssid: String,
    pub security: SecurityKind,
    pub key: String,
    pub ip_mode: IpMode,
    pub address: String,
    pub gateway: String,
    pub dns: String,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: "wifi-profile".to_string(),
            description: "Red Bear Wi-Fi profile".to_string(),
            interface: "wlan0".to_string(),
            ssid: String::new(),
            security: SecurityKind::Open,
            key: String::new(),
            ip_mode: IpMode::Bounded,
            address: String::new(),
            gateway: String::new(),
            dns: String::new(),
        }
    }
}

impl Profile {
    pub fn validate(&self) -> Result<(), String> {
        validate_profile_name(&self.name)?;
        validate_scalar("interface", &self.interface)?;
        validate_scalar("ssid", &self.ssid)?;
        validate_scalar("description", &self.description)?;

        if matches!(self.security, SecurityKind::Wpa2Psk) && self.key.trim().is_empty() {
            return Err("WPA2-PSK profiles require a key".to_string());
        }

        if matches!(self.ip_mode, IpMode::Static) && self.address.trim().is_empty() {
            return Err("static profiles require an address".to_string());
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WifiRuntimeState {
    pub interface: String,
    pub address: String,
    pub status: String,
    pub link_state: String,
    pub firmware_status: String,
    pub transport_status: String,
    pub transport_init_status: String,
    pub activation_status: String,
    pub connect_result: String,
    pub disconnect_result: String,
    pub last_error: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanResult {
    pub raw: String,
    pub ssid: String,
    pub security_hint: Option<SecurityKind>,
}

impl ScanResult {
    pub fn label(&self) -> String {
        match self.security_hint {
            Some(SecurityKind::Open) => format!("{} [open]", self.ssid),
            Some(SecurityKind::Wpa2Psk) => format!("{} [wpa2-psk]", self.ssid),
            None => self.ssid.clone(),
        }
    }
}

pub trait ConsoleBackend {
    fn list_wifi_profiles(&self) -> Result<Vec<String>, String>;
    fn active_profile_name(&self) -> Result<Option<String>, String>;
    fn load_profile(&self, name: &str) -> Result<Profile, String>;
    fn save_profile(&self, profile: &Profile) -> Result<(), String>;
    fn set_active_profile(&self, profile_name: &str) -> Result<(), String>;
    fn clear_active_profile(&self) -> Result<(), String>;
    fn read_status(&self, interface: &str) -> WifiRuntimeState;
    fn scan(&self, interface: &str) -> Result<Vec<ScanResult>, String>;
    fn connect(&self, profile: &Profile) -> Result<String, String>;
    fn disconnect(&self, profile_name: Option<&str>, interface: &str) -> Result<String, String>;
}

#[derive(Clone, Debug)]
pub struct RuntimePaths {
    pub profile_dir: PathBuf,
    pub active_profile_path: PathBuf,
    pub wifictl_root: PathBuf,
    pub netcfg_root: PathBuf,
    pub dhcpd_command: String,
    pub dhcp_wait_timeout: Duration,
    pub dhcp_poll_interval: Duration,
}

impl RuntimePaths {
    pub fn from_env() -> Self {
        let profile_dir = env::var_os("REDBEAR_NETCTL_PROFILE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/etc/netctl"));
        let active_profile_path = env::var_os("REDBEAR_NETCTL_ACTIVE")
            .map(PathBuf::from)
            .unwrap_or_else(|| profile_dir.join("active"));
        let wifictl_root = env::var_os("REDBEAR_WIFICTL_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/scheme/wifictl"));
        let netcfg_root = env::var_os("REDBEAR_NETCFG_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/scheme/netcfg"));
        let dhcpd_command = env::var("REDBEAR_DHCPD_CMD").unwrap_or_else(|_| "dhcpd".to_string());
        let dhcp_wait_timeout = env::var("REDBEAR_DHCPD_WAIT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or_else(|| Duration::from_millis(1000));
        let dhcp_poll_interval = env::var("REDBEAR_DHCPD_POLL_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or_else(|| Duration::from_millis(50));

        Self {
            profile_dir,
            active_profile_path,
            wifictl_root,
            netcfg_root,
            dhcpd_command,
            dhcp_wait_timeout,
            dhcp_poll_interval,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FsBackend {
    paths: RuntimePaths,
}

impl FsBackend {
    pub fn from_env() -> Self {
        Self {
            paths: RuntimePaths::from_env(),
        }
    }

    pub fn new(paths: RuntimePaths) -> Self {
        Self { paths }
    }

    fn ensure_profile_dir(&self) -> Result<(), String> {
        fs::create_dir_all(&self.paths.profile_dir).map_err(|err| {
            format!(
                "failed to prepare {}: {err}",
                self.paths.profile_dir.display()
            )
        })
    }

    fn profile_path(&self, name: &str) -> PathBuf {
        self.paths.profile_dir.join(name)
    }

    fn write_wifictl(&self, interface: &str, node: &str, value: &str) -> Result<(), String> {
        let iface_root = self.paths.wifictl_root.join("ifaces").join(interface);
        fs::create_dir_all(&iface_root)
            .map_err(|err| format!("failed to prepare {}: {err}", iface_root.display()))?;
        let path = iface_root.join(node);
        fs::write(&path, format!("{}\n", value.trim()))
            .map_err(|err| format!("failed to write {}: {err}", path.display()))
    }

    fn read_wifictl_value(&self, interface: &str, node: &str) -> Option<String> {
        fs::read_to_string(
            self.paths
                .wifictl_root
                .join("ifaces")
                .join(interface)
                .join(node),
        )
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    }

    fn write_netcfg(&self, node: &str, value: &str) -> Result<(), String> {
        let path = self.paths.netcfg_root.join(node);
        fs::write(&path, format!("{}\n", value.trim()))
            .map_err(|err| format!("failed to write {}: {err}", path.display()))
    }

    fn current_addr(&self, interface: &str) -> Option<String> {
        fs::read_to_string(
            self.paths
                .netcfg_root
                .join("ifaces")
                .join(interface)
                .join("addr")
                .join("list"),
        )
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    }

    fn wait_for_address(&self, interface: &str) -> Result<(), String> {
        let deadline = Instant::now() + self.paths.dhcp_wait_timeout;

        loop {
            match self.current_addr(interface).as_deref() {
                Some(addr) if addr != "Not configured" && !addr.is_empty() => return Ok(()),
                _ if Instant::now() >= deadline => {
                    return Err(format!("timed out waiting for DHCP address on {interface}"));
                }
                _ => thread::sleep(self.paths.dhcp_poll_interval),
            }
        }
    }

    fn checked_status(
        &self,
        interface: &str,
        node: &str,
        failed_value: &str,
        context: &str,
    ) -> Result<(), String> {
        if self.read_wifictl_value(interface, node).as_deref() == Some(failed_value)
            || self.read_wifictl_value(interface, "status").as_deref() == Some("failed")
        {
            let last_error = self
                .read_wifictl_value(interface, "last-error")
                .unwrap_or_else(|| format!("{context} failed"));
            return Err(format!("wifictl {context} failed: {last_error}"));
        }
        Ok(())
    }

    fn apply_profile(&self, profile: &Profile) -> Result<(), String> {
        profile.validate()?;
        self.write_wifictl(&profile.interface, "ssid", &profile.ssid)?;
        self.write_wifictl(&profile.interface, "security", profile.security.as_str())?;
        match profile.security {
            SecurityKind::Open => {
                self.write_wifictl(&profile.interface, "key", "")?;
            }
            SecurityKind::Wpa2Psk => {
                self.write_wifictl(&profile.interface, "key", &profile.key)?;
            }
        }

        self.write_wifictl(&profile.interface, "prepare", "1")?;
        self.checked_status(&profile.interface, "status", "failed", "prepare")?;

        self.write_wifictl(&profile.interface, "init-transport", "1")?;
        self.checked_status(
            &profile.interface,
            "transport-init-status",
            "transport_init=failed",
            "init-transport",
        )?;

        self.write_wifictl(&profile.interface, "activate-nic", "1")?;
        self.checked_status(
            &profile.interface,
            "activation-status",
            "activation=failed",
            "activate-nic",
        )?;

        self.write_wifictl(&profile.interface, "connect", "1")?;
        self.checked_status(&profile.interface, "status", "failed", "connect")?;

        match profile.ip_mode {
            IpMode::Bounded => {}
            IpMode::Dhcp => {
                let address = self.current_addr(&profile.interface);
                if address.is_none() || address.as_deref() == Some("Not configured") {
                    let _child = Command::new(&self.paths.dhcpd_command)
                        .arg(&profile.interface)
                        .spawn()
                        .map_err(|err| format!("failed to spawn dhcpd: {err}"))?;
                    self.wait_for_address(&profile.interface)?;
                }
            }
            IpMode::Static => {
                self.write_netcfg(
                    &format!("ifaces/{}/addr/set", profile.interface),
                    &profile.address,
                )?;
                if !profile.gateway.trim().is_empty() {
                    self.write_netcfg(
                        "route/add",
                        &format!("default via {}", profile.gateway.trim()),
                    )?;
                }
                if !profile.dns.trim().is_empty() {
                    self.write_netcfg("resolv/nameserver", &profile.dns)?;
                }
            }
        }

        Ok(())
    }
}

impl ConsoleBackend for FsBackend {
    fn list_wifi_profiles(&self) -> Result<Vec<String>, String> {
        self.ensure_profile_dir()?;
        let entries = fs::read_dir(&self.paths.profile_dir)
            .map_err(|err| format!("failed to read {}: {err}", self.paths.profile_dir.display()))?;
        let mut names = Vec::new();

        for entry in entries {
            let entry = entry.map_err(|err| format!("failed to read profile entry: {err}"))?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if name == "active" || name.starts_with('.') {
                continue;
            }

            if self.load_profile(name).is_ok() {
                names.push(name.to_string());
            }
        }

        names.sort();
        Ok(names)
    }

    fn active_profile_name(&self) -> Result<Option<String>, String> {
        match fs::read_to_string(&self.paths.active_profile_path) {
            Ok(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(trimmed.to_string()))
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(format!(
                "failed to read {}: {err}",
                self.paths.active_profile_path.display()
            )),
        }
    }

    fn load_profile(&self, name: &str) -> Result<Profile, String> {
        validate_profile_name(name)?;
        let path = self.profile_path(name);
        let content = fs::read_to_string(&path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        parse_profile(name, &content)
    }

    fn save_profile(&self, profile: &Profile) -> Result<(), String> {
        profile.validate()?;
        self.ensure_profile_dir()?;
        let path = self.profile_path(&profile.name);
        fs::write(&path, serialize_profile(profile)?)
            .map_err(|err| format!("failed to write {}: {err}", path.display()))
    }

    fn set_active_profile(&self, profile_name: &str) -> Result<(), String> {
        validate_profile_name(profile_name)?;
        self.ensure_profile_dir()?;
        fs::write(&self.paths.active_profile_path, format!("{profile_name}\n")).map_err(|err| {
            format!(
                "failed to write {}: {err}",
                self.paths.active_profile_path.display()
            )
        })
    }

    fn clear_active_profile(&self) -> Result<(), String> {
        match fs::remove_file(&self.paths.active_profile_path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(format!(
                "failed to remove {}: {err}",
                self.paths.active_profile_path.display()
            )),
        }
    }

    fn read_status(&self, interface: &str) -> WifiRuntimeState {
        let interface = if interface.trim().is_empty() {
            "wlan0".to_string()
        } else {
            interface.trim().to_string()
        };

        WifiRuntimeState {
            interface: interface.clone(),
            address: self
                .current_addr(&interface)
                .unwrap_or_else(|| "unconfigured".to_string()),
            status: self
                .read_wifictl_value(&interface, "status")
                .unwrap_or_else(|| "unknown".to_string()),
            link_state: self
                .read_wifictl_value(&interface, "link-state")
                .unwrap_or_else(|| "unknown".to_string()),
            firmware_status: self
                .read_wifictl_value(&interface, "firmware-status")
                .unwrap_or_else(|| "unknown".to_string()),
            transport_status: self
                .read_wifictl_value(&interface, "transport-status")
                .unwrap_or_else(|| "unknown".to_string()),
            transport_init_status: self
                .read_wifictl_value(&interface, "transport-init-status")
                .unwrap_or_else(|| "unknown".to_string()),
            activation_status: self
                .read_wifictl_value(&interface, "activation-status")
                .unwrap_or_else(|| "unknown".to_string()),
            connect_result: self
                .read_wifictl_value(&interface, "connect-result")
                .unwrap_or_else(|| "unknown".to_string()),
            disconnect_result: self
                .read_wifictl_value(&interface, "disconnect-result")
                .unwrap_or_else(|| "unknown".to_string()),
            last_error: self
                .read_wifictl_value(&interface, "last-error")
                .unwrap_or_else(|| "none".to_string()),
        }
    }

    fn scan(&self, interface: &str) -> Result<Vec<ScanResult>, String> {
        validate_scalar("interface", interface)?;
        self.write_wifictl(interface, "prepare", "1")?;
        self.checked_status(interface, "status", "failed", "prepare")?;

        self.write_wifictl(interface, "init-transport", "1")?;
        self.checked_status(
            interface,
            "transport-init-status",
            "transport_init=failed",
            "init-transport",
        )?;

        self.write_wifictl(interface, "activate-nic", "1")?;
        self.checked_status(
            interface,
            "activation-status",
            "activation=failed",
            "activate-nic",
        )?;

        self.write_wifictl(interface, "scan", "1")?;
        let raw = fs::read_to_string(
            self.paths
                .wifictl_root
                .join("ifaces")
                .join(interface)
                .join("scan-results"),
        )
        .unwrap_or_default();

        let results = raw
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(parse_scan_result)
            .collect::<Vec<_>>();

        Ok(results)
    }

    fn connect(&self, profile: &Profile) -> Result<String, String> {
        self.save_profile(profile)?;
        self.apply_profile(profile)?;
        self.set_active_profile(&profile.name)?;
        Ok(format!(
            "applied {} via {}",
            profile.name, profile.interface
        ))
    }

    fn disconnect(&self, profile_name: Option<&str>, interface: &str) -> Result<String, String> {
        validate_scalar("interface", interface)?;
        self.write_wifictl(interface, "disconnect", "1")?;
        if let Some(name) = profile_name
            && self.active_profile_name()?.as_deref() == Some(name)
        {
            self.clear_active_profile()?;
        }

        Ok(format!("disconnected {}", interface.trim()))
    }
}

fn validate_profile_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("profile name is required".to_string());
    }
    if trimmed == "active" || trimmed == "." || trimmed == ".." || trimmed.contains('/') {
        return Err(format!("unsupported profile name {trimmed}"));
    }
    validate_text_value("profile name", trimmed)
}

fn validate_scalar(label: &str, value: &str) -> Result<(), String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{label} is required"));
    }
    validate_text_value(label, trimmed)
}

fn validate_text_value(label: &str, value: &str) -> Result<(), String> {
    if value.contains('\n') || value.contains('\r') {
        return Err(format!("{label} must be a single line"));
    }
    Ok(())
}

fn serialize_profile(profile: &Profile) -> Result<String, String> {
    let mut lines = vec![
        format!("Description={}", quote_value(&profile.description)?),
        format!("Interface={}", quote_value(&profile.interface)?),
        "Connection=wifi".to_string(),
        format!("SSID={}", quote_value(&profile.ssid)?),
        format!("Security={}", profile.security.as_str()),
    ];

    if matches!(profile.security, SecurityKind::Wpa2Psk) {
        lines.push(format!("Key={}", quote_value(&profile.key)?));
    }

    lines.push(format!("IP={}", profile.ip_mode.as_str()));

    if matches!(profile.ip_mode, IpMode::Static) {
        lines.push(format!("Address=({})", quote_value(&profile.address)?));
        if !profile.gateway.trim().is_empty() {
            lines.push(format!("Gateway={}", quote_value(&profile.gateway)?));
        }
        if !profile.dns.trim().is_empty() {
            lines.push(format!("DNS=({})", quote_value(&profile.dns)?));
        }
    }

    Ok(lines.join("\n") + "\n")
}

fn quote_value(value: &str) -> Result<String, String> {
    validate_text_value("value", value)?;
    if !value.contains('\'') {
        return Ok(format!("'{}'", value.trim()));
    }
    if !value.contains('"') {
        return Ok(format!("\"{}\"", value.trim()));
    }
    Err("values containing both quote styles are not supported yet".to_string())
}

fn parse_profile(name: &str, content: &str) -> Result<Profile, String> {
    let mut profile = Profile {
        name: name.to_string(),
        ..Profile::default()
    };
    let mut connection = None;
    let mut ip_mode = None;
    let mut saw_ssid = false;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        match key.trim() {
            "Description" => profile.description = parse_scalar(value),
            "Interface" => profile.interface = parse_scalar(value),
            "Connection" => connection = Some(parse_scalar(value).to_ascii_lowercase()),
            "SSID" => {
                saw_ssid = true;
                profile.ssid = parse_scalar(value);
            }
            "Security" => {
                profile.security = match parse_scalar(value).to_ascii_lowercase().as_str() {
                    "open" => SecurityKind::Open,
                    "wpa2-psk" => SecurityKind::Wpa2Psk,
                    other => return Err(format!("unsupported Security={other}")),
                };
            }
            "Key" | "Passphrase" => profile.key = parse_scalar(value),
            "IP" => {
                ip_mode = Some(match parse_scalar(value).to_ascii_lowercase().as_str() {
                    "bounded" | "none" => IpMode::Bounded,
                    "dhcp" => IpMode::Dhcp,
                    "static" => IpMode::Static,
                    other => return Err(format!("unsupported IP={other}")),
                });
            }
            "Address" => profile.address = parse_first_array_item(value).unwrap_or_default(),
            "Gateway" => profile.gateway = parse_scalar(value),
            "DNS" => profile.dns = parse_first_array_item(value).unwrap_or_default(),
            _ => {}
        }
    }

    match connection.as_deref() {
        Some("wifi") => {}
        Some(other) => {
            return Err(format!(
                "profile {name} is not a Wi-Fi profile: Connection={other}"
            ));
        }
        None => return Err(format!("profile {name} is missing Connection=")),
    }

    profile.ip_mode = ip_mode.ok_or_else(|| format!("profile {name} is missing IP="))?;

    if !saw_ssid {
        return Err(format!("profile {name} is missing SSID="));
    }

    profile.validate()?;
    Ok(profile)
}

fn parse_scan_result(line: &str) -> ScanResult {
    let mut ssid = None;
    let mut security_hint = None;

    for token in line.split_whitespace() {
        let Some((key, value)) = token.split_once('=') else {
            continue;
        };
        match key {
            "ssid" => ssid = Some(parse_scalar(value)),
            "security" => {
                security_hint = match parse_scalar(value).to_ascii_lowercase().as_str() {
                    "open" => Some(SecurityKind::Open),
                    "wpa2-psk" => Some(SecurityKind::Wpa2Psk),
                    _ => None,
                };
            }
            _ => {}
        }
    }

    if security_hint.is_none() {
        let lowercase = line.to_ascii_lowercase();
        if lowercase.contains("wpa2-psk") {
            security_hint = Some(SecurityKind::Wpa2Psk);
        } else if lowercase.contains("open") {
            security_hint = Some(SecurityKind::Open);
        }
    }

    ScanResult {
        raw: line.to_string(),
        ssid: ssid.unwrap_or_else(|| line.trim().to_string()),
        security_hint,
    }
}

fn parse_scalar(value: &str) -> String {
    let trimmed = value.trim();
    trimmed
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

fn parse_first_array_item(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.starts_with('(') && trimmed.ends_with(')') {
        let inner = &trimmed[1..trimmed.len().saturating_sub(1)];
        inner
            .split_whitespace()
            .next()
            .map(parse_scalar)
            .filter(|item| !item.is_empty())
    } else {
        let item = parse_scalar(trimmed);
        (!item.is_empty()).then_some(item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wifi_profile_with_static_fields() {
        let profile = parse_profile(
            "wifi-static",
            "Description='Wi-Fi'\nInterface=wlan0\nConnection=wifi\nSSID='demo'\nSecurity=wpa2-psk\nKey='secret'\nIP=static\nAddress=('192.168.1.10/24')\nGateway='192.168.1.1'\nDNS=('1.1.1.1')\n",
        )
        .unwrap();

        assert_eq!(profile.name, "wifi-static");
        assert_eq!(profile.interface, "wlan0");
        assert_eq!(profile.ssid, "demo");
        assert_eq!(profile.security, SecurityKind::Wpa2Psk);
        assert_eq!(profile.key, "secret");
        assert_eq!(profile.ip_mode, IpMode::Static);
        assert_eq!(profile.address, "192.168.1.10/24");
        assert_eq!(profile.gateway, "192.168.1.1");
        assert_eq!(profile.dns, "1.1.1.1");
    }

    #[test]
    fn serializes_round_trip_profile() {
        let profile = Profile {
            name: "wifi-open-bounded".to_string(),
            description: "Wi-Fi bounded".to_string(),
            interface: "wlan0".to_string(),
            ssid: "demo-open".to_string(),
            security: SecurityKind::Open,
            key: String::new(),
            ip_mode: IpMode::Bounded,
            address: String::new(),
            gateway: String::new(),
            dns: String::new(),
        };

        let serialized = serialize_profile(&profile).unwrap();
        let parsed = parse_profile(&profile.name, &serialized).unwrap();
        assert_eq!(parsed, profile);
    }

    #[test]
    fn parses_scan_result_hints() {
        let open = parse_scan_result("ssid=demo-open security=open");
        assert_eq!(open.ssid, "demo-open");
        assert_eq!(open.security_hint, Some(SecurityKind::Open));

        let raw = parse_scan_result("demo-wpa2-network");
        assert_eq!(raw.ssid, "demo-wpa2-network");
        assert_eq!(raw.security_hint, None);
    }
}
