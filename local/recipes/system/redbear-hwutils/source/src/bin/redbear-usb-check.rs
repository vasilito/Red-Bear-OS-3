use std::fs;
use std::process;

use redbear_hwutils::parse_args;

const PROGRAM: &str = "redbear-usb-check";
const USAGE: &str = "Usage: redbear-usb-check\n\nCheck the USB stack inside a Red Bear guest.\n\nWalks the usb scheme tree and reports controller and device status.";

fn list_scheme_dir(path: &str) -> Vec<String> {
    match fs::read_dir(path) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn run() -> Result<(), String> {
    parse_args(PROGRAM, USAGE, std::env::args()).map_err(|err| {
        if err.is_empty() {
            process::exit(0);
        }
        err
    })?;

    let mut failures = 0;

    let usb_entries = list_scheme_dir("/scheme/usb");
    if usb_entries.is_empty() {
        eprintln!("{PROGRAM}: FAIL: no usb scheme entries found");
        failures += 1;
    } else {
        println!(
            "{PROGRAM}: found {} usb scheme entries: {:?}",
            usb_entries.len(),
            usb_entries
        );

        for entry in &usb_entries {
            let scheme_path = format!("/scheme/usb/{}", entry);
            let sub = list_scheme_dir(&scheme_path);
            println!("{PROGRAM}:   {} -> {:?} ports", entry, sub.len());

            for port in &sub {
                let port_path = format!("{}/{}/descriptors", scheme_path, port);
                if let Ok(data) = fs::read_to_string(&port_path) {
                    if let Ok(dev_desc) = serde_json::from_str::<serde_json::Value>(&data) {
                        let vendor = dev_desc
                            .get("vendor")
                            .and_then(|v| v.as_u64())
                            .map(|v| format!("{:04x}", v))
                            .unwrap_or_else(|| "????".to_string());
                        let product = dev_desc
                            .get("product")
                            .and_then(|v| v.as_u64())
                            .map(|v| format!("{:04x}", v))
                            .unwrap_or_else(|| "????".to_string());
                        let ss = dev_desc
                            .get("supports_superspeed")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let product_str = dev_desc
                            .get("product_str")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let speed_tag = if ss { " [SS]" } else { "" };
                        println!(
                            "{PROGRAM}:     port {} -> {}:{} {}{}",
                            port, vendor, product, product_str, speed_tag
                        );
                    }
                }
            }
        }
    }

    let xhci_entries = list_scheme_dir("/scheme/xhci");
    if xhci_entries.is_empty() {
        eprintln!("{PROGRAM}: FAIL: no xhci scheme entries found");
        failures += 1;
    } else {
        println!("{PROGRAM}: xhci controllers: {:?}", xhci_entries);
    }

    if failures > 0 {
        Err(format!("{} check(s) failed", failures))
    } else {
        println!("{PROGRAM}: all checks passed");
        Ok(())
    }
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{PROGRAM}: {err}");
        process::exit(1);
    }
}
