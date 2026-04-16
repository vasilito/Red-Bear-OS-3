use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_root(prefix: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("{prefix}-{stamp}"));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn ncurses_binary_launches_and_quits_on_q() {
    if Command::new("script")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_err()
    {
        eprintln!("skipping ncurses smoke test: 'script' utility not available");
        return;
    }

    let profile_dir = temp_root("rbos-netctl-console-tty-profiles");
    let wifictl_root = temp_root("rbos-netctl-console-tty-wifictl");
    let netcfg_root = temp_root("rbos-netctl-console-tty-netcfg");
    fs::create_dir_all(wifictl_root.join("ifaces/wlan0")).unwrap();
    fs::create_dir_all(netcfg_root.join("ifaces/wlan0/addr")).unwrap();
    fs::write(
        netcfg_root.join("ifaces/wlan0/addr/list"),
        "Not configured\n",
    )
    .unwrap();
    fs::write(
        wifictl_root.join("ifaces/wlan0/status"),
        "device-detected\n",
    )
    .unwrap();
    fs::write(wifictl_root.join("ifaces/wlan0/link-state"), "link=down\n").unwrap();
    fs::write(
        wifictl_root.join("ifaces/wlan0/firmware-status"),
        "firmware=present\n",
    )
    .unwrap();
    fs::write(
        wifictl_root.join("ifaces/wlan0/transport-status"),
        "transport=ready\n",
    )
    .unwrap();
    fs::write(
        wifictl_root.join("ifaces/wlan0/transport-init-status"),
        "transport_init=not-run\n",
    )
    .unwrap();
    fs::write(
        wifictl_root.join("ifaces/wlan0/activation-status"),
        "activation=not-run\n",
    )
    .unwrap();
    fs::write(
        wifictl_root.join("ifaces/wlan0/connect-result"),
        "connect_result=not-run\n",
    )
    .unwrap();
    fs::write(
        wifictl_root.join("ifaces/wlan0/disconnect-result"),
        "disconnect_result=not-run\n",
    )
    .unwrap();

    let mut child = Command::new("script")
        .args([
            "-qec",
            env!("CARGO_BIN_EXE_redbear-netctl-console"),
            "/dev/null",
        ])
        .env("REDBEAR_NETCTL_PROFILE_DIR", &profile_dir)
        .env("REDBEAR_WIFICTL_ROOT", &wifictl_root)
        .env("REDBEAR_NETCFG_ROOT", &netcfg_root)
        .env("TERM", "xterm")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    child.stdin.as_mut().unwrap().write_all(b"q").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "ncurses console failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
