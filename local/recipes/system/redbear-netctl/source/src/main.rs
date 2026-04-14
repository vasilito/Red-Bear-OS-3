use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{self, Command};

const USAGE: &str = "Usage: netctl [--boot|list|status [profile]|start <profile>|stop <profile>|enable <profile>|disable [profile]|is-enabled [profile]]";

#[derive(Clone, Debug)]
enum ProfileIpMode {
    Dhcp,
    Static {
        address: String,
        gateway: Option<String>,
        dns: Option<String>,
    },
}

#[derive(Clone, Debug)]
struct Profile {
    name: String,
    interface: String,
    connection: String,
    ip_mode: ProfileIpMode,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("netctl: {err}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        return Err(USAGE.into());
    };

    match command.as_str() {
        "--boot" => run_boot_profile(),
        "list" => list_profiles(),
        "status" => status(args.next().as_deref()),
        "start" => start_profile(&required_profile(args.next())?, false),
        "stop" => stop_profile(&required_profile(args.next())?),
        "enable" => enable_profile(&required_profile(args.next())?),
        "disable" => disable_profile(args.next().as_deref()),
        "is-enabled" => is_enabled(args.next().as_deref()),
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}

fn required_profile(profile: Option<String>) -> Result<String, String> {
    profile.ok_or_else(|| USAGE.to_string())
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
    let address = current_addr().unwrap_or_else(|| "unconfigured".into());

    match selected {
        Some(name) => {
            let enabled = active.as_deref() == Some(name.as_str());
            println!(
                "profile={} enabled={} address={}",
                name,
                if enabled { "yes" } else { "no" },
                address
            );
        }
        None => {
            println!("profile=none enabled=no address={address}");
        }
    }

    Ok(())
}

fn start_profile(name: &str, boot: bool) -> Result<(), String> {
    ensure_runtime_surfaces()?;
    let profile = load_profile(name)?;
    apply_profile(&profile, boot)?;
    println!("started {}", profile.name);
    Ok(())
}

fn stop_profile(name: &str) -> Result<(), String> {
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
    if profile.connection != "ethernet" {
        return Err(format!(
            "unsupported Connection={} (only ethernet is supported)",
            profile.connection
        ));
    }
    if profile.interface != "eth0" {
        return Err(format!(
            "unsupported Interface={} (only eth0 is supported)",
            profile.interface
        ));
    }

    match &profile.ip_mode {
        ProfileIpMode::Dhcp => {
            if boot
                || current_addr().as_deref() == Some("Not configured")
                || current_addr().is_none()
            {
                let _child = Command::new("dhcpd")
                    .spawn()
                    .map_err(|err| format!("failed to spawn dhcpd: {err}"))?;
            }
        }
        ProfileIpMode::Static {
            address,
            gateway,
            dns,
        } => {
            write_netcfg("ifaces/eth0/addr/set", address)?;
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

fn ensure_runtime_surfaces() -> Result<(), String> {
    let addr_path = format!("{}/ifaces/eth0/addr/list", netcfg_root().display());
    fs::read_to_string(&addr_path)
        .map(|_| ())
        .map_err(|err| format!("failed to access {addr_path}: {err}"))
}

fn current_addr() -> Option<String> {
    fs::read_to_string(format!("{}/ifaces/eth0/addr/list", netcfg_root().display()))
        .ok()
        .map(|value| value.trim().to_string())
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

fn parse_profile(name: &str, content: &str) -> Result<Profile, String> {
    let mut interface = None;
    let mut connection = None;
    let mut ip = None;
    let mut address = None;
    let mut gateway = None;
    let mut dns = None;

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
            _ => {}
        }
    }

    let interface = interface.ok_or_else(|| format!("profile {name} is missing Interface="))?;
    let connection = connection.ok_or_else(|| format!("profile {name} is missing Connection="))?;
    let ip_mode = match ip
        .ok_or_else(|| format!("profile {name} is missing IP="))?
        .to_ascii_lowercase()
        .as_str()
    {
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
        connection: connection.to_ascii_lowercase(),
        ip_mode,
    })
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
