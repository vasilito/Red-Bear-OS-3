use std::{
    fs,
    path::{Path, PathBuf},
    process,
};

use redbear_hwutils::parse_args;

const PROGRAM: &str = "redbear-phase-acpi-check";
const USAGE: &str = "Usage: redbear-phase-acpi-check\n\nShow the bounded ACPI runtime surface inside the target runtime.";

#[derive(Debug, Default, Eq, PartialEq)]
struct AcpiSurface {
    acpi_root_present: bool,
    kernel_kstop_present: bool,
    dmi_present: bool,
    reboot_present: bool,
    power_present: bool,
    adapter_count: usize,
    battery_count: usize,
    dmi_match_lines: usize,
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

fn discover_surface(root: &Path) -> AcpiSurface {
    let acpi_root = resolve(root, "/scheme/acpi");
    let kernel_kstop = resolve(root, "/scheme/kernel.acpi/kstop");
    let dmi = resolve(root, "/scheme/acpi/dmi");
    let reboot = resolve(root, "/scheme/acpi/reboot");
    let power = resolve(root, "/scheme/acpi/power");

    let dmi_match_lines = fs::read_to_string(&dmi)
        .ok()
        .map(|content| {
            content
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .count()
        })
        .unwrap_or(0);

    let adapter_count = read_dir_names(&power.join("adapters")).len();
    let battery_count = read_dir_names(&power.join("batteries")).len();

    AcpiSurface {
        acpi_root_present: acpi_root.exists(),
        kernel_kstop_present: kernel_kstop.exists(),
        dmi_present: dmi.exists(),
        reboot_present: reboot.exists(),
        power_present: power.exists(),
        adapter_count,
        battery_count,
        dmi_match_lines,
    }
}

fn run() -> Result<(), String> {
    parse_args(PROGRAM, USAGE, std::env::args())?;

    let root = root_prefix();
    let surface = discover_surface(&root);

    println!(
        "ACPI_ROOT={}",
        if surface.acpi_root_present { "present" } else { "missing" }
    );
    println!(
        "KERNEL_KSTOP={}",
        if surface.kernel_kstop_present {
            "present"
        } else {
            "missing"
        }
    );
    println!("ACPI_DMI={}", if surface.dmi_present { "present" } else { "missing" });
    println!(
        "ACPI_REBOOT={}",
        if surface.reboot_present {
            "present"
        } else {
            "missing"
        }
    );
    println!(
        "ACPI_POWER={}",
        if surface.power_present {
            "present"
        } else {
            "unavailable"
        }
    );
    println!("ACPI_POWER_ADAPTERS={}", surface.adapter_count);
    println!("ACPI_POWER_BATTERIES={}", surface.battery_count);
    println!("ACPI_DMI_MATCH_LINES={}", surface.dmi_match_lines);

    if !surface.kernel_kstop_present {
        return Err("missing /scheme/kernel.acpi/kstop".to_string());
    }
    if !surface.dmi_present {
        return Err("missing /scheme/acpi/dmi".to_string());
    }
    if !surface.reboot_present {
        return Err("missing /scheme/acpi/reboot".to_string());
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
        let path = std::env::temp_dir().join(format!("redbear-phase-acpi-check-{unique}"));
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
    fn discover_surface_marks_optional_power_as_unavailable() {
        let root = temp_root();
        create_dir(&root, "/scheme/acpi");
        write_file(&root, "/scheme/kernel.acpi/kstop", "1");
        write_file(&root, "/scheme/acpi/dmi", "sys_vendor=Framework\n");
        write_file(&root, "/scheme/acpi/reboot", "");

        let surface = discover_surface(&root);
        assert!(surface.acpi_root_present);
        assert!(surface.kernel_kstop_present);
        assert!(surface.dmi_present);
        assert!(surface.reboot_present);
        assert!(!surface.power_present);
        assert_eq!(surface.dmi_match_lines, 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discover_surface_counts_power_entries_when_present() {
        let root = temp_root();
        create_dir(&root, "/scheme/acpi/power/adapters/AC");
        create_dir(&root, "/scheme/acpi/power/batteries/BAT0");

        let surface = discover_surface(&root);
        assert!(surface.power_present);
        assert_eq!(surface.adapter_count, 1);
        assert_eq!(surface.battery_count, 1);

        fs::remove_dir_all(root).unwrap();
    }
}
