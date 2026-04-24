mod backend;
mod bond_store;
mod scheme;

use std::env;
#[cfg(target_os = "redox")]
use std::fs;
#[cfg(target_os = "redox")]
use std::os::fd::RawFd;
use std::process;

#[cfg(not(target_os = "redox"))]
use backend::connection_state_lines;
use backend::{Backend, StubBackend};
use bond_store::BondRecord;
#[cfg(target_os = "redox")]
use log::error;
#[cfg(target_os = "redox")]
use log::info;
#[cfg(target_os = "redox")]
use log::warn;
use log::LevelFilter;
#[cfg(target_os = "redox")]
use redox_scheme::{scheme::SchemeSync, SignalBehavior, Socket};
#[cfg(target_os = "redox")]
use scheme::BtCtlScheme;

fn init_logging(level: LevelFilter) {
    log::set_max_level(level);
}

#[cfg(target_os = "redox")]
unsafe fn get_init_notify_fd() -> Option<RawFd> {
    let Ok(value) = env::var("INIT_NOTIFY") else {
        return None;
    };
    let Ok(fd) = value.parse::<RawFd>() else {
        return None;
    };
    unsafe {
        libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
    }
    Some(fd)
}

#[cfg(target_os = "redox")]
fn notify_scheme_ready(notify_fd: Option<RawFd>, socket: &Socket, scheme: &mut BtCtlScheme) {
    let Some(notify_fd) = notify_fd else {
        return;
    };

    let Ok(cap_id) = scheme.scheme_root() else {
        warn!("redbear-btctl: scheme_root failed; continuing without scheme notification");
        return;
    };
    let Ok(cap_fd) = socket.create_this_scheme_fd(0, cap_id, 0, 0) else {
        warn!("redbear-btctl: create_this_scheme_fd failed; continuing without scheme notification");
        return;
    };

    if let Err(err) = syscall::call_wo(
        notify_fd as usize,
        &libredox::Fd::new(cap_fd).into_raw().to_ne_bytes(),
        syscall::CallFlags::FD,
        &[],
    ) {
        warn!(
            "redbear-btctl: failed to notify init that scheme is ready ({err}); continuing with manual startup"
        );
    }
}

fn build_backend() -> Box<dyn Backend> {
    Box::new(StubBackend::from_env())
}

fn default_adapter(backend: &dyn Backend) -> String {
    backend
        .adapters()
        .into_iter()
        .next()
        .unwrap_or_else(|| "hci0".to_string())
}

fn format_lines(lines: &[String]) -> String {
    if lines.is_empty() {
        "\n".to_string()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

fn format_bond_record(lines: &mut Vec<String>, prefix: &str, bond: &BondRecord) {
    lines.push(format!("{prefix}.bond_id={}", bond.bond_id));
    if let Some(alias) = &bond.alias {
        lines.push(format!("{prefix}.alias={alias}"));
    }
    lines.push(format!(
        "{prefix}.created_at_epoch={}",
        bond.created_at_epoch
    ));
    lines.push(format!("{prefix}.source={}", bond.source));
}

fn required_arg(args: &[String], index: usize, usage: &str) -> Result<String, String> {
    args.get(index)
        .cloned()
        .ok_or_else(|| format!("missing argument; usage: {usage}"))
}

#[cfg(target_os = "redox")]
fn scheme_lines(path: &str) -> Result<Vec<String>, String> {
    fs::read_to_string(path)
        .map_err(|err| format!("failed to read {path}: {err}"))
        .map(|content| {
            content
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
}

#[cfg(target_os = "redox")]
fn scheme_line(path: &str, prefix: &str) -> Result<String, String> {
    scheme_lines(path)?
        .into_iter()
        .find(|line| prefix.is_empty() || line.starts_with(prefix))
        .ok_or_else(|| format!("missing expected data in {path}"))
}

#[cfg(target_os = "redox")]
fn execute_scheme_connection(action: &str, adapter: &str, bond_id: &str) -> Result<String, String> {
    let control_path = format!("/scheme/btctl/adapters/{adapter}/{action}");
    if !std::path::Path::new(&control_path).exists() {
        return Err(format!(
            "redbear-btctl daemon is not serving scheme:btctl; {action} requires the live daemon surface on Redox"
        ));
    }

    fs::write(&control_path, bond_id)
        .map_err(|err| format!("failed to write {control_path}: {err}"))?;

    let status_line = scheme_line(
        &format!("/scheme/btctl/adapters/{adapter}/status"),
        "status=",
    )?;
    let transport_status = scheme_line(
        &format!("/scheme/btctl/adapters/{adapter}/transport-status"),
        "",
    )?;
    let connection_state = scheme_lines(&format!(
        "/scheme/btctl/adapters/{adapter}/connection-state"
    ))?;
    let result_line = scheme_line(
        &format!("/scheme/btctl/adapters/{adapter}/{action}-result"),
        &format!("{action}_result="),
    )?;

    let mut lines = vec![
        format!("adapter={adapter}"),
        status_line,
        format!("transport_status={transport_status}"),
    ];
    lines.extend(connection_state);
    lines.push(result_line);
    Ok(format_lines(&lines))
}

#[cfg(target_os = "redox")]
fn execute_scheme_read_char(
    adapter: &str,
    bond_id: &str,
    service_uuid: &str,
    char_uuid: &str,
) -> Result<String, String> {
    let control_path = format!("/scheme/btctl/adapters/{adapter}/read-char");
    if !std::path::Path::new(&control_path).exists() {
        return Err(
            "redbear-btctl daemon is not serving scheme:btctl; read-char requires the live daemon surface on Redox"
                .to_string(),
        );
    }

    let request =
        format!("bond_id={bond_id}\nservice_uuid={service_uuid}\nchar_uuid={char_uuid}\n");
    fs::write(&control_path, request)
        .map_err(|err| format!("failed to write {control_path}: {err}"))?;

    let status_line = scheme_line(
        &format!("/scheme/btctl/adapters/{adapter}/status"),
        "status=",
    )?;
    let transport_status = scheme_line(
        &format!("/scheme/btctl/adapters/{adapter}/transport-status"),
        "",
    )?;
    let connection_state = scheme_lines(&format!(
        "/scheme/btctl/adapters/{adapter}/connection-state"
    ))?;
    let result_line = scheme_line(
        &format!("/scheme/btctl/adapters/{adapter}/read-char-result"),
        "read_char_result=",
    )?;

    let mut lines = vec![
        format!("adapter={adapter}"),
        status_line,
        format!("transport_status={transport_status}"),
    ];
    lines.extend(connection_state);
    lines.push(result_line);
    Ok(format_lines(&lines))
}

fn execute(args: &[String], backend: &mut dyn Backend) -> Result<Option<String>, String> {
    match args.first().map(String::as_str) {
        Some("--probe") => Ok(Some(format_lines(&[
            format!("adapters={}", backend.adapters().join(",")),
            format!("capabilities={}", backend.capabilities().join(",")),
        ]))),
        Some("--status") => {
            let adapter = args
                .get(1)
                .cloned()
                .unwrap_or_else(|| default_adapter(backend));
            let status = backend.status(&adapter)?;
            let bond_store_root = backend.bond_store_path(&adapter)?;
            let bond_count = backend.load_bonds(&adapter)?.len();
            Ok(Some(format_lines(&[
                format!("adapter={adapter}"),
                format!("status={}", status.as_str()),
                format!("transport_status={}", backend.transport_status(&adapter)),
                format!(
                    "scan_results_count={}",
                    backend.default_scan_results(&adapter).len()
                ),
                format!(
                    "connected_bond_count={}",
                    backend.connected_bond_ids(&adapter)?.len()
                ),
                format!("bond_count={bond_count}"),
                format!("bond_store_root={bond_store_root}"),
            ])))
        }
        Some("--scan") => {
            let adapter = args
                .get(1)
                .cloned()
                .unwrap_or_else(|| default_adapter(backend));
            let results = backend.scan(&adapter)?;
            Ok(Some(format_lines(&[
                format!("adapter={adapter}"),
                format!("status={}", backend::AdapterStatus::Scanning.as_str()),
                format!("transport_status={}", backend.transport_status(&adapter)),
                format!("scan_results={}", results.join(",")),
            ])))
        }
        Some("--bond-list") => {
            let adapter = args
                .get(1)
                .cloned()
                .unwrap_or_else(|| default_adapter(backend));
            let bonds = backend.load_bonds(&adapter)?;
            let bond_store_root = backend.bond_store_path(&adapter)?;
            let mut lines = vec![
                format!("adapter={adapter}"),
                format!("bond_store_root={bond_store_root}"),
                format!("bond_count={}", bonds.len()),
                "note=stub-bond-records-only".to_string(),
            ];
            for (index, bond) in bonds.iter().enumerate() {
                format_bond_record(&mut lines, &format!("bond.{index}"), bond);
            }
            Ok(Some(format_lines(&lines)))
        }
        Some("--bond-add-stub") => {
            let adapter = required_arg(args, 1, "--bond-add-stub <adapter> <bond-id> [alias]")?;
            let bond_id = required_arg(args, 2, "--bond-add-stub <adapter> <bond-id> [alias]")?;
            let alias = args.get(3).map(String::as_str);
            let bond = backend.add_stub_bond(&adapter, &bond_id, alias)?;
            let mut lines = vec![
                format!("adapter={adapter}"),
                format!("bond_store_root={}", backend.bond_store_path(&adapter)?),
                "persisted=true".to_string(),
                "note=stub-bond-record-only".to_string(),
            ];
            format_bond_record(&mut lines, "bond", &bond);
            Ok(Some(format_lines(&lines)))
        }
        Some("--bond-remove") => {
            let adapter = required_arg(args, 1, "--bond-remove <adapter> <bond-id>")?;
            let bond_id = required_arg(args, 2, "--bond-remove <adapter> <bond-id>")?;
            let removed = backend.remove_bond(&adapter, &bond_id)?;
            Ok(Some(format_lines(&[
                format!("adapter={adapter}"),
                format!("bond_store_root={}", backend.bond_store_path(&adapter)?),
                format!("bond_id={bond_id}"),
                format!("removed={removed}"),
                "note=stub-bond-record-only".to_string(),
            ])))
        }
        Some("--connect") => {
            let adapter = required_arg(args, 1, "--connect <adapter> <bond-id>")?;
            let bond_id = required_arg(args, 2, "--connect <adapter> <bond-id>")?;
            #[cfg(target_os = "redox")]
            {
                execute_scheme_connection("connect", &adapter, &bond_id).map(Some)
            }

            #[cfg(not(target_os = "redox"))]
            {
                backend.connect(&adapter, &bond_id)?;
                let connected_bond_ids = backend.connected_bond_ids(&adapter)?;
                let mut lines = vec![
                    format!("adapter={adapter}"),
                    format!("status={}", backend.status(&adapter)?.as_str()),
                    format!("transport_status={}", backend.transport_status(&adapter)),
                ];
                lines.extend(connection_state_lines(&connected_bond_ids));
                lines.push(backend.connect_result(&adapter)?);
                lines.push("runtime_scope=process-local-host-cli".to_string());
                lines.push(
                    "note=host-cli-connect-output-is-ephemeral-until-a-live-btctl-daemon-serves-scheme-btctl"
                        .to_string(),
                );
                Ok(Some(format_lines(&lines)))
            }
        }
        Some("--disconnect") => {
            let adapter = required_arg(args, 1, "--disconnect <adapter> <bond-id>")?;
            let bond_id = required_arg(args, 2, "--disconnect <adapter> <bond-id>")?;
            #[cfg(target_os = "redox")]
            {
                execute_scheme_connection("disconnect", &adapter, &bond_id).map(Some)
            }

            #[cfg(not(target_os = "redox"))]
            {
                backend.disconnect(&adapter, &bond_id)?;
                let connected_bond_ids = backend.connected_bond_ids(&adapter)?;
                let mut lines = vec![
                    format!("adapter={adapter}"),
                    format!("status={}", backend.status(&adapter)?.as_str()),
                    format!("transport_status={}", backend.transport_status(&adapter)),
                ];
                lines.extend(connection_state_lines(&connected_bond_ids));
                lines.push(backend.disconnect_result(&adapter)?);
                lines.push("runtime_scope=process-local-host-cli".to_string());
                lines.push(
                    "note=host-cli-disconnect-output-is-ephemeral-until-a-live-btctl-daemon-serves-scheme-btctl"
                        .to_string(),
                );
                Ok(Some(format_lines(&lines)))
            }
        }
        Some("--read-char") => {
            let adapter = required_arg(
                args,
                1,
                "--read-char <adapter> <bond-id> <service-uuid> <char-uuid>",
            )?;
            let bond_id = required_arg(
                args,
                2,
                "--read-char <adapter> <bond-id> <service-uuid> <char-uuid>",
            )?;
            let service_uuid = required_arg(
                args,
                3,
                "--read-char <adapter> <bond-id> <service-uuid> <char-uuid>",
            )?;
            let char_uuid = required_arg(
                args,
                4,
                "--read-char <adapter> <bond-id> <service-uuid> <char-uuid>",
            )?;
            #[cfg(target_os = "redox")]
            {
                execute_scheme_read_char(&adapter, &bond_id, &service_uuid, &char_uuid).map(Some)
            }

            #[cfg(not(target_os = "redox"))]
            {
                backend.read_char(&adapter, &bond_id, &service_uuid, &char_uuid)?;
                let connected_bond_ids = backend.connected_bond_ids(&adapter)?;
                let mut lines = vec![
                    format!("adapter={adapter}"),
                    format!("status={}", backend.status(&adapter)?.as_str()),
                    format!("transport_status={}", backend.transport_status(&adapter)),
                ];
                lines.extend(connection_state_lines(&connected_bond_ids));
                lines.push(backend.read_char_result(&adapter)?);
                lines.push("runtime_scope=process-local-host-cli".to_string());
                lines.push(
                    "note=host-cli-read-char-output-is-ephemeral-until-a-live-btctl-daemon-serves-scheme-btctl"
                        .to_string(),
                );
                Ok(Some(format_lines(&lines)))
            }
        }
        None => Ok(None),
        Some(other) => Err(format!("unknown argument: {other}")),
    }
}

fn main() {
    let log_level = match env::var("REDBEAR_BTCTL_LOG").as_deref() {
        Ok("debug") => LevelFilter::Debug,
        Ok("trace") => LevelFilter::Trace,
        Ok("warn") => LevelFilter::Warn,
        Ok("error") => LevelFilter::Error,
        _ => LevelFilter::Info,
    };
    init_logging(log_level);

    let args = env::args().skip(1).collect::<Vec<_>>();
    let mut backend = build_backend();

    match execute(&args, backend.as_mut()) {
        Ok(Some(output)) => {
            print!("{output}");
            return;
        }
        Ok(None) => {}
        Err(err) => {
            eprintln!("redbear-btctl: {err}");
            process::exit(1);
        }
    }

    #[cfg(not(target_os = "redox"))]
    {
        eprintln!("redbear-btctl: daemon mode is only supported on Redox; use --probe on host");
        process::exit(1);
    }

    #[cfg(target_os = "redox")]
    {
        let notify_fd = unsafe { get_init_notify_fd() };
        let socket = match Socket::create() {
            Ok(s) => s,
            Err(err) => {
                error!("redbear-btctl: failed to create scheme socket: {err}");
                process::exit(1);
            }
        };
        let mut scheme = BtCtlScheme::new(build_backend());
        let mut state = redox_scheme::scheme::SchemeState::new();

        notify_scheme_ready(notify_fd, &socket, &mut scheme);
        match libredox::call::setrens(0, 0) {
            Ok(_) => info!("redbear-btctl: registered scheme:btctl"),
            Err(err) => {
                error!("redbear-btctl: failed to enter null namespace: {err}");
                process::exit(1);
            }
        }

        let mut exit_code = 0;
        loop {
            let request = match socket.next_request(SignalBehavior::Restart) {
                Ok(Some(req)) => req,
                Ok(None) => {
                    info!("redbear-btctl: scheme socket closed, shutting down");
                    break;
                }
                Err(err) => {
                    error!("redbear-btctl: failed to read scheme request: {err}");
                    exit_code = 1;
                    break;
                }
            };
            match request.kind() {
                redox_scheme::RequestKind::Call(request) => {
                    let response = request.handle_sync(&mut scheme, &mut state);
                    if let Err(err) = socket.write_response(response, SignalBehavior::Restart) {
                        error!("redbear-btctl: failed to write response: {err}");
                        exit_code = 1;
                        break;
                    }
                }
                _ => {}
            }
        }

        process::exit(exit_code);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::StubBackend;
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("{name}-{stamp}"))
    }

    fn stub_backend(status_path: PathBuf, bond_store_root: PathBuf) -> StubBackend {
        StubBackend::new_for_test(
            vec!["hci0".to_string()],
            vec!["demo-beacon".to_string(), "demo-sensor".to_string()],
            status_path,
            bond_store_root,
        )
    }

    #[test]
    fn probe_output_matches_bounded_shape() {
        let mut backend = stub_backend(
            temp_path("rbos-btctl-probe"),
            temp_path("rbos-btctl-probe-bonds"),
        );
        let output = execute(&["--probe".to_string()], &mut backend)
            .unwrap()
            .unwrap();
        assert!(output.contains("adapters=hci0"));
        assert!(output.contains("capabilities=backend=stub"));
        assert!(output.contains("transport=usb"));
        assert!(output.contains("mode=ble-first"));
        assert!(output.contains("workload=battery-sensor-battery-level-read"));
        assert!(output.contains("read_char=true"));
        assert!(output.contains("bond_store=stub-cli"));
    }

    #[test]
    fn status_reports_explicit_startup_requirement_without_transport_runtime() {
        let bond_store_root = temp_path("rbos-btctl-status-bonds");
        let mut backend = stub_backend(temp_path("rbos-btctl-status-missing"), bond_store_root);
        let output = execute(&["--status".to_string()], &mut backend)
            .unwrap()
            .unwrap();
        assert!(output.contains("status=explicit-startup-required"));
        assert!(output.contains("runtime_visibility=installed-only"));
        assert!(output.contains("bond_count=0"));
    }

    #[test]
    fn scan_reports_stub_results_when_transport_runtime_is_visible() {
        let status_path = temp_path("rbos-btctl-status-visible");
        let bond_store_root = temp_path("rbos-btctl-status-visible-bonds");
        fs::write(
            &status_path,
            &format!(
                "transport=usb\nstartup=explicit\nupdated_at_epoch={}\nruntime_visibility=runtime-visible\n",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            ),
        )
        .unwrap();
        let mut backend = stub_backend(status_path.clone(), bond_store_root.clone());

        let output = execute(&["--scan".to_string()], &mut backend)
            .unwrap()
            .unwrap();
        assert!(output.contains("status=scanning"));
        assert!(output.contains("scan_results=demo-beacon,demo-sensor"));

        fs::remove_file(status_path).unwrap();
        fs::remove_dir_all(bond_store_root).ok();
    }

    #[test]
    fn bond_commands_persist_stub_records_across_cli_restarts() {
        let status_path = temp_path("rbos-btctl-bond-status");
        let bond_store_root = temp_path("rbos-btctl-bond-root");
        let mut writer = stub_backend(status_path.clone(), bond_store_root.clone());

        let empty = execute(&["--bond-list".to_string()], &mut writer)
            .unwrap()
            .unwrap();
        assert!(empty.contains("bond_count=0"));

        let added = execute(
            &[
                "--bond-add-stub".to_string(),
                "hci0".to_string(),
                "AA:BB:CC:DD:EE:FF".to_string(),
                "demo-sensor".to_string(),
            ],
            &mut writer,
        )
        .unwrap()
        .unwrap();
        assert!(added.contains("persisted=true"));
        assert!(added.contains("bond.bond_id=AA:BB:CC:DD:EE:FF"));

        let mut reader = stub_backend(status_path.clone(), bond_store_root.clone());
        let listed = execute(&["--bond-list".to_string()], &mut reader)
            .unwrap()
            .unwrap();
        assert!(listed.contains("bond_count=1"));
        assert!(listed.contains("bond.0.bond_id=AA:BB:CC:DD:EE:FF"));
        assert!(listed.contains("bond.0.alias=demo-sensor"));

        let removed = execute(
            &[
                "--bond-remove".to_string(),
                "hci0".to_string(),
                "AA:BB:CC:DD:EE:FF".to_string(),
            ],
            &mut reader,
        )
        .unwrap()
        .unwrap();
        assert!(removed.contains("removed=true"));

        let mut verifier = stub_backend(status_path, bond_store_root.clone());
        let final_list = execute(&["--bond-list".to_string()], &mut verifier)
            .unwrap()
            .unwrap();
        assert!(final_list.contains("bond_count=0"));

        fs::remove_dir_all(bond_store_root).unwrap();
    }

    #[test]
    fn connect_and_disconnect_commands_report_stub_control_results() {
        let status_path = temp_path("rbos-btctl-connect-cli-status");
        let bond_store_root = temp_path("rbos-btctl-connect-cli-bonds");
        fs::write(
            &status_path,
            &format!(
                "transport=usb\nstartup=explicit\nupdated_at_epoch={}\nruntime_visibility=runtime-visible\n",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            ),
        )
        .unwrap();
        let mut backend = stub_backend(status_path.clone(), bond_store_root.clone());

        execute(
            &[
                "--bond-add-stub".to_string(),
                "hci0".to_string(),
                "AA:BB:CC:DD:EE:FF".to_string(),
                "demo-sensor".to_string(),
            ],
            &mut backend,
        )
        .unwrap();

        let connected = execute(
            &[
                "--connect".to_string(),
                "hci0".to_string(),
                "AA:BB:CC:DD:EE:FF".to_string(),
            ],
            &mut backend,
        )
        .unwrap()
        .unwrap();
        assert!(connected.contains("connection_state=stub-connected"));
        assert!(connected.contains("connected_bond_ids=AA:BB:CC:DD:EE:FF"));
        assert!(connected.contains("connect_result=stub-connected bond_id=AA:BB:CC:DD:EE:FF"));
        assert!(connected.contains("runtime_scope=process-local-host-cli"));

        let disconnected = execute(
            &[
                "--disconnect".to_string(),
                "hci0".to_string(),
                "AA:BB:CC:DD:EE:FF".to_string(),
            ],
            &mut backend,
        )
        .unwrap()
        .unwrap();
        assert!(disconnected.contains("connection_state=stub-disconnected"));
        assert!(!disconnected.contains("connected_bond_ids=AA:BB:CC:DD:EE:FF"));
        assert!(
            disconnected.contains("disconnect_result=stub-disconnected bond_id=AA:BB:CC:DD:EE:FF")
        );
        assert!(disconnected.contains("runtime_scope=process-local-host-cli"));

        let missing_disconnect = execute(
            &[
                "--disconnect".to_string(),
                "hci0".to_string(),
                "11:22:33:44:55:66".to_string(),
            ],
            &mut backend,
        )
        .unwrap_err();
        assert!(missing_disconnect.contains("bond record not found"));

        fs::remove_file(status_path).unwrap();
        fs::remove_dir_all(bond_store_root).unwrap();
    }

    #[test]
    fn read_char_command_reports_bounded_battery_level_result() {
        let status_path = temp_path("rbos-btctl-read-char-cli-status");
        let bond_store_root = temp_path("rbos-btctl-read-char-cli-bonds");
        fs::write(
            &status_path,
            &format!(
                "transport=usb\nstartup=explicit\nupdated_at_epoch={}\nruntime_visibility=runtime-visible\n",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            ),
        )
        .unwrap();
        let mut backend = stub_backend(status_path.clone(), bond_store_root.clone());

        execute(
            &[
                "--bond-add-stub".to_string(),
                "hci0".to_string(),
                "AA:BB:CC:DD:EE:FF".to_string(),
                "demo-battery-sensor".to_string(),
            ],
            &mut backend,
        )
        .unwrap();
        execute(
            &[
                "--connect".to_string(),
                "hci0".to_string(),
                "AA:BB:CC:DD:EE:FF".to_string(),
            ],
            &mut backend,
        )
        .unwrap();

        let read_output = execute(
            &[
                "--read-char".to_string(),
                "hci0".to_string(),
                "AA:BB:CC:DD:EE:FF".to_string(),
                "0000180f-0000-1000-8000-00805f9b34fb".to_string(),
                "00002a19-0000-1000-8000-00805f9b34fb".to_string(),
            ],
            &mut backend,
        )
        .unwrap()
        .unwrap();
        assert!(read_output.contains("connection_state=stub-connected"));
        assert!(read_output.contains("read_char_result=stub-value"));
        assert!(read_output.contains("workload=battery-sensor-battery-level-read"));
        assert!(read_output.contains("peripheral_class=ble-battery-sensor"));
        assert!(read_output.contains("access=read-only"));
        assert!(read_output.contains("value_percent=87"));
        assert!(read_output.contains("runtime_scope=process-local-host-cli"));

        let read_err = execute(
            &[
                "--read-char".to_string(),
                "hci0".to_string(),
                "AA:BB:CC:DD:EE:FF".to_string(),
                "0000180f-0000-1000-8000-00805f9b34fb".to_string(),
                "00002a1a-0000-1000-8000-00805f9b34fb".to_string(),
            ],
            &mut backend,
        )
        .unwrap_err();
        assert!(read_err.contains("only the experimental"));

        fs::remove_file(status_path).unwrap();
        fs::remove_dir_all(bond_store_root).unwrap();
    }
}
