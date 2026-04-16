use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{self, Command};
use std::thread;
use std::time::{Duration, Instant};

fn program_name() -> String {
    env::args()
        .next()
        .and_then(|path| {
            PathBuf::from(path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "netctl".to_string())
}

fn usage() -> String {
    format!(
        "Usage: {} [--boot|list|status [profile]|scan <profile|iface>|retry <profile|iface>|start <profile>|stop <profile>|enable <profile>|disable [profile]|is-enabled [profile]]",
        program_name()
    )
}

#[derive(Clone, Debug)]
enum ProfileIpMode {
    Bounded,
    Dhcp,
    Static {
        address: String,
        gateway: Option<String>,
        dns: Option<String>,
    },
}

#[derive(Clone, Debug)]
enum WifiSecurity {
    Open,
    Wpa2Psk { key: String },
}

#[derive(Clone, Debug)]
struct WifiSettings {
    ssid: String,
    security: WifiSecurity,
}

#[derive(Clone, Debug)]
enum ConnectionMode {
    Ethernet,
    Wifi(WifiSettings),
}

#[derive(Clone, Debug)]
struct Profile {
    name: String,
    interface: String,
    connection: ConnectionMode,
    ip_mode: ProfileIpMode,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{}: {err}", program_name());
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        return Err(usage());
    };

    match command.as_str() {
        "--boot" => run_boot_profile(),
        "list" => list_profiles(),
        "status" => status(args.next().as_deref()),
        "scan" => scan_wifi(&required_profile(args.next())?),
        "retry" => retry_wifi(&required_profile(args.next())?),
        "start" => start_profile(&required_profile(args.next())?, false),
        "stop" => stop_profile(&required_profile(args.next())?),
        "enable" => enable_profile(&required_profile(args.next())?),
        "disable" => disable_profile(args.next().as_deref()),
        "is-enabled" => is_enabled(args.next().as_deref()),
        "help" | "--help" | "-h" => {
            println!("{}", usage());
            Ok(())
        }
        _ => Err(usage()),
    }
}

fn required_profile(profile: Option<String>) -> Result<String, String> {
    profile.ok_or_else(usage)
}

fn run_boot_profile() -> Result<(), String> {
    let Some(active) = active_profile_name()? else {
        return Ok(());
    };
    start_profile(&active, true)
}

fn list_profiles() -> Result<(), String> {
    let mut entries = profile_names()?;
    entries.sort();
    for entry in entries {
        println!("{entry}");
    }
    Ok(())
}

fn status(profile: Option<&str>) -> Result<(), String> {
    let active = active_profile_name()?;
    let selected = profile.map(str::to_string).or(active.clone());

    match selected {
        Some(name) => {
            let loaded = load_profile(&name)?;
            let enabled = active.as_deref() == Some(name.as_str());
            let address = current_addr(&loaded.interface).unwrap_or_else(|| "unconfigured".into());
            let connection = connection_name(&loaded.connection);
            match &loaded.connection {
                ConnectionMode::Wifi(_) => {
                    let wifi_status = read_wifictl_value(&loaded.interface, "status")
                        .unwrap_or_else(|| "unknown".to_string());
                    let link_state = read_wifictl_value(&loaded.interface, "link-state")
                        .unwrap_or_else(|| "unknown".to_string());
                    let firmware_status = read_wifictl_value(&loaded.interface, "firmware-status")
                        .unwrap_or_else(|| "unknown".to_string());
                    let transport_status =
                        read_wifictl_value(&loaded.interface, "transport-status")
                            .unwrap_or_else(|| "unknown".to_string());
                    let transport_init_status =
                        read_wifictl_value(&loaded.interface, "transport-init-status")
                            .unwrap_or_else(|| "unknown".to_string());
                    let activation_status =
                        read_wifictl_value(&loaded.interface, "activation-status")
                            .unwrap_or_else(|| "unknown".to_string());
                    let connect_result = read_wifictl_value(&loaded.interface, "connect-result")
                        .unwrap_or_else(|| "unknown".to_string());
                    let disconnect_result =
                        read_wifictl_value(&loaded.interface, "disconnect-result")
                            .unwrap_or_else(|| "unknown".to_string());
                    let last_error = read_wifictl_value(&loaded.interface, "last-error")
                        .unwrap_or_else(|| "none".to_string());
                    println!(
                        "profile={} enabled={} connection={} interface={} address={} wifi_status={} link_state={} firmware_status={} transport_status={} transport_init_status={} activation_status={} connect_result={} disconnect_result={} last_error={}",
                        name,
                        if enabled { "yes" } else { "no" },
                        connection,
                        loaded.interface,
                        address,
                        wifi_status,
                        link_state,
                        firmware_status,
                        transport_status,
                        transport_init_status,
                        activation_status,
                        connect_result,
                        disconnect_result,
                        last_error
                    );
                }
                ConnectionMode::Ethernet => {
                    println!(
                        "profile={} enabled={} connection={} interface={} address={}",
                        name,
                        if enabled { "yes" } else { "no" },
                        connection,
                        loaded.interface,
                        address
                    );
                }
            }
        }
        None => {
            println!("profile=none enabled=no address=unconfigured");
        }
    }

    Ok(())
}

fn start_profile(name: &str, boot: bool) -> Result<(), String> {
    let profile = load_profile(name)?;
    apply_profile(&profile, boot)?;
    println!("started {}", profile.name);
    Ok(())
}

fn stop_profile(name: &str) -> Result<(), String> {
    if let Ok(profile) = load_profile(name) {
        if let ConnectionMode::Wifi(_) = profile.connection {
            write_wifictl(&profile.interface, "disconnect", "1")?;
        }
    }
    if active_profile_name()?.as_deref() == Some(name) {
        let _ = fs::remove_file(active_profile_path());
    }
    println!("stopped {}", name);
    Ok(())
}

fn enable_profile(name: &str) -> Result<(), String> {
    let profile = load_profile(name)?;
    let active_path = active_profile_path();
    fs::write(&active_path, format!("{}\n", profile.name))
        .map_err(|err| format!("failed to write {}: {err}", active_path.display()))?;
    println!("enabled {}", profile.name);
    Ok(())
}

fn disable_profile(profile: Option<&str>) -> Result<(), String> {
    if let Some(name) = profile {
        if active_profile_name()?.as_deref() != Some(name) {
            println!("disabled {}", name);
            return Ok(());
        }
    }

    let _ = fs::remove_file(active_profile_path());
    println!("disabled {}", profile.unwrap_or("active"));
    Ok(())
}

fn is_enabled(profile: Option<&str>) -> Result<(), String> {
    let active = active_profile_name()?;
    let enabled = match profile {
        Some(profile) => active.as_deref() == Some(profile),
        None => active.is_some(),
    };
    println!("{}", if enabled { "yes" } else { "no" });
    Ok(())
}

fn apply_profile(profile: &Profile, boot: bool) -> Result<(), String> {
    match &profile.connection {
        ConnectionMode::Ethernet => {}
        ConnectionMode::Wifi(wifi) => apply_wifi_profile(&profile.interface, wifi)?,
    }

    match &profile.ip_mode {
        ProfileIpMode::Bounded => {}
        ProfileIpMode::Dhcp => {
            if boot
                || current_addr(&profile.interface).as_deref() == Some("Not configured")
                || current_addr(&profile.interface).is_none()
            {
                let _child = Command::new(dhcpd_command())
                    .arg(&profile.interface)
                    .spawn()
                    .map_err(|err| format!("failed to spawn dhcpd: {err}"))?;
                wait_for_address(&profile.interface)?;
            }
        }
        ProfileIpMode::Static {
            address,
            gateway,
            dns,
        } => {
            write_netcfg(&format!("ifaces/{}/addr/set", profile.interface), address)?;
            if let Some(gateway) = gateway {
                write_netcfg("route/add", &format!("default via {gateway}"))?;
            }
            if let Some(dns) = dns {
                write_netcfg("resolv/nameserver", dns)?;
            }
        }
    }

    if !boot && active_profile_name()?.as_deref() == Some(profile.name.as_str()) {
        let active_path = active_profile_path();
        fs::write(&active_path, format!("{}\n", profile.name))
            .map_err(|err| format!("failed to update {}: {err}", active_path.display()))?;
    }

    Ok(())
}

fn ensure_runtime_surfaces_for(interface: &str) -> Result<(), String> {
    let addr_path = format!("{}/ifaces/{interface}/addr/list", netcfg_root().display());
    fs::read_to_string(&addr_path)
        .map(|_| ())
        .map_err(|err| format!("failed to access {addr_path}: {err}"))
}

fn current_addr(interface: &str) -> Option<String> {
    fs::read_to_string(format!(
        "{}/ifaces/{interface}/addr/list",
        netcfg_root().display()
    ))
    .ok()
    .map(|value| value.trim().to_string())
}

fn connection_name(connection: &ConnectionMode) -> &'static str {
    match connection {
        ConnectionMode::Ethernet => "ethernet",
        ConnectionMode::Wifi(_) => "wifi",
    }
}

fn scan_wifi(target: &str) -> Result<(), String> {
    let interface = match load_profile(target) {
        Ok(profile) => match profile.connection {
            ConnectionMode::Wifi(_) => profile.interface,
            ConnectionMode::Ethernet => {
                return Err(format!("profile {target} is not a Wi-Fi profile"));
            }
        },
        Err(_) => target.to_string(),
    };

    write_wifictl(&interface, "prepare", "1")?;
    if read_wifictl_value(&interface, "status").as_deref() == Some("failed") {
        let last_error = read_wifictl_value(&interface, "last-error")
            .unwrap_or_else(|| "prepare failed".to_string());
        return Err(format!("wifictl prepare failed: {last_error}"));
    }
    write_wifictl(&interface, "init-transport", "1")?;
    if read_wifictl_value(&interface, "transport-init-status").as_deref()
        == Some("transport_init=failed")
        || read_wifictl_value(&interface, "status").as_deref() == Some("failed")
    {
        let last_error = read_wifictl_value(&interface, "last-error")
            .unwrap_or_else(|| "transport init failed".to_string());
        return Err(format!("wifictl init-transport failed: {last_error}"));
    }
    write_wifictl(&interface, "activate-nic", "1")?;
    if read_wifictl_value(&interface, "activation-status").as_deref() == Some("activation=failed")
        || read_wifictl_value(&interface, "status").as_deref() == Some("failed")
    {
        let last_error = read_wifictl_value(&interface, "last-error")
            .unwrap_or_else(|| "activation failed".to_string());
        return Err(format!("wifictl activate-nic failed: {last_error}"));
    }
    write_wifictl(&interface, "scan", "1")?;
    let results = read_wifictl_value(&interface, "scan-results").unwrap_or_default();
    let status = read_wifictl_value(&interface, "status").unwrap_or_else(|| "unknown".to_string());
    let firmware_status =
        read_wifictl_value(&interface, "firmware-status").unwrap_or_else(|| "unknown".to_string());
    let transport_status =
        read_wifictl_value(&interface, "transport-status").unwrap_or_else(|| "unknown".to_string());
    let transport_init_status = read_wifictl_value(&interface, "transport-init-status")
        .unwrap_or_else(|| "unknown".to_string());
    let activation_status = read_wifictl_value(&interface, "activation-status")
        .unwrap_or_else(|| "unknown".to_string());

    println!(
        "interface={} status={} firmware_status={} transport_status={} transport_init_status={} activation_status={} scan_results={}",
        interface,
        status,
        firmware_status,
        transport_status,
        transport_init_status,
        activation_status,
        if results.is_empty() {
            "none".to_string()
        } else {
            results
        }
    );
    Ok(())
}

fn retry_wifi(target: &str) -> Result<(), String> {
    let interface = match load_profile(target) {
        Ok(profile) => match profile.connection {
            ConnectionMode::Wifi(_) => profile.interface,
            ConnectionMode::Ethernet => {
                return Err(format!("profile {target} is not a Wi-Fi profile"));
            }
        },
        Err(_) => target.to_string(),
    };

    write_wifictl(&interface, "retry", "1")?;
    let status = read_wifictl_value(&interface, "status").unwrap_or_else(|| "unknown".to_string());
    let link_state =
        read_wifictl_value(&interface, "link-state").unwrap_or_else(|| "unknown".to_string());
    let last_error =
        read_wifictl_value(&interface, "last-error").unwrap_or_else(|| "none".to_string());
    println!(
        "interface={} status={} link_state={} last_error={}",
        interface, status, link_state, last_error
    );
    Ok(())
}

fn apply_wifi_profile(interface: &str, wifi: &WifiSettings) -> Result<(), String> {
    let root = wifictl_root();
    let iface_root = root.join("ifaces").join(interface);
    fs::create_dir_all(&iface_root)
        .map_err(|err| format!("failed to prepare {}: {err}", iface_root.display()))?;

    write_wifictl(interface, "ssid", &wifi.ssid)?;
    match &wifi.security {
        WifiSecurity::Open => {
            write_wifictl(interface, "security", "open")?;
        }
        WifiSecurity::Wpa2Psk { key } => {
            write_wifictl(interface, "security", "wpa2-psk")?;
            write_wifictl(interface, "key", key)?;
        }
    }
    write_wifictl(interface, "prepare", "1")?;
    if read_wifictl_value(interface, "status").as_deref() == Some("failed") {
        let last_error = read_wifictl_value(interface, "last-error")
            .unwrap_or_else(|| "prepare failed".to_string());
        return Err(format!("wifictl prepare failed: {last_error}"));
    }
    write_wifictl(interface, "init-transport", "1")?;
    if read_wifictl_value(interface, "transport-init-status").as_deref()
        == Some("transport_init=failed")
        || read_wifictl_value(interface, "status").as_deref() == Some("failed")
    {
        let last_error = read_wifictl_value(interface, "last-error")
            .unwrap_or_else(|| "transport init failed".to_string());
        return Err(format!("wifictl init-transport failed: {last_error}"));
    }
    write_wifictl(interface, "activate-nic", "1")?;
    if read_wifictl_value(interface, "activation-status").as_deref() == Some("activation=failed")
        || read_wifictl_value(interface, "status").as_deref() == Some("failed")
    {
        let last_error = read_wifictl_value(interface, "last-error")
            .unwrap_or_else(|| "activation failed".to_string());
        return Err(format!("wifictl activate-nic failed: {last_error}"));
    }
    write_wifictl(interface, "connect", "1")?;
    if read_wifictl_value(interface, "status").as_deref() == Some("failed") {
        let last_error = read_wifictl_value(interface, "last-error")
            .unwrap_or_else(|| "connect failed".to_string());
        return Err(format!("wifictl connect failed: {last_error}"));
    }
    ensure_runtime_surfaces_for(interface)
}

fn write_wifictl(interface: &str, node: &str, value: &str) -> Result<(), String> {
    let path = wifictl_root().join("ifaces").join(interface).join(node);
    fs::write(&path, format!("{}\n", value.trim()))
        .map_err(|err| format!("failed to write {}: {err}", path.display()))
}

fn wifictl_root() -> PathBuf {
    env::var_os("REDBEAR_WIFICTL_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/scheme/wifictl"))
}

fn read_wifictl_value(interface: &str, node: &str) -> Option<String> {
    fs::read_to_string(wifictl_root().join("ifaces").join(interface).join(node))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn write_netcfg(node: &str, value: &str) -> Result<(), String> {
    let path = format!("{}/{node}", netcfg_root().display());
    fs::write(&path, format!("{}\n", value.trim()))
        .map_err(|err| format!("failed to write {path}: {err}"))
}

fn active_profile_name() -> Result<Option<String>, String> {
    let active_path = active_profile_path();
    match fs::read_to_string(&active_path) {
        Ok(value) => {
            let value = value.trim();
            if value.is_empty() {
                Ok(None)
            } else {
                Ok(Some(value.to_string()))
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(format!("failed to read {}: {err}", active_path.display())),
    }
}

fn profile_names() -> Result<Vec<String>, String> {
    let profile_dir = profile_dir();
    let entries = fs::read_dir(&profile_dir)
        .map_err(|err| format!("failed to read {}: {err}", profile_dir.display()))?;
    let mut names = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|err| format!("failed to read profile entry: {err}"))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name == "active" || name.starts_with('.') {
            continue;
        }
        names.push(name.to_string());
    }

    Ok(names)
}

fn load_profile(name: &str) -> Result<Profile, String> {
    let path = profile_path(name);
    let content = fs::read_to_string(&path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    parse_profile(name, &content)
}

fn profile_path(name: &str) -> PathBuf {
    profile_dir().join(name)
}

fn profile_dir() -> PathBuf {
    env::var_os("REDBEAR_NETCTL_PROFILE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/netctl"))
}

fn active_profile_path() -> PathBuf {
    env::var_os("REDBEAR_NETCTL_ACTIVE")
        .map(PathBuf::from)
        .unwrap_or_else(|| profile_dir().join("active"))
}

fn netcfg_root() -> PathBuf {
    env::var_os("REDBEAR_NETCFG_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/scheme/netcfg"))
}

fn dhcpd_command() -> String {
    env::var("REDBEAR_DHCPD_CMD").unwrap_or_else(|_| "dhcpd".to_string())
}

fn dhcp_wait_timeout() -> Duration {
    env::var("REDBEAR_DHCPD_WAIT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or_else(|| Duration::from_millis(1000))
}

fn dhcp_poll_interval() -> Duration {
    env::var("REDBEAR_DHCPD_POLL_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or_else(|| Duration::from_millis(50))
}

fn wait_for_address(interface: &str) -> Result<(), String> {
    let deadline = Instant::now() + dhcp_wait_timeout();
    let poll = dhcp_poll_interval();

    loop {
        match current_addr(interface).as_deref() {
            Some(addr) if addr != "Not configured" && !addr.is_empty() => return Ok(()),
            _ if Instant::now() >= deadline => {
                return Err(format!(
                    "timed out waiting for DHCP address on {}",
                    interface
                ));
            }
            _ => thread::sleep(poll),
        }
    }
}

fn parse_profile(name: &str, content: &str) -> Result<Profile, String> {
    let mut interface = None;
    let mut connection = None;
    let mut ip = None;
    let mut address = None;
    let mut gateway = None;
    let mut dns = None;
    let mut ssid = None;
    let mut security = None;
    let mut wifi_key = None;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();

        match key {
            "Description" => {}
            "Interface" => interface = Some(parse_scalar(value)),
            "Connection" => connection = Some(parse_scalar(value)),
            "IP" => ip = Some(parse_scalar(value)),
            "Address" => address = parse_first_array_item(value),
            "Gateway" => gateway = Some(parse_scalar(value)),
            "DNS" => dns = parse_first_array_item(value),
            "SSID" => ssid = Some(parse_scalar(value)),
            "Security" => security = Some(parse_scalar(value)),
            "Key" | "Passphrase" => wifi_key = Some(parse_scalar(value)),
            _ => {}
        }
    }

    let interface = interface.ok_or_else(|| format!("profile {name} is missing Interface="))?;
    let connection = match connection
        .ok_or_else(|| format!("profile {name} is missing Connection="))?
        .to_ascii_lowercase()
        .as_str()
    {
        "ethernet" => ConnectionMode::Ethernet,
        "wifi" => {
            let ssid = ssid.ok_or_else(|| format!("profile {name} is missing SSID="))?;
            let security = match security
                .ok_or_else(|| format!("profile {name} is missing Security="))?
                .to_ascii_lowercase()
                .as_str()
            {
                "open" => WifiSecurity::Open,
                "wpa2-psk" => WifiSecurity::Wpa2Psk {
                    key: wifi_key.ok_or_else(|| format!("profile {name} is missing Key="))?,
                },
                other => return Err(format!("unsupported Security={other}")),
            };
            ConnectionMode::Wifi(WifiSettings { ssid, security })
        }
        other => return Err(format!("unsupported Connection={other}")),
    };
    let ip_mode = match ip
        .ok_or_else(|| format!("profile {name} is missing IP="))?
        .to_ascii_lowercase()
        .as_str()
    {
        "bounded" | "none" => ProfileIpMode::Bounded,
        "dhcp" => ProfileIpMode::Dhcp,
        "static" => ProfileIpMode::Static {
            address: address.ok_or_else(|| format!("profile {name} is missing Address="))?,
            gateway,
            dns,
        },
        other => return Err(format!("unsupported IP={other}")),
    };

    Ok(Profile {
        name: name.to_string(),
        interface,
        connection,
        ip_mode,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

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
    fn parses_wifi_profile() {
        let profile = parse_profile(
            "wifi-dhcp",
            "Description='Wi-Fi'\nInterface=wlan0\nConnection=wifi\nSSID='test-ssid'\nSecurity=wpa2-psk\nKey='secret'\nIP=dhcp\n",
        )
        .unwrap();

        assert_eq!(profile.interface, "wlan0");
        match profile.connection {
            ConnectionMode::Wifi(wifi) => {
                assert_eq!(wifi.ssid, "test-ssid");
                match wifi.security {
                    WifiSecurity::Wpa2Psk { key } => assert_eq!(key, "secret"),
                    _ => panic!("expected WPA2 profile"),
                }
            }
            _ => panic!("expected wifi connection"),
        }
    }

    #[test]
    fn parses_bounded_wifi_profile() {
        let profile = parse_profile(
            "wifi-open-bounded",
            "Description='Wi-Fi bounded'\nInterface=wlan0\nConnection=wifi\nSSID='test-ssid'\nSecurity=open\nIP=bounded\n",
        )
        .unwrap();

        assert_eq!(profile.interface, "wlan0");
        match profile.connection {
            ConnectionMode::Wifi(wifi) => {
                assert_eq!(wifi.ssid, "test-ssid");
                assert!(matches!(wifi.security, WifiSecurity::Open));
            }
            _ => panic!("expected wifi connection"),
        }
        assert!(matches!(profile.ip_mode, ProfileIpMode::Bounded));
    }

    #[test]
    fn applies_wifi_profile_to_fake_roots() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let netcfg = temp_root("rbos-netcfg");
        let wifictl = temp_root("rbos-wifictl");
        let dhcp_script = temp_root("rbos-dhcp-script").join("fake-dhcpd.sh");
        fs::create_dir_all(netcfg.join("ifaces/wlan0/addr")).unwrap();
        fs::create_dir_all(wifictl.join("ifaces/wlan0")).unwrap();
        fs::write(netcfg.join("ifaces/wlan0/addr/list"), "Not configured\n").unwrap();
        fs::write(
            &dhcp_script,
            format!(
                "#!/usr/bin/env bash\nset -euo pipefail\nprintf '10.0.0.44/24\\n' > '{}/ifaces/wlan0/addr/list'\n",
                netcfg.display()
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&dhcp_script).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&dhcp_script, perms).unwrap();
        }

        unsafe {
            env::set_var("REDBEAR_NETCFG_ROOT", &netcfg);
            env::set_var("REDBEAR_WIFICTL_ROOT", &wifictl);
            env::set_var("REDBEAR_DHCPD_CMD", &dhcp_script);
            env::set_var("REDBEAR_DHCPD_WAIT_MS", "500");
            env::set_var("REDBEAR_DHCPD_POLL_MS", "10");
        }

        let profile = parse_profile(
            "wifi-dhcp",
            "Description='Wi-Fi'\nInterface=wlan0\nConnection=wifi\nSSID='test-ssid'\nSecurity=wpa2-psk\nKey='secret'\nIP=dhcp\n",
        )
        .unwrap();

        apply_profile(&profile, false).unwrap();

        assert_eq!(
            fs::read_to_string(netcfg.join("ifaces/wlan0/addr/list")).unwrap(),
            "10.0.0.44/24\n"
        );

        assert_eq!(
            fs::read_to_string(wifictl.join("ifaces/wlan0/ssid")).unwrap(),
            "test-ssid\n"
        );
        assert_eq!(
            fs::read_to_string(wifictl.join("ifaces/wlan0/security")).unwrap(),
            "wpa2-psk\n"
        );
        assert_eq!(
            fs::read_to_string(wifictl.join("ifaces/wlan0/key")).unwrap(),
            "secret\n"
        );
        assert_eq!(
            fs::read_to_string(wifictl.join("ifaces/wlan0/prepare")).unwrap(),
            "1\n"
        );
        assert_eq!(
            fs::read_to_string(wifictl.join("ifaces/wlan0/init-transport")).unwrap(),
            "1\n"
        );
        assert_eq!(
            fs::read_to_string(wifictl.join("ifaces/wlan0/connect")).unwrap(),
            "1\n"
        );
    }

    #[test]
    fn applies_bounded_wifi_profile_without_dhcp_handoff() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let netcfg = temp_root("rbos-netcfg-bounded");
        let wifictl = temp_root("rbos-wifictl-bounded");
        let dhcp_log = temp_root("rbos-dhcp-bounded").join("dhcp.log");
        let dhcp_script = temp_root("rbos-dhcp-script-bounded").join("fake-dhcpd.sh");
        fs::create_dir_all(netcfg.join("ifaces/wlan0/addr")).unwrap();
        fs::create_dir_all(wifictl.join("ifaces/wlan0")).unwrap();
        fs::write(netcfg.join("ifaces/wlan0/addr/list"), "Not configured\n").unwrap();
        fs::write(
            &dhcp_script,
            format!(
                "#!/usr/bin/env bash\nset -euo pipefail\nprintf 'called\n' > '{}'\n",
                dhcp_log.display()
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&dhcp_script).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&dhcp_script, perms).unwrap();
        }

        unsafe {
            env::set_var("REDBEAR_NETCFG_ROOT", &netcfg);
            env::set_var("REDBEAR_WIFICTL_ROOT", &wifictl);
            env::set_var("REDBEAR_DHCPD_CMD", &dhcp_script);
        }

        let profile = parse_profile(
            "wifi-open-bounded",
            "Description='Wi-Fi bounded'\nInterface=wlan0\nConnection=wifi\nSSID='test-ssid'\nSecurity=open\nIP=bounded\n",
        )
        .unwrap();

        apply_profile(&profile, false).unwrap();

        assert!(!dhcp_log.exists());
        assert_eq!(
            fs::read_to_string(netcfg.join("ifaces/wlan0/addr/list")).unwrap(),
            "Not configured\n"
        );
        assert_eq!(
            fs::read_to_string(wifictl.join("ifaces/wlan0/connect")).unwrap(),
            "1\n"
        );
    }

    #[test]
    fn reads_wifi_state_values() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let wifictl = temp_root("rbos-wifictl-state");
        fs::create_dir_all(wifictl.join("ifaces/wlan0")).unwrap();
        fs::write(wifictl.join("ifaces/wlan0/status"), "firmware-ready\n").unwrap();
        fs::write(
            wifictl.join("ifaces/wlan0/firmware-status"),
            "firmware=present family=intel-bz-arrow-lake\n",
        )
        .unwrap();
        fs::write(
            wifictl.join("ifaces/wlan0/transport-status"),
            "transport=pci memory_enabled=yes bus_master=yes bar0_present=yes irq_pin_present=yes\n",
        )
        .unwrap();
        fs::write(
            wifictl.join("ifaces/wlan0/transport-init-status"),
            "transport_init=stub\n",
        )
        .unwrap();
        fs::write(
            wifictl.join("ifaces/wlan0/activation-status"),
            "activation=stub\n",
        )
        .unwrap();
        fs::write(
            wifictl.join("ifaces/wlan0/connect-result"),
            "connect_result=bounded-associated ssid=demo security=wpa2-psk\n",
        )
        .unwrap();

        unsafe {
            env::set_var("REDBEAR_WIFICTL_ROOT", &wifictl);
        }

        assert_eq!(
            read_wifictl_value("wlan0", "status").as_deref(),
            Some("firmware-ready")
        );
        assert!(read_wifictl_value("wlan0", "firmware-status")
            .unwrap()
            .contains("intel-bz-arrow-lake"));
        assert!(read_wifictl_value("wlan0", "transport-status")
            .unwrap()
            .contains("memory_enabled=yes"));
        assert_eq!(
            read_wifictl_value("wlan0", "transport-init-status").as_deref(),
            Some("transport_init=stub")
        );
        assert_eq!(
            read_wifictl_value("wlan0", "activation-status").as_deref(),
            Some("activation=stub")
        );
        assert_eq!(
            read_wifictl_value("wlan0", "connect-result").as_deref(),
            Some("connect_result=bounded-associated ssid=demo security=wpa2-psk")
        );
    }

    #[test]
    fn scan_uses_wifi_profile_or_interface() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let profile_dir = temp_root("rbos-netctl-scan-profile");
        let wifictl = temp_root("rbos-netctl-scan-wifictl");
        fs::create_dir_all(&profile_dir).unwrap();
        fs::create_dir_all(wifictl.join("ifaces/wlan0")).unwrap();
        fs::write(
            profile_dir.join("wifi-dhcp"),
            "Description='Wi-Fi'\nInterface=wlan0\nConnection=wifi\nSSID='test-ssid'\nSecurity=open\nIP=dhcp\n",
        )
        .unwrap();
        fs::write(wifictl.join("ifaces/wlan0/status"), "scanning\n").unwrap();
        fs::write(
            wifictl.join("ifaces/wlan0/firmware-status"),
            "firmware=present family=intel-bz-arrow-lake prepared=yes\n",
        )
        .unwrap();
        fs::write(
            wifictl.join("ifaces/wlan0/transport-status"),
            "transport=pci memory_enabled=yes bus_master=yes\n",
        )
        .unwrap();
        fs::write(
            wifictl.join("ifaces/wlan0/transport-init-status"),
            "transport_init=stub\n",
        )
        .unwrap();
        fs::write(
            wifictl.join("ifaces/wlan0/activation-status"),
            "activation=stub\n",
        )
        .unwrap();
        fs::write(
            wifictl.join("ifaces/wlan0/scan-results"),
            "demo-ssid\ndemo-open\n",
        )
        .unwrap();

        unsafe {
            env::set_var("REDBEAR_NETCTL_PROFILE_DIR", &profile_dir);
            env::set_var("REDBEAR_WIFICTL_ROOT", &wifictl);
        }

        scan_wifi("wifi-dhcp").unwrap();
        assert_eq!(
            fs::read_to_string(wifictl.join("ifaces/wlan0/prepare")).unwrap(),
            "1\n"
        );
        assert_eq!(
            fs::read_to_string(wifictl.join("ifaces/wlan0/init-transport")).unwrap(),
            "1\n"
        );
        assert_eq!(
            fs::read_to_string(wifictl.join("ifaces/wlan0/activate-nic")).unwrap(),
            "1\n"
        );
        assert_eq!(
            fs::read_to_string(wifictl.join("ifaces/wlan0/scan")).unwrap(),
            "1\n"
        );
    }

    #[test]
    fn retry_uses_wifi_profile_or_interface() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let profile_dir = temp_root("rbos-netctl-retry-profile");
        let wifictl = temp_root("rbos-netctl-retry-wifictl");
        fs::create_dir_all(&profile_dir).unwrap();
        fs::create_dir_all(wifictl.join("ifaces/wlan0")).unwrap();
        fs::write(
            profile_dir.join("wifi-dhcp"),
            "Description='Wi-Fi'\nInterface=wlan0\nConnection=wifi\nSSID='test-ssid'\nSecurity=open\nIP=dhcp\n",
        )
        .unwrap();
        fs::write(wifictl.join("ifaces/wlan0/status"), "device-detected\n").unwrap();
        fs::write(wifictl.join("ifaces/wlan0/link-state"), "link=retrying\n").unwrap();

        unsafe {
            env::set_var("REDBEAR_NETCTL_PROFILE_DIR", &profile_dir);
            env::set_var("REDBEAR_WIFICTL_ROOT", &wifictl);
        }

        retry_wifi("wifi-dhcp").unwrap();
        assert_eq!(
            fs::read_to_string(wifictl.join("ifaces/wlan0/retry")).unwrap(),
            "1\n"
        );
    }

    #[test]
    fn reports_wifi_last_error() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let wifictl = temp_root("rbos-wifictl-error");
        fs::create_dir_all(wifictl.join("ifaces/wlan0")).unwrap();
        fs::write(
            wifictl.join("ifaces/wlan0/last-error"),
            "missing firmware\n",
        )
        .unwrap();

        unsafe {
            env::set_var("REDBEAR_WIFICTL_ROOT", &wifictl);
        }

        assert_eq!(
            read_wifictl_value("wlan0", "last-error").as_deref(),
            Some("missing firmware")
        );
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
            .filter(|value| !value.is_empty())
    } else {
        let value = parse_scalar(trimmed);
        (!value.is_empty()).then_some(value)
    }
}
