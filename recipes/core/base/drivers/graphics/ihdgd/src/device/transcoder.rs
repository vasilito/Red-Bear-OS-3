use common::io::{Io, MmioPtr};
use syscall::error::Result;

use super::{MmioRegion, Pipe};

// IHD-OS-KBL-Vol 2c-1.17 TRANS_CONF
// IHD-OS-TGL-Vol 2c-12.21 TRANS_CONF
pub const TRANS_CONF_ENABLE: u32 = 1 << 31;
pub const TRANS_CONF_STATE: u32 = 1 << 30;
pub const TRANS_CONF_MODE_MASK: u32 = 0b11 << 21;

// IHD-OS-KBL-Vol 2c-1.17 TRANS_DDI_FUNC_CTL
// IHD-OS-TGL-Vol 2c-12.21 TRANS_DDI_FUNC_CTL
pub const TRANS_DDI_FUNC_CTL_ENABLE: u32 = 1 << 31;
pub const TRANS_DDI_FUNC_CTL_MODE_HDMI: u32 = 0b000 << 24;
pub const TRANS_DDI_FUNC_CTL_MODE_DVI: u32 = 0b001 << 24;
pub const TRANS_DDI_FUNC_CTL_MODE_DP_SST: u32 = 0b010 << 24;
pub const TRANS_DDI_FUNC_CTL_MODE_DP_MST: u32 = 0b011 << 24;
pub const TRANS_DDI_FUNC_CTL_BPC_8: u32 = 0b000 << 20;
pub const TRANS_DDI_FUNC_CTL_BPC_10: u32 = 0b001 << 20;
pub const TRANS_DDI_FUNC_CTL_BPC_6: u32 = 0b010 << 20;
pub const TRANS_DDI_FUNC_CTL_BPC_12: u32 = 0b011 << 20;
pub const TRANS_DDI_FUNC_CTL_SYNC_POLARITY_HSHIGH: u32 = 0b01 << 16;
pub const TRANS_DDI_FUNC_CTL_SYNC_POLARITY_VSHIGH: u32 = 0b10 << 16;
pub const TRANS_DDI_FUNC_CTL_DSI_INPUT_PIPE_SHIFT: u32 = 12;
pub const TRANS_DDI_FUNC_CTL_PORT_WIDTH_1: u32 = 0b000 << 1;
pub const TRANS_DDI_FUNC_CTL_PORT_WIDTH_2: u32 = 0b001 << 1;
pub const TRANS_DDI_FUNC_CTL_PORT_WIDTH_3: u32 = 0b010 << 1;
pub const TRANS_DDI_FUNC_CTL_PORT_WIDTH_4: u32 = 0b011 << 1;

pub struct Transcoder {
    pub name: &'static str,
    pub index: usize,
    pub clk_sel: MmioPtr<u32>,
    pub clk_sel_shift: u32,
    pub conf: MmioPtr<u32>,
    pub ddi_func_ctl: MmioPtr<u32>,
    pub ddi_func_ctl_ddi_shift: u32,
    pub ddi_func_ctl_hdmi_scrambling: u32,
    pub ddi_func_ctl_high_tmds_char_rate: u32,
    pub ddi_func_ctl2: Option<MmioPtr<u32>>,
    pub hblank: MmioPtr<u32>,
    pub hsync: MmioPtr<u32>,
    pub htotal: MmioPtr<u32>,
    pub msa_misc: MmioPtr<u32>,
    pub mult: MmioPtr<u32>,
    pub push: Option<MmioPtr<u32>>,
    pub space: MmioPtr<u32>,
    pub stereo3d_ctl: MmioPtr<u32>,
    pub vblank: MmioPtr<u32>,
    pub vrr_ctl: Option<MmioPtr<u32>>,
    pub vrr_flipline: Option<MmioPtr<u32>>,
    pub vrr_status: Option<MmioPtr<u32>>,
    pub vrr_status2: Option<MmioPtr<u32>>,
    pub vrr_vmax: Option<MmioPtr<u32>>,
    pub vrr_vmaxshift: Option<MmioPtr<u32>>,
    pub vrr_vmin: Option<MmioPtr<u32>>,
    pub vrr_vtotal_prev: Option<MmioPtr<u32>>,
    pub vsync: MmioPtr<u32>,
    pub vsyncshift: MmioPtr<u32>,
    pub vtotal: MmioPtr<u32>,
}

impl Transcoder {
    pub fn dump(&self) {
        eprint!("Transcoder {} {}", self.name, self.index);
        eprint!(" clk_sel {:08X}", self.clk_sel.read());
        eprint!(" conf {:08X}", self.conf.read());
        eprint!(" ddi_func_ctl {:08X}", self.ddi_func_ctl.read());
        if let Some(reg) = &self.ddi_func_ctl2 {
            eprint!(" ddi_func_ctl2 {:08X}", reg.read());
        }
        eprint!(" hblank {:08X}", self.hblank.read());
        eprint!(" hsync {:08X}", self.hsync.read());
        eprint!(" htotal {:08X}", self.htotal.read());
        eprint!(" msa_misc {:08X}", self.msa_misc.read());
        eprint!(" mult {:08X}", self.mult.read());
        if let Some(reg) = &self.push {
            eprint!(" push {:08X}", reg.read());
        }
        eprint!(" space {:08X}", self.space.read());
        eprint!(" stereo3d_ctl {:08X}", self.stereo3d_ctl.read());
        eprint!(" vblank {:08X}", self.vblank.read());
        if let Some(reg) = &self.vrr_ctl {
            eprint!(" vrr_ctl {:08X}", reg.read());
        }
        if let Some(reg) = &self.vrr_flipline {
            eprint!(" vrr_flipline {:08X}", reg.read());
        }
        if let Some(reg) = &self.vrr_status {
            eprint!(" vrr_status {:08X}", reg.read());
        }
        if let Some(reg) = &self.vrr_status2 {
            eprint!(" vrr_status2 {:08X}", reg.read());
        }
        if let Some(reg) = &self.vrr_vmax {
            eprint!(" vrr_vmax {:08X}", reg.read());
        }
        if let Some(reg) = &self.vrr_vmaxshift {
            eprint!(" vrr_vmaxshift {:08X}", reg.read());
        }
        if let Some(reg) = &self.vrr_vmin {
            eprint!(" vrr_vmin {:08X}", reg.read());
        }
        if let Some(reg) = &self.vrr_vtotal_prev {
            eprint!(" vrr_vtotal_prev {:08X}", reg.read());
        }
        eprint!(" vsync {:08X}", self.vsync.read());
        eprint!(" vsyncshift {:08X}", self.vsyncshift.read());
        eprint!(" vtotal {:08X}", self.vtotal.read());
        eprintln!();
    }

    pub fn modeset(&mut self, pipe: &mut Pipe, timing: &edid::DetailedTiming) {
        let hactive = (timing.horizontal_active_pixels as u32) - 1;
        let htotal = hactive + (timing.horizontal_blanking_pixels as u32);
        let hsync_start = hactive + (timing.horizontal_front_porch as u32);
        let hsync_end = hsync_start + (timing.horizontal_sync_width as u32);
        let vactive = (timing.vertical_active_lines as u32) - 1;
        let vtotal = vactive + (timing.vertical_blanking_lines as u32);
        let vsync_start = vactive + (timing.vertical_front_porch as u32);
        let vsync_end = vsync_start + (timing.vertical_sync_width as u32);

        // Configure horizontal sync
        self.htotal.write(hactive | (htotal << 16));
        self.hblank.write(hactive | (htotal << 16));
        self.hsync.write(hsync_start | (hsync_end << 16));

        // Configure vertical sync
        self.vtotal.write(vactive | (vtotal << 16));
        self.vblank.write(vactive | (vtotal << 16));
        self.vsync.write(vsync_start | (vsync_end << 16));

        // Configure pipe
        pipe.srcsz.write(vactive | (hactive << 16));
    }

    pub fn kabylake(gttmm: &MmioRegion) -> Result<Vec<Self>> {
        let mut transcoders = Vec::with_capacity(4);
        //TODO: Transcoder EDP
        for (i, name) in ["A", "B", "C"].iter().enumerate() {
            transcoders.push(Transcoder {
                name,
                index: i,
                // IHD-OS-KBL-Vol 2c-1.17 TRANS_CLK_SEL
                clk_sel: unsafe { gttmm.mmio(0x46140 + i * 0x4)? },
                clk_sel_shift: 29,
                // IHD-OS-KBL-Vol 2c-1.17 TRANS_CONF
                conf: unsafe { gttmm.mmio(0x70008 + i * 0x1000)? },
                // IHD-OS-KBL-Vol 2c-1.17 TRANS_DDI_FUNC_CTL
                ddi_func_ctl: unsafe { gttmm.mmio(0x60400 + i * 0x1000)? },
                ddi_func_ctl_ddi_shift: 28,
                // HDMI scrambling not supported on Kaby Lake
                ddi_func_ctl_hdmi_scrambling: 0,
                ddi_func_ctl_high_tmds_char_rate: 0,
                // N/A
                ddi_func_ctl2: None,
                // IHD-OS-KBL-Vol 2c-1.17 TRANS_HBLANK
                hblank: unsafe { gttmm.mmio(0x60004 + i * 0x1000)? },
                // IHD-OS-KBL-Vol 2c-1.17 TRANS_HSYNC
                hsync: unsafe { gttmm.mmio(0x60008 + i * 0x1000)? },
                // IHD-OS-KBL-Vol 2c-1.17 TRANS_HTOTAL
                htotal: unsafe { gttmm.mmio(0x60000 + i * 0x1000)? },
                // IHD-OS-KBL-Vol 2c-1.17 TRANS_MSA_MISC
                msa_misc: unsafe { gttmm.mmio(0x60410 + i * 0x1000)? },
                // IHD-OS-KBL-Vol 2c-1.17 TRANS_MULT
                mult: unsafe { gttmm.mmio(0x6002C + i * 0x1000)? },
                // N/A
                push: None,
                // IHD-OS-KBL-Vol 2c-1.17 TRANS_SPACE
                space: unsafe { gttmm.mmio(0x60020 + i * 0x1000)? },
                // IHD-OS-KBL-Vol 2c-1.17 TRANS_STEREO3D_CTL
                stereo3d_ctl: unsafe { gttmm.mmio(0x70020 + i * 0x1000)? },
                // IHD-OS-KBL-Vol 2c-1.17 TRANS_VBLANK
                vblank: unsafe { gttmm.mmio(0x60010 + i * 0x1000)? },
                // N/A
                vrr_ctl: None,
                vrr_flipline: None,
                vrr_status: None,
                vrr_status2: None,
                vrr_vmax: None,
                vrr_vmaxshift: None,
                vrr_vmin: None,
                vrr_vtotal_prev: None,
                // IHD-OS-KBL-Vol 2c-1.17 TRANS_VSYNC
                vsync: unsafe { gttmm.mmio(0x60014 + i * 0x1000)? },
                // IHD-OS-KBL-Vol 2c-1.17 TRANS_VSYNCSHIFT
                vsyncshift: unsafe { gttmm.mmio(0x60028 + i * 0x1000)? },
                // IHD-OS-KBL-Vol 2c-1.17 TRANS_VTOTAL
                vtotal: unsafe { gttmm.mmio(0x6000C + i * 0x1000)? },
            });
        }
        Ok(transcoders)
    }

    pub fn tigerlake(gttmm: &MmioRegion) -> Result<Vec<Self>> {
        let mut transcoders = Vec::with_capacity(4);
        for (i, name) in ["A", "B", "C", "D"].iter().enumerate() {
            transcoders.push(Transcoder {
                name,
                index: i,
                // IHD-OS-TGL-Vol 2c-12.21 TRANS_CLK_SEL
                clk_sel: unsafe { gttmm.mmio(0x46140 + i * 0x4)? },
                clk_sel_shift: 28,
                // IHD-OS-TGL-Vol 2c-12.21 TRANS_CONF
                conf: unsafe { gttmm.mmio(0x70008 + i * 0x1000)? },
                // IHD-OS-TGL-Vol 2c-12.21 TRANS_DDI_FUNC_CTL
                ddi_func_ctl: unsafe { gttmm.mmio(0x60400 + i * 0x1000)? },
                ddi_func_ctl_ddi_shift: 27,
                ddi_func_ctl_hdmi_scrambling: 1 << 0,
                ddi_func_ctl_high_tmds_char_rate: 1 << 4,
                // IHD-OS-TGL-Vol 2c-12.21 TRANS_DDI_FUNC_CTL2
                ddi_func_ctl2: Some(unsafe { gttmm.mmio(0x60404 + i * 0x1000)? }),
                // IHD-OS-TGL-Vol 2c-12.21 TRANS_HBLANK
                hblank: unsafe { gttmm.mmio(0x60004 + i * 0x1000)? },
                // IHD-OS-TGL-Vol 2c-12.21 TRANS_HSYNC
                hsync: unsafe { gttmm.mmio(0x60008 + i * 0x1000)? },
                // IHD-OS-TGL-Vol 2c-12.21 TRANS_HTOTAL
                htotal: unsafe { gttmm.mmio(0x60000 + i * 0x1000)? },
                // IHD-OS-TGL-Vol 2c-12.21 TRANS_MSA_MISC
                msa_misc: unsafe { gttmm.mmio(0x60410 + i * 0x1000)? },
                // IHD-OS-TGL-Vol 2c-12.21 TRANS_MULT
                mult: unsafe { gttmm.mmio(0x6002C + i * 0x1000)? },
                // IHD-OS-TGL-Vol 2c-12.21 TRANS_PUSH
                push: Some(unsafe { gttmm.mmio(0x60A70 + i * 0x1000)? }),
                // IHD-OS-TGL-Vol 2c-12.21 TRANS_SPACE
                space: unsafe { gttmm.mmio(0x60020 + i * 0x1000)? },
                // IHD-OS-TGL-Vol 2c-12.21 TRANS_STEREO3D_CTL
                stereo3d_ctl: unsafe { gttmm.mmio(0x70020 + i * 0x1000)? },
                // IHD-OS-TGL-Vol 2c-12.21 TRANS_VBLANK
                vblank: unsafe { gttmm.mmio(0x60010 + i * 0x1000)? },
                // IHD-OS-TGL-Vol 2c-12.21 TRANS_VRR_CTL
                vrr_ctl: Some(unsafe { gttmm.mmio(0x60420 + i * 0x1000)? }),
                // IHD-OS-TGL-Vol 2c-12.21 TRANS_VRR_FLIPLINE
                vrr_flipline: Some(unsafe { gttmm.mmio(0x60438 + i * 0x1000)? }),
                // IHD-OS-TGL-Vol 2c-12.21 TRANS_VRR_STATUS
                vrr_status: Some(unsafe { gttmm.mmio(0x6042C + i * 0x1000)? }),
                // IHD-OS-TGL-Vol 2c-12.21 TRANS_VRR_STATUS2
                vrr_status2: Some(unsafe { gttmm.mmio(0x6043C + i * 0x1000)? }),
                // IHD-OS-TGL-Vol 2c-12.21 TRANS_VRR_VMAX
                vrr_vmax: Some(unsafe { gttmm.mmio(0x60424 + i * 0x1000)? }),
                // IHD-OS-TGL-Vol 2c-12.21 TRANS_VRR_VMAXSHIFT
                vrr_vmaxshift: Some(unsafe { gttmm.mmio(0x60428 + i * 0x1000)? }),
                // IHD-OS-TGL-Vol 2c-12.21 TRANS_VRR_VMIN
                vrr_vmin: Some(unsafe { gttmm.mmio(0x60434 + i * 0x1000)? }),
                // IHD-OS-TGL-Vol 2c-12.21 TRANS_VRR_VTOTAL_PREV
                vrr_vtotal_prev: Some(unsafe { gttmm.mmio(0x60480 + i * 0x1000)? }),
                // IHD-OS-TGL-Vol 2c-12.21 TRANS_VSYNC
                vsync: unsafe { gttmm.mmio(0x60014 + i * 0x1000)? },
                // IHD-OS-TGL-Vol 2c-12.21 TRANS_VSYNCSHIFT
                vsyncshift: unsafe { gttmm.mmio(0x60028 + i * 0x1000)? },
                // IHD-OS-TGL-Vol 2c-12.21 TRANS_VTOTAL
                vtotal: unsafe { gttmm.mmio(0x6000C + i * 0x1000)? },
            })
        }
        Ok(transcoders)
    }
}
