// USB subsystem runtime validation check.
// Validates USB host controllers, device enumeration, topology, and class detection.

use std::process;

const PROGRAM: &str = "redbear-usb-check";
const USAGE: &str = "Usage: redbear-usb-check [--json]\n\n\
     USB subsystem runtime check. Validates xHCI controller registration,\n\
     USB device enumeration, and class detection (HID, storage, hub).";

#[cfg(target_os = "redox")]
use std::fs;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CheckResult { Pass, Fail, Skip }

impl CheckResult {
    fn label(self) -> &'static str {
        match self { Self::Pass => "PASS", Self::Fail => "FAIL", Self::Skip => "SKIP" }
    }
}

struct Check { name: String, result: CheckResult, detail: String }

impl Check {
    fn pass(name: &str, detail: &str) -> Self {
        Check { name: name.to_string(), result: CheckResult::Pass, detail: detail.to_string() }
    }
    fn fail(name: &str, detail: &str) -> Self {
        Check { name: name.to_string(), result: CheckResult::Fail, detail: detail.to_string() }
    }
    fn skip(name: &str, detail: &str) -> Self {
        Check { name: name.to_string(), result: CheckResult::Skip, detail: detail.to_string() }
    }
}

struct Report { checks: Vec<Check>, json_mode: bool }

impl Report {
    fn new(json_mode: bool) -> Self { Report { checks: Vec::new(), json_mode } }
    fn add(&mut self, check: Check) { self.checks.push(check); }
    fn any_failed(&self) -> bool { self.checks.iter().any(|c| c.result == CheckResult::Fail) }

    fn print(&self) {
        if self.json_mode { self.print_json(); } else { self.print_human(); }
    }
    fn print_human(&self) {
        for check in &self.checks {
            let icon = match check.result {
                CheckResult::Pass => "[PASS]", CheckResult::Fail => "[FAIL]", CheckResult::Skip => "[SKIP]",
            };
            println!("{icon} {}: {}", check.name, check.detail);
        }
    }
    fn print_json(&self) {
        #[derive(serde::Serialize)]
        struct JsonCheck { name: String, result: String, detail: String }
        #[derive(serde::Serialize)]
        struct JsonReport {
            xhci_controllers: usize, usb_devices: usize,
            hid_devices: usize, storage_devices: usize, checks: Vec<JsonCheck>,
        }
        let xhci = self.checks.iter().find(|c| c.name == "XHCI_CONTROLLER").map_or(0, |c| {
            c.detail.split(' ').next().and_then(|s| s.parse().ok()).unwrap_or(0)
        });
        let devices = self.checks.iter().find(|c| c.name == "USB_DEVICES").map_or(0, |c| {
            c.detail.strip_prefix("found ").and_then(|s| s.split(' ').next()).and_then(|s| s.parse().ok()).unwrap_or(0)
        });
        let hid = self.checks.iter().find(|c| c.name == "USB_HID").map_or(0, |c| {
            c.detail.strip_prefix("found ").and_then(|s| s.split(' ').next()).and_then(|s| s.parse().ok()).unwrap_or(0)
        });
        let storage = self.checks.iter().find(|c| c.name == "USB_STORAGE").map_or(0, |c| {
            c.detail.strip_prefix("found ").and_then(|s| s.split(' ').next()).and_then(|s| s.parse().ok()).unwrap_or(0)
        });
        let checks: Vec<JsonCheck> = self.checks.iter().map(|c| JsonCheck {
            name: c.name.clone(), result: c.result.label().to_string(), detail: c.detail.clone(),
        }).collect();
        if let Err(err) = serde_json::to_writer(std::io::stdout(), &JsonReport { xhci_controllers: xhci, usb_devices: devices, hid_devices: hid, storage_devices: storage, checks }) {
            eprintln!("{PROGRAM}: failed to serialize JSON: {err}");
        }
    }
}

#[cfg(target_os = "redox")]
fn parse_args() -> Result<bool, String> {
    let mut json_mode = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--json" => json_mode = true,
            "-h" | "--help" => { println!("{USAGE}"); return Err(String::new()); }
            _ => return Err(format!("unsupported argument: {arg}")),
        }
    }
    Ok(json_mode)
}

#[cfg(target_os = "redox")]
fn list_dir(path: &str) -> Vec<String> {
    match fs::read_dir(path) {
        Ok(entries) => entries.filter_map(|e| e.ok()).filter_map(|e| e.file_name().to_str().map(|s| s.to_string())).collect(),
        Err(_) => Vec::new(),
    }
}

#[cfg(target_os = "redox")]
fn check_xhci_controller() -> Check {
    let xhci = list_dir("/scheme/xhci");
    if !xhci.is_empty() {
        Check::pass("XHCI_CONTROLLER", &format!("{} xHCI controller(s): {}", xhci.len(), xhci.join(", ")))
    } else {
        Check::fail("XHCI_CONTROLLER", "no xHCI controllers found under /scheme/xhci")
    }
}

#[cfg(target_os = "redox")]
fn check_usb_devices() -> Check {
    let entries = list_dir("/scheme/usb");
    if entries.is_empty() {
        return Check::fail("USB_DEVICES", "no USB devices found under /scheme/usb");
    }
    let mut total = 0usize;
    for entry in &entries {
        let port_path = format!("/scheme/usb/{}", entry);
        let ports = list_dir(&port_path);
        total += ports.len();
    }
    if total > 0 {
        Check::pass("USB_DEVICES", &format!("found {} device(s) across {} hub(s)", total, entries.len()))
    } else {
        Check::skip("USB_DEVICES", "USB hubs present but no devices attached")
    }
}

#[cfg(target_os = "redox")]
fn check_usb_hid() -> Check {
    let mut hid_count = 0usize;
    let entries = list_dir("/scheme/usb");
    for entry in &entries {
        let port_path = format!("/scheme/usb/{}", entry);
        for port in list_dir(&port_path) {
            let desc_path = format!("{}/{}/descriptors", port_path, port);
            if let Ok(data) = fs::read_to_string(&desc_path) {
                if let Ok(desc) = serde_json::from_str::<serde_json::Value>(&data) {
                    let class = desc.get("class").and_then(|v| v.as_u64()).unwrap_or(0);
                    if class == 3 { hid_count += 1; }
                }
            }
        }
    }
    if hid_count > 0 {
        Check::pass("USB_HID", &format!("found {} HID device(s)", hid_count))
    } else {
        Check::skip("USB_HID", "no USB HID devices found (may need physical hardware)")
    }
}

#[cfg(target_os = "redox")]
fn check_usb_storage() -> Check {
    let mut storage_count = 0usize;
    let entries = list_dir("/scheme/usb");
    for entry in &entries {
        let port_path = format!("/scheme/usb/{}", entry);
        for port in list_dir(&port_path) {
            let desc_path = format!("{}/{}/descriptors", port_path, port);
            if let Ok(data) = fs::read_to_string(&desc_path) {
                if let Ok(desc) = serde_json::from_str::<serde_json::Value>(&data) {
                    let class = desc.get("class").and_then(|v| v.as_u64()).unwrap_or(0);
                    if class == 8 { storage_count += 1; }
                }
            }
        }
    }
    if storage_count > 0 {
        Check::pass("USB_STORAGE", &format!("found {} storage device(s)", storage_count))
    } else {
        Check::skip("USB_STORAGE", "no USB storage devices found (may need physical hardware)")
    }
}

fn run() -> Result<(), String> {
    #[cfg(not(target_os = "redox"))]
    {
        if std::env::args().any(|a| a == "-h" || a == "--help") { println!("{USAGE}"); return Err(String::new()); }
        println!("{PROGRAM}: USB check requires Redox runtime");
        return Ok(());
    }
    #[cfg(target_os = "redox")]
    {
        let json_mode = parse_args()?;
        let mut report = Report::new(json_mode);
        report.add(check_xhci_controller());
        report.add(check_usb_devices());
        report.add(check_usb_hid());
        report.add(check_usb_storage());
        report.print();
        if report.any_failed() { return Err("one or more USB checks failed".to_string()); }
        Ok(())
    }
}

fn main() {
    if let Err(err) = run() {
        if err.is_empty() { process::exit(0); }
        eprintln!("{PROGRAM}: {err}");
        process::exit(1);
    }
}
