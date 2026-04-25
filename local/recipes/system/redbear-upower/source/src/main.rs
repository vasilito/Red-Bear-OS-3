use std::{
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process,
    time::Duration,
};

use tokio::runtime::Builder as RuntimeBuilder;
use zbus::{
    Address,
    connection::Builder as ConnectionBuilder, interface, object_server::SignalEmitter,
    zvariant::OwnedObjectPath,
};

const BUS_NAME: &str = "org.freedesktop.UPower";
const UPOWER_PATH: &str = "/org/freedesktop/UPower";
const DISPLAY_DEVICE_PATH: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
const ACPI_POWER_ROOT: &str = "/scheme/acpi/power";

const DEVICE_KIND_UNKNOWN: u32 = 0;
const DEVICE_KIND_LINE_POWER: u32 = 1;
const DEVICE_KIND_BATTERY: u32 = 2;

const DEVICE_STATE_UNKNOWN: u32 = 0;
const DEVICE_STATE_CHARGING: u32 = 1;
const DEVICE_STATE_DISCHARGING: u32 = 2;
const DEVICE_STATE_EMPTY: u32 = 3;
const DEVICE_STATE_FULLY_CHARGED: u32 = 4;

const POLL_INTERVAL_SECS: u64 = 30;

#[derive(Debug, Clone)]
struct PowerRuntime {
    root: PathBuf,
    adapter_ids: Vec<String>,
    battery_ids: Vec<String>,
    object_paths: Vec<OwnedObjectPath>,
}

#[derive(Debug, Clone)]
struct UPowerDaemon {
    runtime: PowerRuntime,
}

#[derive(Debug, Clone)]
struct DisplayDevice {
    runtime: PowerRuntime,
}

#[derive(Debug, Clone)]
struct PowerDevice {
    runtime: PowerRuntime,
    descriptor: DeviceDescriptor,
}

#[derive(Debug, Clone)]
enum DeviceDescriptor {
    Adapter(String),
    Battery(String),
}

#[derive(Debug, Clone, PartialEq)]
struct AdapterState {
    native_path: String,
    online: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct BatteryState {
    native_path: String,
    state_bits: u64,
    percentage: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct PowerSnapshot {
    adapters: Vec<AdapterState>,
    batteries: Vec<BatteryState>,
}

enum Command {
    Run,
    Help,
}

fn usage() -> &'static str {
    "Usage: redbear-upower [--help]"
}

fn parse_args() -> Result<Command, String> {
    let mut args = env::args().skip(1);

    match args.next() {
        None => Ok(Command::Run),
        Some(arg) if arg == "--help" || arg == "-h" => {
            if args.next().is_some() {
                return Err(String::from("unexpected extra arguments after --help"));
            }

            Ok(Command::Help)
        }
        Some(arg) => Err(format!("unrecognized argument '{arg}'")),
    }
}

async fn wait_for_dbus_socket() {
    let socket_path = env::var("DBUS_STARTER_ADDRESS")
        .ok()
        .and_then(|addr| addr.strip_prefix("unix:path=").map(String::from))
        .unwrap_or_else(|| "/run/dbus/system_bus_socket".to_string());

    for _ in 0..30 {
        if tokio::net::UnixStream::connect(&socket_path).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    eprintln!("redbear-upower: timed out waiting for D-Bus socket at {socket_path}");
}

fn parse_object_path(path: &str) -> Result<OwnedObjectPath, Box<dyn Error>> {
    Ok(OwnedObjectPath::try_from(path.to_owned())?)
}

fn system_connection_builder() -> Result<ConnectionBuilder<'static>, Box<dyn Error>> {
    if let Ok(address) = env::var("DBUS_STARTER_ADDRESS") {
        Ok(ConnectionBuilder::address(Address::try_from(address.as_str())?)?)
    } else {
        Ok(ConnectionBuilder::address(Address::try_from("unix:path=/run/dbus/system_bus_socket")?)?)
    }
}

fn list_dir_names(path: &Path) -> Vec<String> {
    let mut names = match fs::read_dir(path) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect::<Vec<_>>(),
        Err(_) => Vec::new(),
    };
    names.sort();
    names
}

fn read_trimmed(path: impl AsRef<Path>) -> Option<String> {
    let value = fs::read_to_string(path).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn read_u64(path: impl AsRef<Path>) -> Option<u64> {
    read_trimmed(path)?.parse().ok()
}

fn read_f64(path: impl AsRef<Path>) -> Option<f64> {
    read_trimmed(path)?.parse().ok()
}

fn battery_state_to_upower(state_bits: u64, percentage: Option<f64>) -> u32 {
    if state_bits & 0x2 != 0 {
        return DEVICE_STATE_CHARGING;
    }
    if state_bits & 0x1 != 0 {
        return DEVICE_STATE_DISCHARGING;
    }
    if state_bits & 0x4 != 0 {
        return DEVICE_STATE_EMPTY;
    }
    if percentage.is_some_and(|value| value >= 99.0) {
        return DEVICE_STATE_FULLY_CHARGED;
    }

    DEVICE_STATE_UNKNOWN
}

fn adapter_object_path(id: &str) -> String {
    format!("/org/freedesktop/UPower/devices/line_power_{id}")
}

fn battery_object_path(id: &str) -> String {
    format!("/org/freedesktop/UPower/devices/battery_{id}")
}

impl PowerRuntime {
    fn discover() -> Result<Self, Box<dyn Error>> {
        let root = PathBuf::from(ACPI_POWER_ROOT);
        let adapter_ids = list_dir_names(&root.join("adapters"));
        let battery_ids = list_dir_names(&root.join("batteries"));

        let mut object_paths = Vec::with_capacity(adapter_ids.len() + battery_ids.len());
        for adapter_id in &adapter_ids {
            object_paths.push(parse_object_path(&adapter_object_path(adapter_id))?);
        }
        for battery_id in &battery_ids {
            object_paths.push(parse_object_path(&battery_object_path(battery_id))?);
        }

        Ok(Self {
            root,
            adapter_ids,
            battery_ids,
            object_paths,
        })
    }

    fn available(&self) -> bool {
        self.root.exists()
    }

    fn adapter_dir(&self, id: &str) -> PathBuf {
        self.root.join("adapters").join(id)
    }

    fn battery_dir(&self, id: &str) -> PathBuf {
        self.root.join("batteries").join(id)
    }

    fn read_adapter(&self, id: &str) -> Option<AdapterState> {
        let dir = self.adapter_dir(id);
        Some(AdapterState {
            native_path: read_trimmed(dir.join("path"))?,
            online: read_u64(dir.join("online")).map(|value| value != 0)?,
        })
    }

    fn read_battery(&self, id: &str) -> Option<BatteryState> {
        let dir = self.battery_dir(id);
        Some(BatteryState {
            native_path: read_trimmed(dir.join("path"))?,
            state_bits: read_u64(dir.join("state"))?,
            percentage: read_f64(dir.join("percentage")),
        })
    }

    fn snapshot(&self) -> PowerSnapshot {
        PowerSnapshot {
            adapters: self
                .adapter_ids
                .iter()
                .filter_map(|id| self.read_adapter(id))
                .collect(),
            batteries: self
                .battery_ids
                .iter()
                .filter_map(|id| self.read_battery(id))
                .collect(),
        }
    }
}

impl PowerSnapshot {
    fn on_battery(&self) -> bool {
        if self.adapters.iter().any(|adapter| adapter.online) {
            return false;
        }

        self.batteries
            .iter()
            .any(|battery| battery.state_bits & 0x1 != 0)
    }

    fn display_device_kind(&self) -> u32 {
        if self.batteries.is_empty() {
            DEVICE_KIND_UNKNOWN
        } else {
            DEVICE_KIND_BATTERY
        }
    }

    fn display_device_state(&self) -> u32 {
        if self.batteries.is_empty() {
            return DEVICE_STATE_UNKNOWN;
        }
        if self
            .batteries
            .iter()
            .any(|battery| battery.state_bits & 0x2 != 0)
        {
            return DEVICE_STATE_CHARGING;
        }
        if self
            .batteries
            .iter()
            .any(|battery| battery.state_bits & 0x1 != 0)
        {
            return DEVICE_STATE_DISCHARGING;
        }
        if self
            .batteries
            .iter()
            .any(|battery| battery.state_bits & 0x4 != 0)
        {
            return DEVICE_STATE_EMPTY;
        }

        let percentages = self
            .batteries
            .iter()
            .filter_map(|battery| battery.percentage)
            .collect::<Vec<_>>();
        if !percentages.is_empty() && percentages.iter().all(|value| *value >= 99.0) {
            return DEVICE_STATE_FULLY_CHARGED;
        }

        DEVICE_STATE_UNKNOWN
    }

    fn display_device_percentage(&self) -> f64 {
        let percentages = self
            .batteries
            .iter()
            .filter_map(|battery| battery.percentage)
            .collect::<Vec<_>>();

        if percentages.is_empty() {
            0.0
        } else {
            percentages.iter().sum::<f64>() / percentages.len() as f64
        }
    }

    fn display_device_present(&self) -> bool {
        !self.batteries.is_empty()
    }
}

#[interface(name = "org.freedesktop.UPower")]
impl UPowerDaemon {
    fn enumerate_devices(&self) -> Vec<OwnedObjectPath> {
        self.runtime.object_paths.clone()
    }

    fn get_critical_action(&self) -> String {
        String::from("PowerOff")
    }

    #[zbus(property(emits_changed_signal = "const"), name = "DaemonVersion")]
    fn daemon_version(&self) -> String {
        String::from("0.1.0")
    }

    #[zbus(property(emits_changed_signal = "false"), name = "OnBattery")]
    fn on_battery(&self) -> bool {
        self.runtime.snapshot().on_battery()
    }

    #[zbus(signal, name = "Changed")]
    async fn changed(signal_emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

#[interface(name = "org.freedesktop.UPower.Device")]
impl DisplayDevice {
    #[zbus(property(emits_changed_signal = "const"), name = "Type")]
    fn kind(&self) -> u32 {
        self.runtime.snapshot().display_device_kind()
    }

    #[zbus(property(emits_changed_signal = "const"), name = "State")]
    fn state(&self) -> u32 {
        self.runtime.snapshot().display_device_state()
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Percentage")]
    fn percentage(&self) -> f64 {
        self.runtime.snapshot().display_device_percentage()
    }

    #[zbus(property(emits_changed_signal = "const"), name = "IsPresent")]
    fn is_present(&self) -> bool {
        self.runtime.snapshot().display_device_present()
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Online")]
    fn online(&self) -> bool {
        false
    }
}

#[interface(name = "org.freedesktop.UPower.Device")]
impl PowerDevice {
    #[zbus(property(emits_changed_signal = "const"), name = "Type")]
    fn kind(&self) -> u32 {
        match self.descriptor {
            DeviceDescriptor::Adapter(_) => DEVICE_KIND_LINE_POWER,
            DeviceDescriptor::Battery(_) => DEVICE_KIND_BATTERY,
        }
    }

    #[zbus(property(emits_changed_signal = "const"), name = "State")]
    fn state(&self) -> u32 {
        match &self.descriptor {
            DeviceDescriptor::Adapter(_) => DEVICE_STATE_UNKNOWN,
            DeviceDescriptor::Battery(id) => self
                .runtime
                .read_battery(id)
                .map(|battery| battery_state_to_upower(battery.state_bits, battery.percentage))
                .unwrap_or(DEVICE_STATE_UNKNOWN),
        }
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Percentage")]
    fn percentage(&self) -> f64 {
        match &self.descriptor {
            DeviceDescriptor::Adapter(_) => 0.0,
            DeviceDescriptor::Battery(id) => self
                .runtime
                .read_battery(id)
                .and_then(|battery| battery.percentage)
                .unwrap_or(0.0),
        }
    }

    #[zbus(property(emits_changed_signal = "const"), name = "IsPresent")]
    fn is_present(&self) -> bool {
        match &self.descriptor {
            DeviceDescriptor::Adapter(id) => self.runtime.read_adapter(id).is_some(),
            DeviceDescriptor::Battery(id) => self.runtime.read_battery(id).is_some(),
        }
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Online")]
    fn online(&self) -> bool {
        match &self.descriptor {
            DeviceDescriptor::Adapter(id) => self
                .runtime
                .read_adapter(id)
                .map(|adapter| adapter.online)
                .unwrap_or(false),
            DeviceDescriptor::Battery(_) => false,
        }
    }

    #[zbus(property(emits_changed_signal = "const"), name = "NativePath")]
    fn native_path(&self) -> String {
        match &self.descriptor {
            DeviceDescriptor::Adapter(id) => self
                .runtime
                .read_adapter(id)
                .map(|adapter| adapter.native_path)
                .unwrap_or_default(),
            DeviceDescriptor::Battery(id) => self
                .runtime
                .read_battery(id)
                .map(|battery| battery.native_path)
                .unwrap_or_default(),
        }
    }
}

fn spawn_signal_handler(shutdown_tx: tokio::sync::watch::Sender<bool>) {
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};
            if let Ok(mut sigterm) = signal(SignalKind::terminate()) {
                tokio::select! {
                    _ = sigterm.recv() => {},
                    _ = tokio::signal::ctrl_c() => {},
                }
            } else {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }
        let _ = shutdown_tx.send(true);
    });
}

async fn run_daemon() -> Result<(), Box<dyn Error>> {
    wait_for_dbus_socket().await;
    let runtime = PowerRuntime::discover()?;
    if !runtime.available() {
        eprintln!(
            "redbear-upower: /scheme/acpi/power unavailable; serving empty provisional UPower surface"
        );
    }
    let _display_device_path = parse_object_path(DISPLAY_DEVICE_PATH)?;

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
    spawn_signal_handler(shutdown_tx);

    let mut last_err = None;
    for attempt in 1..=5 {
        let mut builder = system_connection_builder()?
            .name(BUS_NAME)?
            .serve_at(
                UPOWER_PATH,
                UPowerDaemon {
                    runtime: runtime.clone(),
                },
            )?
            .serve_at(
                DISPLAY_DEVICE_PATH,
                DisplayDevice {
                    runtime: runtime.clone(),
                },
            )?;

        for adapter_id in &runtime.adapter_ids {
            let path = adapter_object_path(adapter_id);
            builder = builder.serve_at(
                path,
                PowerDevice {
                    runtime: runtime.clone(),
                    descriptor: DeviceDescriptor::Adapter(adapter_id.clone()),
                },
            )?;
        }
        for battery_id in &runtime.battery_ids {
            let path = battery_object_path(battery_id);
            builder = builder.serve_at(
                path,
                PowerDevice {
                    runtime: runtime.clone(),
                    descriptor: DeviceDescriptor::Battery(battery_id.clone()),
                },
            )?;
        }

        match builder.build().await {
            Ok(connection) => {
                eprintln!("redbear-upower: registered {BUS_NAME} on the system bus");

                let upower_path = parse_object_path(UPOWER_PATH)?;
                let signal_emitter = SignalEmitter::new(&connection, upower_path)?;

                let mut last_snapshot = runtime.snapshot();
                let mut poll_interval =
                    tokio::time::interval(Duration::from_secs(POLL_INTERVAL_SECS));

                loop {
                    tokio::select! {
                        result = shutdown_rx.changed() => {
                            if result.is_err() {
                                eprintln!("redbear-upower: signal handler exited unexpectedly");
                            }
                            eprintln!("redbear-upower: shutdown signal received, exiting cleanly");
                            break;
                        }
                        _ = poll_interval.tick() => {
                            let current_snapshot = runtime.snapshot();
                            if current_snapshot != last_snapshot {
                                eprintln!(
                                    "redbear-upower: power state changed, emitting Changed signal"
                                );
                                let _ = UPowerDaemon::changed(&signal_emitter).await;
                                last_snapshot = current_snapshot;
                            }
                        }
                    }
                }

                drop(connection);
                return Ok(());
            }
            Err(err) => {
                if attempt < 5 {
                    eprintln!(
                        "redbear-upower: attempt {attempt}/5 failed ({err}), retrying in 2s..."
                    );
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
                last_err = Some(err.into());
            }
        }
    }
    Err(last_err.unwrap())
}

fn main() {
    match parse_args() {
        Ok(Command::Help) => {
            println!("{}", usage());
        }
        Ok(Command::Run) => {
            let runtime = match RuntimeBuilder::new_multi_thread().enable_all().build() {
                Ok(runtime) => runtime,
                Err(err) => {
                    eprintln!("redbear-upower: failed to create tokio runtime: {err}");
                    process::exit(1);
                }
            };

            if let Err(err) = runtime.block_on(run_daemon()) {
                eprintln!("redbear-upower: fatal error: {err}");
                process::exit(1);
            }
        }
        Err(err) => {
            eprintln!("redbear-upower: {err}");
            eprintln!("{}", usage());
            process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_state_prefers_charging_and_discharging_bits() {
        assert_eq!(battery_state_to_upower(0x2, Some(50.0)), DEVICE_STATE_CHARGING);
        assert_eq!(battery_state_to_upower(0x1, Some(50.0)), DEVICE_STATE_DISCHARGING);
    }

    #[test]
    fn battery_state_reports_full_when_percentage_is_high() {
        assert_eq!(battery_state_to_upower(0, Some(99.5)), DEVICE_STATE_FULLY_CHARGED);
        assert_eq!(battery_state_to_upower(0, None), DEVICE_STATE_UNKNOWN);
    }
}
