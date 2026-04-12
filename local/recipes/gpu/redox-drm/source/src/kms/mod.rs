pub mod connector;
pub mod crtc;
pub mod encoder;
pub mod plane;

#[derive(Clone, Debug)]
pub struct ModeInfo {
    pub clock: u32,
    pub hdisplay: u16,
    pub hsync_start: u16,
    pub hsync_end: u16,
    pub htotal: u16,
    pub hskew: u16,
    pub vdisplay: u16,
    pub vsync_start: u16,
    pub vsync_end: u16,
    pub vtotal: u16,
    pub vscan: u16,
    pub vrefresh: u32,
    pub flags: u32,
    pub type_: u32,
    pub name: String,
}

impl ModeInfo {
    pub fn default_1080p() -> Self {
        Self {
            clock: 148_500,
            hdisplay: 1920,
            hsync_start: 2008,
            hsync_end: 2052,
            htotal: 2200,
            hskew: 0,
            vdisplay: 1080,
            vsync_start: 1084,
            vsync_end: 1089,
            vtotal: 1125,
            vscan: 0,
            vrefresh: 60,
            flags: 0,
            type_: 0,
            name: "1920x1080@60".to_string(),
        }
    }

    pub fn from_edid(edid: &[u8]) -> Vec<Self> {
        const EDID_HEADER: [u8; 8] = [0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00];

        if edid.len() < 128 || edid.get(0..8) != Some(&EDID_HEADER) {
            return Vec::new();
        }

        let mut modes = Vec::new();
        for descriptor in edid[54..126].chunks_exact(18) {
            let pixel_clock = u16::from_le_bytes([descriptor[0], descriptor[1]]) as u32;
            if pixel_clock == 0 {
                continue;
            }

            let hdisplay = descriptor[2] as u16 | (((descriptor[4] >> 4) as u16) << 8);
            let hblank = descriptor[3] as u16 | (((descriptor[4] & 0x0f) as u16) << 8);
            let vdisplay = descriptor[5] as u16 | (((descriptor[7] >> 4) as u16) << 8);
            let vblank = descriptor[6] as u16 | (((descriptor[7] & 0x0f) as u16) << 8);
            let hsync_offset =
                descriptor[8] as u16 | ((((descriptor[11] >> 6) & 0x03) as u16) << 8);
            let hsync_width = descriptor[9] as u16 | ((((descriptor[11] >> 4) & 0x03) as u16) << 8);
            let vsync_offset =
                ((descriptor[10] >> 4) as u16) | ((((descriptor[11] >> 2) & 0x03) as u16) << 4);
            let vsync_width =
                (descriptor[10] & 0x0f) as u16 | (((descriptor[11] & 0x03) as u16) << 4);

            if hdisplay == 0 || vdisplay == 0 {
                continue;
            }

            let htotal = hdisplay.saturating_add(hblank);
            let vtotal = vdisplay.saturating_add(vblank);
            let clock = pixel_clock.saturating_mul(10);
            let vrefresh = if htotal != 0 && vtotal != 0 {
                clock.saturating_mul(1000) / (htotal as u32).saturating_mul(vtotal as u32)
            } else {
                0
            };

            modes.push(Self {
                clock,
                hdisplay,
                hsync_start: hdisplay.saturating_add(hsync_offset),
                hsync_end: hdisplay
                    .saturating_add(hsync_offset)
                    .saturating_add(hsync_width),
                htotal,
                hskew: 0,
                vdisplay,
                vsync_start: vdisplay.saturating_add(vsync_offset),
                vsync_end: vdisplay
                    .saturating_add(vsync_offset)
                    .saturating_add(vsync_width),
                vtotal,
                vscan: 0,
                vrefresh,
                flags: if (descriptor[17] & 0x80) != 0 { 1 } else { 0 },
                type_: 0,
                name: format!("{}x{}@{}", hdisplay, vdisplay, vrefresh),
            });
        }

        modes
    }
}

#[derive(Clone, Debug)]
pub struct ConnectorInfo {
    pub id: u32,
    pub connector_type: ConnectorType,
    #[allow(dead_code)]
    pub connector_type_id: u32,
    pub connection: ConnectorStatus,
    pub mm_width: u32,
    pub mm_height: u32,
    pub encoder_id: u32,
    pub modes: Vec<ModeInfo>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectorType {
    Unknown,
    VGA,
    DVII,
    DVID,
    DVIA,
    #[allow(dead_code)]
    Composite,
    #[allow(dead_code)]
    SVideo,
    #[allow(dead_code)]
    LVDS,
    #[allow(dead_code)]
    Component,
    #[allow(dead_code)]
    NinePinDIN,
    DisplayPort,
    HDMIA,
    #[allow(dead_code)]
    HDMIB,
    #[allow(dead_code)]
    TV,
    EDP,
    Virtual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectorStatus {
    Connected,
    Disconnected,
    Unknown,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct CrtcInfo {
    pub id: u32,
    pub fb_id: u32,
    pub x: u32,
    pub y: u32,
    pub gamma_size: u32,
    pub mode: Option<ModeInfo>,
}

#[derive(Clone, Debug)]
pub struct EncoderInfo {
    #[allow(dead_code)]
    pub id: u32,
    #[allow(dead_code)]
    pub encoder_type: u32,
    #[allow(dead_code)]
    pub crtc_id: u32,
    #[allow(dead_code)]
    pub possible_crtcs: u32,
    #[allow(dead_code)]
    pub possible_clones: u32,
}
