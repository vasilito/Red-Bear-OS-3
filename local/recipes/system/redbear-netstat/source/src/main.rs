use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;

fn main() {
    if let Err(err) = run() {
        eprintln!("{}: {err}", program_name());
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        None => {
            print_report(&Runtime::default())?;
            Ok(())
        }
        Some("help" | "--help" | "-h") => {
            println!("{}", usage());
            Ok(())
        }
        Some(other) => Err(format!("unknown argument: {other}\n{}", usage())),
    }
}

fn program_name() -> String {
    env::args()
        .next()
        .and_then(|path| {
            PathBuf::from(path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "netstat".to_string())
}

fn usage() -> String {
    format!(
        "Usage: {}\n\nCurrent scope: reports interface/address/route/DNS/profile state from Red Bear runtime surfaces.\nNot implemented yet: live TCP/UDP socket table enumeration.",
        program_name()
    )
}

#[derive(Clone, Debug, Default)]
struct Runtime {
    root: Option<PathBuf>,
}

impl Runtime {
    fn resolve(&self, path: &str) -> PathBuf {
        if let Some(root) = &self.root {
            let relative = path.trim_start_matches('/');
            root.join(relative)
        } else {
            PathBuf::from(path)
        }
    }

    fn read_trimmed(&self, path: &str) -> Option<String> {
        fs::read_to_string(self.resolve(path))
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }

    fn read_dir_names(&self, path: &str) -> Option<Vec<String>> {
        let mut names = Vec::new();
        for entry in fs::read_dir(self.resolve(path)).ok()? {
            let entry = entry.ok()?;
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
        names.sort();
        Some(names)
    }

    fn exists(&self, path: &str) -> bool {
        self.resolve(path).exists()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InterfaceReport {
    name: String,
    address: Option<String>,
    mac: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Report {
    interfaces: Vec<InterfaceReport>,
    routes: Vec<String>,
    default_route: Option<String>,
    dns: Option<String>,
    active_profile: Option<String>,
    network_schemes: Vec<String>,
}

fn build_report(runtime: &Runtime) -> Result<Report, String> {
    let interfaces = collect_interfaces(runtime)?;
    let route_text = runtime.read_trimmed("/scheme/netcfg/route/list");
    let routes = route_text.as_deref().map(parse_routes).unwrap_or_default();
    let default_route = route_text.as_deref().and_then(parse_default_route);
    let dns = runtime.read_trimmed("/scheme/netcfg/resolv/nameserver");
    let active_profile = runtime.read_trimmed("/etc/netctl/active");
    let network_schemes = runtime
        .read_dir_names("/scheme")
        .unwrap_or_default()
        .into_iter()
        .filter(|name| name.starts_with("network."))
        .collect();

    Ok(Report {
        interfaces,
        routes,
        default_route,
        dns,
        active_profile,
        network_schemes,
    })
}

fn collect_interfaces(runtime: &Runtime) -> Result<Vec<InterfaceReport>, String> {
    if !runtime.exists("/scheme/netcfg/ifaces") {
        return Ok(Vec::new());
    }

    let mut reports = Vec::new();
    let names = runtime
        .read_dir_names("/scheme/netcfg/ifaces")
        .ok_or_else(|| "failed to read /scheme/netcfg/ifaces".to_string())?;

    for name in names {
        let address = runtime.read_trimmed(&format!("/scheme/netcfg/ifaces/{name}/addr/list"));
        let mac = runtime.read_trimmed(&format!("/scheme/netcfg/ifaces/{name}/mac"));
        reports.push(InterfaceReport { name, address, mac });
    }

    Ok(reports)
}

fn parse_routes(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn parse_default_route(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let trimmed = line.trim();
        if trimmed.starts_with("default via ") || trimmed.starts_with("0.0.0.0/0 via ") {
            Some(trimmed.to_string())
        } else {
            None
        }
    })
}

fn print_report(runtime: &Runtime) -> Result<(), String> {
    let report = build_report(runtime)?;

    println!("interfaces");
    if report.interfaces.is_empty() {
        println!("  none");
    } else {
        for iface in &report.interfaces {
            println!("  {}", iface.name);
            println!(
                "    address={} mac={}",
                iface.address.as_deref().unwrap_or("unknown"),
                iface.mac.as_deref().unwrap_or("unknown")
            );
        }
    }

    println!("routes");
    if report.routes.is_empty() {
        println!("  none");
    } else {
        for route in &report.routes {
            println!("  {route}");
        }
    }

    println!(
        "dns={} active_profile={} default_route={}",
        report.dns.as_deref().unwrap_or("unknown"),
        report.active_profile.as_deref().unwrap_or("none"),
        report.default_route.as_deref().unwrap_or("unknown")
    );

    println!("network_schemes");
    if report.network_schemes.is_empty() {
        println!("  none");
    } else {
        for scheme in &report.network_schemes {
            println!("  {scheme}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = env::temp_dir().join(format!("redbear-netstat-test-{nanos}"));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn write_file(root: &Path, path: &str, contents: &str) {
        let path = root.join(path.trim_start_matches('/'));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn parses_default_route_variants() {
        assert_eq!(
            parse_default_route("default via 10.0.2.2\n10.0.2.0/24 dev eth0"),
            Some("default via 10.0.2.2".to_string())
        );
        assert_eq!(
            parse_default_route("0.0.0.0/0 via 192.168.1.1\n"),
            Some("0.0.0.0/0 via 192.168.1.1".to_string())
        );
        assert_eq!(parse_default_route("10.0.2.0/24 dev eth0\n"), None);
    }

    #[test]
    fn builds_report_from_fake_runtime_root() {
        let root = temp_root();
        write_file(
            &root,
            "/scheme/netcfg/ifaces/eth0/addr/list",
            "10.0.2.15/24\n",
        );
        write_file(
            &root,
            "/scheme/netcfg/ifaces/eth0/mac",
            "52:54:00:12:34:56\n",
        );
        write_file(&root, "/scheme/netcfg/resolv/nameserver", "1.1.1.1\n");
        write_file(
            &root,
            "/scheme/netcfg/route/list",
            "default via 10.0.2.2\n10.0.2.0/24 dev eth0\n",
        );
        write_file(&root, "/etc/netctl/active", "wired-dhcp\n");
        fs::create_dir_all(root.join("scheme/network.virtio_net")).unwrap();

        let runtime = Runtime { root: Some(root) };
        let report = build_report(&runtime).unwrap();

        assert_eq!(report.interfaces.len(), 1);
        assert_eq!(report.interfaces[0].name, "eth0");
        assert_eq!(
            report.interfaces[0].address.as_deref(),
            Some("10.0.2.15/24")
        );
        assert_eq!(
            report.interfaces[0].mac.as_deref(),
            Some("52:54:00:12:34:56")
        );
        assert_eq!(
            report.default_route.as_deref(),
            Some("default via 10.0.2.2")
        );
        assert_eq!(report.dns.as_deref(), Some("1.1.1.1"));
        assert_eq!(report.active_profile.as_deref(), Some("wired-dhcp"));
        assert_eq!(
            report.network_schemes,
            vec!["network.virtio_net".to_string()]
        );
    }
}
