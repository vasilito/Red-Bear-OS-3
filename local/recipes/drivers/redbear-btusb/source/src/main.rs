use std::fs;
use std::path::{Path, PathBuf};
use std::process;
#[cfg(target_os = "redox")]
use std::thread;
#[cfg(target_os = "redox")]
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

const STATUS_FRESHNESS_SECS: u64 = 90;

#[derive(Clone, Debug, PartialEq, Eq)]
struct TransportConfig {
    adapters: Vec<String>,
    controller_family: String,
    status_file: PathBuf,
}

impl TransportConfig {
    fn from_env() -> Self {
        Self {
            adapters: parse_list(
                std::env::var("REDBEAR_BTUSB_STUB_ADAPTERS").ok().as_deref(),
                &["hci0"],
            ),
            controller_family: std::env::var("REDBEAR_BTUSB_STUB_FAMILY")
                .unwrap_or_else(|_| "usb-generic-bounded".to_string()),
            status_file: std::env::var_os("REDBEAR_BTUSB_STATUS_FILE")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/var/run/redbear-btusb/status")),
        }
    }

    fn probe_lines(&self) -> Vec<String> {
        vec![
            format!("adapters={}", self.adapters.join(",")),
            "transport=usb".to_string(),
            "startup=explicit".to_string(),
            "mode=ble-first".to_string(),
            format!("controller_family={}", self.controller_family),
        ]
    }

    fn render_status_lines(&self, runtime_visible: bool) -> Vec<String> {
        let mut lines = self.probe_lines();
        lines.push(format!("updated_at_epoch={}", current_epoch_seconds()));
        lines.push(format!(
            "runtime_visibility={}",
            if runtime_visible {
                "runtime-visible"
            } else {
                "installed-only"
            }
        ));
        lines.push(format!(
            "daemon_status={}",
            if runtime_visible {
                "running"
            } else {
                "inactive"
            }
        ));
        lines.push(format!("status_file={}", self.status_file.display()));
        lines
    }

    fn current_status_lines(&self) -> Vec<String> {
        read_status_lines(&self.status_file)
            .filter(|lines| status_lines_are_fresh(lines))
            .unwrap_or_else(|| self.render_status_lines(false))
    }

    fn write_status_file(&self) -> Result<(), String> {
        if let Some(parent) = self.status_file.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                format!(
                    "failed to create transport status directory {}: {err}",
                    parent.display()
                )
            })?;
        }
        fs::write(
            &self.status_file,
            format_lines(&self.render_status_lines(true)),
        )
        .map_err(|err| {
            format!(
                "failed to write transport status file {}: {err}",
                self.status_file.display()
            )
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
    Probe,
    Status,
    Daemon,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CommandOutcome {
    Print(String),
    RunDaemon,
}

fn parse_list(raw: Option<&str>, default: &[&str]) -> Vec<String> {
    raw.map(|value| {
        value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>()
    })
    .filter(|values| !values.is_empty())
    .unwrap_or_else(|| default.iter().map(|value| (*value).to_string()).collect())
}

fn format_lines(lines: &[String]) -> String {
    if lines.is_empty() {
        "\n".to_string()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

fn current_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn read_status_lines(path: &Path) -> Option<Vec<String>> {
    let content = fs::read_to_string(path).ok()?;
    let lines = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    Some(lines)
}

fn status_lines_are_fresh(lines: &[String]) -> bool {
    let updated_at = lines.iter().find_map(|line| {
        line.strip_prefix("updated_at_epoch=")
            .and_then(|value| value.parse::<u64>().ok())
    });

    updated_at
        .map(|timestamp| current_epoch_seconds().saturating_sub(timestamp) <= STATUS_FRESHNESS_SECS)
        .unwrap_or(false)
}

fn parse_command(args: &[String]) -> Result<Command, String> {
    match args.first().map(String::as_str) {
        Some("--probe") => Ok(Command::Probe),
        Some("--status") => Ok(Command::Status),
        Some("--daemon") | None => Ok(Command::Daemon),
        Some(other) => Err(format!("unknown argument: {other}")),
    }
}

fn execute(command: Command, config: &TransportConfig) -> CommandOutcome {
    match command {
        Command::Probe => CommandOutcome::Print(format_lines(&config.probe_lines())),
        Command::Status => CommandOutcome::Print(format_lines(&config.current_status_lines())),
        Command::Daemon => CommandOutcome::RunDaemon,
    }
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let config = TransportConfig::from_env();

    let command = match parse_command(&args) {
        Ok(command) => command,
        Err(err) => {
            eprintln!("redbear-btusb: {err}");
            process::exit(1);
        }
    };

    match execute(command, &config) {
        CommandOutcome::Print(output) => {
            print!("{output}");
        }
        CommandOutcome::RunDaemon => {
            if let Err(err) = daemon_main(&config) {
                eprintln!("redbear-btusb: {err}");
                process::exit(1);
            }
        }
    }
}

#[cfg(not(target_os = "redox"))]
fn daemon_main(_config: &TransportConfig) -> Result<(), String> {
    Err("daemon mode is only supported on Redox; use --probe or --status on host".to_string())
}

#[cfg(target_os = "redox")]
fn daemon_main(config: &TransportConfig) -> Result<(), String> {
    struct StatusFileGuard<'a> {
        path: &'a Path,
    }

    impl Drop for StatusFileGuard<'_> {
        fn drop(&mut self) {
            let _ = fs::remove_file(self.path);
        }
    }

    config.write_status_file()?;
    let _status_file_guard = StatusFileGuard {
        path: &config.status_file,
    };

    loop {
        thread::sleep(Duration::from_secs(30));
        config.write_status_file()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("{name}-{stamp}"))
    }

    fn test_config(status_file: PathBuf) -> TransportConfig {
        TransportConfig {
            adapters: vec!["hci0".to_string()],
            controller_family: "usb-bounded-test".to_string(),
            status_file,
        }
    }

    #[test]
    fn probe_contract_is_bounded_and_usb_scoped() {
        let output = execute(Command::Probe, &test_config(temp_path("rbos-btusb-status")));
        let CommandOutcome::Print(output) = output else {
            panic!("expected printable output");
        };
        assert!(output.contains("adapters=hci0"));
        assert!(output.contains("transport=usb"));
        assert!(output.contains("startup=explicit"));
        assert!(output.contains("mode=ble-first"));
    }

    #[test]
    fn status_defaults_to_installed_only_without_runtime_file() {
        let status_file = temp_path("rbos-btusb-status-missing");
        let output = execute(Command::Status, &test_config(status_file));
        let CommandOutcome::Print(output) = output else {
            panic!("expected printable output");
        };
        assert!(output.contains("runtime_visibility=installed-only"));
        assert!(output.contains("daemon_status=inactive"));
    }

    #[test]
    fn status_uses_runtime_file_when_present() {
        let status_file = temp_path("rbos-btusb-status-present");
        let config = test_config(status_file.clone());
        config.write_status_file().unwrap();

        let output = execute(Command::Status, &config);
        let CommandOutcome::Print(output) = output else {
            panic!("expected printable output");
        };
        assert!(output.contains("runtime_visibility=runtime-visible"));
        assert!(output.contains("daemon_status=running"));

        fs::remove_file(status_file).unwrap();
    }

    #[test]
    fn stale_status_file_is_treated_as_installed_only() {
        let status_file = temp_path("rbos-btusb-status-stale");
        fs::write(
            &status_file,
            "adapters=hci0\ntransport=usb\nstartup=explicit\nmode=ble-first\nupdated_at_epoch=1\nruntime_visibility=runtime-visible\ndaemon_status=running\n",
        )
        .unwrap();

        let output = execute(Command::Status, &test_config(status_file.clone()));
        let CommandOutcome::Print(output) = output else {
            panic!("expected printable output");
        };
        assert!(output.contains("runtime_visibility=installed-only"));
        assert!(output.contains("daemon_status=inactive"));

        fs::remove_file(status_file).unwrap();
    }

    #[test]
    fn parse_command_accepts_probe_status_and_daemon() {
        assert_eq!(
            parse_command(&["--probe".to_string()]).unwrap(),
            Command::Probe
        );
        assert_eq!(
            parse_command(&["--status".to_string()]).unwrap(),
            Command::Status
        );
        assert_eq!(parse_command(&[]).unwrap(), Command::Daemon);
    }
}
