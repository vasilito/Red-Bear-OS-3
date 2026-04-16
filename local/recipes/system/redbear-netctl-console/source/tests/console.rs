use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use redbear_netctl_console::backend::{
    ConsoleBackend, FsBackend, IpMode, Profile, RuntimePaths, SecurityKind,
};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn temp_root(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("{prefix}-{nanos}"));
    fs::create_dir_all(&root).unwrap();
    root
}

fn runtime_paths(
    profile_dir: &PathBuf,
    wifictl_root: &PathBuf,
    netcfg_root: &PathBuf,
    dhcpd_command: &str,
) -> RuntimePaths {
    RuntimePaths {
        profile_dir: profile_dir.clone(),
        active_profile_path: profile_dir.join("active"),
        wifictl_root: wifictl_root.clone(),
        netcfg_root: netcfg_root.clone(),
        dhcpd_command: dhcpd_command.to_string(),
        dhcp_wait_timeout: std::time::Duration::from_millis(500),
        dhcp_poll_interval: std::time::Duration::from_millis(10),
    }
}

#[test]
fn saves_and_loads_profile_using_fake_roots() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let profile_dir = temp_root("rbos-netctl-console-profiles");
    let wifictl_root = temp_root("rbos-netctl-console-wifictl");
    let netcfg_root = temp_root("rbos-netctl-console-netcfg");
    let backend = FsBackend::new(runtime_paths(
        &profile_dir,
        &wifictl_root,
        &netcfg_root,
        "/usr/bin/true",
    ));

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

    backend.save_profile(&profile).unwrap();
    let loaded = backend.load_profile("wifi-open-bounded").unwrap();
    assert_eq!(loaded, profile);
    assert_eq!(
        backend.list_wifi_profiles().unwrap(),
        vec!["wifi-open-bounded".to_string()]
    );
}

#[test]
fn scan_writes_bounded_wifictl_flow_nodes() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let profile_dir = temp_root("rbos-netctl-console-scan-profiles");
    let wifictl_root = temp_root("rbos-netctl-console-scan-wifictl");
    let netcfg_root = temp_root("rbos-netctl-console-scan-netcfg");
    fs::create_dir_all(wifictl_root.join("ifaces/wlan0")).unwrap();
    fs::write(wifictl_root.join("ifaces/wlan0/status"), "scanning\n").unwrap();
    fs::write(
        wifictl_root.join("ifaces/wlan0/transport-init-status"),
        "transport_init=ok\n",
    )
    .unwrap();
    fs::write(
        wifictl_root.join("ifaces/wlan0/activation-status"),
        "activation=ok\n",
    )
    .unwrap();
    fs::write(
        wifictl_root.join("ifaces/wlan0/scan-results"),
        "ssid=demo-open security=open\nssid=demo-secure security=wpa2-psk\n",
    )
    .unwrap();

    let backend = FsBackend::new(runtime_paths(
        &profile_dir,
        &wifictl_root,
        &netcfg_root,
        "/usr/bin/true",
    ));
    let results = backend.scan("wlan0").unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].ssid, "demo-open");
    assert_eq!(results[1].ssid, "demo-secure");
    assert_eq!(
        fs::read_to_string(wifictl_root.join("ifaces/wlan0/prepare")).unwrap(),
        "1\n"
    );
    assert_eq!(
        fs::read_to_string(wifictl_root.join("ifaces/wlan0/init-transport")).unwrap(),
        "1\n"
    );
    assert_eq!(
        fs::read_to_string(wifictl_root.join("ifaces/wlan0/activate-nic")).unwrap(),
        "1\n"
    );
    assert_eq!(
        fs::read_to_string(wifictl_root.join("ifaces/wlan0/scan")).unwrap(),
        "1\n"
    );
}

#[test]
fn connect_uses_wifictl_and_marks_active_profile() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let profile_dir = temp_root("rbos-netctl-console-connect-profiles");
    let wifictl_root = temp_root("rbos-netctl-console-connect-wifictl");
    let netcfg_root = temp_root("rbos-netctl-console-connect-netcfg");
    fs::create_dir_all(wifictl_root.join("ifaces/wlan0")).unwrap();
    fs::create_dir_all(netcfg_root.join("ifaces/wlan0/addr")).unwrap();
    fs::write(
        netcfg_root.join("ifaces/wlan0/addr/list"),
        "Not configured\n",
    )
    .unwrap();
    fs::write(wifictl_root.join("ifaces/wlan0/status"), "connected\n").unwrap();
    fs::write(
        wifictl_root.join("ifaces/wlan0/transport-init-status"),
        "transport_init=ok\n",
    )
    .unwrap();
    fs::write(
        wifictl_root.join("ifaces/wlan0/activation-status"),
        "activation=ok\n",
    )
    .unwrap();

    let dhcp_script = profile_dir.join("fake-dhcpd.sh");
    fs::write(
        &dhcp_script,
        format!(
            "#!/usr/bin/env bash\nset -euo pipefail\nprintf '10.0.0.44/24\\n' > '{}/ifaces/wlan0/addr/list'\n",
            netcfg_root.display()
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

    let backend = FsBackend::new(runtime_paths(
        &profile_dir,
        &wifictl_root,
        &netcfg_root,
        dhcp_script.to_str().unwrap(),
    ));
    let profile = Profile {
        name: "wifi-dhcp".to_string(),
        description: "Wi-Fi DHCP".to_string(),
        interface: "wlan0".to_string(),
        ssid: "demo".to_string(),
        security: SecurityKind::Wpa2Psk,
        key: "secret".to_string(),
        ip_mode: IpMode::Dhcp,
        address: String::new(),
        gateway: String::new(),
        dns: String::new(),
    };

    let message = backend.connect(&profile).unwrap();
    assert!(message.contains("applied wifi-dhcp"));
    assert_eq!(
        backend.active_profile_name().unwrap().as_deref(),
        Some("wifi-dhcp")
    );
    assert_eq!(
        fs::read_to_string(wifictl_root.join("ifaces/wlan0/ssid")).unwrap(),
        "demo\n"
    );
    assert_eq!(
        fs::read_to_string(wifictl_root.join("ifaces/wlan0/security")).unwrap(),
        "wpa2-psk\n"
    );
    assert_eq!(
        fs::read_to_string(wifictl_root.join("ifaces/wlan0/key")).unwrap(),
        "secret\n"
    );
    assert_eq!(
        fs::read_to_string(wifictl_root.join("ifaces/wlan0/prepare")).unwrap(),
        "1\n"
    );
    assert_eq!(
        fs::read_to_string(wifictl_root.join("ifaces/wlan0/connect")).unwrap(),
        "1\n"
    );
    assert_eq!(
        fs::read_to_string(netcfg_root.join("ifaces/wlan0/addr/list")).unwrap(),
        "10.0.0.44/24\n"
    );
}

#[test]
fn disconnect_clears_active_profile_when_it_matches() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let profile_dir = temp_root("rbos-netctl-console-disconnect-profiles");
    let wifictl_root = temp_root("rbos-netctl-console-disconnect-wifictl");
    let netcfg_root = temp_root("rbos-netctl-console-disconnect-netcfg");
    fs::create_dir_all(wifictl_root.join("ifaces/wlan0")).unwrap();

    let backend = FsBackend::new(runtime_paths(
        &profile_dir,
        &wifictl_root,
        &netcfg_root,
        "/usr/bin/true",
    ));
    backend.set_active_profile("wifi-dhcp").unwrap();

    let message = backend.disconnect(Some("wifi-dhcp"), "wlan0").unwrap();
    assert!(message.contains("disconnected wlan0"));
    assert_eq!(backend.active_profile_name().unwrap(), None);
    assert_eq!(
        fs::read_to_string(wifictl_root.join("ifaces/wlan0/disconnect")).unwrap(),
        "1\n"
    );
}
