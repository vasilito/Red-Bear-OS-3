/// Spawn a child USB class driver (hub, HID, storage).
/// On Redox, this forks and execs the driver binary with the USB device path.
#[cfg(feature = "std")]
pub fn spawn_usb_driver(driver_binary: &str, device_path: &str) {
    if driver_binary.is_empty()
        || device_path.is_empty()
        || !driver_binary.starts_with('/')
        || !device_path.starts_with('/')
        || !is_trusted_usb_driver(driver_binary)
    {
        return;
    }

    let mut command = std::process::Command::new(driver_binary);
    command.env_clear();
    command.stdin(std::process::Stdio::null());
    command.arg(device_path);
    let _ = command.spawn();
}

#[cfg(feature = "std")]
fn is_trusted_usb_driver(driver_binary: &str) -> bool {
    matches!(
        driver_binary,
        "/usr/bin/usbhubd" | "/usr/bin/usbhidd" | "/usr/bin/usbscsid"
    )
}

/// Spawn a child USB class driver (hub, HID, storage).
/// On no_std builds, class-driver spawning is not available.
#[cfg(not(feature = "std"))]
pub fn spawn_usb_driver(_driver_binary: &str, _device_path: &str) {}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::is_trusted_usb_driver;

    #[test]
    fn trusted_driver_whitelist_allows_expected_binaries() {
        assert!(is_trusted_usb_driver("/usr/bin/usbhubd"));
        assert!(is_trusted_usb_driver("/usr/bin/usbhidd"));
        assert!(is_trusted_usb_driver("/usr/bin/usbscsid"));
    }

    #[test]
    fn trusted_driver_whitelist_rejects_other_binaries() {
        assert!(!is_trusted_usb_driver("/usr/bin/sh"));
        assert!(!is_trusted_usb_driver("/tmp/usbhubd"));
        assert!(!is_trusted_usb_driver("relative/usbhubd"));
        assert!(!is_trusted_usb_driver(""));
    }
}
