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
use redox_scheme::{SignalBehavior, Socket};

use crate::driver::{DriverError, GpuDriver, Result};
use crate::drivers::DriverRegistry;
use crate::scheme::DrmScheme;

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

    let (vblank_tx, vblank_rx) = mpsc::sync_channel::<(u32, u64)>(8);

    let irq_driver: Arc<dyn GpuDriver> = driver.clone();
    std::thread::spawn(move || loop {
        match irq_driver.handle_irq() {
            Ok(Some((crtc_id, count))) => {
                let _ = vblank_tx.try_send((crtc_id, count));
            }
            Ok(None) => {}
            Err(e) => {
                error!("redox-drm: IRQ handler error: {}", e);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(16));
    });

    let drm_scheme = Arc::new(Mutex::new(DrmScheme::new(driver)));
    let vblank_scheme = drm_scheme.clone();

    std::thread::spawn(move || loop {
        if let Ok((crtc_id, vblank_count)) = vblank_rx.recv() {
            if let Ok(mut scheme) = vblank_scheme.lock() {
                scheme.retire_vblank(crtc_id, vblank_count);
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
            Err(_request) => {
                error!("redox-drm: failed to handle request");
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

impl FirmwareCache {
    fn load_for_device(info: &PciDeviceInfo) -> Result<Self> {
        if info.vendor_id != PCI_VENDOR_ID_AMD {
            info!(
                "redox-drm: skipping firmware load for Intel GPU {}",
                info.location
            );
            return Ok(Self {
                blobs: HashMap::new(),
            });
        }

        let firmware_keys: &[&str] = if info.vendor_id == PCI_VENDOR_ID_AMD {
            &[
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
            ]
        } else {
            &[]
        };

        let mut blobs = HashMap::new();
        let mut loaded_any = false;

        for &key in firmware_keys {
            let path = format!("/scheme/firmware/{}", key);
            match File::open(&path) {
                Ok(mut file) => {
                    let metadata = file.metadata();
                    let estimated_size = metadata.map(|m| m.len() as usize).unwrap_or(1024 * 1024);
                    let mut buf = Vec::with_capacity(estimated_size);
                    match file.read_to_end(&mut buf) {
                        Ok(bytes_read) => {
                            info!("redox-drm: loaded firmware {} ({} bytes)", key, bytes_read);
                            loaded_any = true;
                            blobs.insert(key.to_string(), buf);
                        }
                        Err(e) => {
                            info!("redox-drm: failed to read firmware {}: {}", key, e);
                        }
                    }
                }
                Err(e) => {
                    info!("redox-drm: firmware {} not available: {}", key, e);
                }
            }
        }

        if !loaded_any && info.vendor_id == PCI_VENDOR_ID_AMD {
            return Err(DriverError::NotFound(
                "no AMD firmware blobs available from scheme:firmware".to_string(),
            ));
        }

        info!(
            "redox-drm: firmware cache populated with {} blob(s)",
            blobs.len()
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
