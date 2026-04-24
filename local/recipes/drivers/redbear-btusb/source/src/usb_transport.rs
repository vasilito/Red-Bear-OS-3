//! USB transport abstraction for HCI communication with Bluetooth controllers.

use std::io;
use std::path::PathBuf;

use crate::hci::{HciAcl, HciCommand, HciEvent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsbEndpointType {
    Control,
    Interrupt,
    BulkIn,
    BulkOut,
}

#[derive(Clone, Debug)]
pub struct UsbTransportConfig {
    pub device_path: PathBuf,
    pub vendor_id: u16,
    pub device_id: u16,
    pub interrupt_endpoint: u8,
    pub bulk_in_endpoint: u8,
    pub bulk_out_endpoint: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportState {
    Closed,
    Opening,
    Active,
    Error,
}

pub trait UsbHciTransport {
    fn send_command(&mut self, command: &HciCommand) -> io::Result<()>;
    fn recv_event(&mut self) -> io::Result<Option<HciEvent>>;
    fn send_acl(&mut self, acl: &HciAcl) -> io::Result<()>;
    fn recv_acl(&mut self) -> io::Result<Option<HciAcl>>;
    fn state(&self) -> TransportState;
    fn close(&mut self) -> io::Result<()>;
}

pub struct StubTransport {
    config: UsbTransportConfig,
    state: TransportState,
    sent_commands: Vec<HciCommand>,
    sent_acl: Vec<HciAcl>,
    pending_events: Vec<HciEvent>,
    pending_acl: Vec<HciAcl>,
}

impl StubTransport {
    pub fn new(config: UsbTransportConfig) -> Self {
        Self {
            config,
            state: TransportState::Closed,
            sent_commands: Vec::new(),
            sent_acl: Vec::new(),
            pending_events: Vec::new(),
            pending_acl: Vec::new(),
        }
    }

    pub fn inject_event(&mut self, event: HciEvent) {
        self.pending_events.push(event);
    }

    pub fn inject_acl(&mut self, acl: HciAcl) {
        self.pending_acl.push(acl);
    }

    pub fn drain_sent_commands(&mut self) -> Vec<HciCommand> {
        let drained = self.sent_commands.clone();
        self.sent_commands.clear();
        drained
    }

    pub fn drain_sent_acl(&mut self) -> Vec<HciAcl> {
        let drained = self.sent_acl.clone();
        self.sent_acl.clear();
        drained
    }

    #[allow(dead_code)]
    pub fn config(&self) -> &UsbTransportConfig {
        &self.config
    }
}

impl UsbHciTransport for StubTransport {
    fn send_command(&mut self, command: &HciCommand) -> io::Result<()> {
        if self.state == TransportState::Closed {
            return Err(io::Error::new(io::ErrorKind::NotConnected, "transport is closed"));
        }
        self.sent_commands.push(command.clone());
        Ok(())
    }

    fn recv_event(&mut self) -> io::Result<Option<HciEvent>> {
        if self.state == TransportState::Closed {
            return Err(io::Error::new(io::ErrorKind::NotConnected, "transport is closed"));
        }
        Ok(if self.pending_events.is_empty() {
            None
        } else {
            Some(self.pending_events.remove(0))
        })
    }

    fn send_acl(&mut self, acl: &HciAcl) -> io::Result<()> {
        if self.state == TransportState::Closed {
            return Err(io::Error::new(io::ErrorKind::NotConnected, "transport is closed"));
        }
        self.sent_acl.push(acl.clone());
        Ok(())
    }

    fn recv_acl(&mut self) -> io::Result<Option<HciAcl>> {
        if self.state == TransportState::Closed {
            return Err(io::Error::new(io::ErrorKind::NotConnected, "transport is closed"));
        }
        Ok(if self.pending_acl.is_empty() {
            None
        } else {
            Some(self.pending_acl.remove(0))
        })
    }

    fn state(&self) -> TransportState {
        self.state
    }

    fn close(&mut self) -> io::Result<()> {
        self.state = TransportState::Closed;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hci::{EVT_COMMAND_COMPLETE, EVT_COMMAND_STATUS, OP_RESET};

    fn test_config() -> UsbTransportConfig {
        UsbTransportConfig {
            device_path: PathBuf::from("/scheme/usb/test/hci0"),
            vendor_id: 0x8087,
            device_id: 0x0A2B,
            interrupt_endpoint: 0x81,
            bulk_in_endpoint: 0x82,
            bulk_out_endpoint: 0x01,
        }
    }

    fn open_stub() -> StubTransport {
        let mut stub = StubTransport::new(test_config());
        stub.state = TransportState::Active;
        stub
    }

    fn make_cc_event(opcode: u16, status: u8) -> HciEvent {
        let params = vec![0x01, opcode as u8, (opcode >> 8) as u8, status];
        HciEvent {
            event_code: EVT_COMMAND_COMPLETE,
            parameters: params,
        }
    }

    fn make_cs_event(status: u8, opcode: u16) -> HciEvent {
        let params = vec![status, 0x01, opcode as u8, (opcode >> 8) as u8];
        HciEvent {
            event_code: EVT_COMMAND_STATUS,
            parameters: params,
        }
    }

    #[test]
    fn stub_starts_closed() {
        let stub = StubTransport::new(test_config());
        assert_eq!(stub.state(), TransportState::Closed);
    }

    #[test]
    fn send_command_appears_in_drain() {
        let mut stub = open_stub();
        let cmd = HciCommand::new(OP_RESET, vec![]);
        stub.send_command(&cmd).unwrap();
        let sent = stub.drain_sent_commands();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0], cmd);
    }

    #[test]
    fn inject_event_recv_returns_it() {
        let mut stub = open_stub();
        let evt = make_cc_event(OP_RESET, 0x00);
        stub.inject_event(evt.clone());
        let received = stub.recv_event().unwrap();
        assert_eq!(received, Some(evt));
    }

    #[test]
    fn send_acl_appears_in_drain() {
        let mut stub = open_stub();
        let acl = HciAcl::new(0x0001, 0x00, 0x00, vec![0xDE, 0xAD]);
        stub.send_acl(&acl).unwrap();
        let sent = stub.drain_sent_acl();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0], acl);
    }

    #[test]
    fn inject_acl_recv_returns_it() {
        let mut stub = open_stub();
        let acl = HciAcl::new(0x0001, 0x00, 0x00, vec![0xCA, 0xFE]);
        stub.inject_acl(acl.clone());
        let received = stub.recv_acl().unwrap();
        assert_eq!(received, Some(acl));
    }

    #[test]
    fn close_transitions_to_closed() {
        let mut stub = open_stub();
        assert_eq!(stub.state(), TransportState::Active);
        stub.close().unwrap();
        assert_eq!(stub.state(), TransportState::Closed);
    }

    #[test]
    fn recv_event_returns_none_when_empty() {
        let mut stub = open_stub();
        assert_eq!(stub.recv_event().unwrap(), None);
    }

    #[test]
    fn recv_acl_returns_none_when_empty() {
        let mut stub = open_stub();
        assert_eq!(stub.recv_acl().unwrap(), None);
    }

    #[test]
    fn drain_sent_commands_empty_returns_empty_vec() {
        let mut stub = open_stub();
        assert!(stub.drain_sent_commands().is_empty());
    }

    #[test]
    fn drain_sent_acl_empty_returns_empty_vec() {
        let mut stub = open_stub();
        assert!(stub.drain_sent_acl().is_empty());
    }

    #[test]
    fn send_command_on_closed_returns_error() {
        let mut stub = StubTransport::new(test_config());
        let cmd = HciCommand::new(OP_RESET, vec![]);
        let result = stub.send_command(&cmd);
        assert!(result.is_err());
        assert_eq!(result.err().map(|e| e.kind()), Some(io::ErrorKind::NotConnected));
    }

    #[test]
    fn recv_event_on_closed_returns_error() {
        let mut stub = StubTransport::new(test_config());
        let result = stub.recv_event();
        assert!(result.is_err());
        assert_eq!(result.err().map(|e| e.kind()), Some(io::ErrorKind::NotConnected));
    }

    #[test]
    fn send_acl_on_closed_returns_error() {
        let mut stub = StubTransport::new(test_config());
        let acl = HciAcl::new(0x0001, 0x00, 0x00, vec![]);
        let result = stub.send_acl(&acl);
        assert!(result.is_err());
        assert_eq!(result.err().map(|e| e.kind()), Some(io::ErrorKind::NotConnected));
    }

    #[test]
    fn recv_acl_on_closed_returns_error() {
        let mut stub = StubTransport::new(test_config());
        let result = stub.recv_acl();
        assert!(result.is_err());
        assert_eq!(result.err().map(|e| e.kind()), Some(io::ErrorKind::NotConnected));
    }

    #[test]
    fn multiple_commands_queue_in_order() {
        let mut stub = open_stub();
        let cmd1 = HciCommand::new(OP_RESET, vec![]);
        let cmd2 = HciCommand::new(0x1009, vec![0x01]);
        stub.send_command(&cmd1).unwrap();
        stub.send_command(&cmd2).unwrap();
        let sent = stub.drain_sent_commands();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0], cmd1);
        assert_eq!(sent[1], cmd2);
    }

    #[test]
    fn multiple_events_dequeue_in_order() {
        let mut stub = open_stub();
        let evt1 = make_cc_event(OP_RESET, 0x00);
        let evt2 = make_cs_event(0x00, 0x0405);
        stub.inject_event(evt1.clone());
        stub.inject_event(evt2.clone());
        assert_eq!(stub.recv_event().unwrap(), Some(evt1));
        assert_eq!(stub.recv_event().unwrap(), Some(evt2));
        assert_eq!(stub.recv_event().unwrap(), None);
    }

    #[test]
    fn drain_clears_so_second_drain_is_empty() {
        let mut stub = open_stub();
        stub.send_command(&HciCommand::new(OP_RESET, vec![])).unwrap();
        assert_eq!(stub.drain_sent_commands().len(), 1);
        assert!(stub.drain_sent_commands().is_empty());
    }

    #[test]
    fn close_then_reopen_cycle() {
        let mut stub = open_stub();
        stub.close().unwrap();
        assert_eq!(stub.state(), TransportState::Closed);
        stub.state = TransportState::Active;
        let cmd = HciCommand::new(OP_RESET, vec![]);
        stub.send_command(&cmd).unwrap();
        assert_eq!(stub.drain_sent_commands().len(), 1);
    }
}
