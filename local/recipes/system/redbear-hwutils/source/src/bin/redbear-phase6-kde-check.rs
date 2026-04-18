use std::path::Path;
use std::process::{self, Command, Output};

use redbear_hwutils::parse_args;

const PROGRAM: &str = "redbear-phase6-kde-check";
const USAGE: &str = "Usage: redbear-phase6-kde-check\n\nShow the installed Phase 6 KDE session surface inside the guest.";

const DBUS_ERROR_MARKERS: &[&str] = &[
    "org.freedesktop.DBus.Error",
    "QDBusError",
    "Could not connect to D-Bus",
    "ServiceUnknown",
    "No such interface",
    "NoReply",
    "UnknownMethod",
];

const SOLID_CONSUMER_MARKERS: &[&str] = &[
    "StorageAccess.",
    "StorageDrive.",
    "StorageVolume.",
    "OpticalDrive.",
    "AcAdapter.",
    "Battery.",
];

fn require_path(path: &str) -> Result<(), String> {
    if Path::new(path).exists() {
        println!("{path}");
        Ok(())
    } else {
        Err(format!("missing {path}"))
    }
}

fn command_output(command: &mut Command, description: &str) -> Result<Output, String> {
    command
        .output()
        .map_err(|err| format!("failed to run {description}: {err}"))
}

fn output_text(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    format!("{stdout}{stderr}")
}

fn require_success(output: &Output, description: &str) -> Result<(), String> {
    if output.status.success() {
        Ok(())
    } else {
        let text = output_text(output);
        let detail = text.trim();
        if detail.is_empty() {
            Err(format!(
                "{description} exited with status {}",
                output.status
            ))
        } else {
            Err(format!(
                "{description} exited with status {}: {}",
                output.status, detail
            ))
        }
    }
}

fn contains_dbus_error(text: &str) -> bool {
    DBUS_ERROR_MARKERS
        .iter()
        .any(|marker| text.contains(marker))
}

fn contains_solid_consumer_surface(text: &str) -> bool {
    SOLID_CONSUMER_MARKERS
        .iter()
        .any(|marker| text.contains(marker))
}

fn require_dbus_free_output(output: &Output, description: &str) -> Result<(), String> {
    let text = output_text(output);
    if contains_dbus_error(&text) {
        Err(format!(
            "{description} reported a D-Bus error: {}",
            text.trim()
        ))
    } else {
        Ok(())
    }
}

fn check_system_bus_consumers() -> Result<(), String> {
    if !Path::new("/usr/bin/dbus-send").exists() {
        println!("PHASE6_DBUS_SEND=missing");
        println!("PHASE6_UPOWER_ENUMERATE=skipped_missing_dbus_send");
        println!("PHASE6_UDISKS2_OBJECTS=skipped_missing_dbus_send");
        return Ok(());
    }

    println!("/usr/bin/dbus-send");

    let names = command_output(
        Command::new("dbus-send")
            .arg("--system")
            .arg("--dest=org.freedesktop.DBus")
            .arg("--type=method_call")
            .arg("--print-reply")
            .arg("/org/freedesktop/DBus")
            .arg("org.freedesktop.DBus.ListNames"),
        "dbus-send ListNames",
    )?;
    require_success(&names, "dbus-send ListNames")?;
    let names_text = output_text(&names);
    let upower = command_output(
        Command::new("dbus-send")
            .arg("--system")
            .arg("--dest=org.freedesktop.UPower")
            .arg("--type=method_call")
            .arg("--print-reply")
            .arg("/org/freedesktop/UPower")
            .arg("org.freedesktop.UPower.EnumerateDevices"),
        "dbus-send UPower EnumerateDevices",
    )?;
    require_success(&upower, "dbus-send UPower EnumerateDevices")?;
    require_dbus_free_output(&upower, "dbus-send UPower EnumerateDevices")?;
    if names_text.contains("org.freedesktop.UPower") {
        println!("PHASE6_UPOWER_BUS_NAME=present");
    } else {
        println!("PHASE6_UPOWER_BUS_NAME=activated_lazily");
    }
    println!("PHASE6_UPOWER_ENUMERATE=ok");

    let udisks = command_output(
        Command::new("dbus-send")
            .arg("--system")
            .arg("--dest=org.freedesktop.UDisks2")
            .arg("--type=method_call")
            .arg("--print-reply")
            .arg("/org/freedesktop/UDisks2")
            .arg("org.freedesktop.DBus.ObjectManager.GetManagedObjects"),
        "dbus-send UDisks2 GetManagedObjects",
    )?;
    require_success(&udisks, "dbus-send UDisks2 GetManagedObjects")?;
    require_dbus_free_output(&udisks, "dbus-send UDisks2 GetManagedObjects")?;
    if names_text.contains("org.freedesktop.UDisks2") {
        println!("PHASE6_UDISKS2_BUS_NAME=present");
    } else {
        println!("PHASE6_UDISKS2_BUS_NAME=activated_lazily");
    }
    println!("PHASE6_UDISKS2_OBJECTS=ok");

    Ok(())
}

fn check_solid_runtime() -> Result<(), String> {
    let tool = "/usr/bin/solid-hardware6";
    if !Path::new(tool).exists() {
        println!("PHASE6_SOLID_RUNTIME=blocked_missing_tool");
        println!("PHASE6_SOLID_TODO=solid-hardware6_not_present_in_image");
        return Ok(());
    }

    println!("{tool}");

    let output = command_output(
        Command::new("solid-hardware6").arg("list").arg("details"),
        "solid-hardware6 list details",
    )?;
    require_success(&output, "solid-hardware6 list details")?;
    require_dbus_free_output(&output, "solid-hardware6 list details")?;

    let text = output_text(&output);
    if contains_solid_consumer_surface(&text) {
        println!("PHASE6_SOLID_RUNTIME=checked");
    } else {
        println!("PHASE6_SOLID_RUNTIME=blocked_missing_storage_or_power_surface");
        println!("PHASE6_SOLID_TODO=solid-hardware6_did_not_expose_storage_or_power_surfaces");
    }

    Ok(())
}

fn check_redbear_info() -> Result<(), String> {
    let output = command_output(
        Command::new("redbear-info").arg("--json"),
        "redbear-info --json",
    )?;
    if !output.stdout.is_empty() {
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
    }
    require_success(&output, "redbear-info --json")
}

fn run() -> Result<(), String> {
    parse_args(PROGRAM, USAGE, std::env::args()).map_err(|err| {
        if err.is_empty() {
            process::exit(0);
        }
        err
    })?;

    println!("=== Red Bear OS Phase 6 KDE Runtime Check ===");
    require_path("/usr/bin/redbear-kde-session")?;
    require_path("/usr/bin/kwin_wayland")?;
    require_path("/usr/bin/dbus-daemon")?;
    require_path("/usr/bin/seatd")?;
    check_system_bus_consumers()?;
    check_solid_runtime()?;

    check_redbear_info()
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
    fn contains_dbus_error_detects_known_markers() {
        assert!(contains_dbus_error(
            "QDBusError(org.freedesktop.DBus.Error.ServiceUnknown, service missing)"
        ));
    }

    #[test]
    fn contains_dbus_error_ignores_clean_output() {
        assert!(!contains_dbus_error("method return time=0.0 sender=:1.2"));
    }

    #[test]
    fn contains_solid_consumer_surface_detects_storage_and_power_interfaces() {
        assert!(contains_solid_consumer_surface(
            "StorageDrive.removable = false\nBattery.percentage = 100"
        ));
    }

    #[test]
    fn contains_solid_consumer_surface_rejects_unrelated_output() {
        assert!(!contains_solid_consumer_surface("Processor.number = 8"));
    }
}
