use crate::kms::{ConnectorInfo, ConnectorStatus, ConnectorType, ModeInfo};

#[derive(Clone, Debug)]
pub struct Connector {
    pub info: ConnectorInfo,
    #[allow(dead_code)]
    pub edid: Vec<u8>,
}

impl Connector {
    pub fn synthetic_displayport(id: u32, encoder_id: u32) -> Self {
        let edid = synthetic_edid();
        let modes = ModeInfo::from_edid(&edid);

        Self {
            info: ConnectorInfo {
                id,
                connector_type: ConnectorType::DisplayPort,
                connector_type_id: 1,
                connection: ConnectorStatus::Connected,
                mm_width: 600,
                mm_height: 340,
                encoder_id,
                modes: if modes.is_empty() {
                    vec![ModeInfo::default_1080p()]
                } else {
                    modes
                },
            },
            edid,
        }
    }
}

pub fn synthetic_edid() -> Vec<u8> {
    vec![
        0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00, 0x4c, 0x2d, 0xfa, 0x12, 0x01, 0x00, 0x00,
        0x00, 0x01, 0x1e, 0x01, 0x04, 0xa5, 0x3c, 0x22, 0x78, 0x3a, 0xee, 0x95, 0xa3, 0x54, 0x4c,
        0x99, 0x26, 0x0f, 0x50, 0x54, 0xbf, 0xef, 0x80, 0x71, 0x4f, 0x81, 0x80, 0x81, 0x40, 0x81,
        0xc0, 0x95, 0x00, 0xa9, 0xc0, 0xb3, 0x00, 0xd1, 0xc0, 0x02, 0x3a, 0x80, 0x18, 0x71, 0x38,
        0x2d, 0x40, 0x58, 0x2c, 0x45, 0x00, 0x55, 0x50, 0x21, 0x00, 0x00, 0x1e, 0x00, 0x00, 0x00,
        0xfd, 0x00, 0x32, 0x4c, 0x1e, 0x53, 0x11, 0x00, 0x0a, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
        0x00, 0x00, 0x00, 0xfc, 0x00, 0x53, 0x79, 0x6e, 0x74, 0x68, 0x65, 0x74, 0x69, 0x63, 0x20,
        0x44, 0x50, 0x0a, 0x20, 0x20, 0x00, 0xa7,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_displayport_has_correct_fields() {
        let conn = Connector::synthetic_displayport(5, 10);
        assert_eq!(conn.info.id, 5);
        assert_eq!(conn.info.encoder_id, 10);
        assert_eq!(conn.info.connector_type, ConnectorType::DisplayPort);
        assert_eq!(conn.info.connection, ConnectorStatus::Connected);
        assert!(!conn.info.modes.is_empty(), "synthetic DisplayPort should have modes");
    }

    #[test]
    fn synthetic_displayport_modes_have_valid_dimensions() {
        let conn = Connector::synthetic_displayport(1, 1);
        for mode in &conn.info.modes {
            assert!(mode.hdisplay > 0, "mode hdisplay should be > 0");
            assert!(mode.vdisplay > 0, "mode vdisplay should be > 0");
            assert!(mode.vrefresh > 0, "mode vrefresh should be > 0");
            assert!(mode.clock > 0, "mode clock should be > 0");
        }
    }

    #[test]
    fn synthetic_edid_returns_exactly_112_bytes() {
        let edid = synthetic_edid();
        assert_eq!(edid.len(), 112);
    }

    #[test]
    fn synthetic_edid_has_valid_header() {
        let edid = synthetic_edid();
        let header: [u8; 8] = [0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00];
        assert_eq!(&edid[0..8], &header, "EDID header should be valid");
    }
}
