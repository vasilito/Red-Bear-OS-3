use std::fs;
use std::io;
use std::os::unix::fs::symlink;
use std::path::Path;

const DEFAULT_UDEV_RULES: &str = r#"# Network interface naming
SUBSYSTEM=="net", KERNEL=="enp*", NAME="$kernel"

# Storage device naming
SUBSYSTEM=="block", KERNEL=="nvme*", SYMLINK+="disk/by-id/nvme-$attr{model}_$attr{serial}"
SUBSYSTEM=="block", KERNEL=="sd*", SYMLINK+="disk/by-id/ata-$attr{model}_$attr{serial}"
"#;

/// Generate predictable network interface name from PCI location.
///
/// Format: `enp{bus}s{slot}` — for example `enp0s1`.
pub fn predictable_net_name(pci_addr: &str) -> String {
    let parts: Vec<&str> = pci_addr.split(&[':', '.'][..]).collect();
    let (bus_part, slot_part) = match parts.as_slice() {
        [bus, slot, _func] => (*bus, *slot),
        [_segment, bus, slot, _func] => (*bus, *slot),
        _ => return "eth0".to_string(),
    };

    match (parse_hex_byte(bus_part), parse_hex_byte(slot_part)) {
        (Some(bus), Some(slot)) => format!("enp{}s{}", bus, slot),
        _ => "eth0".to_string(),
    }
}

/// Generate predictable NVMe disk name.
///
/// Format: `nvme{cntlid}n{nsid}`.
pub fn predictable_nvme_name(controller_id: u32, namespace_id: u32) -> String {
    format!("nvme{}n{}", controller_id, namespace_id)
}

/// Generate predictable SATA disk name.
///
/// Format: `sd{a,b,c,...}` with Linux-style suffix rollover.
pub fn predictable_sata_name(port: u8) -> String {
    format!("sd{}", alpha_suffix(usize::from(port)))
}

pub fn disk_by_id_path(model: &str, serial: &str) -> String {
    let model = sanitize_component(model);
    let serial = sanitize_component(serial);
    format!("/dev/disk/by-id/{}_{}", model, serial)
}

/// Create a `/dev/disk/by-id/` symlink for a storage device.
pub fn create_disk_by_id(name: &str, model: &str, serial: &str) -> io::Result<String> {
    let dir = Path::new("/dev/disk/by-id");
    fs::create_dir_all(dir)?;

    let link_path = disk_by_id_path(model, serial);
    let target = format!("/dev/{name}");
    let link = Path::new(&link_path);

    if fs::symlink_metadata(link).is_ok() {
        fs::remove_file(link)?;
    }

    symlink(&target, link)?;
    Ok(link_path)
}

pub fn default_udev_rules() -> &'static str {
    DEFAULT_UDEV_RULES
}

pub fn write_default_rules_file() -> io::Result<&'static str> {
    let dir = Path::new("/etc/udev/rules.d");
    fs::create_dir_all(dir)?;

    let path = dir.join("50-default.rules");
    fs::write(&path, default_udev_rules())?;
    Ok("/etc/udev/rules.d/50-default.rules")
}

fn parse_hex_byte(value: &str) -> Option<u8> {
    u8::from_str_radix(value, 16).ok()
}

fn alpha_suffix(mut index: usize) -> String {
    let mut suffix = String::new();

    loop {
        let remainder = index % 26;
        suffix.insert(0, char::from(b'a' + remainder as u8));

        if index < 26 {
            break;
        }

        index = (index / 26).saturating_sub(1);
    }

    suffix
}

fn sanitize_component(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => ch,
            _ => '_',
        })
        .collect();

    if sanitized.is_empty() {
        "unknown".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_name_bus0_slot25() {
        assert_eq!(predictable_net_name("00:19.0"), "enp0s25");
    }

    #[test]
    fn net_name_bus2_slot0() {
        assert_eq!(predictable_net_name("02:00.0"), "enp2s0");
    }

    #[test]
    fn net_name_with_segment_prefix() {
        assert_eq!(predictable_net_name("0000:00:19.0"), "enp0s25");
    }

    #[test]
    fn nvme_name_default() {
        assert_eq!(predictable_nvme_name(0, 1), "nvme0n1");
    }

    #[test]
    fn sata_name_port0() {
        assert_eq!(predictable_sata_name(0), "sda");
    }

    #[test]
    fn sata_name_rolls_over_after_z() {
        assert_eq!(predictable_sata_name(26), "sdaa");
    }

    #[test]
    fn disk_by_id_path_sanitizes_components() {
        assert_eq!(
            disk_by_id_path("Samsung SSD", "pci-0000:00:1f.2"),
            "/dev/disk/by-id/Samsung_SSD_pci-0000_00_1f.2"
        );
    }

    #[test]
    fn default_rules_include_network_and_storage_entries() {
        let rules = default_udev_rules();
        assert!(rules.contains("KERNEL==\"enp*\""));
        assert!(rules.contains("KERNEL==\"nvme*\""));
        assert!(rules.contains("KERNEL==\"sd*\""));
    }
}
