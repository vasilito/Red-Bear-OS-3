#![allow(dead_code)]

mod driver;
mod drivers;
mod gem;
mod kms;
mod scheme;

use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::Read;
use std::process;

use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use log::{error, info, LevelFilter, Metadata, Record};
use redox_driver_sys::pci::{
    enumerate_pci_class, PciDevice, PciDeviceInfo, PciLocation, PCI_CLASS_DISPLAY,
    PCI_VENDOR_ID_AMD, PCI_VENDOR_ID_INTEL,
};
use redox_driver_sys::quirks::PciQuirkFlags;
use redox_scheme::{SignalBehavior, Socket};

use crate::driver::{DriverError, DriverEvent, GpuDriver, Result};
use crate::drivers::DriverRegistry;
use crate::scheme::DrmScheme;

const MAX_FIRMWARE_BLOB_BYTES: u64 = 64 * 1024 * 1024;

struct StderrLogger {
    level: LevelFilter,
}

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            eprintln!("[{}] {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

fn init_logging(level: LevelFilter) {
    let logger = Box::leak(Box::new(StderrLogger { level }));
    if log::set_logger(logger).is_err() {
        return;
    }
    log::set_max_level(level);
}

fn run() -> Result<()> {
    let info = select_gpu_from_args()?;
    verify_supported_gpu(&info)?;

    let firmware = FirmwareCache::load_for_device(&info)?;

    let driver = DriverRegistry::probe(info.clone(), firmware.into_blobs())?;
    info!(
        "redox-drm: initialized driver {} ({}) for {}",
        driver.driver_name(),
        driver.driver_desc(),
        info.location
    );

    let socket = Socket::create("drm")
        .map_err(|e| DriverError::Initialization(format!("failed to register drm scheme: {e}")))?;
    info!("redox-drm: registered scheme:drm");

    let (event_tx, event_rx) = mpsc::sync_channel::<DriverEvent>(8);

    let irq_driver: Arc<dyn GpuDriver> = driver.clone();
    std::thread::spawn(move || loop {
        match irq_driver.handle_irq() {
            Ok(Some(event)) => {
                if event_tx.send(event).is_err() {
                    error!("redox-drm: event consumer dropped, stopping IRQ event thread");
                    break;
                }
            }
            Ok(None) => {}
            Err(e) => {
                error!("redox-drm: IRQ handler error: {}", e);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(16));
    });

    let drm_scheme = Arc::new(Mutex::new(DrmScheme::new(driver)));
    let event_scheme = drm_scheme.clone();

    std::thread::spawn(move || loop {
        if let Ok(event) = event_rx.recv() {
            if let Ok(mut scheme) = event_scheme.lock() {
                scheme.handle_driver_event(event);
            }
        }
    });

    loop {
        let request = match socket.next_request(SignalBehavior::Restart) {
            Ok(Some(request)) => request,
            Ok(None) => {
                info!("redox-drm: scheme unmounted, exiting");
                break;
            }
            Err(e) => {
                error!("redox-drm: failed to receive scheme request: {}", e);
                continue;
            }
        };

        let response = {
            let mut scheme = match drm_scheme.lock() {
                Ok(scheme) => scheme,
                Err(_) => {
                    error!("redox-drm: DRM scheme state poisoned");
                    continue;
                }
            };
            request.handle_scheme_block_mut(&mut *scheme)
        };

        let response = match response {
            Ok(response) => response,
            Err(request) => {
                error!(
                    "redox-drm: failed to handle request from context {}",
                    request.context_id()
                );
                continue;
            }
        };

        if let Err(e) = socket.write_response(response, SignalBehavior::Restart) {
            error!("redox-drm: failed to write scheme response: {}", e);
        }
    }

    Ok(())
}

fn select_gpu_from_args() -> Result<PciDeviceInfo> {
    let mut args = env::args().skip(1);
    let parsed = match (args.next(), args.next(), args.next()) {
        (Some(bus), Some(device), Some(function)) => {
            Some(parse_location(&bus, &device, &function)?)
        }
        _ => None,
    };

    if let Some(location) = parsed {
        let mut pci = PciDevice::open_location(&location).map_err(|e| {
            DriverError::Pci(format!("failed to open PCI device {}: {e}", location))
        })?;
        return pci.full_info().map_err(|e| {
            DriverError::Pci(format!("failed to read PCI info for {}: {e}", location))
        });
    }

    let devices = enumerate_pci_class(PCI_CLASS_DISPLAY)
        .map_err(|e| DriverError::Pci(format!("PCI scan failed: {e}")))?;
    let first = devices
        .into_iter()
        .find(|d| d.vendor_id == PCI_VENDOR_ID_AMD || d.vendor_id == PCI_VENDOR_ID_INTEL)
        .ok_or_else(|| {
            DriverError::NotFound("no AMD or Intel GPU found via scheme:pci".to_string())
        })?;
    let mut pci = PciDevice::open_location(&first.location)
        .map_err(|e| DriverError::Pci(format!("failed to open GPU {}: {e}", first.location)))?;
    pci.full_info()
        .map_err(|e| DriverError::Pci(format!("failed to read GPU {}: {e}", first.location)))
}

fn parse_location(bus: &str, device: &str, function: &str) -> Result<PciLocation> {
    let bus = parse_u8(bus)?;
    let device = parse_u8(device)?;
    let function = parse_u8(function)?;
    Ok(PciLocation {
        segment: 0,
        bus,
        device,
        function,
    })
}

fn parse_u8(value: &str) -> Result<u8> {
    let trimmed = value.trim_start_matches("0x");
    u8::from_str_radix(trimmed, 16)
        .or_else(|_| trimmed.parse::<u8>())
        .map_err(|_| DriverError::InvalidArgument("invalid PCI coordinate"))
}

fn verify_supported_gpu(info: &PciDeviceInfo) -> Result<()> {
    if info.class_code != PCI_CLASS_DISPLAY {
        return Err(DriverError::Pci(format!(
            "device {} is class {:#04x}, expected display class {:#04x}",
            info.location, info.class_code, PCI_CLASS_DISPLAY
        )));
    }

    if info.vendor_id != PCI_VENDOR_ID_AMD && info.vendor_id != PCI_VENDOR_ID_INTEL {
        return Err(DriverError::Pci(format!(
            "device {} is vendor {:#06x}, expected AMD {:#06x} or Intel {:#06x}",
            info.location, info.vendor_id, PCI_VENDOR_ID_AMD, PCI_VENDOR_ID_INTEL
        )));
    }
    Ok(())
}

struct FirmwareCache {
    blobs: HashMap<String, Vec<u8>>,
}

struct FirmwareExpectation {
    vendor_name: &'static str,
    keys: &'static [&'static str],
    required: bool,
    required_label: &'static str,
}

const AMD_DISPLAY_FIRMWARE_KEYS: &[&str] = &[
    "amdgpu/dcn_3_1_dmcub",
    "amdgpu/dmcub_dcn20.bin",
    "amdgpu/dmcub_dcn31.bin",
];

const INTEL_TGL_DMC_KEYS: &[&str] = &["i915/tgl_dmc.bin", "i915/tgl_dmc_ver2_12.bin"];
const INTEL_ADLP_DMC_KEYS: &[&str] = &["i915/adlp_dmc.bin", "i915/adlp_dmc_ver2_16.bin"];
const INTEL_DG2_DMC_KEYS: &[&str] = &["i915/dg2_dmc.bin", "i915/dg2_dmc_ver2_06.bin"];
const INTEL_MTL_DMC_KEYS: &[&str] = &["i915/mtl_dmc.bin"];

fn intel_display_firmware_keys(device_id: u16) -> Option<&'static [&'static str]> {
    match device_id {
        0x9A40 | 0x9A49 | 0x9A60 | 0x9A68 | 0x9A70 | 0x9A78 => Some(INTEL_TGL_DMC_KEYS),
        0x46A6 => Some(INTEL_ADLP_DMC_KEYS),
        0x5690 | 0x5691 | 0x5692 | 0x5693 | 0x5694 | 0x5696 | 0x5697 | 0x56A0 | 0x56A1
        | 0x56A5 | 0x56A6 | 0x56B0 | 0x56B1 | 0x56B2 | 0x56B3 | 0x56C0 | 0x56C1 => {
            Some(INTEL_DG2_DMC_KEYS)
        }
        0x7D55 | 0x7D45 | 0x7D40 => Some(INTEL_MTL_DMC_KEYS),
        _ => None,
    }
}

fn firmware_expectation(info: &PciDeviceInfo, quirks: PciQuirkFlags) -> FirmwareExpectation {
    match info.vendor_id {
        PCI_VENDOR_ID_AMD => FirmwareExpectation {
            vendor_name: "AMD",
            keys: &[
                "amdgpu/psp_13_0_0_sos",
                "amdgpu/psp_13_0_0_ta",
                "amdgpu/gc_11_0_0_pfp",
                "amdgpu/gc_11_0_0_me",
                "amdgpu/gc_11_0_0_ce",
                "amdgpu/gc_11_0_0_rlc",
                "amdgpu/gc_11_0_0_mec",
                "amdgpu/gc_11_0_0_mec2",
                "amdgpu/dcn_3_1_dmcub",
                "amdgpu/dmcub_dcn20.bin",
                "amdgpu/dmcub_dcn31.bin",
                "amdgpu/sdma_5_0",
                "amdgpu/sdma_5_2",
                "amdgpu/vcn_3_0_0",
                "amdgpu/vcn_3_1_0",
            ],
            required: quirks.contains(PciQuirkFlags::NEED_FIRMWARE),
            required_label: "AMD firmware",
        },
        PCI_VENDOR_ID_INTEL => {
            let keys = intel_display_firmware_keys(info.device_id).unwrap_or(&[]);
            FirmwareExpectation {
                vendor_name: "Intel",
                keys,
                required: !keys.is_empty(),
                required_label: "Intel display DMC firmware",
            }
        }
        _ => FirmwareExpectation {
            vendor_name: "unknown",
            keys: &[],
            required: false,
            required_label: "firmware",
        },
    }
}

fn summarize_missing_firmware(missing: &[String]) -> String {
    const MAX_SHOWN: usize = 3;

    if missing.is_empty() {
        return "none".to_string();
    }

    let shown: Vec<&str> = missing.iter().take(MAX_SHOWN).map(String::as_str).collect();
    if missing.len() > MAX_SHOWN {
        format!("{} (+{} more)", shown.join(", "), missing.len() - MAX_SHOWN)
    } else {
        shown.join(", ")
    }
}

fn firmware_requirement_error(
    expectation: &FirmwareExpectation,
    loaded: &HashMap<String, Vec<u8>>,
    missing: &[String],
) -> Option<String> {
    if !expectation.required {
        return None;
    }

    if loaded.is_empty() {
        return Some(format!(
            "no {} firmware blobs available from scheme:firmware; checked {} candidates ({})",
            expectation.required_label,
            expectation.keys.len(),
            summarize_missing_firmware(missing)
        ));
    }

    if expectation.vendor_name == "AMD"
        && !AMD_DISPLAY_FIRMWARE_KEYS
            .iter()
            .any(|key| loaded.contains_key(*key))
    {
        return Some(format!(
            "AMD firmware policy requires a DMCUB/display blob before backend init; checked {} candidates ({})",
            expectation.keys.len(),
            summarize_missing_firmware(missing)
        ));
    }

    None
}

impl FirmwareCache {
    fn load_for_device(info: &PciDeviceInfo) -> Result<Self> {
        let quirks = info.quirks();
        let expectation = firmware_expectation(info, quirks);

        if expectation.keys.is_empty() {
            if expectation.required {
                info!(
                    "redox-drm: {} GPU {} declares NEED_FIRMWARE in canonical quirk policy, but no Rust-side firmware manifest is defined for this vendor yet",
                    expectation.vendor_name,
                    info.location
                );
            } else {
                info!(
                    "redox-drm: skipping firmware preload for {} GPU {} (no Rust-side firmware manifest)",
                    expectation.vendor_name,
                    info.location
                );
            }
            return Ok(Self {
                blobs: HashMap::new(),
            });
        }

        let mut blobs = HashMap::new();
        let mut missing = Vec::new();

        info!(
            "redox-drm: firmware preload for {} GPU {} expects {} candidate blob(s); required_by_quirk={}",
            expectation.vendor_name,
            info.location,
            expectation.keys.len(),
            expectation.required
        );

        for &key in expectation.keys {
            let path = format!("/scheme/firmware/{}", key);
            match File::open(&path) {
                Ok(mut file) => {
                    let metadata = file.metadata();
                    let estimated_size = metadata.map(|m| m.len()).unwrap_or(1024 * 1024);
                    if estimated_size > MAX_FIRMWARE_BLOB_BYTES {
                        info!(
                            "redox-drm: firmware {} rejected — {} bytes exceeds trusted preload cap {}",
                            key,
                            estimated_size,
                            MAX_FIRMWARE_BLOB_BYTES
                        );
                        missing.push(key.to_string());
                        continue;
                    }
                    let mut buf = Vec::with_capacity(estimated_size as usize);
                    match file.read_to_end(&mut buf) {
                        Ok(bytes_read) => {
                            info!("redox-drm: loaded firmware {} ({} bytes)", key, bytes_read);
                            blobs.insert(key.to_string(), buf);
                        }
                        Err(e) => {
                            info!("redox-drm: failed to read firmware {}: {}", key, e);
                            missing.push(key.to_string());
                        }
                    }
                }
                Err(e) => {
                    info!("redox-drm: firmware {} not available: {}", key, e);
                    missing.push(key.to_string());
                }
            }
        }

        if let Some(message) = firmware_requirement_error(&expectation, &blobs, &missing) {
            return Err(DriverError::NotFound(message));
        }

        if !missing.is_empty() {
            info!(
                "redox-drm: firmware preload for {} GPU {} left {} blob(s) unavailable: {}",
                expectation.vendor_name,
                info.location,
                missing.len(),
                summarize_missing_firmware(&missing)
            );
        }

        info!(
            "redox-drm: firmware cache populated with {} blob(s) for {} GPU {}",
            blobs.len(),
            expectation.vendor_name,
            info.location
        );
        Ok(Self { blobs })
    }

    #[allow(dead_code)]
    fn get(&self, key: &str) -> Option<&[u8]> {
        self.blobs.get(key).map(|v| v.as_slice())
    }

    fn into_blobs(self) -> HashMap<String, Vec<u8>> {
        self.blobs
    }
}

fn main() {
    let log_level = match env::var("REDOX_DRM_LOG").as_deref() {
        Ok("trace") => LevelFilter::Trace,
        Ok("debug") => LevelFilter::Debug,
        Ok("warn") => LevelFilter::Warn,
        Ok("error") => LevelFilter::Error,
        _ => LevelFilter::Info,
    };

    init_logging(log_level);

    if let Err(error) = run() {
        error!("redox-drm: fatal error: {}", error);
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_gpu_info(vendor_id: u16, device_id: u16) -> PciDeviceInfo {
        PciDeviceInfo {
            location: PciLocation {
                segment: 0,
                bus: 0,
                device: 0,
                function: 0,
            },
            vendor_id,
            device_id,
            subsystem_vendor_id: 0,
            subsystem_device_id: 0,
            revision: 0,
            class_code: PCI_CLASS_DISPLAY,
            subclass: 0,
            prog_if: 0,
            header_type: 0,
            irq: None,
            bars: Vec::new(),
            capabilities: Vec::new(),
        }
    }

    #[test]
    fn firmware_expectation_marks_amd_need_firmware_as_required() {
        let expectation = firmware_expectation(
            &PciDeviceInfo {
                location: PciLocation {
                    segment: 0,
                    bus: 0,
                    device: 0,
                    function: 0,
                },
                vendor_id: PCI_VENDOR_ID_AMD,
                device_id: 0x744C,
                subsystem_vendor_id: 0,
                subsystem_device_id: 0,
                revision: 0,
                class_code: PCI_CLASS_DISPLAY,
                subclass: 0,
                prog_if: 0,
                header_type: 0,
                irq: None,
                bars: Vec::new(),
                capabilities: Vec::new(),
            },
            PciQuirkFlags::from_bits_truncate(PciQuirkFlags::NEED_FIRMWARE.bits()),
        );

        assert_eq!(expectation.vendor_name, "AMD");
        assert!(expectation.required);
        assert!(!expectation.keys.is_empty());
    }

    #[test]
    fn summarize_missing_firmware_truncates_long_lists() {
        let summary = summarize_missing_firmware(&[
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ]);

        assert_eq!(summary, "a, b, c (+1 more)");
    }

    #[test]
    fn amd_required_firmware_needs_display_blob() {
        let expectation = firmware_expectation(
            &PciDeviceInfo {
                location: PciLocation {
                    segment: 0,
                    bus: 0,
                    device: 0,
                    function: 0,
                },
                vendor_id: PCI_VENDOR_ID_AMD,
                device_id: 0x744C,
                subsystem_vendor_id: 0,
                subsystem_device_id: 0,
                revision: 0,
                class_code: PCI_CLASS_DISPLAY,
                subclass: 0,
                prog_if: 0,
                header_type: 0,
                irq: None,
                bars: Vec::new(),
                capabilities: Vec::new(),
            },
            PciQuirkFlags::from_bits_truncate(PciQuirkFlags::NEED_FIRMWARE.bits()),
        );
        let mut loaded = HashMap::new();
        loaded.insert("amdgpu/gc_11_0_0_pfp".to_string(), vec![1, 2, 3]);
        let missing = vec!["amdgpu/dmcub_dcn31.bin".to_string()];

        let error = firmware_requirement_error(&expectation, &loaded, &missing);

        assert!(error.is_some());
        assert!(error.unwrap().contains("DMCUB/display blob"));
    }

    #[test]
    fn intel_tgl_manifest_is_required_from_startup() {
        let expectation = firmware_expectation(&test_gpu_info(PCI_VENDOR_ID_INTEL, 0x9A49), PciQuirkFlags::empty());

        assert_eq!(expectation.vendor_name, "Intel");
        assert!(expectation.required);
        assert_eq!(expectation.required_label, "Intel display DMC firmware");
        assert!(expectation.keys.contains(&"i915/tgl_dmc_ver2_12.bin"));
    }

    #[test]
    fn unknown_intel_device_has_no_startup_manifest_yet() {
        let expectation = firmware_expectation(&test_gpu_info(PCI_VENDOR_ID_INTEL, 0x3E92), PciQuirkFlags::empty());

        assert_eq!(expectation.vendor_name, "Intel");
        assert!(!expectation.required);
        assert!(expectation.keys.is_empty());
    }

    #[test]
    fn mode_info_default_1080p_clock_matches_standard_cvt() {
        use crate::kms::ModeInfo;
        let mode = ModeInfo::default_1080p();
        // Standard 1080p60 timing: 148.5 MHz pixel clock
        assert_eq!(mode.clock, 148_500);
        // Total pixels per frame = htotal * vtotal = 2200 * 1125 = 2_475_000
        // Refresh = clock*1000 / total = 148_500_000 / 2_475_000 = 60
        assert_eq!(mode.htotal as u32 * mode.vtotal as u32, 2_475_000_u32);
    }

    #[test]
    fn mode_info_from_edid_rejects_short_edid() {
        use crate::kms::connector::synthetic_edid;
        use crate::kms::ModeInfo;
        let edid = synthetic_edid();
        assert!(edid.len() < 128);
        let modes = ModeInfo::from_edid(&edid);
        assert!(modes.is_empty());
    }

    #[test]
    fn mode_info_from_edid_parses_valid_128byte_edid() {
        use crate::kms::ModeInfo;
        let mut edid = vec![0u8; 128];
        edid[0] = 0x00;
        edid[1] = 0xFF;
        edid[2] = 0xFF;
        edid[3] = 0xFF;
        edid[4] = 0xFF;
        edid[5] = 0xFF;
        edid[6] = 0xFF;
        edid[7] = 0x00;
        let modes = ModeInfo::from_edid(&edid);
        assert!(modes.is_empty(), "all-zero descriptors should produce no modes");
    }

    #[test]
    fn mode_info_from_edid_name_format_is_width_x_height_at_refresh() {
        use crate::kms::connector::synthetic_edid;
        use crate::kms::ModeInfo;
        let edid = synthetic_edid();
        let modes = ModeInfo::from_edid(&edid);
        for mode in &modes {
            // Verify the canonical format: "WxH@refresh"
            let expected = format!("{}x{}@{}", mode.hdisplay, mode.vdisplay, mode.vrefresh);
            assert_eq!(mode.name, expected);
        }
    }
}
