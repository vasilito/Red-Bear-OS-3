use std::fs;
use std::path::Path;
use std::process::{self, Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use redbear_hwutils::parse_args;

const PROGRAM: &str = "redbear-bluetooth-battery-check";
const USAGE: &str = "Usage: redbear-bluetooth-battery-check\n\nExercise the bounded Bluetooth Battery Level runtime slice inside a Red Bear OS guest or target runtime.";
const ADAPTER: &str = "hci0";
const BOND_ID: &str = "AA:BB:CC:DD:EE:FF";
const BOND_ALIAS: &str = "demo-battery-sensor";
const EXPERIMENTAL_WORKLOAD: &str = "battery-sensor-battery-level-read";
const PERIPHERAL_CLASS: &str = "ble-battery-sensor";
const BATTERY_SERVICE_UUID: &str = "0000180f-0000-1000-8000-00805f9b34fb";
const BATTERY_LEVEL_CHAR_UUID: &str = "00002a19-0000-1000-8000-00805f9b34fb";
const TRANSPORT_STATUS_PATH: &str = "/var/run/redbear-btusb/status";
const BTCTL_ROOT: &str = "/scheme/btctl";
const BTCTL_ADAPTER_ROOT: &str = "/scheme/btctl/adapters/hci0";
const BTCTL_CONNECTION_STATE_PATH: &str = "/scheme/btctl/adapters/hci0/connection-state";
const BTCTL_CONNECT_RESULT_PATH: &str = "/scheme/btctl/adapters/hci0/connect-result";
const BTCTL_DISCONNECT_RESULT_PATH: &str = "/scheme/btctl/adapters/hci0/disconnect-result";
const BTCTL_READ_CHAR_RESULT_PATH: &str = "/scheme/btctl/adapters/hci0/read-char-result";
const BTCTL_LAST_ERROR_PATH: &str = "/scheme/btctl/adapters/hci0/last-error";
const BTCTL_BONDS_PATH: &str = "/scheme/btctl/adapters/hci0/bonds";
const BTCTL_BOND_PATH: &str = "/scheme/btctl/adapters/hci0/bonds/AA:BB:CC:DD:EE:FF";
const BTUSB_LOG_PATH: &str = "/tmp/redbear-btusb-runtime.log";

struct CommandCapture {
    stdout: String,
    stderr: String,
    success: bool,
}

#[derive(Default)]
struct RuntimeSession {
    btusb: Option<Child>,
}

impl Drop for RuntimeSession {
    fn drop(&mut self) {
        let _ = self.remove_test_bond();
        self.stop_btusb_quietly();
    }
}

impl RuntimeSession {
    fn ensure_btusb_running(&mut self) -> Result<(), String> {
        if transport_runtime_visible() {
            return Ok(());
        }

        let child = Command::new("redbear-btusb")
            .stdout(open_log_file(BTUSB_LOG_PATH)?)
            .stderr(open_log_file(BTUSB_LOG_PATH)?)
            .spawn()
            .map_err(|err| format!("failed to start redbear-btusb: {err}"))?;
        self.btusb = Some(child);
        wait_for_condition(
            "redbear-btusb runtime visibility",
            Duration::from_secs(5),
            || Ok::<bool, String>(transport_runtime_visible()),
        )
    }

    fn ensure_btctl_running(&mut self) -> Result<(), String> {
        wait_for_condition(
            "redbear-btctl scheme registration",
            Duration::from_secs(20),
            || {
                Ok::<bool, String>(
                    Path::new(BTCTL_ROOT).exists() && Path::new(BTCTL_ADAPTER_ROOT).exists(),
                )
            },
        )
        .map_err(|err| {
            format!(
                "{err} (redbear-btctl must be launched through init/profile wiring so /scheme/btctl is visible in the runtime namespace)"
            )
        })
    }

    fn restart_btusb(&mut self) -> Result<(), String> {
        self.stop_btusb()?;
        self.ensure_btusb_running()
    }

    fn stop_btusb(&mut self) -> Result<(), String> {
        let Some(mut child) = self.btusb.take() else {
            return Err("redbear-btusb is not owned by this checker run".to_string());
        };

        child
            .kill()
            .map_err(|err| format!("failed to stop redbear-btusb: {err}"))?;
        let _ = child.wait();
        wait_for_condition(
            "redbear-btusb runtime disappearance",
            Duration::from_secs(5),
            || Ok::<bool, String>(!transport_runtime_visible()),
        )
    }

    fn stop_btusb_quietly(&mut self) {
        if let Some(mut child) = self.btusb.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn remove_test_bond(&self) -> Result<(), String> {
        if !Path::new(BTCTL_ROOT).exists() {
            return Ok(());
        }

        let _ = run_command("redbear-btctl", &["--bond-remove", ADAPTER, BOND_ID])?;
        Ok(())
    }
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{PROGRAM}: {err}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    parse_args(PROGRAM, USAGE, std::env::args()).map_err(|err| {
        if err.is_empty() {
            process::exit(0);
        }
        err
    })?;

    println!("=== Red Bear OS Bluetooth Battery Check ===");
    require_path("/usr/bin/redbear-btusb")?;
    require_path("/usr/bin/redbear-btctl")?;
    require_path("/usr/bin/redbear-info")?;

    let mut session = RuntimeSession::default();
    print_checked_command("transport probe", "redbear-btusb", &["--probe"])?;
    print_checked_command("host/control probe", "redbear-btctl", &["--probe"])?;

    session.ensure_btusb_running()?;
    session.ensure_btctl_running()?;
    verify_scheme_surface()?;
    verify_runtime_status()?;

    run_cycle("cycle-1", true)?;
    run_cycle("cycle-2", false)?;
    verify_btctl_restart_cleanup(&mut session)?;
    verify_btusb_restart_path(&mut session)?;

    ensure_bond_absent()?;
    verify_disconnected_state("final-state")?;

    println!("BLUETOOTH_BATTERY_CHECK=pass");
    println!("PASS: bounded Bluetooth Battery Level slice exercised inside target runtime");
    println!("NOTE: this proves explicit-startup btusb/btctl startup, repeated packaged helper runs in one boot, daemon restart cleanup, stale-state cleanup after disconnect, and one experimental battery-sensor Battery Level read-only workload; it does not prove controller bring-up, general device traffic, generic GATT, real pairing, write support, notify support, or broad BLE maturity");
    Ok(())
}

fn require_path(path: &str) -> Result<(), String> {
    if Path::new(path).exists() {
        println!("{path}");
        Ok(())
    } else {
        Err(format!("missing {path}"))
    }
}

fn open_log_file(path: &str) -> Result<Stdio, String> {
    let file =
        fs::File::create(path).map_err(|err| format!("failed to open log file {path}: {err}"))?;
    Ok(Stdio::from(file))
}

fn run_cycle(label: &str, verify_info: bool) -> Result<(), String> {
    println!("=== {label}: bounded battery-level workload ===");
    ensure_bond_absent()?;
    verify_disconnected_state(label)?;
    let scan_output =
        print_checked_command(&format!("{label}: scan"), "redbear-btctl", &["--scan"])?;
    require_contains(&scan_output, &format!("adapter={ADAPTER}"))?;
    require_contains(&scan_output, "status=scanning")?;
    require_contains(&scan_output, "scan_results=")?;
    expect_read_failure(
        &format!("{label}: read-before-connect"),
        "rejected-not-connected",
        "run --connect before the experimental read",
    )?;

    let add_output = print_checked_command(
        &format!("{label}: add stub bond"),
        "redbear-btctl",
        &["--bond-add-stub", ADAPTER, BOND_ID, BOND_ALIAS],
    )?;
    require_contains(&add_output, "persisted=true")?;
    require_contains(&add_output, &format!("bond.bond_id={BOND_ID}"))?;
    require_contains(&add_output, &format!("bond.alias={BOND_ALIAS}"))?;
    verify_bond_present()?;

    let connect_output = print_checked_command(
        &format!("{label}: connect"),
        "redbear-btctl",
        &["--connect", ADAPTER, BOND_ID],
    )?;
    require_contains(&connect_output, "connection_state=stub-connected")?;
    require_contains(&connect_output, &format!("connected_bond_ids={BOND_ID}"))?;
    require_contains(
        &connect_output,
        &format!("connect_result=stub-connected bond_id={BOND_ID}"),
    )?;
    verify_connection_state_contains(BOND_ID)?;
    require_file_contains(
        BTCTL_CONNECT_RESULT_PATH,
        &format!("connect_result=stub-connected bond_id={BOND_ID}"),
    )?;

    let read_output = print_checked_command(
        &format!("{label}: battery-level read"),
        "redbear-btctl",
        &[
            "--read-char",
            ADAPTER,
            BOND_ID,
            BATTERY_SERVICE_UUID,
            BATTERY_LEVEL_CHAR_UUID,
        ],
    )?;
    require_contains(&read_output, "read_char_result=stub-value")?;
    require_contains(&read_output, &format!("workload={EXPERIMENTAL_WORKLOAD}"))?;
    require_contains(
        &read_output,
        &format!("peripheral_class={PERIPHERAL_CLASS}"),
    )?;
    require_contains(&read_output, &format!("bond_id={BOND_ID}"))?;
    require_contains(
        &read_output,
        &format!("service_uuid={BATTERY_SERVICE_UUID}"),
    )?;
    require_contains(
        &read_output,
        &format!("char_uuid={BATTERY_LEVEL_CHAR_UUID}"),
    )?;
    require_contains(&read_output, "access=read-only")?;
    require_contains(&read_output, "value_percent=87")?;
    require_file_contains(BTCTL_READ_CHAR_RESULT_PATH, "read_char_result=stub-value")?;
    require_file_contains(
        BTCTL_READ_CHAR_RESULT_PATH,
        &format!("workload={EXPERIMENTAL_WORKLOAD}"),
    )?;

    if verify_info {
        let info = print_checked_command(
            &format!("{label}: redbear-info --verbose"),
            "redbear-info",
            &["--verbose"],
        )?;
        require_contains(
            &info,
            &format!("Bluetooth connection state: connection_state=stub-connected"),
        )?;
        require_contains(
            &info,
            &format!("Bluetooth connect result: connect_result=stub-connected bond_id={BOND_ID}"),
        )?;
        require_contains(&info, "Bluetooth bond store: /var/lib/bluetooth/hci0/bonds")?;
        require_contains(&info, "Bluetooth bond count: ")?;
        require_contains(
            &info,
            "Bluetooth experimental BLE read: read_char_result=stub-value",
        )?;
        require_contains(&info, &format!("workload={EXPERIMENTAL_WORKLOAD}"))?;
        require_contains(&info, &format!("peripheral_class={PERIPHERAL_CLASS}"))?;
        require_contains(&info, "does not prove controller bring-up, general device traffic, generic GATT, real pairing, validated reconnect semantics, write support, or notify support beyond the experimental battery-sensor read-only workload")?;
    }

    let disconnect_output = print_checked_command(
        &format!("{label}: disconnect"),
        "redbear-btctl",
        &["--disconnect", ADAPTER, BOND_ID],
    )?;
    require_contains(
        &disconnect_output,
        &format!("disconnect_result=stub-disconnected bond_id={BOND_ID}"),
    )?;
    verify_disconnected_state(label)?;
    expect_read_failure(
        &format!("{label}: read-after-disconnect"),
        "rejected-not-connected",
        "run --connect before the experimental read",
    )?;

    let remove_output = print_checked_command(
        &format!("{label}: remove stub bond"),
        "redbear-btctl",
        &["--bond-remove", ADAPTER, BOND_ID],
    )?;
    require_contains(&remove_output, "removed=true")?;
    ensure_bond_absent()?;
    Ok(())
}

fn verify_btctl_restart_cleanup(_session: &mut RuntimeSession) -> Result<(), String> {
    println!("=== init-managed btctl wiring ===");
    ensure_bond_absent()?;
    verify_scheme_surface()?;
    require_file_contains(
        BTCTL_CONNECTION_STATE_PATH,
        "connection_state=stub-disconnected",
    )?;
    require_file_not_contains(BTCTL_CONNECTION_STATE_PATH, BOND_ID)?;
    require_file_contains(BTCTL_READ_CHAR_RESULT_PATH, "read_char_result=not-run")?;
    let status = print_checked_command("btctl wiring status", "redbear-btctl", &["--status"])?;
    require_contains(&status, "status=adapter-visible")?;
    Ok(())
}

fn verify_btusb_restart_path(session: &mut RuntimeSession) -> Result<(), String> {
    println!("=== restart: redbear-btusb transport honesty ===");
    ensure_bond_absent()?;

    let _ = print_checked_command(
        "transport-restart-prep: add stub bond",
        "redbear-btctl",
        &["--bond-add-stub", ADAPTER, BOND_ID, BOND_ALIAS],
    )?;

    session.stop_btusb()?;
    let btusb_status = print_checked_command(
        "transport status after stop",
        "redbear-btusb",
        &["--status"],
    )?;
    require_contains(&btusb_status, "runtime_visibility=installed-only")?;
    require_contains(&btusb_status, "daemon_status=inactive")?;

    let failure = run_command("redbear-btctl", &["--connect", ADAPTER, BOND_ID])?;
    if failure.success {
        return Err("expected redbear-btctl --connect to fail while btusb is stopped".to_string());
    }
    print_capture("transport-stop connect failure", &failure);
    require_file_contains(
        BTCTL_CONNECT_RESULT_PATH,
        &format!("connect_result=rejected-transport-not-runtime-visible bond_id={BOND_ID}"),
    )?;
    require_file_contains(BTCTL_LAST_ERROR_PATH, "start redbear-btusb explicitly")?;

    session.restart_btusb()?;
    verify_runtime_status()?;

    let connect_output = print_checked_command(
        "transport-restart: connect",
        "redbear-btctl",
        &["--connect", ADAPTER, BOND_ID],
    )?;
    require_contains(&connect_output, "connection_state=stub-connected")?;

    let read_output = print_checked_command(
        "transport-restart: battery-level read",
        "redbear-btctl",
        &[
            "--read-char",
            ADAPTER,
            BOND_ID,
            BATTERY_SERVICE_UUID,
            BATTERY_LEVEL_CHAR_UUID,
        ],
    )?;
    require_contains(&read_output, "read_char_result=stub-value")?;

    let disconnect_output = print_checked_command(
        "transport-restart: disconnect",
        "redbear-btctl",
        &["--disconnect", ADAPTER, BOND_ID],
    )?;
    require_contains(
        &disconnect_output,
        &format!("disconnect_result=stub-disconnected bond_id={BOND_ID}"),
    )?;

    let remove_output = print_checked_command(
        "transport-restart: remove bond",
        "redbear-btctl",
        &["--bond-remove", ADAPTER, BOND_ID],
    )?;
    require_contains(&remove_output, "removed=true")?;
    ensure_bond_absent()?;
    Ok(())
}

fn verify_scheme_surface() -> Result<(), String> {
    require_path(BTCTL_ROOT)?;
    require_path("/scheme/btctl/adapters")?;
    require_path(BTCTL_ADAPTER_ROOT)?;
    require_path(BTCTL_CONNECTION_STATE_PATH)?;
    require_path(BTCTL_CONNECT_RESULT_PATH)?;
    require_path(BTCTL_DISCONNECT_RESULT_PATH)?;
    require_path(BTCTL_READ_CHAR_RESULT_PATH)?;
    require_path(BTCTL_LAST_ERROR_PATH)?;
    require_path(BTCTL_BONDS_PATH)?;
    Ok(())
}

fn verify_runtime_status() -> Result<(), String> {
    let btusb_status = print_checked_command("transport status", "redbear-btusb", &["--status"])?;
    require_contains(&btusb_status, "runtime_visibility=runtime-visible")?;
    require_contains(&btusb_status, "daemon_status=running")?;

    let btctl_status =
        print_checked_command("host/control status", "redbear-btctl", &["--status"])?;
    require_contains(&btctl_status, &format!("adapter={ADAPTER}"))?;
    require_contains(&btctl_status, "status=adapter-visible")?;
    require_contains(&btctl_status, "transport_status=transport=usb")?;
    Ok(())
}

fn verify_bond_present() -> Result<(), String> {
    let bond_list = print_checked_command("bond list", "redbear-btctl", &["--bond-list", ADAPTER])?;
    require_contains(&bond_list, &format!("bond_id={BOND_ID}"))?;
    require_contains(&bond_list, &format!("alias={BOND_ALIAS}"))?;
    require_file_contains(BTCTL_BONDS_PATH, BOND_ID)?;
    require_path(BTCTL_BOND_PATH)?;
    require_file_contains(BTCTL_BOND_PATH, &format!("bond_id={BOND_ID}"))?;
    require_file_contains(BTCTL_BOND_PATH, &format!("alias={BOND_ALIAS}"))?;
    require_file_contains(BTCTL_BOND_PATH, "source=stub-cli")?;
    Ok(())
}

fn ensure_bond_absent() -> Result<(), String> {
    let bond_list = print_checked_command(
        "bond list cleanup",
        "redbear-btctl",
        &["--bond-list", ADAPTER],
    )?;
    if bond_list_contains(&bond_list, BOND_ID) {
        return Err(format!("expected {BOND_ID} to be absent from bond list"));
    }
    require_file_not_contains(BTCTL_BONDS_PATH, BOND_ID).or_else(|_| {
        if Path::new(BTCTL_BONDS_PATH).exists() {
            Ok(())
        } else {
            Err("bond listing path unexpectedly missing".to_string())
        }
    })?;
    if Path::new(BTCTL_BOND_PATH).exists() {
        return Err(format!("expected {BTCTL_BOND_PATH} to be absent"));
    }
    Ok(())
}

fn verify_connection_state_contains(bond_id: &str) -> Result<(), String> {
    let state = read_text(BTCTL_CONNECTION_STATE_PATH)?;
    require_contains(&state, "connection_state=stub-connected")?;
    if connected_bond_ids_include(&state, bond_id) {
        Ok(())
    } else {
        Err(format!("connection state did not include {bond_id}"))
    }
}

fn verify_disconnected_state(context: &str) -> Result<(), String> {
    let state = read_text(BTCTL_CONNECTION_STATE_PATH)?;
    require_contains(&state, "connection_state=stub-disconnected")?;
    if connected_bond_ids_include(&state, BOND_ID) {
        return Err(format!(
            "{context}: expected disconnected state to exclude {BOND_ID}"
        ));
    }
    let _ = require_file_contains(
        BTCTL_DISCONNECT_RESULT_PATH,
        "disconnect_result=stub-disconnected",
    );
    Ok(())
}

fn expect_read_failure(
    label: &str,
    result_marker: &str,
    last_error_marker: &str,
) -> Result<(), String> {
    let failure = run_command(
        "redbear-btctl",
        &[
            "--read-char",
            ADAPTER,
            BOND_ID,
            BATTERY_SERVICE_UUID,
            BATTERY_LEVEL_CHAR_UUID,
        ],
    )?;
    if failure.success {
        return Err(format!("{label}: expected read-char command to fail"));
    }
    print_capture(label, &failure);
    require_file_contains(
        BTCTL_READ_CHAR_RESULT_PATH,
        &format!("read_char_result={result_marker}"),
    )?;
    require_file_contains(BTCTL_LAST_ERROR_PATH, last_error_marker)?;
    Ok(())
}

fn print_checked_command(label: &str, program: &str, args: &[&str]) -> Result<String, String> {
    let capture = run_command(program, args)?;
    if !capture.success {
        return Err(command_failure(
            &format!("{program} {}", args.join(" ")),
            &capture,
        ));
    }
    print_capture(label, &capture);
    Ok(capture.stdout)
}

fn print_capture(label: &str, capture: &CommandCapture) {
    println!("--- {label} ---");
    if !capture.stdout.trim().is_empty() {
        print!("{}", capture.stdout);
    }
    if !capture.stderr.trim().is_empty() {
        eprint!("{}", capture.stderr);
    }
}

fn run_command(program: &str, args: &[&str]) -> Result<CommandCapture, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|err| format!("failed to run {program} {:?}: {err}", args))?;
    Ok(CommandCapture {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        success: output.status.success(),
    })
}

fn command_failure(label: &str, capture: &CommandCapture) -> String {
    let stderr = capture.stderr.trim();
    if !stderr.is_empty() {
        format!("{label} failed: {stderr}")
    } else {
        let stdout = capture.stdout.trim();
        if stdout.is_empty() {
            format!("{label} failed")
        } else {
            format!("{label} failed: {stdout}")
        }
    }
}

fn wait_for_condition<F>(label: &str, timeout: Duration, mut predicate: F) -> Result<(), String>
where
    F: FnMut() -> Result<bool, String>,
{
    let start = Instant::now();
    loop {
        if predicate()? {
            return Ok(());
        }
        if start.elapsed() >= timeout {
            return Err(format!("timed out waiting for {label}"));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn transport_runtime_visible() -> bool {
    read_text(TRANSPORT_STATUS_PATH)
        .map(|status| status.contains("runtime_visibility=runtime-visible"))
        .unwrap_or(false)
}

fn read_text(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|err| format!("failed to read {path}: {err}"))
}

fn require_contains(haystack: &str, needle: &str) -> Result<(), String> {
    if haystack.contains(needle) {
        Ok(())
    } else {
        Err(format!("missing {needle}"))
    }
}

fn require_file_contains(path: &str, needle: &str) -> Result<(), String> {
    let content = read_text(path)?;
    require_contains(&content, needle)
}

fn require_file_not_contains(path: &str, needle: &str) -> Result<(), String> {
    let content = read_text(path)?;
    if content.contains(needle) {
        Err(format!("unexpected {needle} in {path}"))
    } else {
        Ok(())
    }
}

fn line_with_prefix<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.lines()
        .map(str::trim)
        .find(|line| line.starts_with(prefix))
}

fn connected_bond_ids_include(text: &str, bond_id: &str) -> bool {
    line_with_prefix(text, "connected_bond_ids=")
        .map(|line| {
            line.trim_start_matches("connected_bond_ids=")
                .split(',')
                .map(str::trim)
                .any(|candidate| !candidate.is_empty() && candidate == bond_id)
        })
        .unwrap_or(false)
}

fn bond_list_contains(text: &str, bond_id: &str) -> bool {
    text.lines().map(str::trim).any(|line| {
        line == format!("bond_id={bond_id}")
            || line.ends_with(&format!(".bond_id={bond_id}"))
            || line == bond_id
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_with_prefix_returns_matching_line() {
        let text = "alpha=1\nconnected_bond_ids=AA:BB:CC:DD:EE:FF\n";
        assert_eq!(
            line_with_prefix(text, "connected_bond_ids="),
            Some("connected_bond_ids=AA:BB:CC:DD:EE:FF")
        );
    }

    #[test]
    fn connected_bond_ids_include_detects_present_and_absent_ids() {
        let text = "connection_state=stub-connected\nconnected_bond_ids=AA:BB:CC:DD:EE:FF,11:22:33:44:55:66\n";
        assert!(connected_bond_ids_include(text, "AA:BB:CC:DD:EE:FF"));
        assert!(!connected_bond_ids_include(text, "77:88:99:AA:BB:CC"));
    }

    #[test]
    fn bond_list_contains_accepts_cli_and_scheme_shapes() {
        assert!(bond_list_contains(
            "bond.0.bond_id=AA:BB:CC:DD:EE:FF\n",
            "AA:BB:CC:DD:EE:FF"
        ));
        assert!(bond_list_contains(
            "AA:BB:CC:DD:EE:FF\n",
            "AA:BB:CC:DD:EE:FF"
        ));
        assert!(!bond_list_contains(
            "bond.0.bond_id=11:22:33:44:55:66\n",
            "AA:BB:CC:DD:EE:FF"
        ));
    }
}
