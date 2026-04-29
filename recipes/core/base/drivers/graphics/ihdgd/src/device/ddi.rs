use common::io::{Io, MmioPtr, WriteOnly};
use common::timeout::Timeout;
use embedded_hal::prelude::*;
use std::sync::Arc;
use syscall::error::{Error, Result, EIO};

use crate::device::aux::Aux;
use crate::device::power::PowerWells;
use crate::device::{CallbackGuard, Gmbus};

use super::{GpioPort, MmioRegion};

// IHD-OS-TGL-Vol 2c-12.21 DDI_AUX_CTL
pub const DDI_AUX_CTL_BUSY: u32 = 1 << 31;
pub const DDI_AUX_CTL_DONE: u32 = 1 << 30;
pub const DDI_AUX_CTL_TIMEOUT_ERROR: u32 = 1 << 28;
pub const DDI_AUX_CTL_TIMEOUT_SHIFT: u32 = 26;
pub const DDI_AUX_CTL_TIMEOUT_MASK: u32 = 0b11 << DDI_AUX_CTL_TIMEOUT_SHIFT;
pub const DDI_AUX_CTL_TIMEOUT_4000US: u32 = 0b11 << DDI_AUX_CTL_TIMEOUT_SHIFT;
pub const DDI_AUX_CTL_RECEIVE_ERROR: u32 = 1 << 25;
pub const DDI_AUX_CTL_SIZE_SHIFT: u32 = 20;
pub const DDI_AUX_CTL_SIZE_MASK: u32 = 0b11111 << 20;
pub const DDI_AUX_CTL_IO_SELECT: u32 = 1 << 11;

// IHD-OS-TGL-Vol 2c-12.21 DDI_BUF_CTL
pub const DDI_BUF_CTL_ENABLE: u32 = 1 << 31;
pub const DDI_BUF_CTL_IDLE: u32 = 1 << 7;

// IHD-OS-TGL-Vol 2c-12.21 PORT_CL_DW5
pub const PORT_CL_DW5_SUS_CLOCK_MASK: u32 = 0b11 << 0;

// IHD-OS-TGL-Vol 2c-12.21 PORT_CL_DW10
pub const PORT_CL_DW10_EDP4K2K_MODE_OVRD_EN: u32 = 1 << 3;
pub const PORT_CL_DW10_EDP4K2K_MODE_OVRD_VAL: u32 = 1 << 2;

// IHD-OS-TGL-Vol 2c-12.21 PORT_PCS_DW9
pub const PORT_PCS_DW1_CMNKEEPER_ENABLE: u32 = 1 << 26;

// IHD-OS-TGL-Vol 2c-12.21 PORT_TX_DW2
pub const PORT_TX_DW2_SWING_SEL_UPPER_SHIFT: u32 = 15;
pub const PORT_TX_DW2_SWING_SEL_UPPER_MASK: u32 = 1 << PORT_TX_DW2_SWING_SEL_UPPER_SHIFT;
pub const PORT_TX_DW2_SWING_SEL_LOWER_SHIFT: u32 = 11;
pub const PORT_TX_DW2_SWING_SEL_LOWER_MASK: u32 = 0b111 << PORT_TX_DW2_SWING_SEL_LOWER_SHIFT;
pub const PORT_TX_DW2_RCOMP_SCALAR_SHIFT: u32 = 0;
pub const PORT_TX_DW2_RCOMP_SCALAR_MASK: u32 = 0xFF << PORT_TX_DW2_RCOMP_SCALAR_SHIFT;

// IHD-OS-TGL-Vol 2c-12.21 PORT_TX_DW4
pub const PORT_TX_DW4_SELECT: u32 = 1 << 31;
pub const PORT_TX_DW4_POST_CURSOR_1_SHIFT: u32 = 12;
pub const PORT_TX_DW4_POST_CURSOR_1_MASK: u32 = 0b111111 << PORT_TX_DW4_POST_CURSOR_1_SHIFT;
pub const PORT_TX_DW4_POST_CURSOR_2_SHIFT: u32 = 6;
pub const PORT_TX_DW4_POST_CURSOR_2_MASK: u32 = 0b111111 << PORT_TX_DW4_POST_CURSOR_2_SHIFT;
pub const PORT_TX_DW4_CURSOR_COEFF_SHIFT: u32 = 0;
pub const PORT_TX_DW4_CURSOR_COEFF_MASK: u32 = 0b111111 << PORT_TX_DW4_CURSOR_COEFF_SHIFT;

// IHD-OS-TGL-Vol 2c-12.21 PORT_TX_DW5
pub const PORT_TX_DW5_TRAINING_ENABLE: u32 = 1 << 31;
pub const PORT_TX_DW5_DISABLE_2_TAP_SHIFT: u32 = 29;
pub const PORT_TX_DW5_DISABLE_2_TAP: u32 = 1 << PORT_TX_DW5_DISABLE_2_TAP_SHIFT;
pub const PORT_TX_DW5_DISABLE_3_TAP: u32 = 1 << 29;
pub const PORT_TX_DW5_CURSOR_PROGRAM: u32 = 1 << 26;
pub const PORT_TX_DW5_COEFF_POLARITY: u32 = 1 << 25;
pub const PORT_TX_DW5_SCALING_MODE_SEL_SHIFT: u32 = 18;
pub const PORT_TX_DW5_SCALING_MODE_SEL_MASK: u32 = 0b111 << PORT_TX_DW5_SCALING_MODE_SEL_SHIFT;
pub const PORT_TX_DW5_RTERM_SELECT_SHIFT: u32 = 3;
pub const PORT_TX_DW5_RTERM_SELECT_MASK: u32 = 0b111 << PORT_TX_DW5_RTERM_SELECT_SHIFT;

// IHD-OS-TGL-Vol 2c-12.21 PORT_TX_DW7
pub const PORT_TX_DW7_N_SCALAR_SHIFT: u32 = 24;

#[derive(Clone, Copy, Debug)]
#[repr(usize)]
pub enum PortClReg {
    Dw5 = 0x14,
    Dw10 = 0x28,
    Dw12 = 0x30,
    Dw15 = 0x3C,
    Dw16 = 0x40,
}

#[derive(Clone, Copy, Debug)]
#[repr(usize)]
pub enum PortCompReg {
    Dw0 = 0x100,
    Dw1 = 0x104,
    Dw3 = 0x10C,
    Dw8 = 0x120,
    Dw9 = 0x124,
    Dw10 = 0x128,
}

#[derive(Clone, Copy, Debug)]
#[repr(usize)]
pub enum PortPcsReg {
    Dw1 = 0x04,
    Dw9 = 0x24,
}

#[derive(Clone, Copy, Debug)]
#[repr(usize)]
pub enum PortTxReg {
    Dw0 = 0x80,
    Dw1 = 0x84,
    Dw2 = 0x88,
    Dw4 = 0x90,
    Dw5 = 0x94,
    Dw6 = 0x98,
    Dw7 = 0x9C,
    Dw8 = 0xA0,
}

#[derive(Clone, Copy, Debug)]
#[repr(usize)]
pub enum PortLane {
    Aux = 0x300,
    Grp = 0x600,
    Ln0 = 0x800,
    Ln1 = 0x900,
    Ln2 = 0xA00,
    Ln3 = 0xB00,
}

pub struct Ddi {
    pub name: &'static str,
    pub index: usize,
    pub gttmm: Arc<MmioRegion>,
    pub port_base: Option<usize>,
    pub aux_ctl: MmioPtr<u32>,
    pub aux_datas: [MmioPtr<u32>; 5],
    pub buf_ctl: MmioPtr<u32>,
    pub dpclka_cfgcr0_clock_shift: Option<u32>,
    pub dpclka_cfgcr0_clock_off: Option<u32>,
    pub gmbus_pin_pair: Option<u8>,
    pub gpio_port: Option<GpioPort>,
    pub pwr_well_ctl_aux_request: u32,
    pub pwr_well_ctl_aux_state: u32,
    pub pwr_well_ctl_ddi_request: u32,
    pub pwr_well_ctl_ddi_state: u32,
    pub sde_interrupt_hotplug: Option<u32>,
    pub transcoder_index: Option<u32>,
}

//TODO: verify offsets and count using DeviceKind?
impl Ddi {
    pub fn dump(&self) {
        eprint!("Ddi {} {}", self.name, self.index);
        eprint!(" buf_ctl {:08X}", self.buf_ctl.read());
        let lanes = [PortLane::Ln0, PortLane::Ln1, PortLane::Ln2, PortLane::Ln3];
        for reg in [
            PortClReg::Dw5,
            PortClReg::Dw10,
            PortClReg::Dw12,
            PortClReg::Dw15,
            PortClReg::Dw16,
        ] {
            if let Some(mmio) = self.port_cl(reg) {
                eprint!(" CL_{:?} {:08X}", reg, mmio.read());
            }
        }
        for reg in [PortPcsReg::Dw1, PortPcsReg::Dw9] {
            for lane in lanes {
                if let Some(mmio) = self.port_pcs(reg, lane) {
                    eprint!(" PCS_{:?}_{:?} {:08X}", reg, lane, mmio.read());
                }
            }
        }
        for reg in [
            PortTxReg::Dw0,
            PortTxReg::Dw1,
            PortTxReg::Dw2,
            PortTxReg::Dw4,
            PortTxReg::Dw5,
            PortTxReg::Dw6,
            PortTxReg::Dw7,
            PortTxReg::Dw8,
        ] {
            for lane in lanes {
                if let Some(mmio) = self.port_tx(reg, lane) {
                    eprint!(" TX_{:?}_{:?} {:08X}", reg, lane, mmio.read());
                }
            }
        }
        eprintln!();
    }

    fn port_reg(&self, offset: usize) -> Option<MmioPtr<u32>> {
        //TODO: handle gttmm.mmio error?
        unsafe { self.gttmm.mmio(self.port_base? + offset).ok() }
    }

    pub fn port_cl(&self, reg: PortClReg) -> Option<MmioPtr<u32>> {
        self.port_reg(reg as usize)
    }

    pub fn port_comp(&self, reg: PortCompReg) -> Option<MmioPtr<u32>> {
        self.port_reg(reg as usize)
    }

    //TODO: return WriteOnly if PortLane::Grp?
    pub fn port_pcs(&self, reg: PortPcsReg, lane: PortLane) -> Option<MmioPtr<u32>> {
        self.port_reg((reg as usize) + (lane as usize))
    }

    //TODO: return WriteOnly if PortLane::Grp?
    pub fn port_tx(&self, reg: PortTxReg, lane: PortLane) -> Option<MmioPtr<u32>> {
        self.port_reg((reg as usize) + (lane as usize))
    }

    pub fn probe_edid(
        &mut self,
        power_wells: &mut PowerWells,
        gttmm: &MmioRegion,
        gmbus: &mut Gmbus,
    ) -> Result<Option<(&'static str, [u8; 128])>, Error> {
        if let Some(port_comp_dw0) = self.port_comp(PortCompReg::Dw0) {
            log::debug!("PORT_COMP_DW0_{}: {:08X}", self.name, port_comp_dw0.read());
        }
        let mut aux_read_edid = |ddi: &mut Ddi| -> Result<[u8; 128]> {
            //TODO: BLOCK TCCOLD?

            //TODO: the request can be shared by multiple DDIs
            let pwr_well_ctl_aux_request = ddi.pwr_well_ctl_aux_request;
            let pwr_well_ctl_aux_state = ddi.pwr_well_ctl_aux_state;
            let mut pwr_well_ctl_aux = unsafe { MmioPtr::new(power_wells.ctl_aux.as_mut_ptr()) };
            let _pwr_guard = CallbackGuard::new(
                &mut pwr_well_ctl_aux,
                |pwr_well_ctl_aux| {
                    // Enable aux power
                    pwr_well_ctl_aux.writef(pwr_well_ctl_aux_request, true);
                    let timeout = Timeout::from_micros(1500);
                    while !pwr_well_ctl_aux.readf(pwr_well_ctl_aux_state) {
                        timeout.run().map_err(|()| {
                            log::debug!("timeout while requesting DDI {} aux power", ddi.name);
                            Error::new(EIO)
                        })?;
                    }
                    Ok(())
                },
                |pwr_well_ctl_aux| {
                    // Disable aux power
                    pwr_well_ctl_aux.writef(pwr_well_ctl_aux_request, false);
                },
            )?;

            let mut edid_data = [0; 128];
            Aux::new(ddi)
                .write_read(0x50, &[0x00], &mut edid_data)
                .map_err(|_err| Error::new(EIO))?;

            Ok(edid_data)
        };
        let mut gmbus_read_edid = |ddi: &mut Ddi| -> Result<[u8; 128]> {
            let Some(pin_pair) = ddi.gmbus_pin_pair else {
                return Err(Error::new(EIO));
            };

            let mut edid_data = [0; 128];
            gmbus
                .pin_pair(pin_pair)
                .write_read(0x50, &[0x00], &mut edid_data)
                .map_err(|_err| Error::new(EIO))?;

            Ok(edid_data)
        };
        let gpio_read_edid = |ddi: &mut Ddi| -> Result<[u8; 128]> {
            let Some(port) = &ddi.gpio_port else {
                return Err(Error::new(EIO));
            };

            let mut edid_data = [0; 128];
            unsafe { port.i2c(gttmm)? }
                .write_read(0x50, &[0x00], &mut edid_data)
                .map_err(|_err| Error::new(EIO))?;

            Ok(edid_data)
        };
        match aux_read_edid(self) {
            Ok(edid_data) => return Ok(Some(("AUX", edid_data))),
            Err(err) => {
                log::debug!("DDI {} failed to read EDID from AUX: {}", self.name, err);
            }
        }
        match gmbus_read_edid(self) {
            Ok(edid_data) => return Ok(Some(("GMBUS", edid_data))),
            Err(err) => {
                log::debug!("DDI {} failed to read EDID from GMBUS: {}", self.name, err);
            }
        }
        match gpio_read_edid(self) {
            Ok(edid_data) => return Ok(Some(("GPIO", edid_data))),
            Err(err) => {
                log::debug!("DDI {} failed to read EDID from GPIO: {}", self.name, err);
            }
        }
        // Will try again but not fail the driver
        Ok(None)
    }

    pub fn voltage_swing_hdmi(
        &mut self,
        gttmm: &MmioRegion,
        timing: &edid::DetailedTiming,
    ) -> Result<()> {
        struct Setting {
            dw2_swing_sel: u32,
            dw7_n_scalar: u32,
            dw4_cursor_coeff: u32,
            dw4_post_cursor_1: u32,
            dw5_2_tap_disable: u32,
        }

        impl Setting {
            pub fn new(
                dw2_swing_sel: u32,
                dw7_n_scalar: u32,
                dw4_cursor_coeff: u32,
                dw4_post_cursor_1: u32,
                dw5_2_tap_disable: u32,
            ) -> Self {
                Self {
                    dw2_swing_sel,
                    dw7_n_scalar,
                    dw4_cursor_coeff,
                    dw4_post_cursor_1,
                    dw5_2_tap_disable,
                }
            }
        }

        // IHD-OS-TGL-Vol 12-1.22-Rev2.0 "Voltage Swing Programming"
        let settings = vec![
            // HDMI 450mV, 450mV, 0.0dB
            Setting::new(0b1010, 0x60, 0x3F, 0x00, 0b0),
            // HDMI 450mV, 650mV, 3.2dB
            Setting::new(0b1011, 0x73, 0x36, 0x09, 0b0),
            // HDMI 450mV, 850mV, 5.5dB
            Setting::new(0b0110, 0x7F, 0x31, 0x0E, 0b0),
            // HDMI 650mV, 650mV, 0.0dB
            Setting::new(0b1011, 0x73, 0x3F, 0x00, 0b0),
            // HDMI 650mV, 850mV, 2.3dB
            Setting::new(0b0110, 0x7F, 0x37, 0x08, 0b0),
            // HDMI 850mV, 850mV, 0.0dB
            Setting::new(0b0110, 0x7F, 0x3F, 0x00, 0b0),
            // HDMI 600mV, 850mV, 3.0dB
            Setting::new(0b0110, 0x7F, 0x35, 0x0A, 0b0),
        ];

        // Last setting is the default
        //TODO: get correct setting index from BIOS
        let setting = settings.last().unwrap();

        // This allows unwraps on port functions below without panic
        if self.port_base.is_none() {
            log::error!("HDMI voltage swing procedure only implemented on combo DDI");
            return Err(Error::new(EIO));
        };

        // Clear cmnkeeper_enable for HDMI
        {
            // It is not possible to read from GRP register, so use LN0 as template
            let pcs_dw1_ln0 = self.port_pcs(PortPcsReg::Dw1, PortLane::Ln0).unwrap();
            let mut pcs_dw1_grp =
                WriteOnly::new(self.port_pcs(PortPcsReg::Dw1, PortLane::Grp).unwrap());
            let mut v = pcs_dw1_ln0.read();
            v &= !PORT_PCS_DW1_CMNKEEPER_ENABLE;
            pcs_dw1_grp.write(v);
        }

        // Program loadgen select
        //TODO: this assumes bit rate <= 6 GHz and 4 lanes enabled
        {
            let mut tx_dw4_ln0 = self.port_tx(PortTxReg::Dw4, PortLane::Ln0).unwrap();
            tx_dw4_ln0.writef(PORT_TX_DW4_SELECT, false);

            let mut tx_dw4_ln1 = self.port_tx(PortTxReg::Dw4, PortLane::Ln1).unwrap();
            tx_dw4_ln1.writef(PORT_TX_DW4_SELECT, true);

            let mut tx_dw4_ln2 = self.port_tx(PortTxReg::Dw4, PortLane::Ln2).unwrap();
            tx_dw4_ln2.writef(PORT_TX_DW4_SELECT, true);

            let mut tx_dw4_ln3 = self.port_tx(PortTxReg::Dw4, PortLane::Ln3).unwrap();
            tx_dw4_ln3.writef(PORT_TX_DW4_SELECT, true);
        }

        // Set PORT_CL_DW5 sus clock config to 11b
        {
            let mut cl_dw5 = self.port_cl(PortClReg::Dw5).unwrap();
            cl_dw5.writef(PORT_CL_DW5_SUS_CLOCK_MASK, true);
        }

        // Clear training enable to change swing values
        let tx_dw5_ln0 = self.port_tx(PortTxReg::Dw5, PortLane::Ln0).unwrap();
        let mut tx_dw5_grp = WriteOnly::new(self.port_tx(PortTxReg::Dw5, PortLane::Grp).unwrap());
        {
            let mut v = tx_dw5_ln0.read();
            v &= !PORT_TX_DW5_TRAINING_ENABLE;
            tx_dw5_grp.write(v);
        }

        // Program swing and de-emphasis

        // Disable eDP bits in PORT_CL_DW10
        let mut cl_dw10 = self.port_cl(PortClReg::Dw10).unwrap();
        cl_dw10.writef(
            PORT_CL_DW10_EDP4K2K_MODE_OVRD_EN | PORT_CL_DW10_EDP4K2K_MODE_OVRD_VAL,
            false,
        );

        // For PORT_TX_DW5:
        // - Set 2 tap disable from settings
        // - Set scaling mode sel to 010b
        // - Set rterm select to 110b
        // - Set 3 tap disable to 1
        // - Set cursor program to 0
        // - Set coeff polarity to 0
        {
            let mut v = tx_dw5_ln0.read();
            v &= !(PORT_TX_DW5_DISABLE_2_TAP
                | PORT_TX_DW5_CURSOR_PROGRAM
                | PORT_TX_DW5_COEFF_POLARITY
                | PORT_TX_DW5_SCALING_MODE_SEL_MASK
                | PORT_TX_DW5_RTERM_SELECT_MASK);
            v |= (setting.dw5_2_tap_disable << PORT_TX_DW5_DISABLE_2_TAP_SHIFT)
                | PORT_TX_DW5_DISABLE_3_TAP
                | (0b010 << PORT_TX_DW5_SCALING_MODE_SEL_SHIFT)
                | (0b110 << PORT_TX_DW5_RTERM_SELECT_SHIFT);
            tx_dw5_grp.write(v);
        }

        // Individual lane settings are used to avoid overwriting lane-specific settings, and because
        // group registers cannot be read
        let lanes = [PortLane::Ln0, PortLane::Ln1, PortLane::Ln2, PortLane::Ln3];

        // For PORT_TX_DW2:
        // - Set swing sel from settings
        // - Set rcomp scalar to 0x98
        for lane in lanes {
            let mut tx_dw2 = self.port_tx(PortTxReg::Dw2, lane).unwrap();
            let mut v = tx_dw2.read();
            v &= !(PORT_TX_DW2_SWING_SEL_UPPER_MASK
                | PORT_TX_DW2_SWING_SEL_LOWER_MASK
                | PORT_TX_DW2_RCOMP_SCALAR_MASK);
            v |= (((setting.dw2_swing_sel >> 3) & 1) << PORT_TX_DW2_SWING_SEL_UPPER_SHIFT)
                | ((setting.dw2_swing_sel & 0b111) << PORT_TX_DW2_SWING_SEL_LOWER_SHIFT)
                | (0x98 << PORT_TX_DW2_RCOMP_SCALAR_SHIFT);
            tx_dw2.write(v);
        }

        // For PORT_TX_DW4:
        // - Set post cursor 1 from settings
        // - Set post cursor 2 to 0x0
        // - Set cursor coeff from settings
        for lane in lanes {
            let mut tx_dw4 = self.port_tx(PortTxReg::Dw4, lane).unwrap();
            let mut v = tx_dw4.read();
            v &= !(PORT_TX_DW4_POST_CURSOR_1_MASK
                | PORT_TX_DW4_POST_CURSOR_2_MASK
                | PORT_TX_DW4_CURSOR_COEFF_MASK);
            v |= (setting.dw4_post_cursor_1 << PORT_TX_DW4_POST_CURSOR_1_SHIFT)
                | (setting.dw4_cursor_coeff << PORT_TX_DW4_CURSOR_COEFF_SHIFT);
            tx_dw4.write(v);
        }

        // For PORT_TX_DW7:
        // - Set n scalar from settings
        for lane in lanes {
            let mut tx_dw7 = self.port_tx(PortTxReg::Dw7, lane).unwrap();
            // All other bits are spare
            tx_dw7.write(setting.dw7_n_scalar << PORT_TX_DW7_N_SCALAR_SHIFT);
        }

        // Set training enable to trigger update
        {
            let mut v = tx_dw5_ln0.read();
            v |= PORT_TX_DW5_TRAINING_ENABLE;
            tx_dw5_grp.write(v);
        }

        Ok(())
    }

    pub fn kabylake(gttmm: &Arc<MmioRegion>) -> Result<Vec<Self>> {
        let mut ddis = Vec::new();
        for (i, name) in [
            "A", "B", "C", "D",
            //TODO: missing AUX regs? "E",
        ]
        .iter()
        .enumerate()
        {
            ddis.push(Self {
                name,
                index: i,
                port_base: None, //TODO: port regs
                gttmm: gttmm.clone(),
                // IHD-OS-KBL-Vol 2c-1.17 DDI_AUX_CTL
                aux_ctl: unsafe { gttmm.mmio(0x64010 + i * 0x100)? },
                // IHD-OS-KBL-Vol 2c-1.17 DDI_AUX_DATA
                aux_datas: [
                    unsafe { gttmm.mmio(0x64014 + i * 0x100)? },
                    unsafe { gttmm.mmio(0x64018 + i * 0x100)? },
                    unsafe { gttmm.mmio(0x6401C + i * 0x100)? },
                    unsafe { gttmm.mmio(0x64020 + i * 0x100)? },
                    unsafe { gttmm.mmio(0x64024 + i * 0x100)? },
                ],
                // IHD-OS-KBL-Vol 2c-1.17 DDI_BUF_CTL
                buf_ctl: unsafe { gttmm.mmio(0x64000 + i * 0x100)? },
                // N/A
                dpclka_cfgcr0_clock_shift: None,
                dpclka_cfgcr0_clock_off: None,
                // IHD-OS-KBL-Vol 2c-1.17 GMBUS0
                gmbus_pin_pair: match *name {
                    "B" => Some(0b101),
                    "C" => Some(0b100),
                    "D" => Some(0b110),
                    _ => None,
                },
                // IHD-OS-KBL-Vol 12-1.17 GMBUS and GPIO
                gpio_port: match *name {
                    "B" => Some(GpioPort::Port4),
                    "C" => Some(GpioPort::Port3),
                    "D" => Some(GpioPort::Port5),
                    _ => None,
                },
                // IHD-OS-KBL-Vol 2c-1.17 PWR_WELL_CTL
                // All auxes go through the same Misc IO request
                pwr_well_ctl_aux_request: 1 << 1,
                pwr_well_ctl_aux_state: 1 << 0,
                pwr_well_ctl_ddi_request: match *name {
                    "A" | "E" => 1 << 3,
                    "B" => 1 << 5,
                    "C" => 1 << 7,
                    "D" => 1 << 9,
                    _ => unreachable!(),
                },
                pwr_well_ctl_ddi_state: match *name {
                    "A" | "E" => 1 << 2,
                    "B" => 1 << 4,
                    "C" => 1 << 6,
                    "D" => 1 << 8,
                    _ => unreachable!(),
                },
                // IHD-OS-KBL-Vol 2c-1.17 SDE_INTERRUPT
                sde_interrupt_hotplug: match *name {
                    "A" => Some(1 << 24),
                    "B" => Some(1 << 21),
                    "C" => Some(1 << 22),
                    "D" => Some(1 << 23),
                    "E" => Some(1 << 25),
                    _ => None,
                },
                // IHD-OS-KBL-Vol 2c-1.17 TRANS_CLK_SEL
                transcoder_index: match *name {
                    "B" => Some(0b010),
                    "C" => Some(0b011),
                    "D" => Some(0b100),
                    "E" => Some(0b101),
                    _ => None,
                },
            });
        }
        Ok(ddis)
    }

    pub fn tigerlake(gttmm: &Arc<MmioRegion>) -> Result<Vec<Self>> {
        let mut ddis = Vec::new();
        for (i, name) in [
            "A", "B", "C", "USBC1", "USBC2", "USBC3", "USBC4", "USBC5", "USBC6",
        ]
        .iter()
        .enumerate()
        {
            let port_base = match i {
                0 => Some(0x162000),
                1 => Some(0x6C000),
                2 => Some(0x160000),
                _ => None,
            };
            ddis.push(Self {
                name,
                index: i,
                port_base,
                gttmm: gttmm.clone(),
                // IHD-OS-TGL-Vol 2c-12.21 DDI_AUX_CTL
                aux_ctl: unsafe { gttmm.mmio(0x64010 + i * 0x100)? },
                // IHD-OS-TGL-Vol 2c-12.21 DDI_AUX_DATA
                aux_datas: [
                    unsafe { gttmm.mmio(0x64014 + i * 0x100)? },
                    unsafe { gttmm.mmio(0x64018 + i * 0x100)? },
                    unsafe { gttmm.mmio(0x6401C + i * 0x100)? },
                    unsafe { gttmm.mmio(0x64020 + i * 0x100)? },
                    unsafe { gttmm.mmio(0x64024 + i * 0x100)? },
                ],
                // IHD-OS-TGL-Vol 2c-12.21 DDI_BUF_CTL
                buf_ctl: unsafe { gttmm.mmio(0x64000 + i * 0x100)? },
                // IHD-OS-TGL-Vol 2c-12.21 DPCLKA_CFGCR0
                dpclka_cfgcr0_clock_shift: match i {
                    0 => Some(0),
                    1 => Some(2),
                    2 => Some(4),
                    _ => None,
                },
                dpclka_cfgcr0_clock_off: match i {
                    // DDI
                    0 => Some(1 << 10),
                    1 => Some(1 << 11),
                    2 => Some(1 << 24),
                    // Type C
                    3 => Some(1 << 12),
                    4 => Some(1 << 13),
                    5 => Some(1 << 14),
                    6 => Some(1 << 21),
                    7 => Some(1 << 22),
                    8 => Some(1 << 23),
                    _ => None,
                },
                //TODO: link to docs
                gmbus_pin_pair: match i {
                    // DDI pins
                    0 => Some(1),
                    1 => Some(2),
                    2 => Some(3),
                    // Type C pins
                    3 => Some(9),
                    4 => Some(10),
                    5 => Some(11),
                    6 => Some(12),
                    7 => Some(13),
                    8 => Some(14),
                    _ => None,
                },
                // IHD-OS-TGL-Vol 12-1.22-Rev2.0 GMBUS and GPIO
                gpio_port: match *name {
                    "A" => Some(GpioPort::Port1),
                    "B" => Some(GpioPort::Port2),
                    "C" => Some(GpioPort::Port3),
                    "USBC1" => Some(GpioPort::Port9),
                    "USBC2" => Some(GpioPort::Port10),
                    "USBC3" => Some(GpioPort::Port11),
                    "USBC4" => Some(GpioPort::Port12),
                    "USBC5" => Some(GpioPort::Port13),
                    "USBC6" => Some(GpioPort::Port14),
                    _ => None,
                },
                // IHD-OS-TGL-Vol 2c-12.21 PWR_WELL_CTL_AUX
                pwr_well_ctl_aux_request: 2 << (i * 2),
                pwr_well_ctl_aux_state: 1 << (i * 2),
                // IHD-OS-TGL-Vol 2c-12.21 PWR_WELL_CTL_DDI
                pwr_well_ctl_ddi_request: 2 << (i * 2),
                pwr_well_ctl_ddi_state: 1 << (i * 2),
                // IHD-OS-TGL-Vol 2c-12.21 SDE_INTERRUPT
                sde_interrupt_hotplug: match i {
                    0 => Some(1 << 16),
                    1 => Some(1 << 17),
                    2 => Some(1 << 18),
                    _ => None,
                },
                // IHD-OS-TGL-Vol 2c-12.21 TRANS_CLK_SEL
                transcoder_index: Some((i + 1) as u32),
            })
        }
        Ok(ddis)
    }

    pub fn alchemist(gttmm: &Arc<MmioRegion>) -> Result<Vec<Self>> {
        let mut ddis = Vec::new();
        for (i, name) in ["A", "B", "C", "USBC1", "USBC2", "USBC3", "USBC4", "D", "E"]
            .iter()
            .enumerate()
        {
            let port_base = match i {
                0 => Some(0x162000),
                1 => Some(0x6C000),
                2 => Some(0x160000),
                _ => None,
            };
            ddis.push(Self {
                name,
                index: i,
                port_base,
                gttmm: gttmm.clone(),
                // IHD-OS-ACM-Vol 2c-3.23 DDI_AUX_CTL
                aux_ctl: unsafe { gttmm.mmio(0x64010 + i * 0x100)? },
                // IHD-OS-ACM-Vol 2c-3.23 DDI_AUX_DATA
                aux_datas: [
                    unsafe { gttmm.mmio(0x64014 + i * 0x100)? },
                    unsafe { gttmm.mmio(0x64018 + i * 0x100)? },
                    unsafe { gttmm.mmio(0x6401C + i * 0x100)? },
                    unsafe { gttmm.mmio(0x64020 + i * 0x100)? },
                    unsafe { gttmm.mmio(0x64024 + i * 0x100)? },
                ],
                // IHD-OS-ACM-Vol 2c-3.23 DDI_BUF_CTL
                buf_ctl: unsafe { gttmm.mmio(0x64000 + i * 0x100)? },
                // IHD-OS-ACM-Vol 2c-3.23 DPCLKA_CFGCR0
                dpclka_cfgcr0_clock_shift: match i {
                    0 => Some(0),
                    1 => Some(2),
                    2 => Some(4),
                    _ => None,
                },
                dpclka_cfgcr0_clock_off: match i {
                    // DDI
                    0 => Some(1 << 10),
                    1 => Some(1 << 11),
                    2 => Some(1 << 24),
                    // Type C
                    3 => Some(1 << 12),
                    4 => Some(1 << 13),
                    5 => Some(1 << 14),
                    6 => Some(1 << 21),
                    7 => Some(1 << 22),
                    8 => Some(1 << 23),
                    _ => None,
                },
                //TODO: link to docs
                gmbus_pin_pair: match i {
                    // DDI pins
                    0 => Some(1),
                    1 => Some(2),
                    2 => Some(3),
                    // Type C pins
                    3 => Some(9),
                    4 => Some(10),
                    5 => Some(11),
                    6 => Some(12),
                    7 => Some(13),
                    8 => Some(14),
                    _ => None,
                },
                // IHD-OS-ACM-Vol 12-3.23 GMBUS and GPIO
                gpio_port: match *name {
                    "A" => Some(GpioPort::Port1),
                    "B" => Some(GpioPort::Port2),
                    "C" => Some(GpioPort::Port3),
                    "D" => Some(GpioPort::Port4),
                    "USBC1" => Some(GpioPort::Port9),
                    _ => None,
                },
                // IHD-OS-ACM-Vol 2c-3.23 PWR_WELL_CTL_AUX
                pwr_well_ctl_aux_request: 2 << (i * 2),
                pwr_well_ctl_aux_state: 1 << (i * 2),
                // IHD-OS-ACM-Vol 2c-3.23 PWR_WELL_CTL_DDI
                pwr_well_ctl_ddi_request: 2 << (i * 2),
                pwr_well_ctl_ddi_state: 1 << (i * 2),
                // IHD-OS-ACM-Vol 2c-3.23 SDE_INTERRUPT
                sde_interrupt_hotplug: match i {
                    0 => Some(1 << 16),
                    1 => Some(1 << 17),
                    2 => Some(1 << 18),
                    _ => None,
                },
                // IHD-OS-ACM-Vol 2c-3.23 TRANS_CLK_SEL
                transcoder_index: Some((i + 1) as u32),
            })
        }
        Ok(ddis)
    }
}
