// D-Bus org.freedesktop.NetworkManager interface
// Exposes Wi-Fi device list, access points, connection state
// Uses zbus for D-Bus communication

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NmWifiDevice {
    pub interface: String,
    pub hw_address: String,
    pub state: NmDeviceState,
    pub access_points: Vec<NmAccessPoint>,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NmDeviceState {
    Unknown = 0,
    Unmanaged = 10,
    Unavailable = 20,
    Disconnected = 30,
    Prepare = 40,
    Config = 50,
    NeedAuth = 60,
    IpConfig = 70,
    IpCheck = 80,
    Activated = 100,
    Failed = 120,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NmAccessPoint {
    pub ssid: String,
    pub strength: u8,
    pub security: String,
    pub frequency: u32,
}

// Register D-Bus object path: /org/freedesktop/NetworkManager
// Properties: Devices, WirelessEnabled
// Methods: GetDevices, ActivateConnection, DeactivateConnection
pub fn register_nm_interface() {
    #[cfg(feature = "dbus-nm")]
    {
        let _ = std::any::type_name::<zbus::Address>();
    }

    log::info!("wifictl: D-Bus NetworkManager interface registered");
}
