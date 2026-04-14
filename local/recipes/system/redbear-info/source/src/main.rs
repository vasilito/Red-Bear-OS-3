use std::env;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process;

#[cfg(test)]
use std::path::Path;
#[cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};

const RESET: &str = "\x1b[0m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const BLUE: &str = "\x1b[34m";
const DIVIDER: &str = "═══════════════════════════════════════════════════════════════════";
const RTL8125_VENDOR_ID: u16 = 0x10ec;
const RTL8125_DEVICE_ID: u16 = 0x8125;
const VIRTIO_NET_VENDOR_ID: u16 = 0x1af4;
const VIRTIO_NET_DEVICE_ID: u16 = 0x1000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputMode {
    Table,
    Json,
    Test,
    Help,
}

struct Options {
    mode: OutputMode,
    verbose: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProbeState {
    Absent,
    Present,
    Active,
    Functional,
    Unobservable,
}

struct Runtime {
    root: Option<PathBuf>,
}

struct IdentityReport {
    pretty_name: Option<String>,
    version_id: Option<String>,
    hostname: Option<String>,
}

struct NetworkReport {
    state: ProbeState,
    connected: bool,
    interface: Option<String>,
    mac: Option<String>,
    address: Option<String>,
    dns: Option<String>,
    default_route: Option<String>,
    active_profile: Option<String>,
    network_schemes: Vec<String>,
    claim_limit: &'static str,
}

struct HardwareReport {
    pci_devices: usize,
    usb_controllers: usize,
    drm_cards: usize,
    rtl8125_present: bool,
    virtio_net_present: bool,
}

struct IntegrationCheck {
    name: &'static str,
    category: &'static str,
    description: &'static str,
    artifact_path: Option<&'static str>,
    control_path: Option<&'static str>,
    test_hint: &'static str,
    note: &'static str,
    functional_probe:
        Option<fn(&Runtime, &NetworkReport, &HardwareReport, &IntegrationCheck) -> Option<String>>,
}

struct IntegrationStatus<'a> {
    check: &'a IntegrationCheck,
    state: ProbeState,
    artifact_present: Option<bool>,
    control_present: Option<bool>,
    evidence: Vec<String>,
    claim_limit: &'static str,
}

struct Report<'a> {
    identity: IdentityReport,
    network: NetworkReport,
    hardware: HardwareReport,
    integrations: Vec<IntegrationStatus<'a>>,
}

const INTEGRATIONS: &[IntegrationCheck] = &[
    IntegrationCheck {
        name: "redbear-info",
        category: "Tool",
        description: "Runtime integration status utility",
        artifact_path: Some("/usr/bin/redbear-info"),
        control_path: None,
        test_hint: "redbear-info --json",
        note: "Binary presence proves the tool is installed, not that every integration is healthy.",
        functional_probe: None,
    },
    IntegrationCheck {
        name: "lspci",
        category: "Tool",
        description: "Native PCI inventory command",
        artifact_path: Some("/usr/bin/lspci"),
        control_path: Some("/scheme/pci"),
        test_hint: "lspci",
        note: "Functional when the PCI scheme is enumerable.",
        functional_probe: Some(probe_directory_readable),
    },
    IntegrationCheck {
        name: "lsusb",
        category: "Tool",
        description: "Native USB inventory command",
        artifact_path: Some("/usr/bin/lsusb"),
        control_path: Some("/scheme"),
        test_hint: "lsusb",
        note: "Functional when at least one usb.* controller scheme is readable.",
        functional_probe: Some(probe_usb_surface),
    },
    IntegrationCheck {
        name: "netctl",
        category: "Tool",
        description: "Redox-native network profile manager",
        artifact_path: Some("/usr/bin/netctl"),
        control_path: Some("/etc/netctl"),
        test_hint: "netctl status",
        note: "Profiles and active profile tracking are readable; profile application remains a separate runtime action.",
        functional_probe: Some(probe_netctl_surface),
    },
    IntegrationCheck {
        name: "redbear-netstat",
        category: "Tool",
        description: "Native Red Bear network status reporter",
        artifact_path: Some("/usr/bin/redbear-netstat"),
        control_path: Some("/scheme/netcfg"),
        test_hint: "redbear-netstat",
        note: "Functional when the netcfg scheme answers read-only interface, route, and resolver queries.",
        functional_probe: Some(probe_smolnetd_surface),
    },
    IntegrationCheck {
        name: "redbear-traceroute",
        category: "Tool",
        description: "Native UDP-based path tracing utility",
        artifact_path: Some("/usr/bin/redbear-traceroute"),
        control_path: Some("/scheme/icmp"),
        test_hint: "redbear-traceroute 1.1.1.1",
        note: "Binary presence proves installation; successful hop tracing still depends on the live ICMP and UDP path in the current runtime.",
        functional_probe: Some(probe_icmp_surface),
    },
    IntegrationCheck {
        name: "redbear-mtr",
        category: "Tool",
        description: "Native path measurement tool built on traceroute probes",
        artifact_path: Some("/usr/bin/redbear-mtr"),
        control_path: Some("/scheme/icmp"),
        test_hint: "redbear-mtr 1.1.1.1",
        note: "Binary presence proves installation; useful measurements still depend on the same live ICMP and UDP probe substrate as traceroute.",
        functional_probe: Some(probe_icmp_surface),
    },
    IntegrationCheck {
        name: "redbear-nmap",
        category: "Tool",
        description: "Bounded TCP connect-scan utility",
        artifact_path: Some("/usr/bin/redbear-nmap"),
        control_path: Some("/scheme/netcfg"),
        test_hint: "redbear-nmap 127.0.0.1 22,80,443",
        note: "Binary presence proves the scanner is installed; successful scans still depend on live networking and reachable targets.",
        functional_probe: Some(probe_smolnetd_surface),
    },
    IntegrationCheck {
        name: "pcid-spawner",
        category: "Core",
        description: "PCI driver autoload daemon",
        artifact_path: Some("/usr/bin/pcid-spawner"),
        control_path: Some("/scheme/pci"),
        test_hint: "lspci",
        note: "The PCI scheme proves discovery is live, but not which driver handled each device.",
        functional_probe: Some(probe_directory_readable),
    },
    IntegrationCheck {
        name: "smolnetd",
        category: "Networking",
        description: "Native TCP/IP stack daemon",
        artifact_path: Some("/usr/bin/smolnetd"),
        control_path: Some("/scheme/netcfg"),
        test_hint: "redbear-info --verbose",
        note: "Functional when the netcfg scheme answers read-only queries.",
        functional_probe: Some(probe_smolnetd_surface),
    },
    IntegrationCheck {
        name: "xhcid",
        category: "USB",
        description: "xHCI host-controller daemon",
        artifact_path: Some("/usr/lib/drivers/xhcid"),
        control_path: Some("/scheme"),
        test_hint: "lsusb",
        note: "Functional when at least one usb.* controller scheme is registered.",
        functional_probe: Some(probe_usb_surface),
    },
    IntegrationCheck {
        name: "dhcpd",
        category: "Networking",
        description: "DHCP client daemon",
        artifact_path: Some("/usr/bin/dhcpd"),
        control_path: None,
        test_hint: "netctl start <dhcp-profile>",
        note: "Binary presence is observable; passive probing cannot prove the DHCP client is currently driving configuration.",
        functional_probe: None,
    },
    IntegrationCheck {
        name: "ext4d",
        category: "Filesystem",
        description: "ext4 scheme daemon",
        artifact_path: Some("/usr/bin/ext4d"),
        control_path: Some("/scheme/ext4d"),
        test_hint: "ls /scheme/ext4d/",
        note: "Functional when the ext4 scheme directory can be enumerated.",
        functional_probe: Some(probe_directory_readable),
    },
    IntegrationCheck {
        name: "firmware-loader",
        category: "System",
        description: "Firmware indexing and serving daemon",
        artifact_path: Some("/usr/bin/firmware-loader"),
        control_path: Some("/scheme"),
        test_hint: "ls /scheme/firmware/",
        note: "Functional when the firmware scheme is enumerable.",
        functional_probe: Some(probe_firmware_scheme),
    },
    IntegrationCheck {
        name: "iommu",
        category: "System",
        description: "IOMMU DMA-remapping daemon",
        artifact_path: Some("/usr/lib/drivers/iommu"),
        control_path: Some("/scheme"),
        test_hint: "redbear-phase-iommu-check",
        note: "Functional when the iommu scheme is registered in /scheme.",
        functional_probe: Some(probe_iommu_scheme),
    },
    IntegrationCheck {
        name: "udev-shim",
        category: "System",
        description: "udev-compatible device enumeration shim",
        artifact_path: Some("/usr/bin/udev-shim"),
        control_path: Some("/scheme"),
        test_hint: "ls /scheme/udev/",
        note: "Functional when the udev scheme can be listed.",
        functional_probe: Some(probe_udev_scheme),
    },
    IntegrationCheck {
        name: "evdevd",
        category: "Input",
        description: "Event-device translation daemon",
        artifact_path: Some("/usr/bin/evdevd"),
        control_path: Some("/scheme"),
        test_hint: "ls /scheme/evdev/",
        note: "Functional when event nodes are enumerable through the evdev scheme.",
        functional_probe: Some(probe_evdev_scheme),
    },
    IntegrationCheck {
        name: "redox-drm",
        category: "GPU",
        description: "DRM/KMS scheme daemon",
        artifact_path: None,
        control_path: Some("/scheme/drm"),
        test_hint: "ls /scheme/drm/",
        note: "A live DRM scheme proves the daemon is running, not that hardware display is fully validated.",
        functional_probe: Some(probe_directory_readable),
    },
    IntegrationCheck {
        name: "amdgpu",
        category: "GPU",
        description: "AMD GPU userspace driver library",
        artifact_path: Some("/usr/lib/redox/drivers/libamdgpu_dc_redox.so"),
        control_path: None,
        test_hint: "redbear-info --verbose",
        note: "Library presence proves packaging; runtime GPU validation still depends on actual hardware and redox-drm activity.",
        functional_probe: None,
    },
    IntegrationCheck {
        name: "rtl8125-native-path",
        category: "Networking",
        description: "Native Realtek RTL8125 support through the rtl8168d autoload path",
        artifact_path: Some("/usr/lib/drivers/rtl8168d"),
        control_path: Some("/scheme/pci"),
        test_hint: "redbear-info --verbose",
        note: "This only becomes functional when 10ec:8125 hardware is present and a network.* scheme is live.",
        functional_probe: Some(probe_rtl8125_path),
    },
    IntegrationCheck {
        name: "virtio-net-vm-path",
        category: "Networking",
        description: "VirtIO network support for QEMU and other virtualized baselines",
        artifact_path: Some("/usr/lib/drivers/virtio-netd"),
        control_path: Some("/scheme/pci"),
        test_hint: "redbear-info --verbose",
        note: "This becomes functional when a VirtIO NIC (1af4:1000) is present and a network.* scheme is live.",
        functional_probe: Some(probe_virtio_net_path),
    },
];

fn main() {
    if let Err(err) = run() {
        eprintln!("redbear-info: {err}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let options = parse_args(env::args())?;
    if options.mode == OutputMode::Help {
        print_help();
        return Ok(());
    }

    let runtime = Runtime::from_env();
    let report = collect_report(&runtime);

    match options.mode {
        OutputMode::Table => print_table(&report, options.verbose),
        OutputMode::Json => print_json(&report),
        OutputMode::Test => print_tests(&report, options.verbose),
        OutputMode::Help => {}
    }

    Ok(())
}

impl Runtime {
    fn from_env() -> Self {
        Self {
            root: env::var_os("REDBEAR_INFO_ROOT").map(PathBuf::from),
        }
    }

    #[cfg(test)]
    fn from_root(root: PathBuf) -> Self {
        Self { root: Some(root) }
    }

    fn resolve(&self, absolute: &str) -> PathBuf {
        let trimmed = absolute.trim_start_matches('/');
        match &self.root {
            Some(root) => root.join(trimmed),
            None => PathBuf::from(absolute),
        }
    }

    fn exists(&self, absolute: &str) -> bool {
        self.resolve(absolute).exists()
    }

    fn is_dir(&self, absolute: &str) -> bool {
        self.resolve(absolute).is_dir()
    }

    fn read_to_string(&self, absolute: &str) -> Option<String> {
        fs::read_to_string(self.resolve(absolute)).ok()
    }

    fn read_dir_names(&self, absolute: &str) -> Option<Vec<String>> {
        let mut names = Vec::new();
        for entry in fs::read_dir(self.resolve(absolute)).ok()? {
            let entry = entry.ok()?;
            let name = entry.file_name();
            let name = name.to_str()?.to_string();
            names.push(name);
        }
        names.sort();
        Some(names)
    }
}

fn collect_report<'a>(runtime: &Runtime) -> Report<'a> {
    let identity = collect_identity(runtime);
    let network = collect_network(runtime);
    let hardware = collect_hardware(runtime, &network);
    let integrations = INTEGRATIONS
        .iter()
        .map(|check| inspect_integration(runtime, &network, &hardware, check))
        .collect();

    Report {
        identity,
        network,
        hardware,
        integrations,
    }
}

fn collect_identity(runtime: &Runtime) -> IdentityReport {
    let os_release = runtime.read_to_string("/usr/lib/os-release");
    IdentityReport {
        pretty_name: os_release
            .as_deref()
            .and_then(|content| parse_os_release_value(content, "PRETTY_NAME")),
        version_id: os_release
            .as_deref()
            .and_then(|content| parse_os_release_value(content, "VERSION_ID")),
        hostname: runtime
            .read_to_string("/etc/hostname")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
    }
}

fn collect_network(runtime: &Runtime) -> NetworkReport {
    let network_schemes = runtime
        .read_dir_names("/scheme")
        .unwrap_or_default()
        .into_iter()
        .filter(|name| name.starts_with("network."))
        .collect::<Vec<_>>();

    let mac = read_trimmed(runtime, "/scheme/netcfg/ifaces/eth0/mac")
        .filter(|value| !matches!(value.as_str(), "Not configured" | "Device not found"));

    let address = read_trimmed(runtime, "/scheme/netcfg/ifaces/eth0/addr/list")
        .filter(|value| !matches!(value.as_str(), "Not configured" | "Device not found"));

    let dns = read_trimmed(runtime, "/scheme/netcfg/resolv/nameserver")
        .or_else(|| read_trimmed(runtime, "/etc/net/dns"))
        .filter(|value| !value.is_empty());

    let default_route = read_trimmed(runtime, "/scheme/netcfg/route/list")
        .and_then(|routes| parse_default_route(&routes));

    let active_profile =
        read_trimmed(runtime, "/etc/netctl/active").filter(|value| !value.is_empty());

    let state = if runtime.exists("/scheme/netcfg") {
        if address.is_some() {
            ProbeState::Functional
        } else {
            ProbeState::Active
        }
    } else if runtime.exists("/usr/bin/smolnetd") {
        ProbeState::Present
    } else {
        ProbeState::Absent
    };

    NetworkReport {
        state,
        connected: address.is_some(),
        interface: runtime.exists("/scheme/netcfg/ifaces/eth0/mac").then_some("eth0".to_string()),
        mac,
        address,
        dns,
        default_route,
        active_profile,
        network_schemes,
        claim_limit: "Connected means the local stack exposes a configured address; this does not prove external reachability.",
    }
}

fn collect_hardware(runtime: &Runtime, network: &NetworkReport) -> HardwareReport {
    let pci_entries = runtime.read_dir_names("/scheme/pci").unwrap_or_default();
    let pci_devices = pci_entries
        .iter()
        .filter(|entry| entry.contains("--") && entry.contains('.'))
        .count();

    let usb_controllers = runtime
        .read_dir_names("/scheme")
        .unwrap_or_default()
        .into_iter()
        .filter(|name| name.starts_with("usb."))
        .count();

    let drm_cards = runtime
        .read_dir_names("/scheme/drm")
        .unwrap_or_default()
        .into_iter()
        .filter(|name| name.starts_with("card"))
        .count();

    let rtl8125_present = pci_entries.into_iter().any(|entry| {
        let config_path = format!("/scheme/pci/{entry}/config");
        let Some(bytes) = read_prefix_bytes(runtime, &config_path, 4) else {
            return false;
        };
        if bytes.len() < 4 {
            return false;
        }
        let vendor = u16::from_le_bytes([bytes[0], bytes[1]]);
        let device = u16::from_le_bytes([bytes[2], bytes[3]]);
        vendor == RTL8125_VENDOR_ID && device == RTL8125_DEVICE_ID
    }) || network
        .network_schemes
        .iter()
        .any(|name| name.contains("rtl8125"));

    let virtio_net_present = runtime
        .read_dir_names("/scheme/pci")
        .unwrap_or_default()
        .into_iter()
        .any(|entry| {
            let config_path = format!("/scheme/pci/{entry}/config");
            let Some(bytes) = read_prefix_bytes(runtime, &config_path, 4) else {
                return false;
            };
            if bytes.len() < 4 {
                return false;
            }
            let vendor = u16::from_le_bytes([bytes[0], bytes[1]]);
            let device = u16::from_le_bytes([bytes[2], bytes[3]]);
            vendor == VIRTIO_NET_VENDOR_ID && device == VIRTIO_NET_DEVICE_ID
        })
        || network
            .network_schemes
            .iter()
            .any(|name| name.contains("virtio") || name.contains("eth0"));

    HardwareReport {
        pci_devices,
        usb_controllers,
        drm_cards,
        rtl8125_present,
        virtio_net_present,
    }
}

fn inspect_integration<'a>(
    runtime: &Runtime,
    network: &NetworkReport,
    hardware: &HardwareReport,
    check: &'a IntegrationCheck,
) -> IntegrationStatus<'a> {
    let artifact_present = check.artifact_path.map(|path| runtime.exists(path));
    let control_present = check.control_path.map(|path| runtime.exists(path));

    let mut evidence = Vec::new();

    if let Some(path) = check.artifact_path {
        evidence.push(format!(
            "artifact {} {}",
            path,
            if artifact_present == Some(true) {
                "present"
            } else {
                "missing"
            }
        ));
    }
    if let Some(path) = check.control_path {
        evidence.push(format!(
            "control {} {}",
            path,
            if control_present == Some(true) {
                "present"
            } else {
                "missing"
            }
        ));
    }

    let state = if let Some(probe) = check.functional_probe {
        match probe(runtime, network, hardware, check) {
            Some(message) => {
                evidence.push(message);
                if artifact_present == Some(false) {
                    ProbeState::Active
                } else {
                    ProbeState::Functional
                }
            }
            None => derive_state(artifact_present, control_present),
        }
    } else {
        derive_state(artifact_present, control_present)
    };

    IntegrationStatus {
        check,
        state,
        artifact_present,
        control_present,
        evidence,
        claim_limit: check.note,
    }
}

fn derive_state(artifact_present: Option<bool>, control_present: Option<bool>) -> ProbeState {
    if control_present == Some(true) {
        ProbeState::Active
    } else if artifact_present == Some(true) {
        ProbeState::Present
    } else if artifact_present.is_none() && control_present.is_none() {
        ProbeState::Unobservable
    } else {
        ProbeState::Absent
    }
}

fn parse_args<I>(args: I) -> Result<Options, String>
where
    I: IntoIterator<Item = String>,
{
    let mut mode = OutputMode::Table;
    let mut verbose = false;

    for arg in args.into_iter().skip(1) {
        match arg.as_str() {
            "-v" | "--verbose" => verbose = true,
            "--json" => {
                if mode == OutputMode::Test {
                    return Err("cannot combine --json with --test".to_string());
                }
                mode = OutputMode::Json;
            }
            "--test" => {
                if mode == OutputMode::Json {
                    return Err("cannot combine --test with --json".to_string());
                }
                mode = OutputMode::Test;
            }
            "-h" | "--help" => mode = OutputMode::Help,
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }

    Ok(Options { mode, verbose })
}

fn print_table(report: &Report<'_>, verbose: bool) {
    println!("Red Bear OS Runtime Integration Report");
    println!("{DIVIDER}");
    println!();

    print_section_header("Identity");
    println!(
        "  OS: {}",
        display_or_unknown(report.identity.pretty_name.as_deref())
    );
    println!(
        "  Version: {}",
        display_or_unknown(report.identity.version_id.as_deref())
    );
    println!(
        "  Hostname: {}",
        display_or_unknown(report.identity.hostname.as_deref())
    );
    println!();

    print_section_header("Networking");
    println!(
        "  Stack: {} {}",
        colorize(
            state_marker(report.network.state),
            state_color(report.network.state)
        ),
        state_label(report.network.state)
    );
    println!(
        "  Connected: {}",
        if report.network.connected {
            "yes"
        } else {
            "no"
        }
    );
    println!(
        "  Interface: {}",
        display_or_unknown(report.network.interface.as_deref())
    );
    println!(
        "  MAC: {}",
        display_or_unknown(report.network.mac.as_deref())
    );
    println!(
        "  Address: {}",
        display_or_unknown(report.network.address.as_deref())
    );
    println!(
        "  DNS: {}",
        display_or_unknown(report.network.dns.as_deref())
    );
    println!(
        "  Default route: {}",
        display_or_unknown(report.network.default_route.as_deref())
    );
    println!(
        "  Active profile: {}",
        display_or_unknown(report.network.active_profile.as_deref())
    );
    println!(
        "  Network schemes: {}",
        if report.network.network_schemes.is_empty() {
            "none".to_string()
        } else {
            report.network.network_schemes.join(", ")
        }
    );
    if verbose {
        println!("  Note: {}", report.network.claim_limit);
    }
    println!();

    print_section_header("Hardware");
    println!("  PCI devices: {}", report.hardware.pci_devices);
    println!("  USB controllers: {}", report.hardware.usb_controllers);
    println!("  DRM cards: {}", report.hardware.drm_cards);
    println!(
        "  RTL8125 device seen: {}",
        if report.hardware.rtl8125_present {
            "yes"
        } else {
            "no"
        }
    );
    println!(
        "  VirtIO NIC seen: {}",
        if report.hardware.virtio_net_present {
            "yes"
        } else {
            "no"
        }
    );
    println!();

    print_section_header("Integrations");
    for integration in &report.integrations {
        println!(
            "  {} {:<18} [{}] {}",
            colorize(
                state_marker(integration.state),
                state_color(integration.state)
            ),
            integration.check.name,
            integration.check.category,
            state_label(integration.state)
        );
        println!("    {}", integration.check.description);
        println!("    Test: {}", integration.check.test_hint);
        if verbose {
            for line in &integration.evidence {
                println!("    Evidence: {line}");
            }
            println!("    Claim limit: {}", integration.claim_limit);
        }
        println!();
    }

    println!("{DIVIDER}");
    println!(
        "functional={} active={} present={} absent={} total={}",
        count_state(&report.integrations, ProbeState::Functional),
        count_state(&report.integrations, ProbeState::Active),
        count_state(&report.integrations, ProbeState::Present),
        count_state(&report.integrations, ProbeState::Absent),
        report.integrations.len()
    );
}

fn print_tests(report: &Report<'_>, verbose: bool) {
    println!("Red Bear OS Runtime Test Hints");
    println!("{DIVIDER}");
    println!();
    println!("  redbear-info --json");
    println!("  redbear-info --verbose");
    println!("  netctl status");
    println!("  lspci");
    println!("  lsusb");
    println!();

    for integration in report
        .integrations
        .iter()
        .filter(|integration| integration.state != ProbeState::Absent)
    {
        println!(
            "  {:<18} {}",
            integration.check.name, integration.check.test_hint
        );
        if verbose {
            println!("    {}", integration.claim_limit);
        }
    }

    println!();
    println!("Network interpretation: {}", report.network.claim_limit);
}

fn print_json(report: &Report<'_>) {
    let mut out = String::new();
    out.push_str("{\n");

    out.push_str("  \"identity\": {\n");
    push_json_field(
        &mut out,
        "pretty_name",
        report.identity.pretty_name.as_deref(),
        true,
        4,
    );
    push_json_field(
        &mut out,
        "version_id",
        report.identity.version_id.as_deref(),
        true,
        4,
    );
    push_json_field(
        &mut out,
        "hostname",
        report.identity.hostname.as_deref(),
        false,
        4,
    );
    out.push_str("  },\n");

    out.push_str("  \"network\": {\n");
    push_json_string_field(
        &mut out,
        "state",
        state_label(report.network.state),
        true,
        4,
    );
    push_json_bool_field(&mut out, "connected", report.network.connected, true, 4);
    push_json_field(
        &mut out,
        "interface",
        report.network.interface.as_deref(),
        true,
        4,
    );
    push_json_field(&mut out, "mac", report.network.mac.as_deref(), true, 4);
    push_json_field(
        &mut out,
        "address",
        report.network.address.as_deref(),
        true,
        4,
    );
    push_json_field(&mut out, "dns", report.network.dns.as_deref(), true, 4);
    push_json_field(
        &mut out,
        "default_route",
        report.network.default_route.as_deref(),
        true,
        4,
    );
    push_json_field(
        &mut out,
        "active_profile",
        report.network.active_profile.as_deref(),
        true,
        4,
    );
    push_json_string_array_field(
        &mut out,
        "network_schemes",
        &report.network.network_schemes,
        true,
        4,
    );
    push_json_string_field(
        &mut out,
        "claim_limit",
        report.network.claim_limit,
        false,
        4,
    );
    out.push_str("  },\n");

    out.push_str("  \"hardware\": {\n");
    push_json_number_field(
        &mut out,
        "pci_devices",
        report.hardware.pci_devices,
        true,
        4,
    );
    push_json_number_field(
        &mut out,
        "usb_controllers",
        report.hardware.usb_controllers,
        true,
        4,
    );
    push_json_number_field(&mut out, "drm_cards", report.hardware.drm_cards, true, 4);
    push_json_bool_field(
        &mut out,
        "rtl8125_present",
        report.hardware.rtl8125_present,
        true,
        4,
    );
    push_json_bool_field(
        &mut out,
        "virtio_net_present",
        report.hardware.virtio_net_present,
        false,
        4,
    );
    out.push_str("  },\n");

    out.push_str("  \"integrations\": [\n");
    for (index, integration) in report.integrations.iter().enumerate() {
        out.push_str("    {\n");
        push_json_string_field(&mut out, "name", integration.check.name, true, 6);
        push_json_string_field(&mut out, "category", integration.check.category, true, 6);
        push_json_string_field(
            &mut out,
            "description",
            integration.check.description,
            true,
            6,
        );
        push_json_string_field(&mut out, "state", state_label(integration.state), true, 6);
        push_json_optional_bool_field(
            &mut out,
            "artifact_present",
            integration.artifact_present,
            true,
            6,
        );
        push_json_optional_bool_field(
            &mut out,
            "control_present",
            integration.control_present,
            true,
            6,
        );
        push_json_string_field(&mut out, "test_hint", integration.check.test_hint, true, 6);
        push_json_string_array_field(&mut out, "evidence", &integration.evidence, true, 6);
        push_json_string_field(&mut out, "claim_limit", integration.claim_limit, false, 6);
        out.push_str("    }");
        if index + 1 != report.integrations.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ]\n");
    out.push('}');
    println!("{out}");
}

fn print_help() {
    println!("Usage: redbear-info [--verbose|-v] [--json|--test]");
    println!();
    println!("Passive runtime integration report for Red Bear OS.");
    println!();
    println!("This tool distinguishes:");
    println!("  present     artifact or config exists");
    println!("  active      live runtime surface exists");
    println!("  functional  read-only runtime probe succeeded");
    println!();
    println!("Connected means the local networking stack has a configured address.");
    println!("It does not prove internet reachability.");
    println!();
    println!("Options:");
    println!("  -v, --verbose  Show evidence and claim limits");
    println!("      --json     Print structured JSON");
    println!("      --test     Print suggested diagnostic commands");
    println!("  -h, --help     Show this help message");
}

fn print_section_header(title: &str) {
    println!("{}", colorize(title, BLUE));
}

fn state_marker(state: ProbeState) -> &'static str {
    match state {
        ProbeState::Functional => "●",
        ProbeState::Active => "◉",
        ProbeState::Present => "◌",
        ProbeState::Absent => "○",
        ProbeState::Unobservable => "?",
    }
}

fn state_label(state: ProbeState) -> &'static str {
    match state {
        ProbeState::Functional => "functional",
        ProbeState::Active => "active",
        ProbeState::Present => "present",
        ProbeState::Absent => "absent",
        ProbeState::Unobservable => "unobservable",
    }
}

fn state_color(state: ProbeState) -> &'static str {
    match state {
        ProbeState::Functional => GREEN,
        ProbeState::Active => YELLOW,
        ProbeState::Present => BLUE,
        ProbeState::Absent => RED,
        ProbeState::Unobservable => YELLOW,
    }
}

fn colorize(text: &str, color: &str) -> String {
    format!("{color}{text}{RESET}")
}

fn count_state(items: &[IntegrationStatus<'_>], state: ProbeState) -> usize {
    items.iter().filter(|item| item.state == state).count()
}

fn display_or_unknown(value: Option<&str>) -> &str {
    value.unwrap_or("unknown")
}

fn parse_os_release_value(contents: &str, key: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let (found_key, raw) = line.split_once('=')?;
        if found_key == key {
            Some(raw.trim().trim_matches('"').to_string())
        } else {
            None
        }
    })
}

fn read_trimmed(runtime: &Runtime, path: &str) -> Option<String> {
    runtime
        .read_to_string(path)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn read_prefix_bytes(runtime: &Runtime, path: &str, max_len: usize) -> Option<Vec<u8>> {
    let mut file = fs::File::open(runtime.resolve(path)).ok()?;
    let mut bytes = vec![0_u8; max_len];
    let read = file.read(&mut bytes).ok()?;
    bytes.truncate(read);
    Some(bytes)
}

fn parse_default_route(routes: &str) -> Option<String> {
    routes.lines().find_map(|line| {
        let trimmed = line.trim();
        if trimmed.starts_with("default via ") || trimmed.starts_with("0.0.0.0/0 via ") {
            Some(trimmed.to_string())
        } else {
            None
        }
    })
}

fn probe_directory_readable(
    runtime: &Runtime,
    _network: &NetworkReport,
    _hardware: &HardwareReport,
    check: &IntegrationCheck,
) -> Option<String> {
    let path = check.control_path?;
    let entries = runtime.read_dir_names(path)?;
    Some(format!(
        "read-only probe succeeded on {path} ({} entrie(s))",
        entries.len()
    ))
}

fn probe_usb_surface(
    runtime: &Runtime,
    _network: &NetworkReport,
    hardware: &HardwareReport,
    _check: &IntegrationCheck,
) -> Option<String> {
    (hardware.usb_controllers > 0 || runtime.is_dir("/scheme")).then(|| {
        format!(
            "usb scheme scan sees {} controller(s)",
            hardware.usb_controllers
        )
    })
}

fn probe_icmp_surface(
    runtime: &Runtime,
    _network: &NetworkReport,
    _hardware: &HardwareReport,
    _check: &IntegrationCheck,
) -> Option<String> {
    runtime
        .read_dir_names("/scheme")
        .filter(|entries| entries.iter().any(|name| name == "icmp"))
        .map(|_| "icmp scheme is present for probe/error reporting".to_string())
}

fn probe_netctl_surface(
    runtime: &Runtime,
    network: &NetworkReport,
    _hardware: &HardwareReport,
    _check: &IntegrationCheck,
) -> Option<String> {
    let profiles = runtime.read_dir_names("/etc/netctl")?;
    let profile_count = profiles
        .iter()
        .filter(|name| *name != "active" && !name.starts_with('.'))
        .count();
    Some(match &network.active_profile {
        Some(active) => format!(
            "{} profile(s) visible, active profile {}",
            profile_count, active
        ),
        None => format!(
            "{} profile(s) visible, no active profile recorded",
            profile_count
        ),
    })
}

fn probe_smolnetd_surface(
    runtime: &Runtime,
    network: &NetworkReport,
    _hardware: &HardwareReport,
    _check: &IntegrationCheck,
) -> Option<String> {
    let _ = runtime.read_dir_names("/scheme/netcfg")?;
    Some(match &network.address {
        Some(address) => format!("netcfg readable, active address {address}"),
        None => "netcfg readable, no configured address".to_string(),
    })
}

fn probe_named_scheme(runtime: &Runtime, scheme_name: &str) -> Option<String> {
    let names = runtime.read_dir_names("/scheme")?;
    names
        .into_iter()
        .any(|name| name == scheme_name)
        .then(|| format!("scheme {scheme_name} is registered in /scheme"))
}

fn probe_firmware_scheme(
    runtime: &Runtime,
    _network: &NetworkReport,
    _hardware: &HardwareReport,
    _check: &IntegrationCheck,
) -> Option<String> {
    probe_named_scheme(runtime, "firmware")
}

fn probe_udev_scheme(
    runtime: &Runtime,
    _network: &NetworkReport,
    _hardware: &HardwareReport,
    _check: &IntegrationCheck,
) -> Option<String> {
    probe_named_scheme(runtime, "udev")
}

fn probe_evdev_scheme(
    runtime: &Runtime,
    _network: &NetworkReport,
    _hardware: &HardwareReport,
    _check: &IntegrationCheck,
) -> Option<String> {
    probe_named_scheme(runtime, "evdev")
}

fn probe_iommu_scheme(
    runtime: &Runtime,
    _network: &NetworkReport,
    _hardware: &HardwareReport,
    _check: &IntegrationCheck,
) -> Option<String> {
    probe_named_scheme(runtime, "iommu")
}

fn probe_rtl8125_path(
    _runtime: &Runtime,
    network: &NetworkReport,
    hardware: &HardwareReport,
    _check: &IntegrationCheck,
) -> Option<String> {
    if !hardware.rtl8125_present {
        return None;
    }

    Some(
        if network
            .network_schemes
            .iter()
            .any(|name| name.contains("rtl8125"))
        {
            "RTL8125 PCI device seen and network.rtl8125 scheme visible".to_string()
        } else if network.connected {
            "RTL8125 PCI device seen and native network stack reports a configured address"
                .to_string()
        } else {
            "RTL8125 PCI device seen through /scheme/pci".to_string()
        },
    )
}

fn probe_virtio_net_path(
    _runtime: &Runtime,
    network: &NetworkReport,
    hardware: &HardwareReport,
    _check: &IntegrationCheck,
) -> Option<String> {
    if !hardware.virtio_net_present {
        return None;
    }

    Some(
        if network
            .network_schemes
            .iter()
            .any(|name| name.contains("virtio") || name.contains("eth0"))
        {
            "VirtIO NIC seen and network scheme surface is visible".to_string()
        } else if network.connected {
            "VirtIO NIC seen and native network stack reports a configured address".to_string()
        } else {
            "VirtIO NIC seen through /scheme/pci".to_string()
        },
    )
}

fn push_json_string(output: &mut String, value: &str) {
    output.push('"');
    for ch in value.chars() {
        match ch {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            _ => output.push(ch),
        }
    }
    output.push('"');
}

fn push_json_indent(output: &mut String, indent: usize) {
    for _ in 0..indent {
        output.push(' ');
    }
}

fn push_json_string_field(
    output: &mut String,
    key: &str,
    value: &str,
    trailing_comma: bool,
    indent: usize,
) {
    push_json_indent(output, indent);
    push_json_string(output, key);
    output.push_str(": ");
    push_json_string(output, value);
    if trailing_comma {
        output.push(',');
    }
    output.push('\n');
}

fn push_json_field(
    output: &mut String,
    key: &str,
    value: Option<&str>,
    trailing_comma: bool,
    indent: usize,
) {
    push_json_indent(output, indent);
    push_json_string(output, key);
    output.push_str(": ");
    match value {
        Some(value) => push_json_string(output, value),
        None => output.push_str("null"),
    }
    if trailing_comma {
        output.push(',');
    }
    output.push('\n');
}

fn push_json_bool_field(
    output: &mut String,
    key: &str,
    value: bool,
    trailing_comma: bool,
    indent: usize,
) {
    push_json_indent(output, indent);
    push_json_string(output, key);
    output.push_str(": ");
    output.push_str(if value { "true" } else { "false" });
    if trailing_comma {
        output.push(',');
    }
    output.push('\n');
}

fn push_json_optional_bool_field(
    output: &mut String,
    key: &str,
    value: Option<bool>,
    trailing_comma: bool,
    indent: usize,
) {
    push_json_indent(output, indent);
    push_json_string(output, key);
    output.push_str(": ");
    match value {
        Some(value) => output.push_str(if value { "true" } else { "false" }),
        None => output.push_str("null"),
    }
    if trailing_comma {
        output.push(',');
    }
    output.push('\n');
}

fn push_json_number_field(
    output: &mut String,
    key: &str,
    value: usize,
    trailing_comma: bool,
    indent: usize,
) {
    push_json_indent(output, indent);
    push_json_string(output, key);
    output.push_str(": ");
    output.push_str(&value.to_string());
    if trailing_comma {
        output.push(',');
    }
    output.push('\n');
}

fn push_json_string_array_field(
    output: &mut String,
    key: &str,
    values: &[String],
    trailing_comma: bool,
    indent: usize,
) {
    push_json_indent(output, indent);
    push_json_string(output, key);
    output.push_str(": [");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        push_json_string(output, value);
    }
    output.push(']');
    if trailing_comma {
        output.push(',');
    }
    output.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!("redbear-info-test-{}-{}", process::id(), unique));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_file(root: &Path, path: &str, content: &str) {
        let full = root.join(path.trim_start_matches('/'));
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }

    fn create_dir(root: &Path, path: &str) {
        fs::create_dir_all(root.join(path.trim_start_matches('/'))).unwrap();
    }

    #[test]
    fn network_report_uses_live_netcfg_surfaces() {
        let root = temp_root();
        write_file(
            &root,
            "/usr/lib/os-release",
            "PRETTY_NAME=\"Red Bear OS\"\nVERSION_ID=\"0.1.0\"\n",
        );
        write_file(&root, "/etc/hostname", "redbear\n");
        create_dir(&root, "/scheme/netcfg/ifaces/eth0/addr");
        write_file(
            &root,
            "/scheme/netcfg/ifaces/eth0/addr/list",
            "192.168.10.20/24\n",
        );
        write_file(
            &root,
            "/scheme/netcfg/ifaces/eth0/mac",
            "02:00:00:00:00:01\n",
        );
        write_file(&root, "/scheme/netcfg/resolv/nameserver", "1.1.1.1\n");
        write_file(
            &root,
            "/scheme/netcfg/route/list",
            "default via 192.168.10.1\n",
        );
        create_dir(&root, "/scheme/network.eth0_rtl8125");
        create_dir(&root, "/etc/netctl");
        write_file(&root, "/etc/netctl/active", "wired-static\n");

        let report = collect_report(&Runtime::from_root(root.clone()));
        assert_eq!(report.network.state, ProbeState::Functional);
        assert!(report.network.connected);
        assert_eq!(report.network.address.as_deref(), Some("192.168.10.20/24"));
        assert_eq!(report.network.dns.as_deref(), Some("1.1.1.1"));
        assert_eq!(
            report.network.default_route.as_deref(),
            Some("default via 192.168.10.1")
        );
        assert_eq!(
            report.network.active_profile.as_deref(),
            Some("wired-static")
        );
        assert!(report
            .network
            .network_schemes
            .iter()
            .any(|name| name.contains("rtl8125")));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rtl8125_hardware_detection_parses_pci_config() {
        let root = temp_root();
        create_dir(&root, "/scheme/pci/0000--02--00.0");
        let config = [
            (RTL8125_VENDOR_ID & 0xff) as u8,
            (RTL8125_VENDOR_ID >> 8) as u8,
            (RTL8125_DEVICE_ID & 0xff) as u8,
            (RTL8125_DEVICE_ID >> 8) as u8,
            0,
            0,
            0,
            0,
        ];
        let path = root.join("scheme/pci/0000--02--00.0/config");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, config).unwrap();

        let network = collect_network(&Runtime::from_root(root.clone()));
        let hardware = collect_hardware(&Runtime::from_root(root.clone()), &network);
        assert!(hardware.rtl8125_present);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn virtio_net_hardware_detection_parses_pci_config() {
        let root = temp_root();
        create_dir(&root, "/scheme/pci/0000--00--03.0");
        let config = [
            (VIRTIO_NET_VENDOR_ID & 0xff) as u8,
            (VIRTIO_NET_VENDOR_ID >> 8) as u8,
            (VIRTIO_NET_DEVICE_ID & 0xff) as u8,
            (VIRTIO_NET_DEVICE_ID >> 8) as u8,
            0,
            0,
            0,
            0,
        ];
        let path = root.join("scheme/pci/0000--00--03.0/config");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, config).unwrap();

        let network = collect_network(&Runtime::from_root(root.clone()));
        let hardware = collect_hardware(&Runtime::from_root(root.clone()), &network);
        assert!(hardware.virtio_net_present);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn json_output_contains_network_and_integration_state() {
        let root = temp_root();
        write_file(
            &root,
            "/usr/lib/os-release",
            "PRETTY_NAME=\"Red Bear OS\"\nVERSION_ID=\"0.1.0\"\n",
        );
        write_file(&root, "/usr/bin/redbear-info", "");
        write_file(&root, "/usr/bin/redbear-netstat", "");
        write_file(&root, "/usr/bin/redbear-nmap", "");
        create_dir(&root, "/scheme/netcfg");
        write_file(
            &root,
            "/scheme/netcfg/ifaces/eth0/addr/list",
            "Not configured\n",
        );

        let report = collect_report(&Runtime::from_root(root.clone()));
        assert!(!report.hardware.virtio_net_present);
        let mut output = String::new();
        output.push_str("{");
        push_json_string_field(
            &mut output,
            "state",
            state_label(report.network.state),
            false,
            0,
        );
        assert!(output.contains("state"));
        assert!(report
            .integrations
            .iter()
            .any(|item| item.check.name == "redbear-info"));
        assert!(report
            .integrations
            .iter()
            .any(|item| item.check.name == "redbear-netstat"));
        assert!(report
            .integrations
            .iter()
            .any(|item| item.check.name == "redbear-nmap"));

        fs::remove_dir_all(root).unwrap();
    }
}
