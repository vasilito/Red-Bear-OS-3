use std::time::Duration;

use acpi::aml::{
    op_region::{OpRegion, RegionHandler, RegionSpace},
    AmlError,
};
use common::{
    io::{Io, Pio},
    timeout::Timeout,
};
use log::*;

const EC_DATA: u16 = 0x62;
const EC_SC: u16 = 0x66;

const OBF: u8 = 1 << 0; // output full / data ready for host <> empty
const IBF: u8 = 1 << 1; // input full / data ready for ec <> empty
const CMD: u8 = 1 << 3; // byte in data reg is command <> data
const BURST: u8 = 1 << 4; // burst mode <> normal mode
const SCI_EVT: u8 = 1 << 5; // sci event pending <> not
const SMI_EVT: u8 = 1 << 6; // smi event pending <> not

const RD_EC: u8 = 0x80;
const WR_EC: u8 = 0x81;
const BE_EC: u8 = 0x82;
const BD_EC: u8 = 0x83;
const QR_EC: u8 = 0x84;

const BURST_ACK: u8 = 0x90;

pub const DEFAULT_EC_TIMEOUT: Duration = Duration::from_millis(10);

#[repr(transparent)]
pub struct ScBits(u8);
#[allow(dead_code)]
impl ScBits {
    const fn obf(&self) -> bool {
        (self.0 & OBF) != 0
    }
    const fn ibf(&self) -> bool {
        (self.0 & IBF) != 0
    }
    const fn cmd(&self) -> bool {
        (self.0 & CMD) != 0
    }
    const fn burst(&self) -> bool {
        (self.0 & BURST) != 0
    }
    const fn sci_evt(&self) -> bool {
        (self.0 & SCI_EVT) != 0
    }
    const fn smi_evt(&self) -> bool {
        (self.0 & SMI_EVT) != 0
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Ec {
    sc: u16,
    data: u16,

    timeout: Duration,
}
impl Ec {
    pub fn new() -> Self {
        Self {
            sc: EC_SC,
            data: EC_DATA,
            timeout: DEFAULT_EC_TIMEOUT,
        }
    }
    #[allow(dead_code)]
    pub fn with_address(sc: u16, data: u16, timeout: Duration) -> Self {
        Self { sc, data, timeout }
    }
    #[inline]
    fn read_reg_sc(&self) -> ScBits {
        ScBits(Pio::<u8>::new(self.sc).read())
    }
    #[inline]
    fn read_reg_data(&self) -> u8 {
        Pio::<u8>::new(self.data).read()
    }
    #[inline]
    fn write_reg_sc(&self, value: u8) {
        Pio::<u8>::new(self.sc).write(value);
    }
    #[inline]
    fn write_reg_data(&self, value: u8) {
        Pio::<u8>::new(self.data).write(value);
    }
    #[inline]
    fn wait_for_write_ready(&self) -> Option<()> {
        let timeout = Timeout::new(self.timeout);
        loop {
            if !self.read_reg_sc().ibf() {
                return Some(());
            }
            timeout.run().ok()?;
        }
    }
    #[inline]
    fn wait_for_read_ready(&self) -> Option<()> {
        let timeout = Timeout::new(self.timeout);
        loop {
            if self.read_reg_sc().obf() {
                return Some(());
            }
            timeout.run().ok()?;
        }
    }

    //https://uefi.org/htmlspecs/ACPI_Spec_6_4_html/12_ACPI_Embedded_Controller_Interface_Specification/embedded-controller-command-set.html
    pub fn read(&self, address: u8) -> Option<u8> {
        trace!("ec read addr: {:x}", address);
        self.wait_for_write_ready()?;

        self.write_reg_sc(RD_EC);

        self.wait_for_write_ready()?;

        self.write_reg_data(address);

        self.wait_for_read_ready()?;

        let val = self.read_reg_data();
        trace!("got: {:x}", val);
        Some(val)
    }
    pub fn write(&self, address: u8, value: u8) -> Option<()> {
        trace!("ec write addr: {:x}, with: {:x}", address, value);
        self.wait_for_write_ready()?;

        self.write_reg_sc(WR_EC);

        self.wait_for_write_ready()?;

        self.write_reg_data(address);

        self.wait_for_write_ready()?;

        self.write_reg_data(value);
        trace!("done");
        Some(())
    }
    // disabled if not met
    //    First Access - 400 microseconds
    //    Subsequent Accesses - 50 microseconds each
    //    Total Burst Time - 1 millisecond
    //Accesses should be responded to within 50 microseconds.
    #[allow(dead_code)]
    fn enable_burst(&self) -> bool {
        trace!("ec burst enable");
        self.wait_for_write_ready();

        self.write_reg_sc(BE_EC);

        self.wait_for_read_ready();

        let res = self.read_reg_data() == BURST_ACK;
        trace!("success: {}", res);
        res
    }
    #[allow(dead_code)]
    fn disable_burst(&self) {
        trace!("ec burst disable");
        self.wait_for_write_ready();
        self.write_reg_sc(BD_EC);
        trace!("done");
    }
    //OSPM driver sends this command when the SCI_EVT flag in the EC_SC register is set.
    #[allow(dead_code)]
    fn queue_query(&mut self) -> u8 {
        trace!("ec query");
        self.wait_for_write_ready();

        self.write_reg_sc(QR_EC);

        self.wait_for_read_ready();

        let val = self.read_reg_data();
        trace!("got: {}", val);
        val
    }
}
impl RegionHandler for Ec {
    fn read_u8(
        &self,
        region: &acpi::aml::op_region::OpRegion,
        offset: usize,
    ) -> Result<u8, acpi::aml::AmlError> {
        assert_eq!(region.space, RegionSpace::EmbeddedControl);
        self.read(offset as u8).ok_or(AmlError::MutexAcquireTimeout) // TODO proper error type
    }
    fn write_u8(
        &self,
        region: &OpRegion,
        offset: usize,
        value: u8,
    ) -> Result<(), acpi::aml::AmlError> {
        assert_eq!(region.space, RegionSpace::EmbeddedControl);
        self.write(offset as u8, value)
            .ok_or(AmlError::MutexAcquireTimeout) // TODO proper error type
    }
    fn read_u16(&self, _region: &OpRegion, _offset: usize) -> Result<u16, acpi::aml::AmlError> {
        warn!("Got u16 EC read from AML!");
        Err(acpi::aml::AmlError::NoHandlerForRegionAccess(
            RegionSpace::EmbeddedControl,
        )) // TODO proper error type
    }
    fn read_u32(&self, _region: &OpRegion, _offset: usize) -> Result<u32, acpi::aml::AmlError> {
        warn!("Got u32 EC read from AML!");
        Err(acpi::aml::AmlError::NoHandlerForRegionAccess(
            RegionSpace::EmbeddedControl,
        )) // TODO proper error type
    }
    fn read_u64(&self, _region: &OpRegion, _offset: usize) -> Result<u64, acpi::aml::AmlError> {
        warn!("Got u64 EC read from AML!");
        Err(acpi::aml::AmlError::NoHandlerForRegionAccess(
            RegionSpace::EmbeddedControl,
        )) // TODO proper error type
    }
    fn write_u16(
        &self,
        _region: &OpRegion,
        _offset: usize,
        _value: u16,
    ) -> Result<(), acpi::aml::AmlError> {
        warn!("Got u16 EC write from AML!");
        Err(acpi::aml::AmlError::NoHandlerForRegionAccess(
            RegionSpace::EmbeddedControl,
        )) // TODO proper error type
    }
    fn write_u32(
        &self,
        _region: &OpRegion,
        _offset: usize,
        _value: u32,
    ) -> Result<(), acpi::aml::AmlError> {
        warn!("Got u32 EC write from AML!");
        Err(acpi::aml::AmlError::NoHandlerForRegionAccess(
            RegionSpace::EmbeddedControl,
        )) // TODO proper error type
    }
    fn write_u64(
        &self,
        _region: &OpRegion,
        _offset: usize,
        _value: u64,
    ) -> Result<(), acpi::aml::AmlError> {
        warn!("Got u64 EC write from AML!");
        Err(acpi::aml::AmlError::NoHandlerForRegionAccess(
            RegionSpace::EmbeddedControl,
        )) // TODO proper error type
    }
}
