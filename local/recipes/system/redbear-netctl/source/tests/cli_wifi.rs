use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use redbear_netctl_console::backend::{
    ConsoleBackend, FsBackend, IpMode, Profile, RuntimePaths, SecurityKind,
};

fn temp_root(prefix: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("{prefix}-{stamp}"));
    fs::create_dir_all(&path).unwrap();
    path
}

fn run_netctl(
    args: &[&str],
    profile_dir: &PathBuf,
    wifictl_root: &PathBuf,
    netcfg_root: &PathBuf,
) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_redbear-netctl"))
        .args(args)
        .env("REDBEAR_NETCTL_PROFILE_DIR", profile_dir)
        .env("REDBEAR_WIFICTL_ROOT", wifictl_root)
        .env("REDBEAR_NETCFG_ROOT", netcfg_root)
        .env("REDBEAR_DHCPD_CMD", "/usr/bin/true")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "command {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout).unwrap()
}

fn console_runtime_paths(
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
fn cli_start_wifi_profile_writes_connect_path() {
    let profile_dir = temp_root("rbos-netctl-cli-profiles");
    let wifictl_root = temp_root("rbos-netctl-cli-wifictl");
    let netcfg_root = temp_root("rbos-netctl-cli-netcfg");
    fs::create_dir_all(wifictl_root.join("ifaces/wlan0")).unwrap();
    fs::create_dir_all(netcfg_root.join("ifaces/wlan0/addr")).unwrap();
    fs::write(
        netcfg_root.join("ifaces/wlan0/addr/list"),
        "Not configured\n",
    )
    .unwrap();

    fs::write(
        profile_dir.join("wifi-dhcp"),
        "Description='Wi-Fi'\nInterface=wlan0\nConnection=wifi\nSSID='demo'\nSecurity=wpa2-psk\nKey='secret'\nIP=dhcp\n",
    )
    .unwrap();

    fs::write(wifictl_root.join("ifaces/wlan0/status"), "connected\n").unwrap();
    fs::write(
        wifictl_root.join("ifaces/wlan0/link-state"),
        "link=connected\n",
    )
    .unwrap();
    fs::write(
        wifictl_root.join("ifaces/wlan0/firmware-status"),
        "firmware=present\n",
    )
    .unwrap();
    fs::write(
        wifictl_root.join("ifaces/wlan0/transport-status"),
        "transport=active\n",
    )
    .unwrap();
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
        wifictl_root.join("ifaces/wlan0/connect-result"),
        "connect_result=bounded-associated ssid=demo security=wpa2-psk\n",
    )
    .unwrap();
    fs::write(
        wifictl_root.join("ifaces/wlan0/disconnect-result"),
        "disconnect_result=bounded-disconnected\n",
    )
    .unwrap();

    let dhcp_log = profile_dir.join("dhcp.log");
    let dhcp_script = profile_dir.join("fake-dhcpd.sh");
    fs::write(
        &dhcp_script,
        format!(
            "#!/usr/bin/env bash\nset -euo pipefail\nprintf '%s\\n' \"$1\" > '{}'\nprintf '10.0.0.44/24\\n' > '{}/ifaces/wlan0/addr/list'\n",
            dhcp_log.display(),
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

    let output = Command::new(env!("CARGO_BIN_EXE_redbear-netctl"))
        .args(["start", "wifi-dhcp"])
        .env("REDBEAR_NETCTL_PROFILE_DIR", &profile_dir)
        .env("REDBEAR_WIFICTL_ROOT", &wifictl_root)
        .env("REDBEAR_NETCFG_ROOT", &netcfg_root)
        .env("REDBEAR_DHCPD_CMD", &dhcp_script)
        .env("REDBEAR_DHCPD_WAIT_MS", "500")
        .env("REDBEAR_DHCPD_POLL_MS", "10")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "command {:?} failed: {}",
        ["start", "wifi-dhcp"],
        String::from_utf8_lossy(&output.stderr)
    );
    let started = String::from_utf8(output.stdout).unwrap();
    assert!(started.contains("started wifi-dhcp"));
    assert_eq!(fs::read_to_string(&dhcp_log).unwrap(), "wlan0\n");
    assert_eq!(
        fs::read_to_string(netcfg_root.join("ifaces/wlan0/addr/list")).unwrap(),
        "10.0.0.44/24\n"
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
        fs::read_to_string(wifictl_root.join("ifaces/wlan0/init-transport")).unwrap(),
        "1\n"
    );
    assert_eq!(
        fs::read_to_string(wifictl_root.join("ifaces/wlan0/activate-nic")).unwrap(),
        "1\n"
    );
    assert_eq!(
        fs::read_to_string(wifictl_root.join("ifaces/wlan0/connect")).unwrap(),
        "1\n"
    );

    let status = run_netctl(
        &["status", "wifi-dhcp"],
        &profile_dir,
        &wifictl_root,
        &netcfg_root,
    );
    assert!(status.contains("address=10.0.0.44/24"));
    assert!(status.contains("wifi_status=connected"));
    assert!(status.contains("connect_result=bounded-associated ssid=demo security=wpa2-psk"));
    assert!(status.contains("disconnect_result=bounded-disconnected"));

    let stopped = run_netctl(
        &["stop", "wifi-dhcp"],
        &profile_dir,
        &wifictl_root,
        &netcfg_root,
    );
    assert!(stopped.contains("stopped wifi-dhcp"));
    assert_eq!(
        fs::read_to_string(wifictl_root.join("ifaces/wlan0/disconnect")).unwrap(),
        "1\n"
    );
}

#[test]
fn cli_status_reports_pending_wifi_link_honestly() {
    let profile_dir = temp_root("rbos-netctl-cli-pending-profiles");
    let wifictl_root = temp_root("rbos-netctl-cli-pending-wifictl");
    let netcfg_root = temp_root("rbos-netctl-cli-pending-netcfg");
    fs::create_dir_all(wifictl_root.join("ifaces/wlan0")).unwrap();
    fs::create_dir_all(netcfg_root.join("ifaces/wlan0/addr")).unwrap();
    fs::write(
        profile_dir.join("wifi-open-bounded"),
        "Description='Wi-Fi bounded'\nInterface=wlan0\nConnection=wifi\nSSID='demo'\nSecurity=wpa2-psk\nKey='secret'\nIP=bounded\n",
    )
    .unwrap();
    fs::write(profile_dir.join("active"), "wifi-open-bounded\n").unwrap();

    fs::write(wifictl_root.join("ifaces/wlan0/status"), "associating\n").unwrap();
    fs::write(
        wifictl_root.join("ifaces/wlan0/link-state"),
        "link=associating\n",
    )
    .unwrap();
    fs::write(
        wifictl_root.join("ifaces/wlan0/firmware-status"),
        "firmware=present\n",
    )
    .unwrap();
    fs::write(
        wifictl_root.join("ifaces/wlan0/transport-status"),
        "transport=active\n",
    )
    .unwrap();
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
        wifictl_root.join("ifaces/wlan0/connect-result"),
        "connect_result=host-bounded-pending ssid=demo security=wpa2-psk\n",
    )
    .unwrap();
    fs::write(
        wifictl_root.join("ifaces/wlan0/disconnect-result"),
        "disconnect_result=bounded-disconnected\n",
    )
    .unwrap();
    fs::write(
        netcfg_root.join("ifaces/wlan0/addr/list"),
        "Not configured\n",
    )
    .unwrap();

    let status = run_netctl(
        &["status", "wifi-open-bounded"],
        &profile_dir,
        &wifictl_root,
        &netcfg_root,
    );
    assert!(status.contains("wifi_status=associating"));
    assert!(status.contains("link_state=link=associating"));
    assert!(status.contains("connect_result=host-bounded-pending ssid=demo security=wpa2-psk"));
}

#[test]
fn cli_start_consumes_console_written_wifi_profile() {
    let profile_dir = temp_root("rbos-netctl-cli-console-profile");
    let wifictl_root = temp_root("rbos-netctl-cli-console-wifictl");
    let netcfg_root = temp_root("rbos-netctl-cli-console-netcfg");
    fs::create_dir_all(wifictl_root.join("ifaces/wlan0")).unwrap();
    fs::create_dir_all(netcfg_root.join("ifaces/wlan0/addr")).unwrap();
    fs::write(
        netcfg_root.join("ifaces/wlan0/addr/list"),
        "Not configured\n",
    )
    .unwrap();
    fs::write(wifictl_root.join("ifaces/wlan0/status"), "associating\n").unwrap();
    fs::write(
        wifictl_root.join("ifaces/wlan0/link-state"),
        "link=associating\n",
    )
    .unwrap();
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
        wifictl_root.join("ifaces/wlan0/connect-result"),
        "connect_result=host-bounded-pending ssid=console-demo security=wpa2-psk\n",
    )
    .unwrap();
    fs::write(
        wifictl_root.join("ifaces/wlan0/disconnect-result"),
        "disconnect_result=not-run\n",
    )
    .unwrap();

    let console_backend = FsBackend::new(console_runtime_paths(
        &profile_dir,
        &wifictl_root,
        &netcfg_root,
        "/usr/bin/true",
    ));
    let profile = Profile {
        name: "wifi-console-bounded".to_string(),
        description: "Console written Wi-Fi profile".to_string(),
        interface: "wlan0".to_string(),
        ssid: "console-demo".to_string(),
        security: SecurityKind::Wpa2Psk,
        key: "secret".to_string(),
        ip_mode: IpMode::Bounded,
        address: String::new(),
        gateway: String::new(),
        dns: String::new(),
    };
    console_backend.save_profile(&profile).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_redbear-netctl"))
        .args(["start", "wifi-console-bounded"])
        .env("REDBEAR_NETCTL_PROFILE_DIR", &profile_dir)
        .env("REDBEAR_WIFICTL_ROOT", &wifictl_root)
        .env("REDBEAR_NETCFG_ROOT", &netcfg_root)
        .env("REDBEAR_DHCPD_CMD", "/usr/bin/true")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "command {:?} failed: {}",
        ["start", "wifi-console-bounded"],
        String::from_utf8_lossy(&output.stderr)
    );

    let started = String::from_utf8(output.stdout).unwrap();
    assert!(started.contains("started wifi-console-bounded"));
    assert_eq!(
        fs::read_to_string(wifictl_root.join("ifaces/wlan0/ssid")).unwrap(),
        "console-demo\n"
    );
    assert_eq!(
        fs::read_to_string(wifictl_root.join("ifaces/wlan0/security")).unwrap(),
        "wpa2-psk\n"
    );
    assert_eq!(
        fs::read_to_string(wifictl_root.join("ifaces/wlan0/key")).unwrap(),
        "secret\n"
    );

    let status = run_netctl(
        &["status", "wifi-console-bounded"],
        &profile_dir,
        &wifictl_root,
        &netcfg_root,
    );
    assert!(status.contains("profile=wifi-console-bounded"));
    assert!(status.contains("wifi_status=associating"));
    assert!(status.contains("link_state=link=associating"));
    assert!(
        status.contains("connect_result=host-bounded-pending ssid=console-demo security=wpa2-psk")
    );
}
