use std::fs;
use std::path::PathBuf;
use std::process::Command;
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

fn write_intel_candidate(pci_root: &PathBuf) {
    let slot = pci_root.join("0000--00--14.3");
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
}

fn write_mock_driver(path: &PathBuf) {
    fs::write(
        path,
        r##"#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  --transport-probe)
    printf 'transport_status=transport=cli-probe-path\n'
    ;;
  --init-transport)
    printf 'transport_init_status=transport_init=cli-init-path\n'
    printf 'transport_status=transport=cli-init-path\n'
    ;;
  --prepare)
    printf 'status=firmware-ready\n'
    printf 'transport_status=transport=prepared\n'
    ;;
  --activate-nic)
    printf 'activation=ok\n'
    printf 'transport_status=transport=active\n'
    ;;
  --connect)
    printf 'status=associated\n'
    printf 'connect_result=cli-associated ssid=%s security=%s\n' "${3:-}" "${4:-}"
    ;;
  --disconnect)
    printf 'status=device-detected\n'
    printf 'disconnect_result=cli-disconnected\n'
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
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }
}

fn run_wifictl(args: &[&str], pci_root: &PathBuf, fw_root: &PathBuf, driver: &PathBuf) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_redbear-wifictl"))
        .args(args)
        .env("REDBEAR_WIFICTL_BACKEND", "intel")
        .env("REDBEAR_WIFICTL_PCI_ROOT", pci_root)
        .env("REDBEAR_WIFICTL_FIRMWARE_ROOT", fw_root)
        .env("REDBEAR_IWLWIFI_CMD", driver)
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

#[test]
fn cli_transport_probe_uses_probe_path() {
    let pci = temp_root("rbos-wifictl-cli-pci");
    let fw = temp_root("rbos-wifictl-cli-fw");
    write_intel_candidate(&pci);
    fs::write(fw.join("iwlwifi-bz-b0-gf-a0-92.ucode"), []).unwrap();
    fs::write(fw.join("iwlwifi-bz-b0-gf-a0.pnvm"), []).unwrap();

    let driver = temp_root("rbos-wifictl-cli-driver").join("redbear-iwlwifi-mock.sh");
    write_mock_driver(&driver);

    let probe = run_wifictl(&["--transport-probe", "wlan0"], &pci, &fw, &driver);
    assert!(probe.contains("transport_status=transport=cli-probe-path"));
    assert!(!probe.contains("cli-init-path"));
}

#[test]
fn cli_connect_reports_driver_status_honestly() {
    let pci = temp_root("rbos-wifictl-cli-pci-connect");
    let fw = temp_root("rbos-wifictl-cli-fw-connect");
    write_intel_candidate(&pci);
    fs::write(fw.join("iwlwifi-bz-b0-gf-a0-92.ucode"), []).unwrap();
    fs::write(fw.join("iwlwifi-bz-b0-gf-a0.pnvm"), []).unwrap();

    let driver = temp_root("rbos-wifictl-cli-driver-connect").join("redbear-iwlwifi-mock.sh");
    write_mock_driver(&driver);

    let connect = run_wifictl(
        &["--connect", "wlan0", "demo", "wpa2-psk", "secret"],
        &pci,
        &fw,
        &driver,
    );
    assert!(connect.contains("status=connected"));
    assert!(connect.contains("transport_status=transport=active"));

    let pending_driver =
        temp_root("rbos-wifictl-cli-driver-pending").join("redbear-iwlwifi-pending.sh");
    fs::write(
        &pending_driver,
        r##"#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  --prepare)
    printf 'status=firmware-ready\n'
    ;;
  --activate-nic)
    printf 'activation=ok\n'
    ;;
  --connect)
    printf 'status=associating\n'
    printf 'connect_result=host-bounded-pending ssid=%s security=%s\n' "${3:-}" "${4:-}"
    ;;
  --disconnect)
    printf 'status=device-detected\n'
    printf 'disconnect_result=cli-disconnected\n'
    ;;
  *)
    printf 'status=device-detected\n'
    ;;
esac
"##,
    )
    .unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&pending_driver).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&pending_driver, perms).unwrap();
    }

    let pending = run_wifictl(
        &["--connect", "wlan0", "demo", "wpa2-psk", "secret"],
        &pci,
        &fw,
        &pending_driver,
    );
    assert!(pending.contains("status=associating"));
    assert!(pending.contains("connect_result=host-bounded-pending"));

    let disconnect = run_wifictl(&["--disconnect", "wlan0"], &pci, &fw, &driver);
    assert!(disconnect.contains("status=device-detected"));
    assert!(disconnect.contains("disconnect_result=cli-disconnected"));
}
