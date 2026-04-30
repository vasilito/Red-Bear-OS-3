use alloc::vec::Vec;

use crate::types::{
    ConfigDescriptor, DeviceDescriptor, EndpointDescriptor, SetupPacket, TransferDirection,
    TransferType, Urb, UrbStatus,
};

/// Build a standard control transfer setup packet + data stage
pub fn control_transfer(
    device_address: u8,
    request_type: u8,
    request: u8,
    value: u16,
    index: u16,
    data: &[u8],
    direction: TransferDirection,
) -> Urb {
    let data = if data.len() > u16::MAX as usize {
        &data[..u16::MAX as usize]
    } else {
        data
    };

    let setup = SetupPacket {
        request_type,
        request,
        value,
        index,
        length: data.len() as u16,
    };

    let mut buffer = Vec::with_capacity(data.len().saturating_add(8));
    buffer.extend_from_slice(&setup_packet_bytes(&setup));
    buffer.extend_from_slice(data);

    Urb {
        device_address,
        endpoint: 0,
        transfer_type: TransferType::Control,
        direction,
        buffer,
        actual_length: 0,
        status: UrbStatus::Pending,
        timeout_ms: 1000,
    }
}

/// Parse a Device Descriptor from raw bytes
pub fn parse_device_descriptor(data: &[u8]) -> Option<DeviceDescriptor> {
    if data.len() < 18 || data[0] != 18 || data[1] != 0x01 {
        return None;
    }

    Some(DeviceDescriptor {
        length: data[0],
        descriptor_type: data[1],
        usb_version: u16::from_le_bytes([data[2], data[3]]),
        device_class: data[4],
        device_subclass: data[5],
        device_protocol: data[6],
        max_packet_size0: data[7],
        vendor_id: u16::from_le_bytes([data[8], data[9]]),
        product_id: u16::from_le_bytes([data[10], data[11]]),
        device_version: u16::from_le_bytes([data[12], data[13]]),
        manufacturer_idx: data[14],
        product_idx: data[15],
        serial_idx: data[16],
        num_configurations: data[17],
    })
}

/// Parse a Configuration Descriptor from raw bytes
pub fn parse_config_descriptor(data: &[u8]) -> Option<ConfigDescriptor> {
    if data.len() < 9 || data[0] != 9 || data[1] != 0x02 {
        return None;
    }

    Some(ConfigDescriptor {
        length: data[0],
        descriptor_type: data[1],
        total_length: u16::from_le_bytes([data[2], data[3]]),
        num_interfaces: data[4],
        config_value: data[5],
        config_idx: data[6],
        attributes: data[7],
        max_power: data[8],
    })
}

/// Parse an Endpoint Descriptor from raw bytes
pub fn parse_endpoint_descriptor(data: &[u8]) -> Option<EndpointDescriptor> {
    if data.len() < 7 || data[0] != 7 || data[1] != 0x05 {
        return None;
    }

    Some(EndpointDescriptor {
        length: data[0],
        descriptor_type: data[1],
        endpoint_address: data[2],
        attributes: data[3],
        max_packet_size: u16::from_le_bytes([data[4], data[5]]),
        interval: data[6],
    })
}

fn setup_packet_bytes(setup: &SetupPacket) -> [u8; 8] {
    let value = setup.value.to_le_bytes();
    let index = setup.index.to_le_bytes();
    let length = setup.length.to_le_bytes();

    [
        setup.request_type,
        setup.request,
        value[0],
        value[1],
        index[0],
        index[1],
        length[0],
        length[1],
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        parse_config_descriptor, parse_device_descriptor, parse_endpoint_descriptor,
    };

    #[test]
    fn parses_valid_device_descriptor() {
        let raw = [
            18, 0x01, 0x00, 0x02, 0xff, 0x00, 0x00, 64, 0x34, 0x12, 0x78, 0x56, 0x00, 0x01,
            1, 2, 3, 1,
        ];

        match parse_device_descriptor(&raw) {
            Some(descriptor) => {
                assert_eq!(descriptor.length, 18);
                assert_eq!(descriptor.descriptor_type, 0x01);
                assert_eq!(descriptor.usb_version, 0x0200);
                assert_eq!(descriptor.vendor_id, 0x1234);
                assert_eq!(descriptor.product_id, 0x5678);
                assert_eq!(descriptor.num_configurations, 1);
            }
            None => panic!("valid device descriptor should parse"),
        }
    }

    #[test]
    fn parses_valid_config_descriptor() {
        let raw = [9, 0x02, 32, 0, 1, 1, 0, 0x80, 50];

        match parse_config_descriptor(&raw) {
            Some(descriptor) => {
                assert_eq!(descriptor.length, 9);
                assert_eq!(descriptor.descriptor_type, 0x02);
                assert_eq!(descriptor.total_length, 32);
                assert_eq!(descriptor.num_interfaces, 1);
                assert_eq!(descriptor.max_power, 50);
            }
            None => panic!("valid config descriptor should parse"),
        }
    }

    #[test]
    fn parses_valid_endpoint_descriptor() {
        let raw = [7, 0x05, 0x81, 0x03, 8, 0, 10];

        match parse_endpoint_descriptor(&raw) {
            Some(descriptor) => {
                assert_eq!(descriptor.length, 7);
                assert_eq!(descriptor.descriptor_type, 0x05);
                assert_eq!(descriptor.endpoint_address, 0x81);
                assert_eq!(descriptor.attributes, 0x03);
                assert_eq!(descriptor.max_packet_size, 8);
                assert_eq!(descriptor.interval, 10);
            }
            None => panic!("valid endpoint descriptor should parse"),
        }
    }
}
