use common::{
    io::{Io, MmioPtr},
    timeout::Timeout,
};
use syscall::error::{Error, Result, EIO};

use super::MmioRegion;

#[derive(Clone, Copy)]
pub struct PowerWell {
    pub name: &'static str,
    pub depends: &'static [&'static str],
    pub ddis: &'static [&'static str],
    pub pipes: &'static [&'static str],
    pub transcoders: &'static [&'static str],
    pub request: u32,
    pub state: u32,
    pub fuse_status: u32,
}

pub struct PowerWells {
    pub ctl: MmioPtr<u32>,
    pub ctl_aux: MmioPtr<u32>,
    pub ctl_ddi: MmioPtr<u32>,
    pub fuse_status: MmioPtr<u32>,
    pub fuse_status_pg0: u32,
    pub wells: Vec<PowerWell>,
}

impl PowerWells {
    //TODO: return guard?
    pub fn enable_well(&mut self, name: &'static str) -> Result<()> {
        // Wait 20us for distribution of PG0
        {
            let timeout = Timeout::from_micros(20);
            while !self.fuse_status.readf(self.fuse_status_pg0) {
                timeout.run().map_err(|()| {
                    log::warn!("timeout on distribution of power well 0");
                    Error::new(EIO)
                })?;
            }
        }

        // self.wells iter copied to allow mutable self.enable_well later
        for well in self.wells.iter().copied() {
            if well.name == name {
                // Enable dependent wells
                for depend in well.depends.iter() {
                    self.enable_well(depend)?;
                }

                if !self.ctl.readf(well.request) {
                    log::info!("enabling power well {}", well.name);
                }

                // Set request bit
                self.ctl.writef(well.request, true);

                // Wait 100us for enabled state
                {
                    let timeout = Timeout::from_micros(100);
                    while !self.ctl.readf(well.state) {
                        timeout.run().map_err(|()| {
                            log::warn!("timeout enabling power well {}", well.name);
                            Error::new(EIO)
                        })?;
                    }
                }

                // Wait 20us for distribution
                {
                    let timeout = Timeout::from_micros(20);
                    while !self.fuse_status.readf(well.fuse_status) {
                        timeout.run().map_err(|()| {
                            log::warn!("timeout on distribution of power well {}", well.name);
                            Error::new(EIO)
                        })?;
                    }
                }

                return Ok(());
            }
        }
        log::warn!("power well {} not found", name);
        Err(Error::new(EIO))
    }

    pub fn enable_well_by_ddi(&mut self, name: &'static str) -> Result<()> {
        for well in self.wells.iter() {
            if well.ddis.contains(&name) {
                return self.enable_well(well.name);
            }
        }
        log::warn!("power well for DDI {} not found", name);
        Err(Error::new(EIO))
    }

    pub fn enable_well_by_pipe(&mut self, name: &'static str) -> Result<()> {
        for well in self.wells.iter() {
            if well.pipes.contains(&name) {
                return self.enable_well(well.name);
            }
        }
        log::warn!("power well for pipe {} not found", name);
        Err(Error::new(EIO))
    }

    pub fn enable_well_by_transcoder(&mut self, name: &'static str) -> Result<()> {
        for well in self.wells.iter() {
            if well.transcoders.contains(&name) {
                return self.enable_well(well.name);
            }
        }
        log::warn!("power well for transcoder {} not found", name);
        Err(Error::new(EIO))
    }

    pub fn kabylake(gttmm: &MmioRegion) -> Result<Self> {
        // IHD-OS-KBL-Vol 2c-1.17 PWR_WELL_CTL
        let ctl = unsafe { gttmm.mmio(0x45404)? };
        // Hack since these power ctl registers are combined
        let ctl_aux = unsafe { gttmm.mmio(0x45404)? };
        let ctl_ddi = unsafe { gttmm.mmio(0x45404)? };
        // IHD-OS-KBL-Vol 2c-1.17 FUSE_STATUS
        let fuse_status = unsafe { gttmm.mmio(0x42000)? };
        let fuse_status_pg0 = 1 << 27;
        let wells = vec![
            PowerWell {
                name: "1",
                depends: &[],
                ddis: &["A"],
                pipes: &["A"],
                transcoders: &["EDP"],
                request: 1 << 29,
                state: 1 << 28,
                fuse_status: 1 << 26,
            },
            PowerWell {
                name: "2",
                depends: &["1"],
                ddis: &["B", "C", "D", "E"],
                pipes: &["B", "C"],
                transcoders: &["A", "B", "C"],
                request: 1 << 31,
                state: 1 << 30,
                fuse_status: 1 << 25,
            },
        ];
        Ok(Self {
            ctl,
            ctl_aux,
            ctl_ddi,
            fuse_status,
            fuse_status_pg0,
            wells,
        })
    }

    pub fn tigerlake(gttmm: &MmioRegion) -> Result<Self> {
        // IHD-OS-TGL-Vol 2c-12.21 PWR_WELL_CTL
        let ctl = unsafe { gttmm.mmio(0x45404)? };
        // IHD-OS-TGL-Vol 2c-12.21 PWR_WELL_CTL_AUX
        let ctl_aux = unsafe { gttmm.mmio(0x45444)? };
        // IHD-OS-TGL-Vol 2c-12.21 PWR_WELL_CTL_DDI
        let ctl_ddi = unsafe { gttmm.mmio(0x45454)? };
        // IHD-OS-TGL-Vol 2c-12.21 FUSE_STATUS
        let fuse_status = unsafe { gttmm.mmio(0x42000)? };
        let fuse_status_pg0 = 1 << 27;
        let wells = vec![
            // DBUF functionality, Pipe A, Transcoder A and DSI, DDI A-C, FBC, DSS
            PowerWell {
                name: "1",
                depends: &[],
                ddis: &["A", "B", "C"],
                pipes: &["A"],
                transcoders: &["A"],
                request: 1 << 1,
                state: 1 << 0,
                fuse_status: 1 << 26,
            },
            // VDSC for pipe A
            PowerWell {
                name: "2",
                depends: &["1"],
                ddis: &[],
                pipes: &[],
                transcoders: &[],
                request: 1 << 3,
                state: 1 << 2,
                fuse_status: 1 << 25,
            },
            // Pipe B, Audio, Transcoder WD, VGA, Transcoder B, DDI USBC1-6, KVMR
            PowerWell {
                name: "3",
                depends: &["2"],
                ddis: &["USBC1", "USBC2", "USBC3", "USBC4", "USBC5", "USBC6"],
                pipes: &["B"],
                transcoders: &["B"],
                request: 1 << 5,
                state: 1 << 4,
                fuse_status: 1 << 24,
            },
            // Pipe C, Transcoder C
            PowerWell {
                name: "4",
                depends: &["3"],
                ddis: &[],
                pipes: &["C"],
                transcoders: &["C"],
                request: 1 << 7,
                state: 1 << 6,
                fuse_status: 1 << 23,
            },
            // Pipe D, Transcoder D
            PowerWell {
                name: "5",
                depends: &["4"],
                ddis: &[],
                pipes: &["D"],
                transcoders: &["D"],
                request: 1 << 9,
                state: 1 << 8,
                fuse_status: 1 << 22,
            },
        ];
        Ok(Self {
            ctl,
            ctl_aux,
            ctl_ddi,
            fuse_status,
            fuse_status_pg0,
            wells,
        })
    }

    pub fn alchemist(gttmm: &MmioRegion) -> Result<Self> {
        // IHD-OS-ACM-Vol 2c-3.23 PWR_WELL_CTL
        let ctl = unsafe { gttmm.mmio(0x45404)? };
        // IHD-OS-ACM-Vol 2c-3.23 PWR_WELL_CTL_AUX
        let ctl_aux = unsafe { gttmm.mmio(0x45444)? };
        // IHD-OS-ACM-Vol 2c-3.23 PWR_WELL_CTL_DDI
        let ctl_ddi = unsafe { gttmm.mmio(0x45454)? };
        // IHD-OS-ACM-Vol 2c-3.23 FUSE_STATUS
        let fuse_status = unsafe { gttmm.mmio(0x42000)? };
        let fuse_status_pg0 = 1 << 27;
        let wells = vec![
            // DBUF functionality, Transcoder A, DDI A-B
            PowerWell {
                name: "1",
                depends: &[],
                ddis: &["A", "B"],
                pipes: &[],
                transcoders: &["A"],
                request: 1 << 1,
                state: 1 << 0,
                fuse_status: 1 >> 26,
            },
            // Audio playback, Transcoder WD, VGA, DDI C-E, Type-C, KVMR
            PowerWell {
                name: "2",
                depends: &["1"],
                ddis: &["C", "D", "E", "USBC1", "USBC2", "USBC3", "USBC4"],
                pipes: &[],
                transcoders: &[],
                request: 1 << 3,
                state: 1 << 2,
                fuse_status: 1 << 25,
            },
            // Pipe A, FBC
            PowerWell {
                name: "A",
                depends: &["1"],
                ddis: &[],
                pipes: &["A"],
                transcoders: &[],
                request: 1 << 11,
                state: 1 << 10,
                fuse_status: 1 << 21,
            },
            // Pipe B, Transcoder B
            PowerWell {
                name: "B",
                depends: &["2"],
                ddis: &[],
                pipes: &["B"],
                transcoders: &["B"],
                request: 1 << 13,
                state: 1 << 12,
                fuse_status: 1 << 20,
            },
            // Pipe C, Transcoder C
            PowerWell {
                name: "C",
                depends: &["2"],
                ddis: &[],
                pipes: &["C"],
                transcoders: &["C"],
                request: 1 << 15,
                state: 1 << 14,
                fuse_status: 1 << 19,
            },
            // Pipe D, Transcoder D
            PowerWell {
                name: "D",
                depends: &["2"],
                ddis: &[],
                pipes: &["D"],
                transcoders: &["D"],
                request: 1 << 17,
                state: 1 << 16,
                fuse_status: 1 << 18,
            },
        ];
        Ok(Self {
            ctl,
            ctl_aux,
            ctl_ddi,
            fuse_status,
            fuse_status_pg0,
            wells,
        })
    }
}
