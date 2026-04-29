use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process,
};

#[cfg(target_os = "redox")]
use std::{fs::File, io::Read};

use redbear_hwutils::parse_args;
#[cfg(target_os = "redox")]
use redox_driver_sys::irq::IrqHandle;
use redox_driver_sys::pci::{PciLocation, parse_device_info_from_config_space};

const PROGRAM: &str = "redbear-phase-pci-irq-check";
const USAGE: &str = "Usage: redbear-phase-pci-irq-check\n\nShow bounded live PCI/IRQ runtime reporting from the current target runtime.";
const IRQ_REPORT_DIRS: &[&str] = &[
    "/tmp/redbear-irq-report",
    "/tmp/run/redbear-irq-report",
    "/run/redbear-irq-report",
    "/var/run/redbear-irq-report",
    "/scheme/initfs/tmp/redbear-irq-report",
    "/scheme/initfs/tmp/run/redbear-irq-report",
    "/scheme/initfs/run/redbear-irq-report",
    "/scheme/initfs/var/run/redbear-irq-report",
];

#[derive(Debug, Clone, Eq, PartialEq)]
struct IrqReport {
    driver: String,
    pid: u32,
    device: String,
    mode: String,
    reason: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct PciDeviceProbe {
    device: String,
    irq_line: Option<u32>,
    interrupt_support: String,
    supports_msi: bool,
    supports_msix: bool,
    msix_table_size: Option<u16>,
    msix_function_masked: Option<bool>,
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
struct SpuriousIrqStats {
    irq7: u64,
    irq15: u64,
    total: u64,
}

#[cfg(target_os = "redox")]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct AffinityProbe {
    irq: u32,
    cpu_id: u8,
    cpu_mask: u64,
}

fn root_prefix() -> PathBuf {
    std::env::var_os("REDBEAR_HWUTILS_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn resolve(root: &Path, absolute: &str) -> PathBuf {
    root.join(absolute.trim_start_matches('/'))
}

fn require_path(root: &Path, absolute: &str) -> Result<(), String> {
    let path = resolve(root, absolute);
    if path.exists() {
        println!("present={absolute}");
        Ok(())
    } else {
        Err(format!("missing {absolute}"))
    }
}

fn read_dir_names(path: &Path) -> Vec<String> {
    let mut names = match fs::read_dir(path) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect::<Vec<_>>(),
        Err(_) => Vec::new(),
    };
    names.sort();
    names
}

fn collect_irq_reports(root: &Path) -> Vec<IrqReport> {
    let mut reports = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    for dir in IRQ_REPORT_DIRS {
        for name in read_dir_names(&resolve(root, dir))
            .into_iter()
            .filter(|name| name.ends_with(".env"))
        {
            let path = resolve(root, &format!("{dir}/{name}"));
            if !seen.insert(path.clone()) {
                continue;
            }
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };

            let mut driver = None;
            let mut pid = None;
            let mut device = None;
            let mut mode = None;
            let mut reason = None;

            for line in content.lines() {
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                match key.trim() {
                    "driver" => driver = Some(value.trim().to_string()),
                    "pid" => pid = value.trim().parse::<u32>().ok(),
                    "device" => device = Some(value.trim().to_string()),
                    "mode" => mode = Some(value.trim().to_string()),
                    "reason" => reason = Some(value.trim().to_string()),
                    _ => {}
                }
            }

            if let (Some(driver), Some(pid), Some(device), Some(mode), Some(reason)) =
                (driver, pid, device, mode, reason)
            {
                if !resolve(root, &format!("/proc/{pid}")).exists() {
                    continue;
                }
                reports.push(IrqReport {
                    driver,
                    pid,
                    device,
                    mode,
                    reason,
                });
            }
        }
    }

    reports.sort_by(|left, right| {
        left.driver
            .cmp(&right.driver)
            .then(left.device.cmp(&right.device))
    });
    reports
}

fn parse_runtime_pci_location(device: &str) -> Option<PciLocation> {
    let (segment, rest) = device.split_once(':')?;
    let (bus, rest) = rest.split_once(':')?;
    let (slot, function) = rest.split_once('.')?;

    Some(PciLocation {
        segment: u16::from_str_radix(segment, 16).ok()?,
        bus: u8::from_str_radix(bus, 16).ok()?,
        device: u8::from_str_radix(slot, 16).ok()?,
        function: function.parse().ok()?,
    })
}

fn collect_device_probes(
    root: &Path,
    reports: &[IrqReport],
) -> Result<Vec<PciDeviceProbe>, String> {
    let mut probes = Vec::new();
    let mut seen = BTreeSet::new();

    for report in reports {
        if !seen.insert(report.device.clone()) {
            continue;
        }

        let location = parse_runtime_pci_location(&report.device)
            .ok_or_else(|| format!("invalid PCI location {} in IRQ report", report.device))?;
        let config_path = resolve(root, &format!("{}/config", location.scheme_path()));
        let config = fs::read(&config_path)
            .map_err(|err| format!("failed to read {}: {err}", config_path.display()))?;

        if config.len() < 64 {
            return Err(format!(
                "PCI config space for {} was too short: {} bytes",
                report.device,
                config.len()
            ));
        }

        let info = parse_device_info_from_config_space(location, &config).ok_or_else(|| {
            format!(
                "failed to parse PCI config space for {} from {}",
                report.device,
                config_path.display()
            )
        })?;
        let msix = info.find_msix();

        probes.push(PciDeviceProbe {
            device: report.device.clone(),
            irq_line: info.irq,
            interrupt_support: info.interrupt_support().as_str().to_string(),
            supports_msi: info.supports_msi(),
            supports_msix: info.supports_msix(),
            msix_table_size: msix.as_ref().map(|cap| cap.table_size),
            msix_function_masked: msix.as_ref().map(|cap| cap.masked),
        });
    }

    probes.sort_by(|left, right| left.device.cmp(&right.device));
    Ok(probes)
}

fn parse_spurious_irq_stats(text: &str) -> Result<SpuriousIrqStats, String> {
    let mut stats = SpuriousIrqStats::default();
    let mut saw_irq7 = false;
    let mut saw_irq15 = false;
    let mut saw_total = false;

    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let Some(value) = parts.next() else {
            continue;
        };
        let Some(label) = parts.next() else {
            continue;
        };
        let count = value
            .parse::<u64>()
            .map_err(|err| format!("invalid spurious IRQ count '{value}': {err}"))?;

        match label {
            "IRQ7" => {
                stats.irq7 = count;
                saw_irq7 = true;
            }
            "IRQ15" => {
                stats.irq15 = count;
                saw_irq15 = true;
            }
            "total" => {
                stats.total = count;
                saw_total = true;
            }
            _ => {}
        }
    }

    if !saw_irq7 || !saw_irq15 || !saw_total {
        return Err("spurious IRQ report was missing IRQ7, IRQ15, or total counters".to_string());
    }

    Ok(stats)
}

fn probe_spurious_irqs(root: &Path) -> Result<SpuriousIrqStats, String> {
    let path = resolve(root, "/scheme/sys/spurious_irq");
    let content = fs::read_to_string(&path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    parse_spurious_irq_stats(&content)
}

#[cfg(target_os = "redox")]
fn read_bsp_cpu_id() -> Result<u8, String> {
    let mut file = File::open("/scheme/irq/bsp")
        .map_err(|err| format!("failed to open /scheme/irq/bsp: {err}"))?;
    let mut buf = [0u8; 8];
    let bytes_read = file
        .read(&mut buf)
        .map_err(|err| format!("failed to read /scheme/irq/bsp: {err}"))?;

    let raw = match bytes_read {
        8 => u64::from_le_bytes(buf),
        4 => u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as u64,
        _ => {
            return Err(format!(
                "unexpected /scheme/irq/bsp payload size {bytes_read}"
            ));
        }
    };

    u8::try_from(raw).map_err(|_| format!("BSP CPU id {raw} does not fit in u8"))
}

#[cfg(target_os = "redox")]
fn probe_interrupt_affinity(probes: &[PciDeviceProbe]) -> Result<AffinityProbe, String> {
    let irq = probes
        .iter()
        .filter_map(|probe| probe.irq_line)
        .next()
        .ok_or_else(|| {
            "no active PCI device exposed a legacy IRQ line for affinity validation".to_string()
        })?;

    let cpu_id = read_bsp_cpu_id()?;
    let cpu_mask = 1u64
        .checked_shl(u32::from(cpu_id))
        .ok_or_else(|| format!("BSP CPU id {cpu_id} exceeds 64-bit affinity mask width"))?;

    let handle = IrqHandle::request(irq)
        .map_err(|err| format!("failed to request IRQ {irq} for affinity validation: {err}"))?;
    handle
        .set_affinity(cpu_mask)
        .map_err(|err| format!("failed to set IRQ {irq} affinity to mask {cpu_mask:#x}: {err}"))?;

    Ok(AffinityProbe {
        irq,
        cpu_id,
        cpu_mask,
    })
}

fn run() -> Result<(), String> {
    parse_args(PROGRAM, USAGE, std::env::args())?;
    let root = root_prefix();

    println!("=== Red Bear OS PCI/IRQ Runtime Check ===");
    require_path(&root, "/scheme/irq")?;

    let reports = collect_irq_reports(&root);

    println!("PCI_IRQ_REPORTS={}", reports.len());
    for report in &reports {
        println!(
            "PCI_IRQ_REPORT={} pid={} device={} mode={} reason={}",
            report.driver, report.pid, report.device, report.mode, report.reason
        );
    }

    if reports.is_empty() {
        return Err("no live PCI/IRQ runtime reports found".to_string());
    }

    let probes = collect_device_probes(&root, &reports)?;
    println!("PCI_IRQ_ACTIVE_DEVICES={}", probes.len());

    let msi_capable = probes.iter().filter(|probe| probe.supports_msi).count();
    let msix_capable = probes.iter().filter(|probe| probe.supports_msix).count();
    println!("PCI_IRQ_MSI_CAPABLE={msi_capable}");
    println!("PCI_IRQ_MSIX_CAPABLE={msix_capable}");

    for probe in &probes {
        let irq_line = probe
            .irq_line
            .map(|irq| irq.to_string())
            .unwrap_or_else(|| "none".to_string());
        let msix_table_size = probe
            .msix_table_size
            .map(|size| size.to_string())
            .unwrap_or_else(|| "none".to_string());
        let msix_function_masked = probe
            .msix_function_masked
            .map(|masked| if masked { "1" } else { "0" }.to_string())
            .unwrap_or_else(|| "none".to_string());

        println!(
            "PCI_IRQ_CAPABILITY={} irq_line={} interrupt_support={} msi_capable={} msix_capable={} msix_table_size={} msix_function_masked={}",
            probe.device,
            irq_line,
            probe.interrupt_support,
            if probe.supports_msi { 1 } else { 0 },
            if probe.supports_msix { 1 } else { 0 },
            msix_table_size,
            msix_function_masked
        );
    }

    if msi_capable == 0 && msix_capable == 0 {
        return Err("no live PCI device exposed MSI/MSI-X capability".to_string());
    }

    let spurious = probe_spurious_irqs(&root)?;
    println!("PCI_IRQ_SPURIOUS_IRQ7={}", spurious.irq7);
    println!("PCI_IRQ_SPURIOUS_IRQ15={}", spurious.irq15);
    println!("PCI_IRQ_SPURIOUS_TOTAL={}", spurious.total);

    if spurious.total > 0 {
        return Err(format!(
            "spurious IRQs observed (irq7={} irq15={} total={})",
            spurious.irq7, spurious.irq15, spurious.total
        ));
    }

    #[cfg(target_os = "redox")]
    {
        let affinity = probe_interrupt_affinity(&probes)?;
        println!(
            "PCI_IRQ_AFFINITY=ok irq={} cpu={} mask={:#x}",
            affinity.irq, affinity.cpu_id, affinity.cpu_mask
        );
    }

    #[cfg(not(target_os = "redox"))]
    {
        println!("PCI_IRQ_AFFINITY=host_build_stub");
    }

    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{PROGRAM}: {err}");
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("redbear-phase-pci-irq-check-{unique}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn create_dir(root: &Path, absolute: &str) {
        fs::create_dir_all(resolve(root, absolute)).unwrap();
    }

    fn write_file(root: &Path, absolute: &str, content: &str) {
        let path = resolve(root, absolute);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn collect_irq_reports_uses_live_pid_entries() {
        let root = temp_root();
        create_dir(&root, "/proc/42");
        write_file(
            &root,
            "/tmp/redbear-irq-report/xhcid--0000_00_14.0.env",
            "driver=xhcid\npid=42\ndevice=0000:00:14.0\nmode=msi_or_msix\nreason=driver_selected_interrupt_delivery\n",
        );

        let reports = collect_irq_reports(&root);
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].driver, "xhcid");
        assert_eq!(reports[0].mode, "msi_or_msix");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn collect_irq_reports_ignores_stale_entries() {
        let root = temp_root();
        write_file(
            &root,
            "/scheme/initfs/tmp/redbear-irq-report/virtio-netd--0000_00_03.0.env",
            "driver=virtio-netd\npid=99\ndevice=0000:00:03.0\nmode=msix\nreason=virtio_driver_selected_msix\n",
        );

        let reports = collect_irq_reports(&root);
        assert!(reports.is_empty());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parse_runtime_pci_location_accepts_standard_bdf_string() {
        let location = parse_runtime_pci_location("0000:00:14.0").unwrap();
        assert_eq!(location.segment, 0);
        assert_eq!(location.bus, 0);
        assert_eq!(location.device, 0x14);
        assert_eq!(location.function, 0);
    }

    #[test]
    fn parse_spurious_irq_stats_reads_all_counters() {
        let stats = parse_spurious_irq_stats("0\tIRQ7\n1\tIRQ15\n1\ttotal\n").unwrap();
        assert_eq!(stats.irq7, 0);
        assert_eq!(stats.irq15, 1);
        assert_eq!(stats.total, 1);
    }
}
