//! IOMMU daemon — provides scheme:iommu for DMA remapping.

use std::env;
use std::fs;
use std::process;

use iommu::amd_vi::AmdViUnit;
#[cfg(target_os = "redox")]
use iommu::IommuScheme;
use log::{error, info, LevelFilter, Metadata, Record};
#[cfg(target_os = "redox")]
use redox_scheme::{SignalBehavior, Socket};

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
    if log::set_boxed_logger(Box::new(StderrLogger { level })).is_err() {
        return;
    }

    log::set_max_level(level);
}

fn detect_units_from_env() -> Result<Vec<AmdViUnit>, String> {
    let Some(path) = env::var_os("IOMMU_IVRS_PATH") else {
        return Ok(Vec::new());
    };

    let bytes = fs::read(&path).map_err(|err| {
        format!(
            "failed to read IVRS table from {}: {err}",
            path.to_string_lossy()
        )
    })?;
    let units = AmdViUnit::detect(&bytes).map_err(|err| format!("failed to parse IVRS: {err}"))?;
    Ok(units)
}

#[cfg(target_os = "redox")]
fn run() -> Result<(), String> {
    let mut units = detect_units_from_env()?;
    info!("iommu: detected {} AMD-Vi unit(s)", units.len());
    for (index, unit) in units.iter_mut().enumerate() {
        match unit.init() {
            Ok(()) => info!(
                "iommu: initialized unit {} at MMIO {:#x}",
                index,
                unit.info().mmio_base
            ),
            Err(err) => error!(
                "iommu: failed to initialize unit {} at MMIO {:#x}: {}",
                index,
                unit.info().mmio_base,
                err
            ),
        }
    }

    let socket =
        Socket::create("iommu").map_err(|e| format!("failed to register iommu scheme: {e}"))?;
    info!("iommu: registered scheme:iommu");

    let mut scheme = IommuScheme::with_units(units);

    loop {
        let request = match socket.next_request(SignalBehavior::Restart) {
            Ok(Some(request)) => request,
            Ok(None) => {
                info!("iommu: scheme unmounted, exiting");
                break;
            }
            Err(e) => {
                error!("iommu: failed to read scheme request: {e}");
                continue;
            }
        };

        let response = match request.handle_scheme_block_mut(&mut scheme) {
            Ok(response) => response,
            Err(_request) => {
                error!("iommu: failed to handle request");
                continue;
            }
        };

        if let Err(e) = socket.write_response(response, SignalBehavior::Restart) {
            error!("iommu: failed to write response: {e}");
        }
    }

    Ok(())
}

#[cfg(not(target_os = "redox"))]
fn run() -> Result<(), String> {
    let units = detect_units_from_env()?;
    info!(
        "iommu: host build stub active; parsed {} AMD-Vi unit(s) from IOMMU_IVRS_PATH",
        units.len()
    );
    Ok(())
}

fn main() {
    let log_level = match env::var("IOMMU_LOG").as_deref() {
        Ok("debug") => LevelFilter::Debug,
        Ok("trace") => LevelFilter::Trace,
        Ok("warn") => LevelFilter::Warn,
        Ok("error") => LevelFilter::Error,
        _ => LevelFilter::Info,
    };

    init_logging(log_level);

    if let Err(e) = run() {
        error!("iommu: fatal error: {e}");
        process::exit(1);
    }
}
