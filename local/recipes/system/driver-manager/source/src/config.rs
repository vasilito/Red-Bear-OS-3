use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::Path;
use std::process::Command;
use std::string::String;
use std::sync::Mutex;
use std::vec::Vec;

use pcid_interface::PciFunctionHandle;
use redox_driver_core::device::DeviceInfo;
use redox_driver_core::driver::{Driver, DriverError, ProbeResult};
use redox_driver_core::r#match::DriverMatch;
use redox_driver_core::params::{DriverParams, ParamValue};

use serde::Deserialize;

#[derive(Debug)]
struct SpawnedDriver {
    pid: u32,
    bind_handle: File,
}

#[derive(Debug)]
pub struct DriverConfig {
    pub name: String,
    pub description: String,
    pub priority: i32,
    pub command: Vec<String>,
    pub matches: Vec<DriverMatch>,
    pub depends_on: Vec<String>,
    spawned: Mutex<HashMap<String, SpawnedDriver>>,
}

impl Clone for DriverConfig {
    fn clone(&self) -> Self {
        DriverConfig {
            name: self.name.clone(),
            description: self.description.clone(),
            priority: self.priority,
            command: self.command.clone(),
            matches: self.matches.clone(),
            depends_on: self.depends_on.clone(),
            spawned: Mutex::new(HashMap::new()),
        }
    }
}

#[derive(Deserialize)]
struct RawDriverMatch {
    vendor: Option<u16>,
    device: Option<u16>,
    class: Option<u8>,
    subclass: Option<u8>,
    prog_if: Option<u8>,
    subsystem_vendor: Option<u16>,
    subsystem_device: Option<u16>,
}

impl From<RawDriverMatch> for DriverMatch {
    fn from(r: RawDriverMatch) -> Self {
        DriverMatch {
            vendor: r.vendor,
            device: r.device,
            class: r.class,
            subclass: r.subclass,
            prog_if: r.prog_if,
            subsystem_vendor: r.subsystem_vendor,
            subsystem_device: r.subsystem_device,
        }
    }
}

impl DriverConfig {
    pub fn load_all(dir: &str) -> Result<Vec<DriverConfig>, String> {
        let entries = fs::read_dir(dir).map_err(|e| format!("read_dir failed: {}", e))?;

        let mut configs = Vec::new();

        for entry in entries {
            let entry = entry.map_err(|e| format!("entry error: {}", e))?;
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            let data = fs::read_to_string(&path)
                .map_err(|e| format!("read {} failed: {}", path.display(), e))?;

            let parsed: RawDriverToml = toml::from_str(&data)
                .map_err(|e| format!("parse {} failed: {}", path.display(), e))?;

            for driver in parsed.driver {
                let matches: Vec<DriverMatch> =
                    driver.r#match.into_iter().map(DriverMatch::from).collect();

                configs.push(DriverConfig {
                    name: driver.name,
                    description: driver.description,
                    priority: driver.priority,
                    command: driver.command,
                    matches,
                    depends_on: driver.depends_on,
                    spawned: Mutex::new(HashMap::new()),
                });
            }
        }

        configs.sort_by(|a, b| b.priority.cmp(&a.priority));
        Ok(configs)
    }
}

fn pci_device_path(info: &DeviceInfo) -> String {
    if info.raw_path.starts_with("/scheme/pci/") {
        info.raw_path.clone()
    } else {
        format!("/scheme/pci/{}", info.id.path)
    }
}

fn claim_pci_device(info: &DeviceInfo) -> Result<(String, File), ProbeResult> {
    let device_path = pci_device_path(info);
    let bind_path = format!("{}/bind", device_path);

    match OpenOptions::new().read(true).write(true).open(&bind_path) {
        Ok(bind_handle) => Ok((device_path, bind_handle)),
        Err(err) => match err.raw_os_error() {
            Some(code) if code == syscall::EALREADY as i32 || code == 114 => {
                log::debug!("device {} already claimed via {}", info.id.path, bind_path);
                Err(ProbeResult::NotSupported)
            }
            _ => Err(ProbeResult::Deferred {
                reason: format!("bind {} failed: {}", bind_path, err),
            }),
        },
    }
}

fn open_pcid_channel(device_path: &str) -> Result<OwnedFd, ProbeResult> {
    let mut handle = match PciFunctionHandle::connect_by_path(Path::new(device_path)) {
        Ok(handle) => handle,
        Err(err) => {
            return Err(ProbeResult::Deferred {
                reason: format!("open channel for {} failed: {}", device_path, err),
            });
        }
    };

    handle.enable_device();

    let channel_fd = handle.into_inner_fd();
    let channel_fd = unsafe { OwnedFd::from_raw_fd(channel_fd) };
    Ok(channel_fd)
}

fn check_scheme_available(name: &str) -> bool {
    if std::path::Path::new(&format!("/scheme/{}", name)).exists() {
        return true;
    }
    false
}

impl Driver for DriverConfig {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn priority(&self) -> i32 {
        self.priority
    }

    fn match_table(&self) -> &[DriverMatch] {
        &self.matches
    }

    fn probe(&self, info: &DeviceInfo) -> ProbeResult {
        let device_key = info.id.path.clone();

        {
            let spawned = self.spawned.lock().unwrap();
            if spawned.contains_key(&device_key) {
                log::debug!("driver {} already bound to {}", self.name, device_key);
                return ProbeResult::Bound;
            }
        }

        if self.command.is_empty() {
            return ProbeResult::Fatal {
                reason: String::from("empty command"),
            };
        }

        let actual_path = if self.command[0].starts_with('/') {
            self.command[0].clone()
        } else {
            format!("/usr/lib/drivers/{}", self.command[0])
        };

        if !std::path::Path::new(&actual_path).exists() {
            return ProbeResult::Deferred {
                reason: format!("driver binary not found: {}", actual_path),
            };
        }

        let deps: Vec<String> = if !self.depends_on.is_empty() {
            self.depends_on.clone()
        } else {
            guess_dependencies(&self.name)
        };
        for dep in &deps {
            if !check_scheme_available(dep) {
                return ProbeResult::Deferred {
                    reason: format!("dependency scheme not ready: {}", dep),
                };
            }
        }

        log::info!("probing {} with driver {}", device_key, self.name);

        let (device_path, bind_handle) = match claim_pci_device(info) {
            Ok(claimed) => claimed,
            Err(result) => return result,
        };

        let channel_fd = match open_pcid_channel(&device_path) {
            Ok(channel_fd) => channel_fd,
            Err(result) => return result,
        };

        let mut cmd = Command::new(&actual_path);
        for arg in &self.command[1..] {
            cmd.arg(arg);
        }

        cmd.env("PCID_CLIENT_CHANNEL", channel_fd.as_raw_fd().to_string());
        cmd.env("PCID_DEVICE_PATH", &device_path);

        match cmd.spawn() {
            Ok(child) => {
                let pid = child.id();
                log::info!(
                    "driver {} spawned (pid {}) for device {}",
                    self.name,
                    pid,
                    device_key
                );
                let mut spawned = self.spawned.lock().unwrap();
                spawned.insert(device_key, SpawnedDriver { pid, bind_handle });
                ProbeResult::Bound
            }
            Err(e) => ProbeResult::Fatal {
                reason: format!("spawn failed: {}", e),
            },
        }
    }

    fn remove(&self, info: &DeviceInfo) -> Result<(), DriverError> {
        let device_key = info.id.path.clone();
        let binding = {
            let mut spawned = self.spawned.lock().unwrap();
            spawned.remove(&device_key)
        };

        match binding {
            Some(binding) => {
                let bind_fd = binding.bind_handle.as_raw_fd();
                log::info!(
                    "unbound: device {} from driver {} (pid {}, bind fd {})",
                    device_key,
                    self.name,
                    binding.pid,
                    bind_fd
                );
                Ok(())
            }
            _ => {
                log::warn!("driver {} not bound to device {}", self.name, device_key);
                Err(DriverError::Other("not bound"))
            }
        }
    }

    fn params(&self) -> DriverParams {
        let mut p = DriverParams::new();
        p.define(
            "enabled",
            "Whether this driver is active",
            ParamValue::Bool(true),
            true,
        );
        p.define(
            "priority",
            "Probe priority (higher = earlier)",
            ParamValue::Int(self.priority as i64),
            false,
        );
        p
    }
}

/// Driver-specified dependencies. Parsed from [driver.depends] TOML field.
/// Example: depends_on = ["pci", "acpi"]
/// When specified, takes precedence over guess_dependencies().
fn guess_dependencies(driver_name: &str) -> Vec<String> {
    match driver_name {
        "xhcid" | "usbhubd" | "usbctl" | "usbhidd" | "usbscsid" => {
            vec![String::from("pci")]
        }
        "nvmed" | "ahcid" | "ided" | "virtio-blkd" => {
            vec![String::from("pci")]
        }
        "e1000d" | "rtl8168d" | "rtl8139d" | "ixgbed" | "virtio-netd" => {
            vec![String::from("pci")]
        }
        "vesad" | "virtio-gpud" | "redox-drm" => {
            vec![String::from("pci")]
        }
        "ihdad" | "ac97d" | "sb16d" => {
            vec![String::from("pci")]
        }
        "ps2d" => vec![String::from("serio")],
        "i2c-hidd" => vec![String::from("i2c")],
        "dw-acpi-i2cd" | "amd-mp2-i2cd" | "intel-lpss-i2cd" => {
            vec![String::from("acpi"), String::from("i2c")]
        }
        _ => vec![String::from("pci")],
    }
}

#[derive(Deserialize)]
struct RawDriverToml {
    driver: Vec<RawDriverEntry>,
}

#[derive(Deserialize)]
struct RawDriverEntry {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    priority: i32,
    #[serde(default)]
    command: Vec<String>,
    #[serde(rename = "match")]
    r#match: Vec<RawDriverMatch>,
    #[serde(default)]
    depends_on: Vec<String>,
}
