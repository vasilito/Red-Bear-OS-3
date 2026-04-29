#[cfg(any(target_os = "redox", test))]
use std::collections::BTreeMap;
use std::process;
#[cfg(target_os = "redox")]
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
    process::Command,
};

use redbear_hwutils::parse_args;

const PROGRAM: &str = "redbear-phase-iommu-check";
const USAGE: &str = "Usage: redbear-phase-iommu-check\n\nShow the installed IOMMU validation surface inside the guest.";
#[cfg(any(target_os = "redox", test))]
const IOMMU_PROTOCOL_VERSION: u16 = 1;
#[cfg(any(target_os = "redox", test))]
const IOMMU_REQUEST_SIZE: usize = 32;
#[cfg(any(target_os = "redox", test))]
const IOMMU_RESPONSE_SIZE: usize = 36;
#[cfg(target_os = "redox")]
const IOMMU_ALL_UNITS: u32 = u32::MAX;
#[cfg(any(target_os = "redox", test))]
const OPCODE_QUERY: u16 = 0x0000;
#[cfg(target_os = "redox")]
const OPCODE_INIT_UNITS: u16 = 0x0003;
#[cfg(target_os = "redox")]
const OPCODE_DRAIN_EVENTS: u16 = 0x0030;

#[cfg(any(target_os = "redox", test))]
#[derive(Debug, Clone, Eq, PartialEq)]
struct SelfTestSummary {
    discovery_source: String,
    kernel_acpi_status: String,
    dmar_present: bool,
    units_detected: u64,
    units_initialized_now: u64,
    units_initialized_after: u64,
    events_drained: u64,
}

#[cfg(any(target_os = "redox", test))]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct IommuRequest {
    opcode: u16,
    version: u16,
    arg0: u32,
    arg1: u64,
    arg2: u64,
    arg3: u64,
}

#[cfg(any(target_os = "redox", test))]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct IommuResponse {
    status: i32,
    kind: u16,
    version: u16,
    arg0: u32,
    arg1: u64,
    arg2: u64,
    arg3: u64,
}

#[cfg(target_os = "redox")]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct SchemeProbe {
    units_detected: u32,
    domains: u64,
    device_assignments: u64,
    units_initialized_before: u64,
    units_initialized_now: u32,
    units_attempted: u64,
    units_initialized_after: u64,
    events_drained: u32,
    first_event_code: u64,
    first_event_device: u64,
    first_event_address: u64,
}

#[cfg(any(target_os = "redox", test))]
impl IommuRequest {
    const fn new(opcode: u16, arg0: u32, arg1: u64, arg2: u64, arg3: u64) -> Self {
        Self {
            opcode,
            version: IOMMU_PROTOCOL_VERSION,
            arg0,
            arg1,
            arg2,
            arg3,
        }
    }

    fn to_bytes(self) -> [u8; IOMMU_REQUEST_SIZE] {
        let mut bytes = [0u8; IOMMU_REQUEST_SIZE];
        bytes[0..2].copy_from_slice(&self.opcode.to_le_bytes());
        bytes[2..4].copy_from_slice(&self.version.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.arg0.to_le_bytes());
        bytes[8..16].copy_from_slice(&self.arg1.to_le_bytes());
        bytes[16..24].copy_from_slice(&self.arg2.to_le_bytes());
        bytes[24..32].copy_from_slice(&self.arg3.to_le_bytes());
        bytes
    }
}

#[cfg(any(target_os = "redox", test))]
impl IommuResponse {
    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let header = bytes.get(..IOMMU_RESPONSE_SIZE)?;
        Some(Self {
            status: i32::from_le_bytes(header.get(0..4)?.try_into().ok()?),
            kind: u16::from_le_bytes(header.get(4..6)?.try_into().ok()?),
            version: u16::from_le_bytes(header.get(6..8)?.try_into().ok()?),
            arg0: u32::from_le_bytes(header.get(8..12)?.try_into().ok()?),
            arg1: u64::from_le_bytes(header.get(12..20)?.try_into().ok()?),
            arg2: u64::from_le_bytes(header.get(20..28)?.try_into().ok()?),
            arg3: u64::from_le_bytes(header.get(28..36)?.try_into().ok()?),
        })
    }
}

#[cfg(target_os = "redox")]
fn require_path(path: &str) -> Result<(), String> {
    if Path::new(path).exists() {
        println!("present={path}");
        Ok(())
    } else {
        Err(format!("missing {path}"))
    }
}

#[cfg(any(target_os = "redox", test))]
fn parse_key_value_output(text: &str) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    for line in text.lines() {
        if let Some((key, value)) = line.split_once('=') {
            values.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    values
}

#[cfg(any(target_os = "redox", test))]
fn parse_u64_field(values: &BTreeMap<String, String>, key: &str) -> Result<u64, String> {
    let value = values
        .get(key)
        .ok_or_else(|| format!("iommu self-test did not report {key}"))?;
    value
        .parse::<u64>()
        .map_err(|err| format!("invalid {key} value '{value}': {err}"))
}

#[cfg(any(target_os = "redox", test))]
fn parse_bool_field(values: &BTreeMap<String, String>, key: &str) -> Result<bool, String> {
    let value = values
        .get(key)
        .ok_or_else(|| format!("iommu self-test did not report {key}"))?;
    match value.as_str() {
        "1" | "true" | "yes" => Ok(true),
        "0" | "false" | "no" => Ok(false),
        _ => Err(format!("invalid {key} value '{value}'")),
    }
}

#[cfg(any(target_os = "redox", test))]
fn parse_self_test_summary(stdout: &str) -> Result<SelfTestSummary, String> {
    let values = parse_key_value_output(stdout);

    Ok(SelfTestSummary {
        discovery_source: values
            .get("discovery_source")
            .cloned()
            .ok_or_else(|| "iommu self-test did not report discovery_source".to_string())?,
        kernel_acpi_status: values
            .get("kernel_acpi_status")
            .cloned()
            .ok_or_else(|| "iommu self-test did not report kernel_acpi_status".to_string())?,
        dmar_present: parse_bool_field(&values, "dmar_present")?,
        units_detected: parse_u64_field(&values, "units_detected")?,
        units_initialized_now: parse_u64_field(&values, "units_initialized_now")?,
        units_initialized_after: parse_u64_field(&values, "units_initialized_after")?,
        events_drained: parse_u64_field(&values, "events_drained")?,
    })
}

#[cfg(any(target_os = "redox", test))]
fn iommu_vendor_detection(summary: &SelfTestSummary) -> &'static str {
    match (summary.units_detected > 0, summary.dmar_present) {
        (true, true) => "amd-vi+intel-vt-d-dmar",
        (true, false) => "amd-vi",
        (false, true) => "intel-vt-d-dmar",
        (false, false) => "none",
    }
}

#[cfg(target_os = "redox")]
fn send_request(control: &mut File, request: IommuRequest) -> Result<IommuResponse, String> {
    control.write_all(&request.to_bytes()).map_err(|err| {
        format!(
            "failed to write IOMMU request opcode {:#06x}: {err}",
            request.opcode
        )
    })?;
    control.flush().map_err(|err| {
        format!(
            "failed to flush IOMMU request opcode {:#06x}: {err}",
            request.opcode
        )
    })?;

    let mut response_bytes = [0u8; IOMMU_RESPONSE_SIZE];
    control.read_exact(&mut response_bytes).map_err(|err| {
        format!(
            "failed to read IOMMU response for opcode {:#06x}: {err}",
            request.opcode
        )
    })?;

    let response = IommuResponse::from_bytes(&response_bytes).ok_or_else(|| {
        format!(
            "failed to decode IOMMU response for opcode {:#06x}",
            request.opcode
        )
    })?;

    if response.version != IOMMU_PROTOCOL_VERSION {
        return Err(format!(
            "IOMMU response version mismatch for opcode {:#06x}: expected {} got {}",
            request.opcode, IOMMU_PROTOCOL_VERSION, response.version
        ));
    }
    if response.kind != request.opcode {
        return Err(format!(
            "IOMMU response kind mismatch for opcode {:#06x}: got {:#06x}",
            request.opcode, response.kind
        ));
    }
    if response.status != 0 {
        return Err(format!(
            "IOMMU request opcode {:#06x} failed with status {}",
            request.opcode, response.status
        ));
    }

    Ok(response)
}

#[cfg(target_os = "redox")]
fn probe_iommu_scheme() -> Result<SchemeProbe, String> {
    let mut control = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/scheme/iommu/control")
        .map_err(|err| format!("failed to open /scheme/iommu/control: {err}"))?;

    let query = send_request(&mut control, IommuRequest::new(OPCODE_QUERY, 0, 0, 0, 0))?;
    let init = send_request(
        &mut control,
        IommuRequest::new(OPCODE_INIT_UNITS, IOMMU_ALL_UNITS, 0, 0, 0),
    )?;
    let drain = send_request(
        &mut control,
        IommuRequest::new(OPCODE_DRAIN_EVENTS, IOMMU_ALL_UNITS, 0, 0, 0),
    )?;

    Ok(SchemeProbe {
        units_detected: query.arg0,
        domains: query.arg1,
        device_assignments: query.arg2,
        units_initialized_before: query.arg3,
        units_initialized_now: init.arg0,
        units_attempted: init.arg1,
        units_initialized_after: init.arg2,
        events_drained: drain.arg0,
        first_event_code: drain.arg1,
        first_event_device: drain.arg2,
        first_event_address: drain.arg3,
    })
}

#[cfg(target_os = "redox")]
fn run() -> Result<(), String> {
    parse_args(PROGRAM, USAGE, std::env::args()).map_err(|err| {
        if err.is_empty() {
            process::exit(0);
        }
        err
    })?;

    println!("=== Red Bear OS IOMMU Runtime Check ===");
    require_path("/usr/bin/iommu")?;

    let output = Command::new("/usr/bin/iommu")
        .env("IOMMU_LOG", "info")
        .arg("--self-test-init")
        .output()
        .map_err(|err| format!("failed to run /usr/bin/iommu --self-test-init: {err}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    print!("{}", stdout);
    print!("{}", stderr);

    let summary = parse_self_test_summary(&stdout)?;
    println!(
        "amd_vi_present={}",
        if summary.units_detected > 0 { 1 } else { 0 }
    );
    println!(
        "intel_vtd_dmar_present={}",
        if summary.dmar_present { 1 } else { 0 }
    );
    println!(
        "iommu_vendor_detection={}",
        iommu_vendor_detection(&summary)
    );

    if !output.status.success() && !(summary.units_detected == 0 && summary.dmar_present) {
        return Err(format!(
            "iommu self-test exited with status {:?}",
            output.status.code()
        ));
    }

    if summary.units_detected == 0 && !summary.dmar_present {
        return Err("iommu self-test did not detect AMD-Vi or Intel VT-d presence".to_string());
    }

    if summary.units_detected == 0 {
        println!("iommu_scheme_probe=unavailable reason=no_amd_vi_units");
        println!("iommu_event_log_probe=unavailable reason=no_amd_vi_units");
        println!("interrupt_remap_table_probe=unavailable reason=no_amd_vi_units");
        return Ok(());
    }

    require_path("/scheme/iommu")?;
    require_path("/scheme/iommu/control")?;

    let scheme = probe_iommu_scheme()?;
    println!(
        "IOMMU_SCHEME_QUERY units_detected={} domains={} device_assignments={} units_initialized_before={}",
        scheme.units_detected,
        scheme.domains,
        scheme.device_assignments,
        scheme.units_initialized_before
    );
    println!(
        "IOMMU_INIT_UNITS units_initialized_now={} units_attempted={} units_initialized_after={}",
        scheme.units_initialized_now, scheme.units_attempted, scheme.units_initialized_after
    );
    println!(
        "IOMMU_EVENT_LOG drained_events={} first_code={} first_device={:#x} first_address={:#x}",
        scheme.events_drained,
        scheme.first_event_code,
        scheme.first_event_device,
        scheme.first_event_address
    );
    println!("iommu_event_log_probe=ok");

    if u64::from(scheme.units_detected) == 0 {
        return Err("scheme:iommu reported zero units".to_string());
    }
    if scheme.units_initialized_after == 0 {
        return Err(
            "scheme:iommu did not leave any units initialized after INIT_UNITS".to_string(),
        );
    }

    println!(
        "interrupt_remap_table_probe=indirect basis=init_units_success initialized_units={}",
        scheme.units_initialized_after
    );

    Ok(())
}

#[cfg(not(target_os = "redox"))]
fn run() -> Result<(), String> {
    parse_args(PROGRAM, USAGE, std::env::args()).map_err(|err| {
        if err.is_empty() {
            process::exit(0);
        }
        err
    })?;
    Err("redbear-phase-iommu-check requires target_os=redox".to_string())
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

    #[test]
    fn parse_self_test_summary_reads_required_fields() {
        let summary = parse_self_test_summary(
            "discovery_source=kernel-acpi\nkernel_acpi_status=ok\ndmar_present=0\nunits_detected=1\nunits_initialized_now=1\nunits_initialized_after=1\nevents_drained=0\n",
        )
        .unwrap();

        assert_eq!(summary.discovery_source, "kernel-acpi");
        assert_eq!(summary.kernel_acpi_status, "ok");
        assert!(summary.units_detected > 0);
        assert!(!summary.dmar_present);
    }

    #[test]
    fn iommu_vendor_detection_prefers_combined_label_when_both_are_visible() {
        let summary = SelfTestSummary {
            discovery_source: "test".to_string(),
            kernel_acpi_status: "ok".to_string(),
            dmar_present: true,
            units_detected: 1,
            units_initialized_now: 1,
            units_initialized_after: 1,
            events_drained: 0,
        };

        assert_eq!(iommu_vendor_detection(&summary), "amd-vi+intel-vt-d-dmar");
    }

    #[test]
    fn iommu_response_decodes_wire_format() {
        let mut bytes = [0u8; IOMMU_RESPONSE_SIZE];
        bytes[0..4].copy_from_slice(&0i32.to_le_bytes());
        bytes[4..6].copy_from_slice(&OPCODE_QUERY.to_le_bytes());
        bytes[6..8].copy_from_slice(&IOMMU_PROTOCOL_VERSION.to_le_bytes());
        bytes[8..12].copy_from_slice(&2u32.to_le_bytes());
        bytes[12..20].copy_from_slice(&3u64.to_le_bytes());
        bytes[20..28].copy_from_slice(&4u64.to_le_bytes());
        bytes[28..36].copy_from_slice(&5u64.to_le_bytes());

        let response = IommuResponse::from_bytes(&bytes).unwrap();
        assert_eq!(response.kind, OPCODE_QUERY);
        assert_eq!(response.arg0, 2);
        assert_eq!(response.arg3, 5);
    }

    #[test]
    fn iommu_request_encodes_wire_format() {
        let request = IommuRequest::new(OPCODE_QUERY, 7, 8, 9, 10);
        let bytes = request.to_bytes();

        assert_eq!(bytes.len(), IOMMU_REQUEST_SIZE);
        assert_eq!(u16::from_le_bytes([bytes[0], bytes[1]]), OPCODE_QUERY);
        assert_eq!(
            u16::from_le_bytes([bytes[2], bytes[3]]),
            IOMMU_PROTOCOL_VERSION
        );
        assert_eq!(
            u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            7
        );
    }
}
