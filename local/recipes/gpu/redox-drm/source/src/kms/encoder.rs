use crate::kms::EncoderInfo;

#[derive(Clone, Debug)]
pub struct Encoder {
    #[allow(dead_code)]
    pub info: EncoderInfo,
}

impl Encoder {
    pub fn new(id: u32, crtc_id: u32) -> Self {
        Self {
            info: EncoderInfo {
                id,
                encoder_type: 0,
                crtc_id,
                possible_crtcs: 1,
                possible_clones: 0,
            },
        }
    }
}
