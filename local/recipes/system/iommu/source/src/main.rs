//! IOMMU daemon — provides scheme:iommu for DMA remapping.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;

use iommu::amd_vi::AmdViUnit;
#[cfg(target_os = "redox")]
use iommu::IommuScheme;
use log::{error, info, LevelFilter, Metadata, Record};
#[cfg(target_os = "redox")]
use redox_driver_sys::memory::{CacheType, MmioProt, MmioRegion};
#[cfg(target_os = "redox")]
use redox_scheme::{SignalBehavior, Socket};
#[cfg(target_os = "redox")]
use syscall::EBADF;
#[cfg(target_os = "redox")]
use syscall::PAGE_SIZE;

struct StderrLogger {
    level: LevelFilter,
}

#[cfg_attr(not(target_os = "redox"), allow(dead_code))]
struct DiscoveryResult {
    units: Vec<AmdViUnit>,
    source: DiscoverySource,
    kernel_acpi_status: &'static str,
    ivrs_path: Option<PathBuf>,
    dmar_present: bool,
}

#[cfg_attr(not(target_os = "redox"), allow(dead_code))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DiscoverySource {
    KernelAcpi,
    Filesystem,
    None,
}

impl DiscoverySource {
    fn as_str(self) -> &'static str {
        match self {
            Self::KernelAcpi => "kernel_acpi",
            Self::Filesystem => "filesystem",
            Self::None => "none",
        }
    }
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

fn candidate_ivrs_paths() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/sys/firmware/acpi/tables/IVRS"),
        PathBuf::from("/sys/firmware/acpi/tables/data/IVRS"),
        PathBuf::from("/boot/acpi/IVRS"),
        PathBuf::from("/acpi/tables/IVRS"),
    ]
}

fn discover_ivrs_path_from_candidates(candidates: &[PathBuf]) -> Option<PathBuf> {
    if let Some(path) = env::var_os("IOMMU_IVRS_PATH") {
        return Some(PathBuf::from(path));
    }

    candidates.iter().find(|path| path.exists()).cloned()
}

fn discover_ivrs_path() -> Option<PathBuf> {
    discover_ivrs_path_from_candidates(&candidate_ivrs_paths())
}

fn detect_units_from_ivrs_path(path: &PathBuf) -> Result<Vec<AmdViUnit>, String> {
    let bytes = fs::read(path)
        .map_err(|err| format!("failed to read IVRS table from {}: {err}", path.display()))?;
    let units = AmdViUnit::detect(&bytes).map_err(|err| format!("failed to parse IVRS: {err}"))?;
    Ok(units)
}

fn detect_units_from_discovered_ivrs() -> Result<(Vec<AmdViUnit>, Option<PathBuf>), String> {
    let Some(path) = discover_ivrs_path() else {
        return Ok((Vec::new(), None));
    };

    let units = detect_units_from_ivrs_path(&path)?;
    Ok((units, Some(path)))
}

#[cfg(target_os = "redox")]
const ACPI_HEADER_LEN: usize = 36;

#[cfg(target_os = "redox")]
fn read_sdt_from_physical(phys_addr: u64) -> Result<Vec<u8>, String> {
    let page_base = phys_addr / PAGE_SIZE as u64 * PAGE_SIZE as u64;
    let page_offset = (phys_addr - page_base) as usize;

    let header_map = MmioRegion::map(page_base, PAGE_SIZE, CacheType::WriteBack, MmioProt::READ)
        .map_err(|err| format!("failed to map ACPI header page at {page_base:#x}: {err}"))?;

    let mut header = vec![0_u8; ACPI_HEADER_LEN];
    for (i, byte) in header.iter_mut().enumerate() {
        *byte = header_map.read8(page_offset + i);
    }
    let length = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    if length < ACPI_HEADER_LEN {
        return Err(format!(
            "invalid ACPI SDT length {length} at {phys_addr:#x}"
        ));
    }

    let map_len = (page_offset + length).next_multiple_of(PAGE_SIZE);
    let full_map = MmioRegion::map(page_base, map_len, CacheType::WriteBack, MmioProt::READ)
        .map_err(|err| format!("failed to map ACPI table at {page_base:#x}: {err}"))?;

    let mut bytes = vec![0_u8; length];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = full_map.read8(page_offset + i);
    }
    Ok(bytes)
}

#[cfg(target_os = "redox")]
fn find_kernel_acpi_table(signature: &[u8; 4]) -> Result<Option<Vec<u8>>, String> {
    let rxsdt = match fs::read("/scheme/kernel.acpi/rxsdt") {
        Ok(bytes) => bytes,
        Err(err) => {
            return Err(format!("failed to read /scheme/kernel.acpi/rxsdt: {err}"));
        }
    };

    if rxsdt.len() < ACPI_HEADER_LEN {
        return Ok(None);
    }

    let root_signature = &rxsdt[0..4];
    let entry_size = match root_signature {
        b"RSDT" => 4,
        b"XSDT" => 8,
        _ => return Ok(None),
    };

    let mut offset = ACPI_HEADER_LEN;
    while offset + entry_size <= rxsdt.len() {
        let phys_addr = if entry_size == 4 {
            u32::from_le_bytes(rxsdt[offset..offset + 4].try_into().unwrap()) as u64
        } else {
            u64::from_le_bytes(rxsdt[offset..offset + 8].try_into().unwrap())
        };

        let table = read_sdt_from_physical(phys_addr)?;
        if table.len() >= 4 && &table[0..4] == signature {
            return Ok(Some(table));
        }

        offset += entry_size;
    }

    Ok(None)
}

#[cfg(target_os = "redox")]
fn detect_units_from_kernel_acpi() -> Result<Vec<AmdViUnit>, String> {
    match find_kernel_acpi_table(b"IVRS")? {
        Some(table) => AmdViUnit::detect(&table).map_err(|err| format!("failed to parse IVRS: {err}")),
        None => Ok(Vec::new()),
    }
}

#[cfg(target_os = "redox")]
fn detect_dmar_from_kernel_acpi() -> Result<bool, String> {
    Ok(find_kernel_acpi_table(b"DMAR")?.is_some())
}

#[cfg(target_os = "redox")]
fn discover_units() -> Result<DiscoveryResult, String> {
    let dmar_present = match detect_dmar_from_kernel_acpi() {
        Ok(present) => present,
        Err(err) => {
            info!("iommu: kernel ACPI DMAR discovery unavailable: {err}");
            false
        }
    };

    match detect_units_from_kernel_acpi() {
        Ok(units) if !units.is_empty() => Ok(DiscoveryResult {
            units,
            source: DiscoverySource::KernelAcpi,
            kernel_acpi_status: "ok",
            ivrs_path: None,
            dmar_present,
        }),
        Ok(_units) => {
            let (units, ivrs_path) = detect_units_from_discovered_ivrs()?;
            Ok(DiscoveryResult {
                source: if ivrs_path.is_some() {
                    DiscoverySource::Filesystem
                } else {
                    DiscoverySource::None
                },
                units,
                kernel_acpi_status: "empty",
                ivrs_path,
                dmar_present,
            })
        }
        Err(err) => {
            info!("iommu: kernel ACPI discovery unavailable: {err}");
            let (units, ivrs_path) = detect_units_from_discovered_ivrs()?;
            Ok(DiscoveryResult {
                source: if ivrs_path.is_some() {
                    DiscoverySource::Filesystem
                } else {
                    DiscoverySource::None
                },
                units,
                kernel_acpi_status: "error",
                ivrs_path,
                dmar_present,
            })
        }
    }
}

#[cfg(not(target_os = "redox"))]
fn discover_units() -> Result<DiscoveryResult, String> {
    let (units, ivrs_path) = detect_units_from_discovered_ivrs()?;
    Ok(DiscoveryResult {
        source: if ivrs_path.is_some() {
            DiscoverySource::Filesystem
        } else {
            DiscoverySource::None
        },
        units,
        kernel_acpi_status: "unsupported",
        ivrs_path,
        dmar_present: false,
    })
}

#[cfg(target_os = "redox")]
fn run() -> Result<(), String> {
    let discovery = discover_units()?;
    if discovery.units.is_empty() {
        info!(
            "iommu: no AMD-Vi units found (source={}, kernel_acpi_status={}, ivrs_path={})",
            discovery.source.as_str(),
            discovery.kernel_acpi_status,
            discovery
                .ivrs_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "none".to_string())
        );
    } else {
        info!(
            "iommu: detected {} AMD-Vi unit(s) via {}",
            discovery.units.len(),
            discovery.source.as_str()
        );
    }
    if discovery.dmar_present {
        info!(
            "iommu: detected kernel ACPI DMAR table; Intel VT-d runtime ownership should converge here rather than remain in acpid"
        );
    }
    for (index, unit) in discovery.units.iter().enumerate() {
        info!(
            "iommu: discovered unit {} at MMIO {:#x}; initialization is deferred until first use",
            index,
            unit.info().mmio_base
        );
    }

    let socket =
        Socket::create("iommu").map_err(|e| format!("failed to register iommu scheme: {e}"))?;
    info!("iommu: registered scheme:iommu");

    let mut scheme = IommuScheme::with_units(discovery.units);

    loop {
        let request = match socket.next_request(SignalBehavior::Restart) {
            Ok(Some(request)) => request,
            Ok(None) => {
                info!("iommu: scheme unmounted, exiting");
                break;
            }
            Err(e) => {
                if e.errno == EBADF {
                    info!("iommu: scheme fd closed, exiting");
                    break;
                }
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

#[cfg(target_os = "redox")]
fn run_self_test() -> Result<(), String> {
    let discovery = discover_units()?;
    let mut units = discovery.units;

    println!("discovery_source={}", discovery.source.as_str());
    println!("kernel_acpi_status={}", discovery.kernel_acpi_status);
    println!("dmar_present={}", if discovery.dmar_present { 1 } else { 0 });
    println!(
        "ivrs_path={}",
        discovery
            .ivrs_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    println!("units_detected={}", units.len());
    if units.is_empty() {
        return Err("iommu self-test detected zero AMD-Vi unit(s)".to_string());
    }

    let mut initialized_now = 0u32;
    let mut events_drained = 0u32;

    for (index, unit) in units.iter_mut().enumerate() {
        let was_initialized = unit.initialized();
        unit.init().map_err(|err| {
            format!(
                "iommu self-test failed to initialize unit {} at MMIO {:#x}: {}",
                index,
                unit.info().mmio_base,
                err
            )
        })?;

        if !was_initialized {
            initialized_now = initialized_now.saturating_add(1);
        }

        let drained = unit.drain_events().map_err(|err| {
            format!(
                "iommu self-test failed to drain events for unit {} at MMIO {:#x}: {}",
                index,
                unit.info().mmio_base,
                err
            )
        })?;
        events_drained = events_drained.saturating_add(drained.len() as u32);
    }

    let initialized_after = units.iter().filter(|unit| unit.initialized()).count() as u64;
    println!("units_initialized_now={}", initialized_now);
    println!("units_attempted={}", units.len());
    println!("units_initialized_after={}", initialized_after);
    println!("events_drained={}", events_drained);

    Ok(())
}

#[cfg(not(target_os = "redox"))]
fn run() -> Result<(), String> {
    let discovery = discover_units()?;
    info!(
        "iommu: host build stub active; parsed {} AMD-Vi unit(s) via {}",
        discovery.units.len(),
        discovery.source.as_str()
    );
    Ok(())
}

#[cfg(not(target_os = "redox"))]
fn run_self_test() -> Result<(), String> {
    Err("iommu self-test requires target_os=redox".to_string())
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

    let result = if env::args().any(|arg| arg == "--self-test-init") {
        run_self_test()
    } else {
        run()
    };

    if let Err(e) = result {
        error!("iommu: fatal error: {e}");
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        candidate_ivrs_paths, discover_ivrs_path_from_candidates, DiscoverySource,
    };
    use std::path::PathBuf;

    #[test]
    fn candidate_paths_include_standard_ivrs_locations() {
        let candidates = candidate_ivrs_paths();
        assert!(candidates.contains(&PathBuf::from("/sys/firmware/acpi/tables/IVRS")));
        assert!(candidates.contains(&PathBuf::from("/sys/firmware/acpi/tables/data/IVRS")));
        assert!(candidates.contains(&PathBuf::from("/boot/acpi/IVRS")));
        assert!(candidates.contains(&PathBuf::from("/acpi/tables/IVRS")));
    }

    #[test]
    fn discovery_chooses_first_existing_candidate() {
        let candidates = vec![
            PathBuf::from("/definitely/missing/ivrs"),
            PathBuf::from("/tmp"),
        ];
        let discovered = discover_ivrs_path_from_candidates(&candidates);
        assert_eq!(discovered, Some(PathBuf::from("/tmp")));
    }

    #[test]
    fn discovery_source_strings_are_stable() {
        assert_eq!(DiscoverySource::KernelAcpi.as_str(), "kernel_acpi");
        assert_eq!(DiscoverySource::Filesystem.as_str(), "filesystem");
        assert_eq!(DiscoverySource::None.as_str(), "none");
    }

    #[test]
    fn host_discovery_defaults_to_no_dmar() {
        let discovery = super::discover_units().expect("host discovery should succeed");
        assert!(!discovery.dmar_present);
    }
}
