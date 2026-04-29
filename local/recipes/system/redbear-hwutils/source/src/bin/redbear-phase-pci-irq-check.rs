use std::{
    fs,
    path::{Path, PathBuf},
    process,
};

use redbear_hwutils::parse_args;

const PROGRAM: &str = "redbear-phase-pci-irq-check";
const USAGE: &str = "Usage: redbear-phase-pci-irq-check\n\nShow bounded live PCI/IRQ runtime reporting from the current target runtime.";

#[derive(Debug, Clone, Eq, PartialEq)]
struct IrqReport {
    driver: String,
    pid: u32,
    device: String,
    mode: String,
    reason: String,
}

fn root_prefix() -> PathBuf {
    std::env::var_os("REDBEAR_HWUTILS_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn resolve(root: &Path, absolute: &str) -> PathBuf {
    root.join(absolute.trim_start_matches('/'))
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

    for dir in [
        "/tmp/redbear-irq-report",
        "/tmp/run/redbear-irq-report",
        "/run/redbear-irq-report",
        "/var/run/redbear-irq-report",
        "/scheme/initfs/tmp/redbear-irq-report",
        "/scheme/initfs/tmp/run/redbear-irq-report",
        "/scheme/initfs/run/redbear-irq-report",
        "/scheme/initfs/var/run/redbear-irq-report",
    ] {
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

fn run() -> Result<(), String> {
    parse_args(PROGRAM, USAGE, std::env::args())?;
    let root = root_prefix();
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
}
