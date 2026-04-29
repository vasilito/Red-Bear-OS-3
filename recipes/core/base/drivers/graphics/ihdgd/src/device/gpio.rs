use std::convert::Infallible;
use std::time::Duration;

use common::io::{Io, MmioPtr};
use embedded_hal::digital::v2 as digital;

use crate::device::HalTimer;

use super::MmioRegion;

const GPIO_DIR_MASK: u32 = 1 << 0;
const GPIO_DIR_OUT: u32 = 1 << 1;
const GPIO_VAL_MASK: u32 = 1 << 2;
const GPIO_VAL_OUT: u32 = 1 << 3;
const GPIO_VAL_IN: u32 = 1 << 4;
const GPIO_CLOCK_SHIFT: u32 = 0;
const GPIO_DATA_SHIFT: u32 = 8;

#[derive(Copy, Clone, Debug)]
#[repr(usize)]
pub enum GpioPort {
    Port0 = 0xC5010,
    Port1 = 0xC5014,
    Port2 = 0xC5018,
    Port3 = 0xC501C,
    Port4 = 0xC5020,
    Port5 = 0xC5024,
    Port6 = 0xC5028,
    Port7 = 0xC502C,
    Port8 = 0xC5030,
    Port9 = 0xC5034,
    Port10 = 0xC5038,
    Port11 = 0xC503C,
    Port12 = 0xC5040,
    Port13 = 0xC5044,
    Port14 = 0xC5048,
    Port15 = 0xC504C,
}

impl GpioPort {
    pub unsafe fn i2c(
        &self,
        gttmm: &MmioRegion,
    ) -> syscall::Result<bitbang_hal::i2c::I2cBB<GpioPin, GpioPin, HalTimer>> {
        let i2c_freq = 100_000.0;
        let (scl, sda) = unsafe {
            (
                GpioPin {
                    ctl: gttmm.mmio(*self as usize)?,
                    shift: GPIO_CLOCK_SHIFT,
                },
                GpioPin {
                    ctl: gttmm.mmio(*self as usize)?,
                    shift: GPIO_DATA_SHIFT,
                },
            )
        };
        Ok(bitbang_hal::i2c::I2cBB::new(
            scl,
            sda,
            HalTimer::new(Duration::from_secs_f64(1.0 / i2c_freq)),
        ))
    }
}

pub struct GpioPin {
    ctl: MmioPtr<u32>,
    shift: u32,
}

impl digital::InputPin for GpioPin {
    type Error = Infallible;

    fn is_high(&self) -> Result<bool, Infallible> {
        Ok(((self.ctl.read() >> self.shift) & GPIO_VAL_IN) == GPIO_VAL_IN)
    }

    fn is_low(&self) -> Result<bool, Infallible> {
        Ok(((self.ctl.read() >> self.shift) & GPIO_VAL_IN) == 0)
    }
}

impl digital::OutputPin for GpioPin {
    type Error = Infallible;

    fn set_low(&mut self) -> Result<(), Infallible> {
        // Set GPIO to output with value 0
        let value = GPIO_DIR_MASK | GPIO_DIR_OUT | GPIO_VAL_MASK;
        self.ctl.write(value << self.shift);
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Infallible> {
        // Assuming external pull-up, set GPIO to input
        let value = GPIO_DIR_MASK;
        self.ctl.write(value << self.shift);
        Ok(())
    }
}
