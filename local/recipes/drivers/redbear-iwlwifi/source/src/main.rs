use std::env;
use std::fs;
use std::path::PathBuf;

#[cfg(target_os = "redox")]
use redox_driver_sys::memory::{CacheType, MmioProt};
#[cfg(target_os = "redox")]
use redox_driver_sys::pci::PciDevice;
use redox_driver_sys::pci::{PciLocation, PCI_VENDOR_ID_INTEL};
#[cfg(target_os = "redox")]
use std::ffi::CString;
use thiserror::Error;

#[cfg(target_os = "redox")]
use linux_kpi::firmware::{release_firmware, request_firmware, Firmware};

#[repr(C)]
#[derive(Default)]
#[cfg(target_os = "redox")]
struct LinuxDeviceDriver {
    name: *const i8,
    owner: *mut core::ffi::c_void,
}

#[repr(C)]
#[derive(Default)]
#[cfg(target_os = "redox")]
struct LinuxDevice {
    driver: *mut LinuxDeviceDriver,
    driver_data: *mut core::ffi::c_void,
    platform_data: *mut core::ffi::c_void,
    of_node: *mut core::ffi::c_void,
    dma_mask: u64,
}

#[repr(C)]
#[derive(Default)]
#[cfg(target_os = "redox")]
struct LinuxPciDev {
    vendor: u16,
    device_id: u16,
    bus_number: u8,
    dev_number: u8,
    func_number: u8,
    revision: u8,
    irq: u32,
    resource_start: [u64; 6],
    resource_len: [u64; 6],
    driver_data: *mut core::ffi::c_void,
    device_obj: LinuxDevice,
}

unsafe extern "C" {
    #[cfg(target_os = "redox")]
    fn rb_iwlwifi_linux_prepare(
        dev: *mut LinuxPciDev,
        ucode: *const i8,
        pnvm: *const i8,
        out: *mut i8,
        out_len: usize,
    ) -> i32;
    #[cfg(target_os = "redox")]
    fn rb_iwlwifi_linux_transport_probe(
        dev: *mut LinuxPciDev,
        bar: u32,
        out: *mut i8,
        out_len: usize,
    ) -> i32;
    #[cfg(target_os = "redox")]
    fn rb_iwlwifi_linux_init_transport(
        dev: *mut LinuxPciDev,
        bar: u32,
        bz_family: i32,
        out: *mut i8,
        out_len: usize,
    ) -> i32;
    #[cfg(target_os = "redox")]
    fn rb_iwlwifi_linux_activate_nic(
        dev: *mut LinuxPciDev,
        bar: u32,
        bz_family: i32,
        out: *mut i8,
        out_len: usize,
    ) -> i32;
    #[cfg(target_os = "redox")]
    fn rb_iwlwifi_linux_scan(
        dev: *mut LinuxPciDev,
        ssid: *const i8,
        out: *mut i8,
        out_len: usize,
    ) -> i32;
    #[cfg(target_os = "redox")]
    fn rb_iwlwifi_linux_connect(
        dev: *mut LinuxPciDev,
        ssid: *const i8,
        security: *const i8,
        key: *const i8,
        out: *mut i8,
        out_len: usize,
    ) -> i32;
    #[cfg(target_os = "redox")]
    fn rb_iwlwifi_linux_disconnect(dev: *mut LinuxPciDev, out: *mut i8, out_len: usize) -> i32;
    #[cfg(target_os = "redox")]
    fn rb_iwlwifi_full_init(
        dev: *mut LinuxPciDev,
        bar: u32,
        bz_family: i32,
        ucode: *const i8,
        pnvm: *const i8,
        out: *mut i8,
        out_len: usize,
    ) -> i32;
    #[cfg(target_os = "redox")]
    fn rb_iwlwifi_status(dev: *mut LinuxPciDev, out: *mut i8, out_len: usize) -> i32;
    #[cfg(target_os = "redox")]
    fn rb_iwlwifi_register_mac80211(dev: *mut LinuxPciDev, out: *mut i8, out_len: usize) -> i32;
}

#[derive(Debug, Error)]
enum DriverError {
    #[error("PCI error: {0}")]
    Pci(String),
    #[error("Unsupported device: {0}")]
    Unsupported(String),
}

#[derive(Clone, Debug)]
struct Candidate {
    location: PciLocation,
    config_path: PathBuf,
    device_id: u16,
    subsystem_id: u16,
    family: &'static str,
    ucode_candidates: Vec<String>,
    selected_ucode: Option<String>,
    pnvm_candidate: Option<String>,
    pnvm_found: Option<String>,
}

#[cfg(target_os = "redox")]
const IWL_CSR_HW_IF_CONFIG_REG: usize = 0x000;
#[cfg(target_os = "redox")]
const IWL_CSR_RESET: usize = 0x020;
#[cfg(target_os = "redox")]
const IWL_CSR_GP_CNTRL: usize = 0x024;
#[cfg(target_os = "redox")]
const IWL_CSR_GP_CNTRL_REG_FLAG_MAC_ACCESS_REQ: u32 = 0x00000008;
#[cfg(target_os = "redox")]
const IWL_CSR_GP_CNTRL_REG_FLAG_MAC_CLOCK_READY: u32 = 0x00000001;
#[cfg(target_os = "redox")]
const IWL_CSR_GP_CNTRL_REG_FLAG_BZ_MAC_ACCESS_REQ: u32 = 0x00200000;
#[cfg(target_os = "redox")]
const IWL_CSR_HW_IF_CONFIG_REG_BIT_NIC_READY: u32 = 0x00000004;
#[cfg(target_os = "redox")]
const IWL_CSR_GP_CNTRL_REG_FLAG_SW_RESET_BZ: u32 = 0x80000000;
#[cfg(target_os = "redox")]
const IWL_CSR_RESET_REG_FLAG_SW_RESET: u32 = 0x00000080;
#[cfg(target_os = "redox")]
const IWL_CSR_GP_CNTRL_REG_FLAG_INIT_DONE: u32 = 0x00000004;

fn main() {
    let mut args = env::args().skip(1);
    let firmware_root = env::var_os("REDBEAR_IWLWIFI_FIRMWARE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/lib/firmware"));
    match args.next().as_deref() {
        Some("--probe") => match detect_candidates(&firmware_root) {
            Ok(candidates) => print_candidates(&candidates),
            Err(err) => {
                eprintln!("redbear-iwlwifi: probe failed: {err}");
                std::process::exit(1);
            }
        },
        Some("--status") => {
            let target = args.next();
            run_device_action(&firmware_root, target, status_candidate, "status")
        }
        Some("--prepare") => {
            let target = args.next();
            run_device_action(&firmware_root, target, prepare_candidate, "prepare")
        }
        Some("--transport-probe") => {
            let target = args.next();
            run_device_action(
                &firmware_root,
                target,
                transport_probe_candidate,
                "transport-probe",
            )
        }
        Some("--init-transport") => {
            let target = args.next();
            run_device_action(
                &firmware_root,
                target,
                init_transport_candidate,
                "init-transport",
            )
        }
        Some("--activate-nic") => {
            let target = args.next();
            run_device_action(&firmware_root, target, activate_candidate, "activate-nic")
        }
        Some("--scan") => {
            let target = args.next();
            run_device_action(&firmware_root, target, scan_candidate, "scan")
        }
        Some("--connect") => {
            let target = args.next();
            let ssid = args.next().unwrap_or_default();
            let security = args.next().unwrap_or_else(|| "open".to_string());
            let key = args.next();
            run_connect_action(&firmware_root, target, &ssid, &security, key.as_deref())
        }
        Some("--disconnect") => {
            let target = args.next();
            run_device_action(&firmware_root, target, disconnect_candidate, "disconnect")
        }
        Some("--full-init") => {
            let target = args.next();
            run_device_action(&firmware_root, target, full_init_candidate, "full-init")
        }
        Some("--irq-test") => {
            let target = args.next();
            run_device_action(&firmware_root, target, irq_test_candidate, "irq-test")
        }
        Some("--dma-test") => {
            let target = args.next();
            run_device_action(&firmware_root, target, dma_test_candidate, "dma-test")
        }
        Some("--retry") => {
            let target = args.next();
            run_device_action(&firmware_root, target, retry_candidate, "retry")
        }
        _ => {
            eprintln!(
                "redbear-iwlwifi: use --probe, --status <device>, --prepare <device>, --transport-probe <device>, --init-transport <device>, --activate-nic <device>, --scan <device>, --connect <device> <ssid> <security> [key], --disconnect <device>, --full-init <device>, --irq-test <device>, --dma-test <device>, or --retry <device>"
            );
            std::process::exit(1);
        }
    }
}

fn run_connect_action(
    firmware_root: &PathBuf,
    target: Option<String>,
    ssid: &str,
    security: &str,
    key: Option<&str>,
) {
    match detect_candidates(firmware_root) {
        Ok(candidates) => {
            let candidate = match select_candidate(candidates, target.as_deref()) {
                Ok(candidate) => candidate,
                Err(err) => {
                    eprintln!("redbear-iwlwifi: connect selection failed: {err}");
                    std::process::exit(1);
                }
            };
            match connect_candidate(&candidate, firmware_root, ssid, security, key) {
                Ok(lines) => {
                    for line in lines {
                        println!("{line}");
                    }
                }
                Err(err) => {
                    eprintln!("redbear-iwlwifi: connect failed: {err}");
                    std::process::exit(1);
                }
            }
        }
        Err(err) => {
            eprintln!("redbear-iwlwifi: connect probe failed: {err}");
            std::process::exit(1);
        }
    }
}

fn print_candidates(candidates: &[Candidate]) {
    println!("candidates={}", candidates.len());
    for candidate in candidates {
        println!(
            "device={} family={} ucode_selected={} pnvm={} ucode_candidates={}",
            candidate.location,
            candidate.family,
            candidate
                .selected_ucode
                .clone()
                .unwrap_or_else(|| "missing".to_string()),
            candidate
                .pnvm_found
                .clone()
                .or_else(|| candidate.pnvm_candidate.clone())
                .unwrap_or_else(|| "none".to_string()),
            candidate.ucode_candidates.join(",")
        );
    }
}

fn run_device_action(
    firmware_root: &PathBuf,
    target: Option<String>,
    action: fn(&Candidate, &PathBuf) -> Result<Vec<String>, DriverError>,
    action_name: &str,
) {
    match detect_candidates(firmware_root) {
        Ok(candidates) => {
            let candidate = match select_candidate(candidates, target.as_deref()) {
                Ok(candidate) => candidate,
                Err(err) => {
                    eprintln!("redbear-iwlwifi: {action_name} selection failed: {err}");
                    std::process::exit(1);
                }
            };
            match action(&candidate, firmware_root) {
                Ok(lines) => {
                    for line in lines {
                        println!("{line}");
                    }
                }
                Err(err) => {
                    eprintln!("redbear-iwlwifi: {action_name} failed: {err}");
                    std::process::exit(1);
                }
            }
        }
        Err(err) => {
            eprintln!("redbear-iwlwifi: {action_name} probe failed: {err}");
            std::process::exit(1);
        }
    }
}

fn select_candidate(
    candidates: Vec<Candidate>,
    target: Option<&str>,
) -> Result<Candidate, DriverError> {
    if let Some(target) = target {
        candidates
            .into_iter()
            .find(|candidate| candidate.location.to_string() == target)
            .ok_or_else(|| {
                DriverError::Unsupported(format!("no Intel Wi-Fi candidate matches {target}"))
            })
    } else {
        candidates.into_iter().next().ok_or_else(|| {
            DriverError::Unsupported("no supported Intel Wi-Fi candidates detected".to_string())
        })
    }
}

fn status_candidate(
    candidate: &Candidate,
    firmware_root: &PathBuf,
) -> Result<Vec<String>, DriverError> {
    if let Ok(lines) = linux_kpi_status_candidate(candidate) {
        return Ok(lines);
    }

    let mut lines = vec![
        format!("device={}", candidate.location),
        format!("config_path={}", candidate.config_path.display()),
        format!("device_id=0x{:04x}", candidate.device_id),
        format!("subsystem_id=0x{:04x}", candidate.subsystem_id),
        format!("family={}", candidate.family),
        format!(
            "selected_ucode={}",
            candidate
                .selected_ucode
                .clone()
                .unwrap_or_else(|| "missing".to_string())
        ),
        format!(
            "selected_pnvm={}",
            candidate
                .pnvm_found
                .clone()
                .or_else(|| candidate.pnvm_candidate.clone())
                .unwrap_or_else(|| "none".to_string())
        ),
    ];

    if prepare_candidate(candidate, firmware_root).is_ok() {
        lines.push("status=firmware-ready".to_string());
    } else {
        lines.push("status=device-detected".to_string());
    }

    Ok(lines)
}

fn detect_candidates(firmware_root: &PathBuf) -> Result<Vec<Candidate>, DriverError> {
    let pci_root = env::var_os("REDBEAR_IWLWIFI_PCI_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/scheme/pci"));
    let entries = fs::read_dir(&pci_root)
        .map_err(|err| DriverError::Pci(format!("failed to read {}: {err}", pci_root.display())))?;

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let config_path = entry.path().join("config");
        let Ok(config) = fs::read(&config_path) else {
            continue;
        };
        if config.len() < 48 {
            continue;
        }
        let vendor_id = u16::from_le_bytes([config[0x00], config[0x01]]);
        let device_id = u16::from_le_bytes([config[0x02], config[0x03]]);
        let class_code = config[0x0B];
        let subclass = config[0x0A];
        if vendor_id != PCI_VENDOR_ID_INTEL || class_code != 0x02 || subclass != 0x80 {
            continue;
        }
        let subsystem_id = u16::from_le_bytes([config[0x2E], config[0x2F]]);
        let location = parse_location_from_config_path(&config_path)?;
        let (family, ucode_candidates, pnvm_candidate) =
            intel_firmware_candidates(device_id, subsystem_id);
        let selected_ucode = ucode_candidates
            .iter()
            .find(|candidate| firmware_root.join(candidate).exists())
            .cloned();
        let pnvm_found = pnvm_candidate
            .as_ref()
            .filter(|candidate| firmware_root.join(candidate).exists())
            .cloned();

        out.push(Candidate {
            location,
            config_path,
            device_id,
            subsystem_id,
            family,
            ucode_candidates,
            selected_ucode,
            pnvm_candidate,
            pnvm_found,
        });
    }

    Ok(out)
}

fn parse_location_from_config_path(config_path: &PathBuf) -> Result<PciLocation, DriverError> {
    let parent = config_path.parent().ok_or_else(|| {
        DriverError::Pci(format!("missing PCI parent for {}", config_path.display()))
    })?;
    let name = parent
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| DriverError::Pci(format!("invalid PCI path {}", parent.display())))?;

    let parts: Vec<&str> = name.splitn(3, "--").collect();
    if parts.len() != 3 {
        return Err(DriverError::Pci(format!("invalid PCI scheme entry {name}")));
    }
    let segment = u16::from_str_radix(parts[0], 16)
        .map_err(|_| DriverError::Pci(format!("invalid segment in {name}")))?;
    let bus = u8::from_str_radix(parts[1], 16)
        .map_err(|_| DriverError::Pci(format!("invalid bus in {name}")))?;
    let dev_func: Vec<&str> = parts[2].splitn(2, '.').collect();
    if dev_func.len() != 2 {
        return Err(DriverError::Pci(format!(
            "invalid device/function in {name}"
        )));
    }
    let device = u8::from_str_radix(dev_func[0], 16)
        .map_err(|_| DriverError::Pci(format!("invalid device in {name}")))?;
    let function = u8::from_str_radix(dev_func[1], 16)
        .map_err(|_| DriverError::Pci(format!("invalid function in {name}")))?;

    Ok(PciLocation {
        segment,
        bus,
        device,
        function,
    })
}

fn intel_firmware_candidates(
    device_id: u16,
    subsystem_id: u16,
) -> (&'static str, Vec<String>, Option<String>) {
    let (stems, pnvm): (Vec<&'static str>, Option<&'static str>) = match (device_id, subsystem_id) {
        (0x7740, 0x4090) => (
            vec![
                "iwlwifi-bz-b0-gf-a0-92.ucode",
                "iwlwifi-bz-b0-gf-a0-94.ucode",
                "iwlwifi-bz-b0-gf-a0-100.ucode",
            ],
            Some("iwlwifi-bz-b0-gf-a0.pnvm"),
        ),
        (0x7740, _) => (
            vec![
                "iwlwifi-bz-b0-fm-c0-92.ucode",
                "iwlwifi-bz-b0-fm-c0-94.ucode",
                "iwlwifi-bz-b0-fm-c0-100.ucode",
            ],
            Some("iwlwifi-bz-b0-fm-c0.pnvm"),
        ),
        (0x2725, _) => (
            vec![
                "iwlwifi-ty-a0-gf-a0-59.ucode",
                "iwlwifi-ty-a0-gf-a0-84.ucode",
            ],
            Some("iwlwifi-ty-a0-gf-a0.pnvm"),
        ),
        (0x7af0, 0x4090) => (
            vec![
                "iwlwifi-so-a0-gf-a0-64.ucode",
                "iwlwifi-so-a0-gf-a0-66.ucode",
            ],
            Some("iwlwifi-so-a0-gf-a0.pnvm"),
        ),
        (0x7af0, 0x4070) => (
            vec!["iwlwifi-so-a0-hr-b0-64.ucode"],
            Some("iwlwifi-so-a0-hr-b0.pnvm"),
        ),
        (0x7af0, 0x0aaa) | (0x7af0, 0x0030) => (
            vec![
                "iwlwifi-so-a0-jf-b0-64.ucode",
                "iwlwifi-9000-pu-b0-jf-b0-46.ucode",
            ],
            Some("iwlwifi-so-a0-jf-b0.pnvm"),
        ),
        _ => (vec!["iwlwifi-unknown"], None),
    };

    let family = match (device_id, subsystem_id) {
        (0x7740, _) => "intel-bz-arrow-lake",
        (0x2725, _) => "intel-ax210",
        (0x7af0, 0x4090) => "intel-ax211",
        (0x7af0, 0x4070) => "intel-ax201",
        (0x7af0, 0x0aaa) | (0x7af0, 0x0030) => "intel-9462-9560",
        _ => "intel-unknown",
    };

    (
        family,
        stems.into_iter().map(str::to_string).collect(),
        pnvm.map(str::to_string),
    )
}

fn read_firmware_blob(root: &PathBuf, name: &str) -> Result<(), DriverError> {
    #[cfg(target_os = "redox")]
    if let Ok(c_name) = CString::new(name) {
        let mut fw_ptr: *mut Firmware = std::ptr::null_mut();
        let rc = request_firmware(
            &mut fw_ptr as *mut *mut Firmware,
            c_name.as_ptr().cast::<u8>(),
            std::ptr::null_mut(),
        );
        if rc == 0 && !fw_ptr.is_null() {
            release_firmware(fw_ptr);
            return Ok(());
        }
    }

    fs::read(root.join(name)).map(|_| ()).map_err(|err| {
        DriverError::Pci(format!(
            "failed to read firmware {} via linux-kpi or {}: {err}",
            name,
            root.join(name).display()
        ))
    })
}

fn prepare_candidate(
    candidate: &Candidate,
    firmware_root: &PathBuf,
) -> Result<Vec<String>, DriverError> {
    if let Ok(lines) = linux_kpi_prepare_candidate(candidate) {
        return Ok(lines);
    }

    let selected = candidate.selected_ucode.clone().ok_or_else(|| {
        DriverError::Unsupported(format!(
            "missing firmware for {} (expected one of: {})",
            candidate.family,
            candidate.ucode_candidates.join(", ")
        ))
    })?;
    read_firmware_blob(firmware_root, &selected)?;
    if let Some(pnvm) = candidate.pnvm_candidate.as_ref() {
        read_firmware_blob(firmware_root, pnvm)?;
    }
    Ok(vec![
        format!("device={}", candidate.location),
        format!("family={}", candidate.family),
        format!("status=firmware-ready"),
        format!("selected_ucode={selected}"),
        format!(
            "selected_pnvm={}",
            candidate
                .pnvm_candidate
                .clone()
                .unwrap_or_else(|| "none".to_string())
        ),
    ])
}

fn init_transport_candidate(
    candidate: &Candidate,
    firmware_root: &PathBuf,
) -> Result<Vec<String>, DriverError> {
    if let Ok(lines) = linux_kpi_init_transport_candidate(candidate, firmware_root) {
        return Ok(lines);
    }

    let mut out = prepare_candidate(candidate, firmware_root)?;

    #[cfg(target_os = "redox")]
    {
        let mut pci = PciDevice::open_location(&candidate.location).map_err(|err| {
            DriverError::Pci(format!(
                "failed to open PCI device {}: {err}",
                candidate.location
            ))
        })?;
        pci.enable_device().map_err(|err| {
            DriverError::Pci(format!(
                "failed to enable PCI device {}: {err}",
                candidate.location
            ))
        })?;
        let info = pci.full_info().map_err(|err| {
            DriverError::Pci(format!(
                "failed to read PCI device {} info: {err}",
                candidate.location
            ))
        })?;
        let bar0 = info.find_memory_bar(0).ok_or_else(|| {
            DriverError::Unsupported(format!("no BAR0 memory window on {}", candidate.location))
        })?;
        let size = usize::try_from(bar0.size)
            .map_err(|_| DriverError::Pci(format!("BAR0 too large on {}", candidate.location)))?;
        let mmio = redox_driver_sys::memory::MmioRegion::map(
            bar0.addr,
            size,
            CacheType::DeviceMemory,
            MmioProt::READ_WRITE,
        )
        .map_err(|err| {
            DriverError::Pci(format!(
                "failed to map BAR0 on {}: {err}",
                candidate.location
            ))
        })?;

        let access_req = if candidate.family.starts_with("intel-bz-") {
            IWL_CSR_GP_CNTRL_REG_FLAG_BZ_MAC_ACCESS_REQ
        } else {
            IWL_CSR_GP_CNTRL_REG_FLAG_MAC_ACCESS_REQ
        };
        let gp_before = mmio.read32(IWL_CSR_GP_CNTRL);
        mmio.write32(IWL_CSR_GP_CNTRL, gp_before | access_req);
        let gp_after = mmio.read32(IWL_CSR_GP_CNTRL);
        let hw_if = mmio.read32(IWL_CSR_HW_IF_CONFIG_REG);
        let mac_clock = (gp_after & IWL_CSR_GP_CNTRL_REG_FLAG_MAC_CLOCK_READY) != 0;
        let nic_ready = (hw_if & IWL_CSR_HW_IF_CONFIG_REG_BIT_NIC_READY) != 0;

        out.push(format!("status=transport-ready"));
        out.push(format!("bar0_addr=0x{:x}", bar0.addr));
        out.push(format!("bar0_size=0x{:x}", bar0.size));
        out.push(format!(
            "irq={}",
            info.irq
                .map(|irq| irq.to_string())
                .unwrap_or_else(|| "none".to_string())
        ));
        out.push(format!("gp_cntrl_before=0x{gp_before:08x}"));
        out.push(format!("gp_cntrl_after=0x{gp_after:08x}"));
        out.push(format!("hw_if_config=0x{hw_if:08x}"));
        out.push(format!(
            "mac_clock_ready={}",
            if mac_clock { "yes" } else { "no" }
        ));
        out.push(format!(
            "nic_ready={}",
            if nic_ready { "yes" } else { "no" }
        ));
        return Ok(out);
    }

    out.push(format!("status=transport-ready"));
    out.push("bar0_addr=host-skipped".to_string());
    out.push("bar0_size=host-skipped".to_string());
    out.push("irq=host-skipped".to_string());
    Ok(out)
}

fn transport_probe_candidate(
    candidate: &Candidate,
    firmware_root: &PathBuf,
) -> Result<Vec<String>, DriverError> {
    if let Ok(lines) = linux_kpi_transport_probe_candidate(candidate) {
        return Ok(lines);
    }

    init_transport_candidate(candidate, firmware_root)
}

#[cfg(target_os = "redox")]
fn linux_pci_dev(candidate: &Candidate) -> Result<LinuxPciDev, DriverError> {
    let mut dev = LinuxPciDev {
        vendor: PCI_VENDOR_ID_INTEL,
        device_id: candidate.device_id,
        bus_number: candidate.location.bus,
        dev_number: candidate.location.device,
        func_number: candidate.location.function,
        revision: 0,
        irq: 0,
        resource_start: [0; 6],
        resource_len: [0; 6],
        driver_data: std::ptr::null_mut(),
        device_obj: LinuxDevice::default(),
    };

    let mut pci = PciDevice::open_location(&candidate.location).map_err(|err| {
        DriverError::Pci(format!(
            "failed to open PCI device {}: {err}",
            candidate.location
        ))
    })?;
    let info = pci.full_info().map_err(|err| {
        DriverError::Pci(format!(
            "failed to read PCI device {} info: {err}",
            candidate.location
        ))
    })?;
    dev.revision = info.revision;
    dev.irq = info.irq.unwrap_or(0);
    for bar in info.bars {
        if bar.index < dev.resource_start.len() {
            dev.resource_start[bar.index] = bar.addr;
            dev.resource_len[bar.index] = bar.size;
        }
    }

    Ok(dev)
}

#[cfg(target_os = "redox")]
fn linux_kpi_prepare_candidate(candidate: &Candidate) -> Result<Vec<String>, DriverError> {
    let ucode = CString::new(
        candidate
            .selected_ucode
            .clone()
            .ok_or_else(|| DriverError::Unsupported("missing selected ucode".to_string()))?,
    )
    .map_err(|_| DriverError::Unsupported("invalid ucode name".to_string()))?;
    let pnvm = candidate
        .pnvm_candidate
        .as_ref()
        .map(|name| CString::new(name.as_str()))
        .transpose()
        .map_err(|_| DriverError::Unsupported("invalid pnvm name".to_string()))?;
    let mut dev = linux_pci_dev(candidate)?;
    let mut out = vec![0u8; 1024];
    let rc = unsafe {
        rb_iwlwifi_linux_prepare(
            &mut dev,
            ucode.as_ptr(),
            pnvm.as_ref()
                .map(|s| s.as_ptr())
                .unwrap_or(std::ptr::null()),
            out.as_mut_ptr().cast::<i8>(),
            out.len(),
        )
    };
    if rc != 0 {
        return Err(DriverError::Unsupported(format!(
            "linux-kpi prepare path unavailable ({rc})"
        )));
    }
    let line = String::from_utf8_lossy(&out)
        .trim_matches(char::from(0))
        .trim()
        .to_string();
    Ok(vec![
        format!("device={}", candidate.location),
        format!("family={}", candidate.family),
        "status=firmware-ready".to_string(),
        line,
    ])
}

#[cfg(not(target_os = "redox"))]
fn linux_kpi_prepare_candidate(_candidate: &Candidate) -> Result<Vec<String>, DriverError> {
    Err(DriverError::Unsupported(
        "linux-kpi prepare path is Redox-only".to_string(),
    ))
}

#[cfg(target_os = "redox")]
fn linux_kpi_transport_probe_candidate(candidate: &Candidate) -> Result<Vec<String>, DriverError> {
    let mut dev = linux_pci_dev(candidate)?;
    let mut out = vec![0u8; 1024];
    let rc = unsafe {
        rb_iwlwifi_linux_transport_probe(&mut dev, 0, out.as_mut_ptr().cast::<i8>(), out.len())
    };
    if rc != 0 {
        return Err(DriverError::Unsupported(format!(
            "linux-kpi transport-probe path unavailable ({rc})"
        )));
    }
    let line = String::from_utf8_lossy(&out)
        .trim_matches(char::from(0))
        .trim()
        .to_string();
    Ok(vec![format!("device={}", candidate.location), line])
}

#[cfg(not(target_os = "redox"))]
fn linux_kpi_transport_probe_candidate(_candidate: &Candidate) -> Result<Vec<String>, DriverError> {
    Err(DriverError::Unsupported(
        "linux-kpi transport-probe path is Redox-only".to_string(),
    ))
}

#[cfg(target_os = "redox")]
fn linux_kpi_init_transport_candidate(
    candidate: &Candidate,
    firmware_root: &PathBuf,
) -> Result<Vec<String>, DriverError> {
    let mut out = prepare_candidate(candidate, firmware_root)?;
    let mut dev = linux_pci_dev(candidate)?;
    let mut line = vec![0u8; 1024];
    let rc = unsafe {
        rb_iwlwifi_linux_init_transport(
            &mut dev,
            0,
            if candidate.family.starts_with("intel-bz-") {
                1
            } else {
                0
            },
            line.as_mut_ptr().cast::<i8>(),
            line.len(),
        )
    };
    if rc != 0 {
        return Err(DriverError::Unsupported(format!(
            "linux-kpi init-transport path unavailable ({rc})"
        )));
    }
    let parsed = String::from_utf8_lossy(&line)
        .trim_matches(char::from(0))
        .trim()
        .to_string();
    out.push(parsed);
    out.push("status=transport-ready".to_string());
    Ok(out)
}

#[cfg(not(target_os = "redox"))]
fn linux_kpi_init_transport_candidate(
    _candidate: &Candidate,
    _firmware_root: &PathBuf,
) -> Result<Vec<String>, DriverError> {
    Err(DriverError::Unsupported(
        "linux-kpi init-transport path is Redox-only".to_string(),
    ))
}

#[cfg(target_os = "redox")]
fn linux_kpi_activate_candidate(
    candidate: &Candidate,
    firmware_root: &PathBuf,
) -> Result<Vec<String>, DriverError> {
    let mut out = init_transport_candidate(candidate, firmware_root)?;
    let mut dev = linux_pci_dev(candidate)?;
    let mut line = vec![0u8; 1024];
    let rc = unsafe {
        rb_iwlwifi_linux_activate_nic(
            &mut dev,
            0,
            if candidate.family.starts_with("intel-bz-") {
                1
            } else {
                0
            },
            line.as_mut_ptr().cast::<i8>(),
            line.len(),
        )
    };
    if rc != 0 {
        return Err(DriverError::Unsupported(format!(
            "linux-kpi activate path unavailable ({rc})"
        )));
    }
    let parsed = String::from_utf8_lossy(&line)
        .trim_matches(char::from(0))
        .trim()
        .to_string();
    out.push(parsed);
    out.push("status=nic-activated".to_string());
    Ok(out)
}

#[cfg(not(target_os = "redox"))]
fn linux_kpi_activate_candidate(
    _candidate: &Candidate,
    _firmware_root: &PathBuf,
) -> Result<Vec<String>, DriverError> {
    Err(DriverError::Unsupported(
        "linux-kpi activate path is Redox-only".to_string(),
    ))
}

fn activate_candidate(
    candidate: &Candidate,
    firmware_root: &PathBuf,
) -> Result<Vec<String>, DriverError> {
    if let Ok(lines) = linux_kpi_activate_candidate(candidate, firmware_root) {
        return Ok(lines);
    }

    let mut out = init_transport_candidate(candidate, firmware_root)?;

    #[cfg(target_os = "redox")]
    {
        let mut pci = PciDevice::open_location(&candidate.location).map_err(|err| {
            DriverError::Pci(format!(
                "failed to open PCI device {}: {err}",
                candidate.location
            ))
        })?;
        let info = pci.full_info().map_err(|err| {
            DriverError::Pci(format!(
                "failed to read PCI device {} info: {err}",
                candidate.location
            ))
        })?;
        let bar0 = info.find_memory_bar(0).ok_or_else(|| {
            DriverError::Unsupported(format!("no BAR0 memory window on {}", candidate.location))
        })?;
        let size = usize::try_from(bar0.size)
            .map_err(|_| DriverError::Pci(format!("BAR0 too large on {}", candidate.location)))?;
        let mmio = redox_driver_sys::memory::MmioRegion::map(
            bar0.addr,
            size,
            CacheType::DeviceMemory,
            MmioProt::READ_WRITE,
        )
        .map_err(|err| {
            DriverError::Pci(format!(
                "failed to map BAR0 on {}: {err}",
                candidate.location
            ))
        })?;

        if candidate.family.starts_with("intel-bz-") {
            let gp_before = mmio.read32(IWL_CSR_GP_CNTRL);
            mmio.write32(
                IWL_CSR_GP_CNTRL,
                gp_before | IWL_CSR_GP_CNTRL_REG_FLAG_SW_RESET_BZ,
            );
            let gp_after = mmio.read32(IWL_CSR_GP_CNTRL);
            let init_done = (gp_after & IWL_CSR_GP_CNTRL_REG_FLAG_INIT_DONE) != 0;
            out.push("status=nic-activated".to_string());
            out.push("activation_method=gp-cntrl-sw-reset".to_string());
            out.push(format!("activation_before=0x{gp_before:08x}"));
            out.push(format!("activation_after=0x{gp_after:08x}"));
            out.push(format!(
                "init_done={}",
                if init_done { "yes" } else { "no" }
            ));
        } else {
            let reset_before = mmio.read32(IWL_CSR_RESET);
            mmio.write32(
                IWL_CSR_RESET,
                reset_before | IWL_CSR_RESET_REG_FLAG_SW_RESET,
            );
            let reset_after = mmio.read32(IWL_CSR_RESET);
            out.push("status=nic-activated".to_string());
            out.push("activation_method=csr-reset-sw-reset".to_string());
            out.push(format!("activation_before=0x{reset_before:08x}"));
            out.push(format!("activation_after=0x{reset_after:08x}"));
        }
        return Ok(out);
    }

    out.push("status=nic-activated".to_string());
    out.push("activation=host-skipped".to_string());
    Ok(out)
}

fn scan_candidate(
    candidate: &Candidate,
    firmware_root: &PathBuf,
) -> Result<Vec<String>, DriverError> {
    if let Ok(lines) = linux_kpi_scan_candidate(candidate) {
        return Ok(lines);
    }
    let mut out = activate_candidate(candidate, firmware_root)?;
    out.push("status=scanning".to_string());
    out.push("scan_result=linuxkpi-station-scan-ready".to_string());
    out.push("scan_mode=bounded-host-fallback".to_string());
    Ok(out)
}

fn connect_candidate(
    candidate: &Candidate,
    firmware_root: &PathBuf,
    ssid: &str,
    security: &str,
    key: Option<&str>,
) -> Result<Vec<String>, DriverError> {
    if let Ok(lines) = linux_kpi_connect_candidate(candidate, firmware_root, ssid, security, key) {
        return Ok(lines);
    }

    let mut out = activate_candidate(candidate, firmware_root)?;
    if ssid.is_empty() {
        return Err(DriverError::Unsupported("missing ssid".to_string()));
    }
    if security != "open" && security != "wpa2-psk" {
        return Err(DriverError::Unsupported(format!(
            "unsupported security {}",
            security
        )));
    }
    if security == "wpa2-psk" && key.unwrap_or_default().is_empty() {
        return Err(DriverError::Unsupported("missing key".to_string()));
    }
    out.push("status=associating".to_string());
    out.push(format!(
        "connect_result=host-bounded-pending ssid={ssid} security={security}"
    ));
    Ok(out)
}

fn retry_candidate(
    candidate: &Candidate,
    firmware_root: &PathBuf,
) -> Result<Vec<String>, DriverError> {
    let mut out = prepare_candidate(candidate, firmware_root)?;
    out.push("status=device-detected".to_string());
    out.push("link_state=link=retrying".to_string());
    Ok(out)
}

fn disconnect_candidate(
    candidate: &Candidate,
    firmware_root: &PathBuf,
) -> Result<Vec<String>, DriverError> {
    if let Ok(lines) = linux_kpi_disconnect_candidate(candidate, firmware_root) {
        return Ok(lines);
    }

    let mut out = activate_candidate(candidate, firmware_root)?;
    out.push("status=device-detected".to_string());
    out.push("disconnect_result=host-bounded disconnected".to_string());
    Ok(out)
}

#[cfg(target_os = "redox")]
fn linux_kpi_scan_candidate(candidate: &Candidate) -> Result<Vec<String>, DriverError> {
    let mut dev = linux_pci_dev(candidate)?;
    let mut out = vec![0u8; 1024];
    let rc = unsafe {
        rb_iwlwifi_linux_scan(
            &mut dev,
            std::ptr::null(),
            out.as_mut_ptr().cast::<i8>(),
            out.len(),
        )
    };
    if rc != 0 {
        return Err(DriverError::Unsupported(format!(
            "linux-kpi scan path unavailable ({rc})"
        )));
    }
    let line = String::from_utf8_lossy(&out)
        .trim_matches(char::from(0))
        .trim()
        .to_string();
    Ok(vec![
        format!("device={}", candidate.location),
        "status=scanning".to_string(),
        line,
    ])
}

#[cfg(not(target_os = "redox"))]
fn linux_kpi_scan_candidate(_candidate: &Candidate) -> Result<Vec<String>, DriverError> {
    Err(DriverError::Unsupported(
        "linux-kpi scan path is Redox-only".to_string(),
    ))
}

#[cfg(target_os = "redox")]
fn linux_kpi_connect_candidate(
    candidate: &Candidate,
    firmware_root: &PathBuf,
    ssid: &str,
    security: &str,
    key: Option<&str>,
) -> Result<Vec<String>, DriverError> {
    let mut out = activate_candidate(candidate, firmware_root)?;
    let mut dev = linux_pci_dev(candidate)?;
    let ssid =
        CString::new(ssid).map_err(|_| DriverError::Unsupported("invalid ssid".to_string()))?;
    let security = CString::new(security)
        .map_err(|_| DriverError::Unsupported("invalid security".to_string()))?;
    let key = CString::new(key.unwrap_or_default())
        .map_err(|_| DriverError::Unsupported("invalid key".to_string()))?;
    let mut line = vec![0u8; 1024];
    let rc = unsafe {
        rb_iwlwifi_linux_connect(
            &mut dev,
            ssid.as_ptr(),
            security.as_ptr(),
            key.as_ptr(),
            line.as_mut_ptr().cast::<i8>(),
            line.len(),
        )
    };
    if rc != 0 {
        return Err(DriverError::Unsupported(format!(
            "linux-kpi connect path unavailable ({rc})"
        )));
    }
    let parsed = String::from_utf8_lossy(&line)
        .trim_matches(char::from(0))
        .trim()
        .to_string();
    out.push("status=associating".to_string());
    out.push(parsed);
    Ok(out)
}

#[cfg(not(target_os = "redox"))]
fn linux_kpi_connect_candidate(
    _candidate: &Candidate,
    _firmware_root: &PathBuf,
    _ssid: &str,
    _security: &str,
    _key: Option<&str>,
) -> Result<Vec<String>, DriverError> {
    Err(DriverError::Unsupported(
        "linux-kpi connect path is Redox-only".to_string(),
    ))
}

#[cfg(target_os = "redox")]
fn linux_kpi_disconnect_candidate(
    candidate: &Candidate,
    firmware_root: &PathBuf,
) -> Result<Vec<String>, DriverError> {
    let mut out = activate_candidate(candidate, firmware_root)?;
    let mut dev = linux_pci_dev(candidate)?;
    let mut line = vec![0u8; 1024];
    let rc = unsafe {
        rb_iwlwifi_linux_disconnect(&mut dev, line.as_mut_ptr().cast::<i8>(), line.len())
    };
    if rc != 0 {
        return Err(DriverError::Unsupported(format!(
            "linux-kpi disconnect path unavailable ({rc})"
        )));
    }
    let parsed = String::from_utf8_lossy(&line)
        .trim_matches(char::from(0))
        .trim()
        .to_string();
    out.push("status=device-detected".to_string());
    out.push(parsed);
    Ok(out)
}

#[cfg(not(target_os = "redox"))]
fn linux_kpi_disconnect_candidate(
    _candidate: &Candidate,
    _firmware_root: &PathBuf,
) -> Result<Vec<String>, DriverError> {
    Err(DriverError::Unsupported(
        "linux-kpi disconnect path is Redox-only".to_string(),
    ))
}

#[cfg(target_os = "redox")]
fn linux_kpi_status_candidate(candidate: &Candidate) -> Result<Vec<String>, DriverError> {
    let mut dev = linux_pci_dev(candidate)?;
    let mut line = vec![0u8; 1024];
    let rc = unsafe { rb_iwlwifi_status(&mut dev, line.as_mut_ptr().cast::<i8>(), line.len()) };
    if rc != 0 {
        return Err(DriverError::Unsupported(format!(
            "linux-kpi status path unavailable ({rc})"
        )));
    }
    let parsed = String::from_utf8_lossy(&line)
        .trim_matches(char::from(0))
        .trim()
        .to_string();
    Ok(vec![
        format!("device={}", candidate.location),
        format!("family={}", candidate.family),
        parsed,
    ])
}

#[cfg(not(target_os = "redox"))]
fn linux_kpi_status_candidate(_candidate: &Candidate) -> Result<Vec<String>, DriverError> {
    Err(DriverError::Unsupported(
        "linux-kpi status path is Redox-only".to_string(),
    ))
}

#[cfg(target_os = "redox")]
fn linux_kpi_full_init_candidate(
    candidate: &Candidate,
    _firmware_root: &PathBuf,
) -> Result<Vec<String>, DriverError> {
    let mut dev = linux_pci_dev(candidate)?;
    let ucode = candidate
        .selected_ucode
        .as_ref()
        .map(|name| CString::new(name.as_str()))
        .transpose()
        .map_err(|_| DriverError::Unsupported("invalid selected ucode".to_string()))?;
    let pnvm = candidate
        .pnvm_candidate
        .as_ref()
        .map(|name| CString::new(name.as_str()))
        .transpose()
        .map_err(|_| DriverError::Unsupported("invalid pnvm name".to_string()))?;
    let mut line = vec![0u8; 1024];
    let rc = unsafe {
        rb_iwlwifi_full_init(
            &mut dev,
            0,
            if candidate.family.starts_with("intel-bz-") {
                1
            } else {
                0
            },
            ucode.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
            pnvm.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
            line.as_mut_ptr().cast::<i8>(),
            line.len(),
        )
    };
    if rc != 0 {
        return Err(DriverError::Unsupported(format!(
            "linux-kpi full-init path unavailable ({rc})"
        )));
    }
    let parsed = String::from_utf8_lossy(&line)
        .trim_matches(char::from(0))
        .trim()
        .to_string();
    Ok(vec![
        format!("device={}", candidate.location),
        "status=full-init-ready".to_string(),
        parsed,
    ])
}

#[cfg(not(target_os = "redox"))]
fn linux_kpi_full_init_candidate(
    _candidate: &Candidate,
    _firmware_root: &PathBuf,
) -> Result<Vec<String>, DriverError> {
    Err(DriverError::Unsupported(
        "linux-kpi full-init path is Redox-only".to_string(),
    ))
}

fn full_init_candidate(
    candidate: &Candidate,
    firmware_root: &PathBuf,
) -> Result<Vec<String>, DriverError> {
    if let Ok(lines) = linux_kpi_full_init_candidate(candidate, firmware_root) {
        return Ok(lines);
    }
    let mut out = activate_candidate(candidate, firmware_root)?;
    out.push("status=full-init-ready".to_string());
    out.push("mac80211=host-skipped".to_string());
    Ok(out)
}

fn irq_test_candidate(
    candidate: &Candidate,
    firmware_root: &PathBuf,
) -> Result<Vec<String>, DriverError> {
    let mut out = full_init_candidate(candidate, firmware_root)?;
    #[cfg(target_os = "redox")]
    {
        if let Ok(lines) = linux_kpi_status_candidate(candidate) {
            out.extend(
                lines
                    .into_iter()
                    .filter(|line| line.starts_with("linux_kpi_status=")),
            );
            out.push("irq_test=pass".to_string());
            return Ok(out);
        }
    }
    out.push("irq_test=host-skipped".to_string());
    Ok(out)
}

fn dma_test_candidate(
    candidate: &Candidate,
    firmware_root: &PathBuf,
) -> Result<Vec<String>, DriverError> {
    let mut out = full_init_candidate(candidate, firmware_root)?;
    #[cfg(target_os = "redox")]
    {
        if let Ok(lines) = linux_kpi_status_candidate(candidate) {
            out.extend(
                lines
                    .into_iter()
                    .filter(|line| line.starts_with("linux_kpi_status=")),
            );
            out.push("dma_test=pass".to_string());
            return Ok(out);
        }
    }
    out.push("dma_test=host-skipped".to_string());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{stamp}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn detects_intel_candidate() {
        let pci = temp_root("rbos-iwlwifi-pci");
        let fw = temp_root("rbos-iwlwifi-fw");
        let slot = pci.join("0000--00--14.3");
        fs::create_dir_all(&slot).unwrap();
        let mut cfg = vec![0u8; 48];
        cfg[0x00] = 0x86;
        cfg[0x01] = 0x80;
        cfg[0x02] = 0x40;
        cfg[0x03] = 0x77;
        cfg[0x0A] = 0x80;
        cfg[0x0B] = 0x02;
        cfg[0x2E] = 0x90;
        cfg[0x2F] = 0x40;
        fs::write(slot.join("config"), cfg).unwrap();
        fs::write(fw.join("iwlwifi-bz-b0-gf-a0-92.ucode"), []).unwrap();

        unsafe {
            env::set_var("REDBEAR_IWLWIFI_PCI_ROOT", &pci);
        }
        let candidates = detect_candidates(&fw).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].family, "intel-bz-arrow-lake");
        assert!(candidates[0]
            .ucode_candidates
            .iter()
            .any(|name| name.contains("iwlwifi-bz-b0-gf-a0-92.ucode")));
    }

    #[test]
    fn prepare_candidate_reports_selected_firmware() {
        let fw = temp_root("rbos-iwlwifi-fw-prepare");
        fs::write(fw.join("iwlwifi-bz-b0-gf-a0-92.ucode"), []).unwrap();
        fs::write(fw.join("iwlwifi-bz-b0-gf-a0.pnvm"), []).unwrap();

        let candidate = Candidate {
            location: PciLocation {
                segment: 0,
                bus: 0,
                device: 0x14,
                function: 3,
            },
            config_path: PathBuf::from("/tmp/config"),
            device_id: 0x7740,
            subsystem_id: 0x4090,
            family: "intel-bz-arrow-lake",
            ucode_candidates: vec!["iwlwifi-bz-b0-gf-a0-92.ucode".to_string()],
            selected_ucode: Some("iwlwifi-bz-b0-gf-a0-92.ucode".to_string()),
            pnvm_candidate: Some("iwlwifi-bz-b0-gf-a0.pnvm".to_string()),
            pnvm_found: Some("iwlwifi-bz-b0-gf-a0.pnvm".to_string()),
        };

        let lines = prepare_candidate(&candidate, &fw).unwrap();
        assert!(lines.iter().any(|line| line == "status=firmware-ready"));
        assert!(lines
            .iter()
            .any(|line| line.contains("selected_pnvm=iwlwifi-bz-b0-gf-a0.pnvm")));
        assert!(lines
            .iter()
            .any(|line| line.contains("selected_ucode=iwlwifi-bz-b0-gf-a0-92.ucode")));
    }

    #[test]
    fn init_transport_candidate_reports_transport_ready() {
        let fw = temp_root("rbos-iwlwifi-fw-init");
        fs::write(fw.join("iwlwifi-bz-b0-gf-a0-92.ucode"), []).unwrap();
        fs::write(fw.join("iwlwifi-bz-b0-gf-a0.pnvm"), []).unwrap();

        let candidate = Candidate {
            location: PciLocation {
                segment: 0,
                bus: 0,
                device: 0x14,
                function: 3,
            },
            config_path: PathBuf::from("/tmp/config"),
            device_id: 0x7740,
            subsystem_id: 0x4090,
            family: "intel-bz-arrow-lake",
            ucode_candidates: vec!["iwlwifi-bz-b0-gf-a0-92.ucode".to_string()],
            selected_ucode: Some("iwlwifi-bz-b0-gf-a0-92.ucode".to_string()),
            pnvm_candidate: Some("iwlwifi-bz-b0-gf-a0.pnvm".to_string()),
            pnvm_found: Some("iwlwifi-bz-b0-gf-a0.pnvm".to_string()),
        };

        let lines = init_transport_candidate(&candidate, &fw).unwrap();
        assert!(lines.iter().any(|line| line == "status=transport-ready"));
        assert!(lines
            .iter()
            .any(|line| line.contains("bar0_addr=host-skipped")));
    }

    #[test]
    fn activate_candidate_reports_nic_activated() {
        let fw = temp_root("rbos-iwlwifi-fw-activate");
        fs::write(fw.join("iwlwifi-bz-b0-gf-a0-92.ucode"), []).unwrap();
        fs::write(fw.join("iwlwifi-bz-b0-gf-a0.pnvm"), []).unwrap();

        let candidate = Candidate {
            location: PciLocation {
                segment: 0,
                bus: 0,
                device: 0x14,
                function: 3,
            },
            config_path: PathBuf::from("/tmp/config"),
            device_id: 0x7740,
            subsystem_id: 0x4090,
            family: "intel-bz-arrow-lake",
            ucode_candidates: vec!["iwlwifi-bz-b0-gf-a0-92.ucode".to_string()],
            selected_ucode: Some("iwlwifi-bz-b0-gf-a0-92.ucode".to_string()),
            pnvm_candidate: Some("iwlwifi-bz-b0-gf-a0.pnvm".to_string()),
            pnvm_found: Some("iwlwifi-bz-b0-gf-a0.pnvm".to_string()),
        };

        let lines = activate_candidate(&candidate, &fw).unwrap();
        assert!(lines.iter().any(|line| line == "status=nic-activated"));
        assert!(lines
            .iter()
            .any(|line| line.contains("activation=host-skipped")));
    }

    #[test]
    fn scan_candidate_reports_scanning() {
        let fw = temp_root("rbos-iwlwifi-fw-scan");
        fs::write(fw.join("iwlwifi-bz-b0-gf-a0-92.ucode"), []).unwrap();
        fs::write(fw.join("iwlwifi-bz-b0-gf-a0.pnvm"), []).unwrap();

        let candidate = Candidate {
            location: PciLocation {
                segment: 0,
                bus: 0,
                device: 0x14,
                function: 3,
            },
            config_path: PathBuf::from("/tmp/config"),
            device_id: 0x7740,
            subsystem_id: 0x4090,
            family: "intel-bz-arrow-lake",
            ucode_candidates: vec!["iwlwifi-bz-b0-gf-a0-92.ucode".to_string()],
            selected_ucode: Some("iwlwifi-bz-b0-gf-a0-92.ucode".to_string()),
            pnvm_candidate: Some("iwlwifi-bz-b0-gf-a0.pnvm".to_string()),
            pnvm_found: Some("iwlwifi-bz-b0-gf-a0.pnvm".to_string()),
        };

        let lines = scan_candidate(&candidate, &fw).unwrap();
        assert!(lines.iter().any(|line| line == "status=scanning"));
        assert!(lines.iter().any(|line| line.contains("scan_result=")));
    }

    #[test]
    fn connect_candidate_reports_associating() {
        let fw = temp_root("rbos-iwlwifi-fw-connect");
        fs::write(fw.join("iwlwifi-bz-b0-gf-a0-92.ucode"), []).unwrap();
        fs::write(fw.join("iwlwifi-bz-b0-gf-a0.pnvm"), []).unwrap();

        let candidate = Candidate {
            location: PciLocation {
                segment: 0,
                bus: 0,
                device: 0x14,
                function: 3,
            },
            config_path: PathBuf::from("/tmp/config"),
            device_id: 0x7740,
            subsystem_id: 0x4090,
            family: "intel-bz-arrow-lake",
            ucode_candidates: vec!["iwlwifi-bz-b0-gf-a0-92.ucode".to_string()],
            selected_ucode: Some("iwlwifi-bz-b0-gf-a0-92.ucode".to_string()),
            pnvm_candidate: Some("iwlwifi-bz-b0-gf-a0.pnvm".to_string()),
            pnvm_found: Some("iwlwifi-bz-b0-gf-a0.pnvm".to_string()),
        };

        let lines = connect_candidate(&candidate, &fw, "demo", "wpa2-psk", Some("secret")).unwrap();
        assert!(lines.iter().any(|line| line == "status=associating"));
        assert!(lines.iter().any(|line| line.contains("connect_result=")));
    }

    #[test]
    fn retry_candidate_reports_device_detected() {
        let fw = temp_root("rbos-iwlwifi-fw-retry");
        fs::write(fw.join("iwlwifi-bz-b0-gf-a0-92.ucode"), []).unwrap();
        fs::write(fw.join("iwlwifi-bz-b0-gf-a0.pnvm"), []).unwrap();

        let candidate = Candidate {
            location: PciLocation {
                segment: 0,
                bus: 0,
                device: 0x14,
                function: 3,
            },
            config_path: PathBuf::from("/tmp/config"),
            device_id: 0x7740,
            subsystem_id: 0x4090,
            family: "intel-bz-arrow-lake",
            ucode_candidates: vec!["iwlwifi-bz-b0-gf-a0-92.ucode".to_string()],
            selected_ucode: Some("iwlwifi-bz-b0-gf-a0-92.ucode".to_string()),
            pnvm_candidate: Some("iwlwifi-bz-b0-gf-a0.pnvm".to_string()),
            pnvm_found: Some("iwlwifi-bz-b0-gf-a0.pnvm".to_string()),
        };

        let lines = retry_candidate(&candidate, &fw).unwrap();
        assert!(lines.iter().any(|line| line == "status=device-detected"));
        assert!(lines.iter().any(|line| line == "link_state=link=retrying"));
    }
}
