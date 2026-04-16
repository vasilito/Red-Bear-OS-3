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
    let mut cfg = vec![0u8; 48];
    cfg[0x00] = 0x86;
    cfg[0x01] = 0x80;
    cfg[0x02] = 0x40;
    cfg[0x03] = 0x77;
    cfg[0x0A] = 0x80;
    cfg[0x0B] = 0x02;
    cfg[0x2E] = 0x90;
    cfg[0x2F] = 0x40;
    fs::write(slot.join("config"), cfg).unwrap();
}

fn run_iwlwifi(args: &[&str], pci_root: &PathBuf, fw_root: &PathBuf) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_redbear-iwlwifi"))
        .args(args)
        .env("REDBEAR_IWLWIFI_PCI_ROOT", pci_root)
        .env("REDBEAR_IWLWIFI_FIRMWARE_ROOT", fw_root)
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
fn cli_flow_reports_bounded_intel_progression() {
    let pci = temp_root("rbos-iwlwifi-cli-pci");
    let fw = temp_root("rbos-iwlwifi-cli-fw");
    write_intel_candidate(&pci);
    fs::write(fw.join("iwlwifi-bz-b0-gf-a0-92.ucode"), []).unwrap();
    fs::write(fw.join("iwlwifi-bz-b0-gf-a0.pnvm"), []).unwrap();

    let status = run_iwlwifi(&["--status"], &pci, &fw);
    assert!(status.contains("status=firmware-ready"));
    assert!(status.contains("selected_pnvm=iwlwifi-bz-b0-gf-a0.pnvm"));

    let prepare = run_iwlwifi(&["--prepare"], &pci, &fw);
    assert!(prepare.contains("status=firmware-ready"));
    assert!(prepare.contains("selected_ucode=iwlwifi-bz-b0-gf-a0-92.ucode"));

    let init = run_iwlwifi(&["--init-transport"], &pci, &fw);
    assert!(init.contains("status=transport-ready"));
    assert!(init.contains("bar0_addr=host-skipped"));

    let activate = run_iwlwifi(&["--activate-nic"], &pci, &fw);
    assert!(activate.contains("status=nic-activated"));
    assert!(activate.contains("activation=host-skipped"));

    let connect = run_iwlwifi(
        &["--connect", "0000:00:14.3", "demo", "wpa2-psk", "secret"],
        &pci,
        &fw,
    );
    assert!(connect.contains("status=associating"));
    assert!(connect.contains("connect_result="));

    let disconnect = run_iwlwifi(&["--disconnect", "0000:00:14.3"], &pci, &fw);
    assert!(disconnect.contains("status=device-detected"));
    assert!(disconnect.contains("disconnect_result="));
}
