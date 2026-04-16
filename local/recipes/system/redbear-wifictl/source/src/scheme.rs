use std::collections::BTreeMap;

use redox_scheme::scheme::SchemeSync;
use redox_scheme::{CallerCtx, OpenResult};
use syscall::error::{Error, Result, EACCES, EBADF, EINVAL, ENOENT, EROFS};
use syscall::flag::{EventFlags, MODE_DIR, MODE_FILE};
use syscall::schemev2::NewFdFlags;
use syscall::Stat;

use crate::backend::{Backend, InterfaceState, WifiStatus};

const SCHEME_ROOT_ID: usize = 1;

#[derive(Clone)]
enum HandleKind {
    Root,
    Ifaces,
    Interface(String),
    Capabilities,
    Status(String),
    LinkState(String),
    FirmwareStatus(String),
    TransportStatus(String),
    TransportInitStatus(String),
    ActivationStatus(String),
    ConnectResult(String),
    DisconnectResult(String),
    ScanResults(String),
    LastError(String),
    Ssid(String),
    Security(String),
    Key(String),
    Scan(String),
    Prepare(String),
    TransportProbe(String),
    InitTransport(String),
    ActivateNic(String),
    Connect(String),
    Disconnect(String),
    Retry(String),
}

pub struct WifiCtlScheme {
    backend: Box<dyn Backend>,
    next_id: usize,
    handles: BTreeMap<usize, HandleKind>,
    states: BTreeMap<String, InterfaceState>,
}

impl WifiCtlScheme {
    pub fn new(backend: Box<dyn Backend>) -> Self {
        let mut states = BTreeMap::new();
        for iface in backend.interfaces() {
            states.insert(
                iface.clone(),
                InterfaceState {
                    status: backend.initial_status(&iface).as_str().to_string(),
                    link_state: backend.initial_link_state(&iface),
                    firmware_status: backend.firmware_status(&iface),
                    transport_status: backend.transport_status(&iface),
                    transport_init_status: "transport_init=not-run".to_string(),
                    activation_status: "activation=not-run".to_string(),
                    connect_result: backend.connect_result(&iface),
                    disconnect_result: backend.disconnect_result(&iface),
                    scan_results: backend.default_scan_results(&iface),
                    ..Default::default()
                },
            );
        }

        Self {
            backend,
            next_id: SCHEME_ROOT_ID + 1,
            handles: BTreeMap::new(),
            states,
        }
    }

    fn alloc_handle(&mut self, kind: HandleKind) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.handles.insert(id, kind);
        id
    }

    fn handle(&self, id: usize) -> Result<&HandleKind> {
        self.handles.get(&id).ok_or(Error::new(EBADF))
    }

    fn state(&self, iface: &str) -> Result<&InterfaceState> {
        self.states.get(iface).ok_or(Error::new(ENOENT))
    }

    fn state_mut(&mut self, iface: &str) -> Result<&mut InterfaceState> {
        self.states.get_mut(iface).ok_or(Error::new(ENOENT))
    }

    fn read_handle(&self, kind: &HandleKind) -> Result<String> {
        Ok(match kind {
            HandleKind::Root => "ifaces\ncapabilities\n".to_string(),
            HandleKind::Ifaces => self.states.keys().cloned().collect::<Vec<_>>().join("\n") + "\n",
            HandleKind::Interface(_) => {
                "status\nlink-state\nfirmware-status\ntransport-status\ntransport-init-status\nactivation-status\nconnect-result\ndisconnect-result\nscan-results\nlast-error\nssid\nsecurity\nkey\nscan\nprepare\ntransport-probe\ninit-transport\nactivate-nic\nconnect\ndisconnect\nretry\n"
                    .to_string()
            }
            HandleKind::Capabilities => self.backend.capabilities().join("\n") + "\n",
            HandleKind::Status(iface) => {
                let state = self.state(iface)?;
                format!(
                    "status={}\nlink_state={}\nfirmware_status={}\ntransport_status={}\ntransport_init_status={}\nactivation_status={}\nconnect_result={}\ndisconnect_result={}\nssid={}\nsecurity={}\n",
                    state.status,
                    state.link_state,
                    state.firmware_status,
                    state.transport_status,
                    state.transport_init_status,
                    state.activation_status,
                    state.connect_result,
                    state.disconnect_result,
                    state.ssid,
                    state.security
                )
            }
            HandleKind::LinkState(iface) => format!("{}\n", self.state(iface)?.link_state),
            HandleKind::FirmwareStatus(iface) => format!("{}\n", self.state(iface)?.firmware_status),
            HandleKind::TransportStatus(iface) => {
                format!("{}\n", self.state(iface)?.transport_status)
            }
            HandleKind::TransportInitStatus(iface) => {
                format!("{}\n", self.state(iface)?.transport_init_status)
            }
            HandleKind::ActivationStatus(iface) => {
                format!("{}\n", self.state(iface)?.activation_status)
            }
            HandleKind::ConnectResult(iface) => format!("{}\n", self.state(iface)?.connect_result),
            HandleKind::DisconnectResult(iface) => {
                format!("{}\n", self.state(iface)?.disconnect_result)
            }
            HandleKind::ScanResults(iface) => self.state(iface)?.scan_results.join("\n") + "\n",
            HandleKind::LastError(iface) => format!("{}\n", self.state(iface)?.last_error),
            HandleKind::Ssid(iface) => format!("{}\n", self.state(iface)?.ssid),
            HandleKind::Security(iface) => format!("{}\n", self.state(iface)?.security),
            HandleKind::Key(_iface) => "[redacted]\n".to_string(),
            HandleKind::Scan(_)
            | HandleKind::TransportProbe(_)
            | HandleKind::InitTransport(_)
            | HandleKind::ActivateNic(_)
            | HandleKind::Retry(_)
            | HandleKind::Prepare(_)
            | HandleKind::Connect(_)
            | HandleKind::Disconnect(_) => String::new(),
        })
    }

    fn link_state_for_status(status: &WifiStatus) -> &'static str {
        match status {
            WifiStatus::Connected => "link=connected",
            WifiStatus::Associating => "link=associating",
            WifiStatus::Scanning => "link=scanning",
            WifiStatus::FirmwareReady | WifiStatus::DeviceDetected => "link=down",
            WifiStatus::Down => "link=down",
            WifiStatus::Failed => "link=down",
        }
    }

    fn apply_connect_outcome(
        &mut self,
        iface: &str,
        status: WifiStatus,
        firmware_status: String,
        transport_status: String,
        connect_result: String,
        disconnect_result: String,
    ) -> Result<()> {
        let state = self.state_mut(iface)?;
        state.status = status.as_str().to_string();
        state.link_state = Self::link_state_for_status(&status).to_string();
        state.firmware_status = firmware_status;
        state.transport_status = transport_status;
        state.connect_result = connect_result;
        state.disconnect_result = disconnect_result;
        Ok(())
    }
}

impl SchemeSync for WifiCtlScheme {
    fn scheme_root(&mut self) -> Result<usize> {
        Ok(SCHEME_ROOT_ID)
    }

    fn openat(
        &mut self,
        dirfd: usize,
        path: &str,
        _flags: usize,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<OpenResult> {
        let kind = if dirfd == SCHEME_ROOT_ID {
            match path.trim_matches('/') {
                "" => HandleKind::Root,
                "ifaces" => HandleKind::Ifaces,
                "capabilities" => HandleKind::Capabilities,
                _ => return Err(Error::new(ENOENT)),
            }
        } else {
            match self.handle(dirfd)? {
                HandleKind::Ifaces => {
                    let iface = path.trim_matches('/');
                    self.state(iface)?;
                    HandleKind::Interface(iface.to_string())
                }
                HandleKind::Interface(iface) => match path.trim_matches('/') {
                    "status" => HandleKind::Status(iface.clone()),
                    "link-state" => HandleKind::LinkState(iface.clone()),
                    "firmware-status" => HandleKind::FirmwareStatus(iface.clone()),
                    "transport-status" => HandleKind::TransportStatus(iface.clone()),
                    "transport-init-status" => HandleKind::TransportInitStatus(iface.clone()),
                    "activation-status" => HandleKind::ActivationStatus(iface.clone()),
                    "connect-result" => HandleKind::ConnectResult(iface.clone()),
                    "disconnect-result" => HandleKind::DisconnectResult(iface.clone()),
                    "scan-results" => HandleKind::ScanResults(iface.clone()),
                    "last-error" => HandleKind::LastError(iface.clone()),
                    "ssid" => HandleKind::Ssid(iface.clone()),
                    "security" => HandleKind::Security(iface.clone()),
                    "key" => HandleKind::Key(iface.clone()),
                    "scan" => HandleKind::Scan(iface.clone()),
                    "prepare" => HandleKind::Prepare(iface.clone()),
                    "transport-probe" => HandleKind::TransportProbe(iface.clone()),
                    "init-transport" => HandleKind::InitTransport(iface.clone()),
                    "activate-nic" => HandleKind::ActivateNic(iface.clone()),
                    "connect" => HandleKind::Connect(iface.clone()),
                    "disconnect" => HandleKind::Disconnect(iface.clone()),
                    "retry" => HandleKind::Retry(iface.clone()),
                    _ => return Err(Error::new(ENOENT)),
                },
                _ => return Err(Error::new(EACCES)),
            }
        };

        Ok(OpenResult::ThisScheme {
            number: self.alloc_handle(kind),
            flags: NewFdFlags::empty(),
        })
    }

    fn read(
        &mut self,
        id: usize,
        buf: &mut [u8],
        offset: u64,
        _flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let data = self.read_handle(self.handle(id)?)?;
        let bytes = data.as_bytes();
        let offset = usize::try_from(offset).map_err(|_| Error::new(EINVAL))?;
        if offset >= bytes.len() {
            return Ok(0);
        }
        let count = (bytes.len() - offset).min(buf.len());
        buf[..count].copy_from_slice(&bytes[offset..offset + count]);
        Ok(count)
    }

    fn write(
        &mut self,
        id: usize,
        buf: &[u8],
        _offset: u64,
        _flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let value = std::str::from_utf8(buf)
            .map_err(|_| Error::new(EINVAL))?
            .trim()
            .to_string();

        match self.handle(id)?.clone() {
            HandleKind::Ssid(iface) => self.state_mut(&iface)?.ssid = value,
            HandleKind::Security(iface) => self.state_mut(&iface)?.security = value,
            HandleKind::Key(iface) => self.state_mut(&iface)?.key = value,
            HandleKind::Scan(iface) => {
                let results = match self.backend.scan(&iface) {
                    Ok(results) => results,
                    Err(err) => {
                        let firmware_status = self.backend.firmware_status(&iface);
                        let transport_status = self.backend.transport_status(&iface);
                        let state = self.state_mut(&iface)?;
                        state.last_error = err;
                        state.status = WifiStatus::Failed.as_str().to_string();
                        state.link_state = "link=down".to_string();
                        state.firmware_status = firmware_status;
                        state.transport_status = transport_status;
                        return Ok(buf.len());
                    }
                };
                let firmware_status = self.backend.firmware_status(&iface);
                let transport_status = self.backend.transport_status(&iface);
                let state = self.state_mut(&iface)?;
                state.status = WifiStatus::Scanning.as_str().to_string();
                state.link_state = "link=scanning".to_string();
                state.firmware_status = firmware_status;
                state.transport_status = transport_status;
                state.scan_results = results;
                state.last_error.clear();
            }
            HandleKind::Prepare(iface) => {
                let status = match self.backend.prepare(&iface) {
                    Ok(status) => status,
                    Err(err) => {
                        let firmware_status = self.backend.firmware_status(&iface);
                        let transport_status = self.backend.transport_status(&iface);
                        let state = self.state_mut(&iface)?;
                        state.last_error = err;
                        state.status = WifiStatus::Failed.as_str().to_string();
                        state.link_state = "link=down".to_string();
                        state.firmware_status = firmware_status;
                        state.transport_status = transport_status;
                        return Ok(buf.len());
                    }
                };
                let firmware_status = self.backend.firmware_status(&iface);
                let transport_status = self.backend.transport_status(&iface);
                let state = self.state_mut(&iface)?;
                state.status = status.as_str().to_string();
                state.link_state = "link=prepared".to_string();
                state.firmware_status = firmware_status;
                state.transport_status = transport_status;
                state.transport_init_status = "transport_init=not-run".to_string();
            }
            HandleKind::TransportProbe(iface) => {
                let transport_status = match self.backend.transport_probe(&iface) {
                    Ok(status) => status,
                    Err(err) => {
                        let firmware_status = self.backend.firmware_status(&iface);
                        let state = self.state_mut(&iface)?;
                        state.last_error = err;
                        state.status = WifiStatus::Failed.as_str().to_string();
                        state.link_state = "link=down".to_string();
                        state.firmware_status = firmware_status;
                        return Ok(buf.len());
                    }
                };
                let state = self.state_mut(&iface)?;
                state.transport_status = transport_status;
            }
            HandleKind::InitTransport(iface) => {
                let transport_init_status = match self.backend.init_transport(&iface) {
                    Ok(status) => status,
                    Err(err) => {
                        let firmware_status = self.backend.firmware_status(&iface);
                        let transport_status = self.backend.transport_status(&iface);
                        let state = self.state_mut(&iface)?;
                        state.last_error = err;
                        state.status = WifiStatus::Failed.as_str().to_string();
                        state.link_state = "link=down".to_string();
                        state.firmware_status = firmware_status;
                        state.transport_status = transport_status;
                        state.transport_init_status = "transport_init=failed".to_string();
                        return Ok(buf.len());
                    }
                };
                let state = self.state_mut(&iface)?;
                state.transport_init_status = transport_init_status;
                state.link_state = "link=transport-initialized".to_string();
                state.activation_status = "activation=not-run".to_string();
            }
            HandleKind::ActivateNic(iface) => {
                let activation_status = match self.backend.activate(&iface) {
                    Ok(status) => status,
                    Err(err) => {
                        let firmware_status = self.backend.firmware_status(&iface);
                        let transport_status = self.backend.transport_status(&iface);
                        let state = self.state_mut(&iface)?;
                        state.last_error = err;
                        state.status = WifiStatus::Failed.as_str().to_string();
                        state.link_state = "link=down".to_string();
                        state.firmware_status = firmware_status;
                        state.transport_status = transport_status;
                        state.activation_status = "activation=failed".to_string();
                        return Ok(buf.len());
                    }
                };
                let connect_result = self.backend.connect_result(&iface);
                let disconnect_result = self.backend.disconnect_result(&iface);
                let state = self.state_mut(&iface)?;
                state.activation_status = activation_status;
                state.link_state = "link=nic-active".to_string();
                state.connect_result = connect_result;
                state.disconnect_result = disconnect_result;
            }
            HandleKind::Connect(iface) => {
                let snapshot = self.state(&iface)?.clone();
                let new_status = match self.backend.connect(&iface, &snapshot) {
                    Ok(status) => status,
                    Err(err) => {
                        let firmware_status = self.backend.firmware_status(&iface);
                        let transport_status = self.backend.transport_status(&iface);
                        let state = self.state_mut(&iface)?;
                        state.last_error = err;
                        state.status = WifiStatus::Failed.as_str().to_string();
                        state.link_state = "link=down".to_string();
                        state.firmware_status = firmware_status;
                        state.transport_status = transport_status;
                        state.transport_init_status = "transport_init=failed".to_string();
                        state.activation_status = "activation=failed".to_string();
                        return Ok(buf.len());
                    }
                };
                let firmware_status = self.backend.firmware_status(&iface);
                let transport_status = self.backend.transport_status(&iface);
                let connect_result = self.backend.connect_result(&iface);
                let disconnect_result = self.backend.disconnect_result(&iface);
                self.apply_connect_outcome(
                    &iface,
                    new_status,
                    firmware_status,
                    transport_status,
                    connect_result,
                    disconnect_result,
                )?;
            }
            HandleKind::Disconnect(iface) => {
                let status = match self.backend.disconnect(&iface) {
                    Ok(status) => status,
                    Err(err) => {
                        let firmware_status = self.backend.firmware_status(&iface);
                        let transport_status = self.backend.transport_status(&iface);
                        let state = self.state_mut(&iface)?;
                        state.last_error = err;
                        state.status = WifiStatus::Failed.as_str().to_string();
                        state.link_state = "link=down".to_string();
                        state.firmware_status = firmware_status;
                        state.transport_status = transport_status;
                        state.activation_status = "activation=failed".to_string();
                        return Ok(buf.len());
                    }
                };
                let firmware_status = self.backend.firmware_status(&iface);
                let transport_status = self.backend.transport_status(&iface);
                let disconnect_result = self.backend.disconnect_result(&iface);
                let state = self.state_mut(&iface)?;
                state.status = status.as_str().to_string();
                state.link_state = "link=down".to_string();
                state.firmware_status = firmware_status;
                state.transport_status = transport_status;
                state.disconnect_result = disconnect_result;
            }
            HandleKind::Retry(iface) => {
                let status = match self.backend.retry(&iface) {
                    Ok(status) => status,
                    Err(err) => {
                        let firmware_status = self.backend.firmware_status(&iface);
                        let transport_status = self.backend.transport_status(&iface);
                        let state = self.state_mut(&iface)?;
                        state.last_error = err;
                        state.status = WifiStatus::Failed.as_str().to_string();
                        state.link_state = "link=retry-failed".to_string();
                        state.firmware_status = firmware_status;
                        state.transport_status = transport_status;
                        state.activation_status = "activation=failed".to_string();
                        return Ok(buf.len());
                    }
                };
                let state = self.state_mut(&iface)?;
                state.status = status.as_str().to_string();
                state.link_state = "link=retrying".to_string();
            }
            _ => return Err(Error::new(EROFS)),
        }

        Ok(buf.len())
    }

    fn fstat(&mut self, id: usize, stat: &mut Stat, _ctx: &CallerCtx) -> Result<()> {
        let kind = self.handle(id)?;
        stat.st_mode = match kind {
            HandleKind::Root | HandleKind::Ifaces | HandleKind::Interface(_) => MODE_DIR | 0o755,
            HandleKind::Connect(_)
            | HandleKind::Disconnect(_)
            | HandleKind::Scan(_)
            | HandleKind::TransportProbe(_)
            | HandleKind::InitTransport(_)
            | HandleKind::Retry(_)
            | HandleKind::Prepare(_)
            | HandleKind::Ssid(_)
            | HandleKind::Security(_)
            | HandleKind::Key(_) => MODE_FILE | 0o644,
            _ => MODE_FILE | 0o444,
        };
        Ok(())
    }

    fn fsync(&mut self, id: usize, _ctx: &CallerCtx) -> Result<()> {
        let _ = self.handle(id)?;
        Ok(())
    }

    fn fpath(&mut self, id: usize, buf: &mut [u8], _ctx: &CallerCtx) -> Result<usize> {
        let path = match self.handle(id)? {
            HandleKind::Root => "wifictl:/".to_string(),
            HandleKind::Ifaces => "wifictl:/ifaces".to_string(),
            HandleKind::Interface(iface) => format!("wifictl:/ifaces/{iface}"),
            HandleKind::Capabilities => "wifictl:/capabilities".to_string(),
            HandleKind::Status(iface) => format!("wifictl:/ifaces/{iface}/status"),
            HandleKind::LinkState(iface) => format!("wifictl:/ifaces/{iface}/link-state"),
            HandleKind::FirmwareStatus(iface) => format!("wifictl:/ifaces/{iface}/firmware-status"),
            HandleKind::TransportStatus(iface) => {
                format!("wifictl:/ifaces/{iface}/transport-status")
            }
            HandleKind::TransportInitStatus(iface) => {
                format!("wifictl:/ifaces/{iface}/transport-init-status")
            }
            HandleKind::ActivationStatus(iface) => {
                format!("wifictl:/ifaces/{iface}/activation-status")
            }
            HandleKind::ConnectResult(iface) => format!("wifictl:/ifaces/{iface}/connect-result"),
            HandleKind::DisconnectResult(iface) => {
                format!("wifictl:/ifaces/{iface}/disconnect-result")
            }
            HandleKind::ScanResults(iface) => format!("wifictl:/ifaces/{iface}/scan-results"),
            HandleKind::LastError(iface) => format!("wifictl:/ifaces/{iface}/last-error"),
            HandleKind::Ssid(iface) => format!("wifictl:/ifaces/{iface}/ssid"),
            HandleKind::Security(iface) => format!("wifictl:/ifaces/{iface}/security"),
            HandleKind::Key(iface) => format!("wifictl:/ifaces/{iface}/key"),
            HandleKind::Scan(iface) => format!("wifictl:/ifaces/{iface}/scan"),
            HandleKind::Prepare(iface) => format!("wifictl:/ifaces/{iface}/prepare"),
            HandleKind::TransportProbe(iface) => format!("wifictl:/ifaces/{iface}/transport-probe"),
            HandleKind::InitTransport(iface) => format!("wifictl:/ifaces/{iface}/init-transport"),
            HandleKind::ActivateNic(iface) => format!("wifictl:/ifaces/{iface}/activate-nic"),
            HandleKind::Connect(iface) => format!("wifictl:/ifaces/{iface}/connect"),
            HandleKind::Disconnect(iface) => format!("wifictl:/ifaces/{iface}/disconnect"),
            HandleKind::Retry(iface) => format!("wifictl:/ifaces/{iface}/retry"),
        };
        let bytes = path.as_bytes();
        let count = bytes.len().min(buf.len());
        buf[..count].copy_from_slice(&bytes[..count]);
        Ok(count)
    }

    fn fevent(&mut self, id: usize, _flags: EventFlags, _ctx: &CallerCtx) -> Result<EventFlags> {
        let _ = self.handle(id)?;
        Ok(EventFlags::empty())
    }

    fn on_close(&mut self, id: usize) {
        if id != SCHEME_ROOT_ID {
            self.handles.remove(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{IntelBackend, StubBackend, TEST_ENV_LOCK};
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!("{prefix}-{stamp}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn status_updates_after_connect_and_disconnect() {
        let mut scheme = WifiCtlScheme::new(Box::new(StubBackend::from_env()));
        let iface = "wlan0".to_string();

        {
            let state = scheme.state_mut(&iface).unwrap();
            state.ssid = "demo-ssid".to_string();
            state.security = "wpa2-psk".to_string();
            state.key = "secret".to_string();
        }

        let snapshot = scheme.state(&iface).unwrap().clone();
        let status = scheme.backend.connect(&iface, &snapshot).unwrap();
        scheme
            .apply_connect_outcome(
                &iface,
                status,
                scheme.backend.firmware_status(&iface),
                scheme.backend.transport_status(&iface),
                scheme.backend.connect_result(&iface),
                scheme.backend.disconnect_result(&iface),
            )
            .unwrap();
        assert_eq!(scheme.state(&iface).unwrap().status, "connected");
        assert_eq!(scheme.state(&iface).unwrap().link_state, "link=connected");

        let status = scheme.backend.disconnect(&iface).unwrap();
        scheme.state_mut(&iface).unwrap().status = status.as_str().to_string();
        assert_eq!(scheme.state(&iface).unwrap().status, "device-detected");
    }

    #[test]
    fn apply_connect_outcome_preserves_pending_link_state() {
        let mut scheme = WifiCtlScheme::new(Box::new(StubBackend::from_env()));
        let iface = "wlan0".to_string();

        scheme
            .apply_connect_outcome(
                &iface,
                WifiStatus::Associating,
                "firmware=present".to_string(),
                "transport=active".to_string(),
                "connect_result=host-bounded-pending ssid=demo security=wpa2-psk".to_string(),
                "disconnect_result=not-run".to_string(),
            )
            .unwrap();

        let state = scheme.state(&iface).unwrap();
        assert_eq!(state.status, "associating");
        assert_eq!(state.link_state, "link=associating");
        assert!(state.connect_result.contains("host-bounded-pending"));
    }

    #[test]
    fn stub_prepare_marks_firmware_ready() {
        let mut scheme = WifiCtlScheme::new(Box::new(StubBackend::from_env()));
        let iface = "wlan0".to_string();

        let status = scheme.backend.prepare(&iface).unwrap();
        let firmware_status = scheme.backend.firmware_status(&iface);
        let state = scheme.state_mut(&iface).unwrap();
        state.status = status.as_str().to_string();
        state.firmware_status = firmware_status;

        assert_eq!(scheme.state(&iface).unwrap().status, "firmware-ready");
        assert_eq!(
            scheme.state(&iface).unwrap().firmware_status,
            "firmware=stub"
        );
        assert_eq!(
            scheme.state(&iface).unwrap().transport_status,
            "transport=stub"
        );
        assert_eq!(
            scheme.state(&iface).unwrap().transport_init_status,
            "transport_init=not-run"
        );
    }

    #[test]
    fn stub_scan_updates_scan_results() {
        let mut scheme = WifiCtlScheme::new(Box::new(StubBackend::from_env()));
        let iface = "wlan0".to_string();

        let results = scheme.backend.scan(&iface).unwrap();
        let state = scheme.state_mut(&iface).unwrap();
        state.status = WifiStatus::Scanning.as_str().to_string();
        state.scan_results = results;

        assert_eq!(scheme.state(&iface).unwrap().status, "scanning");
        assert_eq!(
            scheme.state(&iface).unwrap().scan_results,
            vec!["demo-ssid".to_string(), "demo-open".to_string()]
        );
    }

    #[test]
    fn intel_prepare_failure_records_last_error() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let pci = temp_root("rbos-wifictl-pci-missing");
        let firmware = temp_root("rbos-wifictl-fw-missing");
        let slot = pci.join("0000--00--14.3");
        fs::create_dir_all(&slot).unwrap();
        let mut cfg = vec![0u8; 64];
        cfg[0x00] = 0x86;
        cfg[0x01] = 0x80;
        cfg[0x02] = 0x40;
        cfg[0x03] = 0x77;
        cfg[0x0A] = 0x80;
        cfg[0x0B] = 0x02;
        cfg[0x04] = 0x06;
        cfg[0x10] = 0x01;
        cfg[0x2E] = 0x90;
        cfg[0x2F] = 0x40;
        cfg[0x3D] = 0x01;
        fs::write(slot.join("config"), cfg).unwrap();

        unsafe {
            env::set_var("REDBEAR_WIFICTL_PCI_ROOT", &pci);
            env::set_var("REDBEAR_WIFICTL_FIRMWARE_ROOT", &firmware);
            env::remove_var("REDBEAR_IWLWIFI_CMD");
        }

        let mut scheme = WifiCtlScheme::new(Box::new(IntelBackend::from_env()));
        let iface = "wlan0".to_string();

        let err = scheme.backend.prepare(&iface).unwrap_err();
        let firmware_status = scheme.backend.firmware_status(&iface);
        let state = scheme.state_mut(&iface).unwrap();
        state.last_error = err.clone();
        state.status = WifiStatus::Failed.as_str().to_string();
        state.firmware_status = firmware_status;

        assert!(scheme
            .state(&iface)
            .unwrap()
            .last_error
            .contains("missing firmware"));
        assert_eq!(scheme.state(&iface).unwrap().status, "failed");
        assert!(scheme
            .state(&iface)
            .unwrap()
            .firmware_status
            .contains("firmware=missing"));
    }

    #[test]
    fn stub_transport_probe_updates_transport_status() {
        let mut scheme = WifiCtlScheme::new(Box::new(StubBackend::from_env()));
        let iface = "wlan0".to_string();
        let transport_status = scheme.backend.transport_probe(&iface).unwrap();
        scheme.state_mut(&iface).unwrap().transport_status = transport_status;
        assert!(scheme
            .state(&iface)
            .unwrap()
            .transport_status
            .contains("mmio_probe=host-skipped"));
    }

    #[test]
    fn stub_init_transport_records_state() {
        let mut scheme = WifiCtlScheme::new(Box::new(StubBackend::from_env()));
        let iface = "wlan0".to_string();
        let status = scheme.backend.init_transport(&iface).unwrap();
        scheme.state_mut(&iface).unwrap().transport_init_status = status;
        assert_eq!(
            scheme.state(&iface).unwrap().transport_init_status,
            "transport_init=stub"
        );
    }
}
