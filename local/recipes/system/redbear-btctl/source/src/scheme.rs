use std::collections::BTreeMap;

use redox_scheme::scheme::SchemeSync;
use redox_scheme::{CallerCtx, OpenResult};
use syscall::error::{Error, Result, EACCES, EBADF, EINVAL, ENOENT, EROFS};
use syscall::flag::{EventFlags, MODE_DIR, MODE_FILE};
use syscall::schemev2::NewFdFlags;
use syscall::Stat;

use crate::backend::{connection_state_lines, AdapterState, AdapterStatus, Backend};
use crate::bond_store::BondRecord;

const SCHEME_ROOT_ID: usize = 1;

#[derive(Clone)]
enum HandleKind {
    Root,
    Adapters,
    Adapter(String),
    Capabilities,
    Status(String),
    TransportStatus(String),
    ScanResults(String),
    ConnectionState(String),
    ConnectResult(String),
    DisconnectResult(String),
    ReadCharResult(String),
    LastError(String),
    BondStorePath(String),
    BondCount(String),
    Bonds(String),
    BondMetadata(String, String),
    Scan(String),
    Connect(String),
    Disconnect(String),
    ReadChar(String),
}

pub struct BtCtlScheme {
    backend: Box<dyn Backend>,
    next_id: usize,
    handles: BTreeMap<usize, HandleKind>,
    states: BTreeMap<String, AdapterState>,
}

impl BtCtlScheme {
    pub fn new(backend: Box<dyn Backend>) -> Self {
        let mut states = BTreeMap::new();
        for adapter in backend.adapters() {
            states.insert(
                adapter.clone(),
                AdapterState {
                    status: backend.initial_status(&adapter).as_str().to_string(),
                    transport_status: backend.transport_status(&adapter),
                    scan_results: backend.default_scan_results(&adapter),
                    connected_bond_ids: backend.connected_bond_ids(&adapter).unwrap_or_default(),
                    connect_result: backend
                        .connect_result(&adapter)
                        .unwrap_or_else(|_| "connect_result=not-run".to_string()),
                    disconnect_result: backend
                        .disconnect_result(&adapter)
                        .unwrap_or_else(|_| "disconnect_result=not-run".to_string()),
                    read_char_result: backend
                        .read_char_result(&adapter)
                        .unwrap_or_else(|_| "read_char_result=not-run".to_string()),
                    bond_store_path: backend.bond_store_path(&adapter).unwrap_or_default(),
                    bonds: backend.load_bonds(&adapter).unwrap_or_default(),
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

    fn state(&self, adapter: &str) -> Result<&AdapterState> {
        self.states.get(adapter).ok_or(Error::new(ENOENT))
    }

    fn state_mut(&mut self, adapter: &str) -> Result<&mut AdapterState> {
        self.states.get_mut(adapter).ok_or(Error::new(ENOENT))
    }

    fn refreshed_status(&mut self, adapter: &str) -> Result<String> {
        let status = self
            .backend
            .status(adapter)
            .map_err(|_| Error::new(ENOENT))?
            .as_str()
            .to_string();
        let transport_status = self.backend.transport_status(adapter);
        let state = self.state_mut(adapter)?;
        state.status = status.clone();
        state.transport_status = transport_status.clone();
        if status != AdapterStatus::Scanning.as_str() {
            state.scan_results.clear();
        }
        Ok(status)
    }

    fn refreshed_transport_status(&mut self, adapter: &str) -> Result<String> {
        let transport_status = self.backend.transport_status(adapter);
        let state = self.state_mut(adapter)?;
        state.transport_status = transport_status.clone();
        Ok(transport_status)
    }

    fn refreshed_bonds(&mut self, adapter: &str) -> Result<Vec<BondRecord>> {
        let bond_store_path = self
            .backend
            .bond_store_path(adapter)
            .map_err(|_| Error::new(ENOENT))?;
        let bonds = self
            .backend
            .load_bonds(adapter)
            .map_err(|_| Error::new(ENOENT))?;
        let state = self.state_mut(adapter)?;
        state.bond_store_path = bond_store_path;
        state.bonds = bonds.clone();
        Ok(bonds)
    }

    fn refreshed_connected_bond_ids(&mut self, adapter: &str) -> Result<Vec<String>> {
        let connected_bond_ids = self
            .backend
            .connected_bond_ids(adapter)
            .map_err(|_| Error::new(ENOENT))?;
        self.state_mut(adapter)?.connected_bond_ids = connected_bond_ids.clone();
        Ok(connected_bond_ids)
    }

    fn refreshed_connect_result(&mut self, adapter: &str) -> Result<String> {
        let connect_result = self
            .backend
            .connect_result(adapter)
            .map_err(|_| Error::new(ENOENT))?;
        self.state_mut(adapter)?.connect_result = connect_result.clone();
        Ok(connect_result)
    }

    fn refreshed_disconnect_result(&mut self, adapter: &str) -> Result<String> {
        let disconnect_result = self
            .backend
            .disconnect_result(adapter)
            .map_err(|_| Error::new(ENOENT))?;
        self.state_mut(adapter)?.disconnect_result = disconnect_result.clone();
        Ok(disconnect_result)
    }

    fn refreshed_read_char_result(&mut self, adapter: &str) -> Result<String> {
        let read_char_result = self
            .backend
            .read_char_result(adapter)
            .map_err(|_| Error::new(ENOENT))?;
        self.state_mut(adapter)?.read_char_result = read_char_result.clone();
        Ok(read_char_result)
    }

    fn parse_read_char_request(value: &str) -> Result<(String, String, String)> {
        let mut bond_id = None;
        let mut service_uuid = None;
        let mut char_uuid = None;

        for line in value.lines().map(str::trim).filter(|line| !line.is_empty()) {
            let Some((key, raw_value)) = line.split_once('=') else {
                return Err(Error::new(EINVAL));
            };
            let parsed = raw_value.trim().to_string();
            if parsed.is_empty() {
                return Err(Error::new(EINVAL));
            }
            match key.trim() {
                "bond_id" => bond_id = Some(parsed),
                "service_uuid" => service_uuid = Some(parsed),
                "char_uuid" => char_uuid = Some(parsed),
                _ => return Err(Error::new(EINVAL)),
            }
        }

        match (bond_id, service_uuid, char_uuid) {
            (Some(bond_id), Some(service_uuid), Some(char_uuid)) => {
                Ok((bond_id, service_uuid, char_uuid))
            }
            _ => Err(Error::new(EINVAL)),
        }
    }

    fn format_bond_metadata(bond: &BondRecord) -> String {
        let mut lines = vec![format!("bond_id={}", bond.bond_id)];
        if let Some(alias) = &bond.alias {
            lines.push(format!("alias={alias}"));
        }
        lines.push(format!("created_at_epoch={}", bond.created_at_epoch));
        lines.push(format!("source={}", bond.source));
        format!("{}\n", lines.join("\n"))
    }

    fn status_string(&self, adapter: &str) -> String {
        self.backend
            .status(adapter)
            .map(|status| status.as_str().to_string())
            .unwrap_or_else(|_| AdapterStatus::Failed.as_str().to_string())
    }

    fn write_handle(&mut self, kind: HandleKind, value: &str) -> Result<()> {
        match kind {
            HandleKind::Scan(adapter) => {
                let results = match self.backend.scan(&adapter) {
                    Ok(results) => results,
                    Err(err) => {
                        let transport_status = self.backend.transport_status(&adapter);
                        let status = self.status_string(&adapter);
                        let state = self.state_mut(&adapter)?;
                        state.last_error = err;
                        state.status = status;
                        state.transport_status = transport_status;
                        return Ok(());
                    }
                };

                let transport_status = self.backend.transport_status(&adapter);
                let state = self.state_mut(&adapter)?;
                state.status = AdapterStatus::Scanning.as_str().to_string();
                state.transport_status = transport_status;
                state.scan_results = results;
                state.last_error.clear();
            }
            HandleKind::Connect(adapter) => {
                let bond_id = value.trim();
                if bond_id.is_empty() {
                    return Err(Error::new(EINVAL));
                }

                let outcome = self.backend.connect(&adapter, bond_id);
                let status = self.status_string(&adapter);
                let transport_status = self.backend.transport_status(&adapter);
                let connected_bond_ids = self.refreshed_connected_bond_ids(&adapter)?;
                let connect_result = self.refreshed_connect_result(&adapter)?;
                let disconnect_result = self.refreshed_disconnect_result(&adapter)?;
                let rejected = outcome.is_err();
                let state = self.state_mut(&adapter)?;
                state.status = status;
                state.transport_status = transport_status;
                state.connected_bond_ids = connected_bond_ids;
                state.connect_result = connect_result;
                state.disconnect_result = disconnect_result;
                let last_error = match outcome {
                    Ok(()) => String::new(),
                    Err(err) => err,
                };
                state.last_error = last_error;
                if rejected {
                    return Err(Error::new(EINVAL));
                }
            }
            HandleKind::Disconnect(adapter) => {
                let bond_id = value.trim();
                if bond_id.is_empty() {
                    return Err(Error::new(EINVAL));
                }

                let outcome = self.backend.disconnect(&adapter, bond_id);
                let status = self.status_string(&adapter);
                let transport_status = self.backend.transport_status(&adapter);
                let connected_bond_ids = self.refreshed_connected_bond_ids(&adapter)?;
                let connect_result = self.refreshed_connect_result(&adapter)?;
                let disconnect_result = self.refreshed_disconnect_result(&adapter)?;
                let rejected = outcome.is_err();
                let state = self.state_mut(&adapter)?;
                state.status = status;
                state.transport_status = transport_status;
                state.connected_bond_ids = connected_bond_ids;
                state.connect_result = connect_result;
                state.disconnect_result = disconnect_result;
                let last_error = match outcome {
                    Ok(()) => String::new(),
                    Err(err) => err,
                };
                state.last_error = last_error;
                if rejected {
                    return Err(Error::new(EINVAL));
                }
            }
            HandleKind::ReadChar(adapter) => {
                let (bond_id, service_uuid, char_uuid) = Self::parse_read_char_request(value)?;

                let outcome = self
                    .backend
                    .read_char(&adapter, &bond_id, &service_uuid, &char_uuid);
                let status = self.status_string(&adapter);
                let transport_status = self.backend.transport_status(&adapter);
                let connected_bond_ids = self.refreshed_connected_bond_ids(&adapter)?;
                let connect_result = self.refreshed_connect_result(&adapter)?;
                let disconnect_result = self.refreshed_disconnect_result(&adapter)?;
                let read_char_result = self.refreshed_read_char_result(&adapter)?;
                let rejected = outcome.is_err();
                let state = self.state_mut(&adapter)?;
                state.status = status;
                state.transport_status = transport_status;
                state.connected_bond_ids = connected_bond_ids;
                state.connect_result = connect_result;
                state.disconnect_result = disconnect_result;
                state.read_char_result = read_char_result;
                let last_error = match outcome {
                    Ok(()) => String::new(),
                    Err(err) => err,
                };
                state.last_error = last_error;
                if rejected {
                    return Err(Error::new(EINVAL));
                }
            }
            _ => return Err(Error::new(EROFS)),
        }

        Ok(())
    }

    fn read_handle(&mut self, kind: &HandleKind) -> Result<String> {
        Ok(match kind {
            HandleKind::Root => "adapters\ncapabilities\n".to_string(),
            HandleKind::Adapters => {
                self.states.keys().cloned().collect::<Vec<_>>().join("\n") + "\n"
            }
            HandleKind::Adapter(_) => {
                "status\ntransport-status\nscan-results\nconnection-state\nconnect-result\ndisconnect-result\nread-char-result\nlast-error\nbond-store-path\nbond-count\nbonds\nscan\nconnect\ndisconnect\nread-char\n"
                    .to_string()
            }
            HandleKind::Capabilities => self.backend.capabilities().join("\n") + "\n",
            HandleKind::Status(adapter) => {
                let status = self.refreshed_status(adapter)?;
                let transport_status = self.refreshed_transport_status(adapter)?;
                let connected_bond_ids = self.refreshed_connected_bond_ids(adapter)?;
                let bonds = self.refreshed_bonds(adapter)?;
                let scan_results_count = self.state(adapter)?.scan_results.len();
                let state = self.state(adapter)?;
                format!(
                    "status={}\ntransport_status={}\nscan_results_count={}\nconnected_bond_count={}\nbond_count={}\nbond_store_path={}\n",
                    status,
                    transport_status,
                    scan_results_count.max(state.scan_results.len()),
                    connected_bond_ids.len(),
                    bonds.len(),
                    state.bond_store_path
                )
            }
            HandleKind::TransportStatus(adapter) => {
                format!("{}\n", self.refreshed_transport_status(adapter)?)
            }
            HandleKind::ScanResults(adapter) => self.state(adapter)?.scan_results.join("\n") + "\n",
            HandleKind::ConnectionState(adapter) => {
                let connected_bond_ids = self.refreshed_connected_bond_ids(adapter)?;
                format!("{}\n", connection_state_lines(&connected_bond_ids).join("\n"))
            }
            HandleKind::ConnectResult(adapter) => {
                format!("{}\n", self.refreshed_connect_result(adapter)?)
            }
            HandleKind::DisconnectResult(adapter) => {
                format!("{}\n", self.refreshed_disconnect_result(adapter)?)
            }
            HandleKind::ReadCharResult(adapter) => {
                format!("{}\n", self.refreshed_read_char_result(adapter)?)
            }
            HandleKind::LastError(adapter) => format!("{}\n", self.state(adapter)?.last_error),
            HandleKind::BondStorePath(adapter) => {
                let bonds = self.refreshed_bonds(adapter)?;
                let _ = bonds;
                format!("{}\n", self.state(adapter)?.bond_store_path)
            }
            HandleKind::BondCount(adapter) => {
                let bonds = self.refreshed_bonds(adapter)?;
                format!(
                    "bond_count={}\nbond_store_path={}\n",
                    bonds.len(),
                    self.state(adapter)?.bond_store_path
                )
            }
            HandleKind::Bonds(adapter) => {
                let bonds = self.refreshed_bonds(adapter)?;
                if bonds.is_empty() {
                    "\n".to_string()
                } else {
                    bonds
                        .iter()
                        .map(|bond| bond.bond_id.clone())
                        .collect::<Vec<_>>()
                        .join("\n")
                        + "\n"
                }
            }
            HandleKind::BondMetadata(adapter, bond_id) => self
                .refreshed_bonds(adapter)?
                .into_iter()
                .find(|bond| &bond.bond_id == bond_id)
                .map(|bond| Self::format_bond_metadata(&bond))
                .ok_or(Error::new(ENOENT))?,
            HandleKind::Scan(_) | HandleKind::Connect(_) | HandleKind::Disconnect(_) | HandleKind::ReadChar(_) => {
                String::new()
            }
        })
    }
}

impl SchemeSync for BtCtlScheme {
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
                "adapters" => HandleKind::Adapters,
                "capabilities" => HandleKind::Capabilities,
                _ => return Err(Error::new(ENOENT)),
            }
        } else {
            let parent = self.handle(dirfd)?.clone();
            match parent {
                HandleKind::Adapters => {
                    let adapter = path.trim_matches('/');
                    self.state(adapter)?;
                    HandleKind::Adapter(adapter.to_string())
                }
                HandleKind::Adapter(adapter) => match path.trim_matches('/') {
                    "status" => HandleKind::Status(adapter.clone()),
                    "transport-status" => HandleKind::TransportStatus(adapter.clone()),
                    "scan-results" => HandleKind::ScanResults(adapter.clone()),
                    "connection-state" => HandleKind::ConnectionState(adapter.clone()),
                    "connect-result" => HandleKind::ConnectResult(adapter.clone()),
                    "disconnect-result" => HandleKind::DisconnectResult(adapter.clone()),
                    "read-char-result" => HandleKind::ReadCharResult(adapter.clone()),
                    "last-error" => HandleKind::LastError(adapter.clone()),
                    "bond-store-path" => HandleKind::BondStorePath(adapter.clone()),
                    "bond-count" => HandleKind::BondCount(adapter.clone()),
                    "bonds" => HandleKind::Bonds(adapter.clone()),
                    "scan" => HandleKind::Scan(adapter.clone()),
                    "connect" => HandleKind::Connect(adapter.clone()),
                    "disconnect" => HandleKind::Disconnect(adapter.clone()),
                    "read-char" => HandleKind::ReadChar(adapter.clone()),
                    _ => return Err(Error::new(ENOENT)),
                },
                HandleKind::Bonds(adapter) => {
                    let bond_id = path.trim_matches('/');
                    if bond_id.is_empty() {
                        return Err(Error::new(ENOENT));
                    }
                    let adapter_name = adapter.clone();
                    let exists = self
                        .refreshed_bonds(&adapter_name)?
                        .into_iter()
                        .any(|bond| bond.bond_id == bond_id);
                    if exists {
                        HandleKind::BondMetadata(adapter_name, bond_id.to_string())
                    } else {
                        return Err(Error::new(ENOENT));
                    }
                }
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
        let kind = self.handle(id)?.clone();
        let data = self.read_handle(&kind)?;
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
        let value = std::str::from_utf8(buf).map_err(|_| Error::new(EINVAL))?;
        let kind = self.handle(id)?.clone();
        self.write_handle(kind, value)?;

        Ok(buf.len())
    }

    fn fstat(&mut self, id: usize, stat: &mut Stat, _ctx: &CallerCtx) -> Result<()> {
        let kind = self.handle(id)?;
        stat.st_mode = match kind {
            HandleKind::Root
            | HandleKind::Adapters
            | HandleKind::Adapter(_)
            | HandleKind::Bonds(_) => MODE_DIR | 0o755,
            HandleKind::Scan(_)
            | HandleKind::Connect(_)
            | HandleKind::Disconnect(_)
            | HandleKind::ReadChar(_) => MODE_FILE | 0o644,
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
            HandleKind::Root => "btctl:/".to_string(),
            HandleKind::Adapters => "btctl:/adapters".to_string(),
            HandleKind::Adapter(adapter) => format!("btctl:/adapters/{adapter}"),
            HandleKind::Capabilities => "btctl:/capabilities".to_string(),
            HandleKind::Status(adapter) => format!("btctl:/adapters/{adapter}/status"),
            HandleKind::TransportStatus(adapter) => {
                format!("btctl:/adapters/{adapter}/transport-status")
            }
            HandleKind::ScanResults(adapter) => format!("btctl:/adapters/{adapter}/scan-results"),
            HandleKind::ConnectionState(adapter) => {
                format!("btctl:/adapters/{adapter}/connection-state")
            }
            HandleKind::ConnectResult(adapter) => {
                format!("btctl:/adapters/{adapter}/connect-result")
            }
            HandleKind::DisconnectResult(adapter) => {
                format!("btctl:/adapters/{adapter}/disconnect-result")
            }
            HandleKind::ReadCharResult(adapter) => {
                format!("btctl:/adapters/{adapter}/read-char-result")
            }
            HandleKind::LastError(adapter) => format!("btctl:/adapters/{adapter}/last-error"),
            HandleKind::BondStorePath(adapter) => {
                format!("btctl:/adapters/{adapter}/bond-store-path")
            }
            HandleKind::BondCount(adapter) => format!("btctl:/adapters/{adapter}/bond-count"),
            HandleKind::Bonds(adapter) => format!("btctl:/adapters/{adapter}/bonds"),
            HandleKind::BondMetadata(adapter, bond_id) => {
                format!("btctl:/adapters/{adapter}/bonds/{bond_id}")
            }
            HandleKind::Scan(adapter) => format!("btctl:/adapters/{adapter}/scan"),
            HandleKind::Connect(adapter) => format!("btctl:/adapters/{adapter}/connect"),
            HandleKind::Disconnect(adapter) => format!("btctl:/adapters/{adapter}/disconnect"),
            HandleKind::ReadChar(adapter) => format!("btctl:/adapters/{adapter}/read-char"),
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
    use crate::backend::StubBackend;
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("{name}-{stamp}"))
    }

    fn build_scheme(status_path: PathBuf, bond_store_root: PathBuf) -> BtCtlScheme {
        BtCtlScheme::new(Box::new(StubBackend::new_for_test(
            vec!["hci0".to_string()],
            vec!["demo-beacon".to_string(), "demo-sensor".to_string()],
            status_path,
            bond_store_root,
        )))
    }

    #[test]
    fn root_surface_lists_expected_nodes() {
        let mut scheme = build_scheme(
            temp_path("rbos-btctl-root"),
            temp_path("rbos-btctl-root-bonds"),
        );
        assert_eq!(
            scheme.read_handle(&HandleKind::Root).unwrap(),
            "adapters\ncapabilities\n"
        );
    }

    #[test]
    fn adapter_surface_lists_bond_nodes() {
        let mut scheme = build_scheme(
            temp_path("rbos-btctl-adapter-root"),
            temp_path("rbos-btctl-adapter-root-bonds"),
        );
        assert_eq!(
            scheme
                .read_handle(&HandleKind::Adapter("hci0".to_string()))
                .unwrap(),
            "status\ntransport-status\nscan-results\nconnection-state\nconnect-result\ndisconnect-result\nread-char-result\nlast-error\nbond-store-path\nbond-count\nbonds\nscan\nconnect\ndisconnect\nread-char\n"
        );
    }

    #[test]
    fn scan_failure_records_last_error_when_transport_is_missing() {
        let missing = temp_path("rbos-btctl-scan-missing");
        let mut scheme = build_scheme(missing, temp_path("rbos-btctl-scan-missing-bonds"));
        let adapter = "hci0".to_string();

        let err = scheme.backend.scan(&adapter).unwrap_err();
        let transport_status = scheme.backend.transport_status(&adapter);
        let state = scheme.state_mut(&adapter).unwrap();
        state.last_error = err.clone();
        state.status = AdapterStatus::Failed.as_str().to_string();
        state.transport_status = transport_status;

        assert!(scheme
            .state(&adapter)
            .unwrap()
            .last_error
            .contains("start redbear-btusb explicitly"));
        assert_eq!(scheme.state(&adapter).unwrap().status, "failed");
    }

    #[test]
    fn scan_updates_state_when_transport_status_file_is_present() {
        let status_path = temp_path("rbos-btctl-scan-visible");
        fs::write(
            &status_path,
            &format!(
                "transport=usb\nstartup=explicit\nupdated_at_epoch={}\nruntime_visibility=runtime-visible\n",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            ),
        )
        .unwrap();
        let bond_store_root = temp_path("rbos-btctl-scan-visible-bonds");
        let mut scheme = build_scheme(status_path.clone(), bond_store_root.clone());
        let adapter = "hci0".to_string();

        let results = scheme.backend.scan(&adapter).unwrap();
        let transport_status = scheme.backend.transport_status(&adapter);
        let state = scheme.state_mut(&adapter).unwrap();
        state.status = AdapterStatus::Scanning.as_str().to_string();
        state.transport_status = transport_status;
        state.scan_results = results;

        assert_eq!(
            scheme
                .read_handle(&HandleKind::Status(adapter.clone()))
                .unwrap()
                .lines()
                .next()
                .unwrap(),
            "status=adapter-visible"
        );
        assert_eq!(
            scheme.state(&adapter).unwrap().scan_results,
            Vec::<String>::new()
        );

        fs::remove_file(status_path).unwrap();
        fs::remove_dir_all(bond_store_root).ok();
    }

    #[test]
    fn status_read_refreshes_when_transport_becomes_stale() {
        let status_path = temp_path("rbos-btctl-scan-stale-read");
        fs::write(
            &status_path,
            "transport=usb\nstartup=explicit\nupdated_at_epoch=1\nruntime_visibility=runtime-visible\n",
        )
        .unwrap();
        let mut scheme = build_scheme(
            status_path.clone(),
            temp_path("rbos-btctl-scan-stale-read-bonds"),
        );

        let status = scheme
            .read_handle(&HandleKind::Status("hci0".to_string()))
            .unwrap();
        let transport = scheme
            .read_handle(&HandleKind::TransportStatus("hci0".to_string()))
            .unwrap();

        assert!(status.contains("status=explicit-startup-required"));
        assert!(transport.contains("runtime_visibility=installed-only"));

        fs::remove_file(status_path).unwrap();
    }

    #[test]
    fn bond_nodes_refresh_from_store_without_write_api() {
        let bond_store_root = temp_path("rbos-btctl-scheme-bonds");
        let mut scheme = build_scheme(
            temp_path("rbos-btctl-scheme-status"),
            bond_store_root.clone(),
        );
        let adapter = "hci0".to_string();

        assert_eq!(
            scheme
                .read_handle(&HandleKind::BondCount(adapter.clone()))
                .unwrap(),
            format!(
                "bond_count=0\nbond_store_path={}\n",
                bond_store_root.join("hci0").join("bonds").display()
            )
        );

        scheme
            .backend
            .add_stub_bond(&adapter, "AA:BB:CC:DD:EE:FF", Some("demo-sensor"))
            .unwrap();

        let count = scheme
            .read_handle(&HandleKind::BondCount(adapter.clone()))
            .unwrap();
        let bonds = scheme
            .read_handle(&HandleKind::Bonds(adapter.clone()))
            .unwrap();
        let metadata = scheme
            .read_handle(&HandleKind::BondMetadata(
                adapter.clone(),
                "AA:BB:CC:DD:EE:FF".to_string(),
            ))
            .unwrap();

        assert!(count.contains("bond_count=1"));
        assert!(bonds.contains("AA:BB:CC:DD:EE:FF"));
        assert!(metadata.contains("bond_id=AA:BB:CC:DD:EE:FF"));
        assert!(metadata.contains("alias=demo-sensor"));
        assert!(metadata.contains("source=stub-cli"));

        scheme
            .backend
            .remove_bond(&adapter, "AA:BB:CC:DD:EE:FF")
            .unwrap();

        let count_after_remove = scheme
            .read_handle(&HandleKind::BondCount(adapter.clone()))
            .unwrap();
        assert!(count_after_remove.contains("bond_count=0"));

        fs::remove_dir_all(bond_store_root).unwrap();
    }

    #[test]
    fn connect_write_updates_connection_surfaces() {
        let status_path = temp_path("rbos-btctl-scheme-connect-status");
        fs::write(
            &status_path,
            &format!(
                "transport=usb\nstartup=explicit\nupdated_at_epoch={}\nruntime_visibility=runtime-visible\n",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            ),
        )
        .unwrap();
        let bond_store_root = temp_path("rbos-btctl-scheme-connect-bonds");
        let mut scheme = build_scheme(status_path.clone(), bond_store_root.clone());
        let adapter = "hci0".to_string();

        scheme
            .backend
            .add_stub_bond(&adapter, "AA:BB:CC:DD:EE:FF", Some("demo-sensor"))
            .unwrap();

        scheme
            .write_handle(HandleKind::Connect(adapter.clone()), "AA:BB:CC:DD:EE:FF")
            .unwrap();

        let connection_state = scheme
            .read_handle(&HandleKind::ConnectionState(adapter.clone()))
            .unwrap();
        let connect_result = scheme
            .read_handle(&HandleKind::ConnectResult(adapter.clone()))
            .unwrap();

        assert!(connection_state.contains("connection_state=stub-connected"));
        assert!(connection_state.contains("connected_bond_ids=AA:BB:CC:DD:EE:FF"));
        assert!(connect_result.contains("connect_result=stub-connected bond_id=AA:BB:CC:DD:EE:FF"));

        scheme
            .write_handle(HandleKind::Disconnect(adapter.clone()), "AA:BB:CC:DD:EE:FF")
            .unwrap();

        let disconnected_state = scheme
            .read_handle(&HandleKind::ConnectionState(adapter.clone()))
            .unwrap();
        let disconnect_result = scheme
            .read_handle(&HandleKind::DisconnectResult(adapter.clone()))
            .unwrap();

        assert!(disconnected_state.contains("connection_state=stub-disconnected"));
        assert!(disconnected_state.contains("connected_bond_ids="));
        assert!(!disconnected_state.contains("connected_bond_ids=AA:BB:CC:DD:EE:FF"));
        assert!(disconnect_result
            .contains("disconnect_result=stub-disconnected bond_id=AA:BB:CC:DD:EE:FF"));

        fs::remove_file(status_path).unwrap();
        fs::remove_dir_all(bond_store_root).unwrap();
    }

    #[test]
    fn read_char_write_updates_bounded_result_surface() {
        let status_path = temp_path("rbos-btctl-scheme-read-char-status");
        fs::write(
            &status_path,
            &format!(
                "transport=usb\nstartup=explicit\nupdated_at_epoch={}\nruntime_visibility=runtime-visible\n",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            ),
        )
        .unwrap();
        let bond_store_root = temp_path("rbos-btctl-scheme-read-char-bonds");
        let mut scheme = build_scheme(status_path.clone(), bond_store_root.clone());
        let adapter = "hci0".to_string();

        scheme
            .backend
            .add_stub_bond(&adapter, "AA:BB:CC:DD:EE:FF", Some("demo-battery-sensor"))
            .unwrap();
        scheme
            .write_handle(HandleKind::Connect(adapter.clone()), "AA:BB:CC:DD:EE:FF")
            .unwrap();

        scheme
            .write_handle(
                HandleKind::ReadChar(adapter.clone()),
                "bond_id=AA:BB:CC:DD:EE:FF\nservice_uuid=0000180f-0000-1000-8000-00805f9b34fb\nchar_uuid=00002a19-0000-1000-8000-00805f9b34fb\n",
            )
            .unwrap();

        let read_result = scheme
            .read_handle(&HandleKind::ReadCharResult(adapter.clone()))
            .unwrap();
        assert!(read_result.contains("read_char_result=stub-value"));
        assert!(read_result.contains("workload=battery-sensor-battery-level-read"));
        assert!(read_result.contains("access=read-only"));
        assert!(read_result.contains("value_percent=87"));

        scheme
            .write_handle(
                HandleKind::ReadChar(adapter.clone()),
                "bond_id=AA:BB:CC:DD:EE:FF\nservice_uuid=0000180f-0000-1000-8000-00805f9b34fb\nchar_uuid=00002a1a-0000-1000-8000-00805f9b34fb\n",
            )
            .unwrap_err();

        let read_result_after_reject = scheme
            .read_handle(&HandleKind::ReadCharResult(adapter.clone()))
            .unwrap();
        let last_error = scheme
            .read_handle(&HandleKind::LastError(adapter.clone()))
            .unwrap();
        assert!(read_result_after_reject
            .contains("read_char_result=rejected-unsupported-characteristic"));
        assert!(last_error.contains("only the experimental"));

        fs::remove_file(status_path).unwrap();
        fs::remove_dir_all(bond_store_root).unwrap();
    }

    #[test]
    fn connect_write_records_last_error_when_bond_is_missing() {
        let status_path = temp_path("rbos-btctl-scheme-connect-missing-bond-status");
        fs::write(
            &status_path,
            &format!(
                "transport=usb\nstartup=explicit\nupdated_at_epoch={}\nruntime_visibility=runtime-visible\n",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            ),
        )
        .unwrap();
        let bond_store_root = temp_path("rbos-btctl-scheme-connect-missing-bond-bonds");
        let mut scheme = build_scheme(status_path.clone(), bond_store_root.clone());
        let adapter = "hci0".to_string();

        scheme
            .write_handle(HandleKind::Connect(adapter.clone()), "AA:BB:CC:DD:EE:FF")
            .unwrap_err();

        let last_error = scheme
            .read_handle(&HandleKind::LastError(adapter.clone()))
            .unwrap();
        let connect_result = scheme
            .read_handle(&HandleKind::ConnectResult(adapter.clone()))
            .unwrap();

        assert!(last_error.contains("bond record not found"));
        assert!(connect_result
            .contains("connect_result=rejected-missing-bond bond_id=AA:BB:CC:DD:EE:FF"));

        fs::remove_file(status_path).unwrap();
        fs::remove_dir_all(bond_store_root).ok();
    }

    #[test]
    fn disconnect_write_records_last_error_when_bond_is_missing() {
        let status_path = temp_path("rbos-btctl-scheme-disconnect-missing-bond-status");
        fs::write(
            &status_path,
            &format!(
                "transport=usb\nstartup=explicit\nupdated_at_epoch={}\nruntime_visibility=runtime-visible\n",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            ),
        )
        .unwrap();
        let bond_store_root = temp_path("rbos-btctl-scheme-disconnect-missing-bond-bonds");
        let mut scheme = build_scheme(status_path.clone(), bond_store_root.clone());
        let adapter = "hci0".to_string();

        scheme
            .write_handle(HandleKind::Disconnect(adapter.clone()), "AA:BB:CC:DD:EE:FF")
            .unwrap_err();

        let last_error = scheme
            .read_handle(&HandleKind::LastError(adapter.clone()))
            .unwrap();
        let disconnect_result = scheme
            .read_handle(&HandleKind::DisconnectResult(adapter.clone()))
            .unwrap();

        assert!(last_error.contains("bond record not found"));
        assert!(disconnect_result
            .contains("disconnect_result=rejected-missing-bond bond_id=AA:BB:CC:DD:EE:FF"));

        fs::remove_file(status_path).unwrap();
        fs::remove_dir_all(bond_store_root).ok();
    }
}
