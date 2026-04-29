use common::{
    io::{Io, MmioPtr},
    timeout::Timeout,
};
use embedded_hal::blocking::i2c::{self, Operation, SevenBitAddress, Transactional};

use super::MmioRegion;

const GMBUS1_SW_RDY: u32 = 1 << 30;
const GMBUS1_CYCLE_STOP: u32 = 1 << 27;
const GMBUS1_CYCLE_INDEX: u32 = 1 << 26;
const GMBUS1_CYCLE_WAIT: u32 = 1 << 25;
const GMBUS1_SIZE_SHIFT: u32 = 16;
const GMBUS1_INDEX_SHIFT: u32 = 8;

const GMBUS2_HW_RDY: u32 = 1 << 11;
const GMBUS2_ACTIVE: u32 = 1 << 9;

pub struct Gmbus {
    regs: [MmioPtr<u32>; 6],
}

impl Gmbus {
    pub unsafe fn new(gttmm: &MmioRegion) -> syscall::Result<Self> {
        Ok(Self {
            regs: [
                gttmm.mmio(0xC5100)?,
                gttmm.mmio(0xC5104)?,
                gttmm.mmio(0xC5108)?,
                gttmm.mmio(0xC510C)?,
                gttmm.mmio(0xC5110)?,
                gttmm.mmio(0xC5120)?,
            ],
        })
    }

    pub fn pin_pair<'a>(&'a mut self, pin_pair: u8) -> GmbusPinPair<'a> {
        GmbusPinPair {
            regs: &mut self.regs,
            pin_pair,
        }
    }
}

pub struct GmbusPinPair<'a> {
    regs: &'a mut [MmioPtr<u32>; 6],
    pin_pair: u8,
}

impl<'a> Transactional for GmbusPinPair<'a> {
    type Error = ();
    fn exec(&mut self, addr7: SevenBitAddress, ops: &mut [Operation<'_>]) -> Result<(), ()> {
        let mut ops_iter = ops.iter_mut();
        //TODO: gmbus is actually smbus, not fully i2c compatible!
        // The first operation MUST be a write of the index
        let index = match ops_iter.next() {
            Some(Operation::Write(buf)) if buf.len() == 1 => buf[0],
            unsupported => {
                log::error!("GMBUS unsupported first operation {:?}", unsupported);
                return Err(());
            }
        };

        // Reset
        self.regs[1].write(0);

        // Set pin pair, enabling interface
        self.regs[0].write(self.pin_pair as u32);

        for op in ops_iter {
            // Start operation
            let (addr8, size) = match op {
                Operation::Read(buf) => ((addr7 << 1) | 1, buf.len() as u32),
                Operation::Write(buf) => (addr7 << 1, buf.len() as u32),
            };
            if size >= 512 {
                log::error!("GMBUS transaction size {} too large", size);
                return Err(());
            }
            self.regs[1].write(
                GMBUS1_SW_RDY
                    | GMBUS1_CYCLE_INDEX
                    | GMBUS1_CYCLE_WAIT
                    | (size << GMBUS1_SIZE_SHIFT)
                    | (index as u32) << GMBUS1_INDEX_SHIFT
                    | (addr8 as u32),
            );

            // Perform transaction
            match op {
                Operation::Read(buf) => {
                    for chunk in buf.chunks_mut(4) {
                        {
                            //TODO: ideal timeout for gmbus read?
                            let timeout = Timeout::from_millis(10);
                            while !self.regs[2].readf(GMBUS2_HW_RDY) {
                                timeout.run().map_err(|()| {
                                    log::debug!(
                                        "timeout on GMBUS read 0x{:08x}",
                                        self.regs[2].read()
                                    );
                                    ()
                                })?;
                            }
                        }

                        let bytes = self.regs[3].read().to_le_bytes();
                        chunk.copy_from_slice(&bytes[..chunk.len()]);
                    }
                }
                Operation::Write(buf) => {
                    log::warn!("TODO: GMBUS WRITE");
                    return Err(());
                }
            }
        }

        // Stop transaction
        self.regs[1].write(GMBUS1_SW_RDY | GMBUS1_CYCLE_STOP);

        // Wait idle
        let timeout = Timeout::from_millis(10);
        while self.regs[2].readf(GMBUS2_ACTIVE) {
            timeout.run().map_err(|()| {
                log::debug!("timeout on GMBUS active 0x{:08x}", self.regs[2].read());
                ()
            })?;
        }

        // Disable GMBUS interface
        self.regs[0].write(0);

        Ok(())
    }
}

impl<'a> i2c::WriteRead for GmbusPinPair<'a> {
    type Error = ();
    fn write_read(
        &mut self,
        addr7: SevenBitAddress,
        bytes: &[u8],
        buffer: &mut [u8],
    ) -> Result<(), ()> {
        self.exec(
            addr7,
            &mut [Operation::Write(bytes), Operation::Read(buffer)],
        )
    }
}
