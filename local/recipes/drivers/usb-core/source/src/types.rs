use alloc::vec::Vec;

// USB Standard Device Descriptor (USB 2.0 §9.6.1)
#[derive(Clone, Debug)]
pub struct DeviceDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub usb_version: u16,
    pub device_class: u8,
    pub device_subclass: u8,
    pub device_protocol: u8,
    pub max_packet_size0: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub device_version: u16,
    pub manufacturer_idx: u8,
    pub product_idx: u8,
    pub serial_idx: u8,
    pub num_configurations: u8,
}

// USB Configuration Descriptor (USB 2.0 §9.6.3)
#[derive(Clone, Debug)]
pub struct ConfigDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub total_length: u16,
    pub num_interfaces: u8,
    pub config_value: u8,
    pub config_idx: u8,
    pub attributes: u8,
    pub max_power: u8,
}

// USB Endpoint Descriptor (USB 2.0 §9.6.6)
#[derive(Clone, Debug)]
pub struct EndpointDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub endpoint_address: u8,
    pub attributes: u8,
    pub max_packet_size: u16,
    pub interval: u8,
}

// Standard Setup Packet (USB 2.0 §9.3)
#[derive(Clone, Debug)]
pub struct SetupPacket {
    pub request_type: u8,
    pub request: u8,
    pub value: u16,
    pub index: u16,
    pub length: u16,
}

// USB Request Block — a transfer to/from a device
#[derive(Clone, Debug)]
pub struct Urb {
    pub device_address: u8,
    pub endpoint: u8,
    pub transfer_type: TransferType,
    pub direction: TransferDirection,
    pub buffer: Vec<u8>,
    pub actual_length: usize,
    pub status: UrbStatus,
    pub timeout_ms: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferType {
    Control,
    Bulk,
    Interrupt,
    Isochronous,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferDirection {
    Out,
    In,
    Setup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UrbStatus {
    Pending,
    Complete,
    Error,
    Timeout,
}

// USB port status
#[derive(Clone, Debug)]
pub struct PortStatus {
    pub connected: bool,
    pub enabled: bool,
    pub suspended: bool,
    pub over_current: bool,
    pub reset: bool,
    pub power: bool,
    pub low_speed: bool,
    pub high_speed: bool,
    pub test_mode: bool,
    pub indicator: bool,
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::{SetupPacket, TransferDirection, TransferType, Urb, UrbStatus};

    #[test]
    fn setup_packet_fields_are_accessible() {
        let packet = SetupPacket {
            request_type: 0x80,
            request: 0x06,
            value: 0x0100,
            index: 0x0000,
            length: 18,
        };

        assert_eq!(packet.request_type, 0x80);
        assert_eq!(packet.request, 0x06);
        assert_eq!(packet.value, 0x0100);
        assert_eq!(packet.index, 0x0000);
        assert_eq!(packet.length, 18);
    }

    #[test]
    fn urb_status_can_transition_between_states() {
        let mut urb = Urb {
            device_address: 1,
            endpoint: 1,
            transfer_type: TransferType::Bulk,
            direction: TransferDirection::In,
            buffer: Vec::new(),
            actual_length: 0,
            status: UrbStatus::Pending,
            timeout_ms: 1000,
        };

        assert_eq!(urb.status, UrbStatus::Pending);

        urb.status = UrbStatus::Complete;
        assert_eq!(urb.status, UrbStatus::Complete);

        urb.status = UrbStatus::Timeout;
        assert_eq!(urb.status, UrbStatus::Timeout);
    }
}
