use common::{
    io::{Io, MmioPtr},
    timeout::Timeout,
};
use pcid_interface::{PciFunction, PciFunctionHandle};
use range_alloc::RangeAllocator;
use std::{collections::VecDeque, fmt, mem, sync::Arc};
use syscall::error::{Error, Result, EIO, ENODEV, ERANGE};

mod aux;
mod bios;
use self::bios::*;
mod buffer;
mod ddi;
use self::ddi::*;
mod dpll;
use self::dpll::*;
mod gmbus;
pub use self::gmbus::*;
mod gpio;
pub use self::gpio::*;
mod ggtt;
use ggtt::*;
mod hal;
pub use self::hal::*;
mod pipe;
use self::pipe::*;
mod power;
use self::power::*;
mod scheme;
mod transcoder;
use self::transcoder::*;

//TODO: move to common?
pub struct CallbackGuard<'a, T, F: FnOnce(&mut T)> {
    value: &'a mut T,
    fini: Option<F>,
}

impl<'a, T, F: FnOnce(&mut T)> CallbackGuard<'a, T, F> {
    // Note that fini will also run if init fails
    pub fn new(value: &'a mut T, init: impl FnOnce(&mut T) -> Result<()>, fini: F) -> Result<Self> {
        let mut this = Self {
            value,
            fini: Some(fini),
        };
        init(&mut this.value)?;
        Ok(this)
    }
}

impl<'a, T, F: FnOnce(&mut T)> Drop for CallbackGuard<'a, T, F> {
    fn drop(&mut self) {
        let fini = self.fini.take().unwrap();
        fini(&mut self.value);
    }
}

pub struct ChangeDetect {
    name: &'static str,
    reg: MmioPtr<u32>,
    value: u32,
}

impl ChangeDetect {
    fn new(name: &'static str, reg: MmioPtr<u32>) -> Self {
        let value = reg.read();
        Self { name, reg, value }
    }

    fn log(&self) {
        log::info!("{} {:08X}", self.name, self.value);
    }

    fn check(&mut self) {
        let value = self.reg.read();
        if value != self.value {
            self.value = value;
            self.log();
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum DeviceKind {
    KabyLake,
    TigerLake,
    Alchemist,
}

pub enum Event {
    DdiHotplug(&'static str),
}

pub struct InterruptRegs {
    // Interrupt status register, has live status of interrupts
    pub isr: MmioPtr<u32>,
    // Interrupt mask register, masks isr for iir, 0 is unmasked
    pub imr: MmioPtr<u32>,
    // Interrupt identity register, write 1 to clear
    pub iir: MmioPtr<u32>,
    // Interrupt enable register, 1 allows interrupt to propogate
    pub ier: MmioPtr<u32>,
}

pub struct Interrupter {
    change_detects: Vec<ChangeDetect>,
    display_int_ctl: MmioPtr<u32>,
    display_int_ctl_enable: u32,
    display_int_ctl_sde: u32,
    gfx_mstr_intr: Option<MmioPtr<u32>>,
    gfx_mstr_intr_display: u32,
    gfx_mstr_intr_enable: u32,
    sde_interrupt: InterruptRegs,
}

#[derive(Debug)]
pub struct MmioRegion {
    phys: usize,
    virt: usize,
    size: usize,
}

impl MmioRegion {
    fn new(phys: usize, size: usize, memory_type: common::MemoryType) -> Result<Self> {
        let virt = unsafe { common::physmap(phys, size, common::Prot::RW, memory_type)? as usize };
        Ok(Self { phys, virt, size })
    }

    unsafe fn mmio(&self, offset: usize) -> Result<MmioPtr<u32>> {
        // Any errors here will return ERANGE
        let err = Error::new(ERANGE);
        if offset.checked_add(mem::size_of::<u32>()).ok_or(err)? > self.size {
            return Err(err);
        }
        let addr = self.virt.checked_add(offset).ok_or(err)?;
        Ok(unsafe { MmioPtr::new(addr as *mut u32) })
    }
}

impl Drop for MmioRegion {
    fn drop(&mut self) {
        unsafe {
            let _ = libredox::call::munmap(self.virt as *mut (), self.size);
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum VideoInput {
    Hdmi,
    Dp,
}

pub struct Device {
    kind: DeviceKind,
    alloc_buffers: RangeAllocator<u32>,
    bios: Option<Bios>,
    ddis: Vec<Ddi>,
    dpclka_cfgcr0: Option<MmioPtr<u32>>,
    dplls: Vec<Dpll>,
    events: VecDeque<Event>,
    framebuffers: Vec<DeviceFb>,
    int: Interrupter,
    gttmm: Arc<MmioRegion>,
    ggtt: GlobalGtt,
    gm: MmioRegion,
    gmbus: Gmbus,
    pipes: Vec<Pipe>,
    power_wells: PowerWells,
    ref_freq: u64,
    transcoders: Vec<Transcoder>,
}

impl fmt::Debug for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Device")
            .field("kind", &self.kind)
            .field("alloc_buffers", &self.alloc_buffers)
            .field("gttmm", &self.gttmm)
            .field("gm", &self.gm)
            .field("ref_freq", &self.ref_freq)
            .finish_non_exhaustive()
    }
}

impl Device {
    pub fn new(pcid_handle: &mut PciFunctionHandle, func: &PciFunction) -> Result<Self> {
        let kind = match (func.full_device_id.vendor_id, func.full_device_id.device_id) {
            // Kaby Lake
            (0x8086, 0x5912) |
            (0x8086, 0x5916) |
            (0x8086, 0x591B) |
            (0x8086, 0x591E) |
            (0x8086, 0x5926) |
            // Comet Lake, seems to be compatible with Kaby Lake
            (0x8086, 0x9B21) |
            (0x8086, 0x9B41) |
            (0x8086, 0x9BA4) |
            (0x8086, 0x9BAA) |
            (0x8086, 0x9BAC) |
            (0x8086, 0x9BC4) |
            (0x8086, 0x9BC5) |
            (0x8086, 0x9BC6) |
            (0x8086, 0x9BC8) |
            (0x8086, 0x9BCA) |
            (0x8086, 0x9BCC) |
            (0x8086, 0x9BE6) |
            (0x8086, 0x9BF6) => {
                DeviceKind::KabyLake
            }
            // Tiger Lake
            (0x8086, 0x9A40) |
            (0x8086, 0x9A49) |
            (0x8086, 0x9A60) |
            (0x8086, 0x9A68) |
            (0x8086, 0x9A70) |
            (0x8086, 0x9A78) => {
                DeviceKind::TigerLake
            }
            // Alchemist
            (0x8086, 0x5690) | // A770M
            (0x8086, 0x5691) | // A730M
            (0x8086, 0x5692) | // A550M
            (0x8086, 0x5693) | // A370M
            (0x8086, 0x5694) | // A350M
            (0x8086, 0x5696) | // A570M
            (0x8086, 0x5697) | // A530M
            (0x8086, 0x56A0) | // A770
            (0x8086, 0x56A1) | // A750
            (0x8086, 0x56A5) | // A380
            (0x8086, 0x56A6) | // A310
            (0x8086, 0x56B0) | // Pro A30M
            (0x8086, 0x56B1) | // Pro A40/A50
            (0x8086, 0x56B2) | // Pro A60M
            (0x8086, 0x56B3) | // Pro A60
            (0x8086, 0x56C0) | // GPU Flex 170
            (0x8086, 0x56C1)   // GPU Flex 140
            => {
                DeviceKind::Alchemist
            }
            (vendor_id, device_id) => {
                log::error!("unsupported ID {:04X}:{:04X}", vendor_id, device_id);
                return Err(Error::new(ENODEV));
            }
        };

        let gttmm = {
            let (phys, size) = func.bars[0].expect_mem();
            Arc::new(MmioRegion::new(
                phys,
                size,
                common::MemoryType::Uncacheable,
            )?)
        };
        log::info!("GTTMM {:X?}", gttmm);
        let gm = {
            let (phys, size) = func.bars[2].expect_mem();
            MmioRegion::new(phys, size, common::MemoryType::WriteCombining)?
        };
        log::info!("GM {:X?}", gm);
        /* IOBAR not used, not present on all generations
        let iobar = func.bars[4].expect_port();
        log::debug!("IOBAR {:X?}", iobar);
        */

        // IGD OpRegion/Software SCI/_DSM for Skylake Processors
        let bios_base = unsafe { pcid_handle.read_config(0xFC) };
        let bios = if bios_base != 0 {
            log::info!("BIOS {:X?}", bios_base);
            // This is the default BIOS size
            let bios_size = 8 * 1024;
            match MmioRegion::new(
                bios_base as usize,
                bios_size,
                common::MemoryType::Uncacheable,
            ) {
                Ok(region) => match Bios::new(region) {
                    Ok(bios) => Some(bios),
                    Err(err) => {
                        log::warn!("failed to parse BIOS at {:08X}: {}", bios_base, err);
                        None
                    }
                },
                Err(err) => {
                    log::warn!("failed to map BIOS at {:08X}: {}", bios_base, err);
                    None
                }
            }
        } else {
            None
        };

        let ggtt = unsafe {
            GlobalGtt::new(
                pcid_handle,
                gttmm.clone(),
                //TODO: how to use 64-bit surface addresses?
                gm.size.min(u32::MAX as usize) as u32,
            )
        };
        //unsafe { ggtt.reset() };

        // GMBUS seems to be stable for all generations
        let gmbus = unsafe { Gmbus::new(&gttmm)? };

        let dpclka_cfgcr0;
        let int;
        let ref_freq;
        match kind {
            DeviceKind::KabyLake => {
                dpclka_cfgcr0 = None;

                int = Interrupter {
                    change_detects: Vec::new(),
                    // IHD-OS-KBL-Vol 2c-1.17 MASTER_INT_CTL
                    display_int_ctl: unsafe { gttmm.mmio(0x44200)? },
                    display_int_ctl_enable: 1 << 31,
                    display_int_ctl_sde: 1 << 23,
                    gfx_mstr_intr: None,
                    gfx_mstr_intr_display: 0,
                    gfx_mstr_intr_enable: 0,
                    sde_interrupt: InterruptRegs {
                        isr: unsafe { gttmm.mmio(0xC4000)? },
                        imr: unsafe { gttmm.mmio(0xC4004)? },
                        iir: unsafe { gttmm.mmio(0xC4008)? },
                        ier: unsafe { gttmm.mmio(0xC400C)? },
                    },
                };

                // IHD-OS-KBL-Vol 12-1.17
                ref_freq = 24_000_000;
            }
            DeviceKind::TigerLake | DeviceKind::Alchemist => {
                // TigerLake: IHD-OS-TGL-Vol 2c-12.21
                // Alchemist: IHD-OS-ACM-Vol 2c-3.23

                dpclka_cfgcr0 = Some(unsafe { gttmm.mmio(0x164280)? });

                let dssm = unsafe { gttmm.mmio(0x51004)? };
                log::debug!("dssm {:08X}", dssm.read());

                const DSSM_REF_FREQ_24_MHZ: u32 = 0b000 << 29;
                const DSSM_REF_FREQ_19_2_MHZ: u32 = 0b001 << 29;
                const DSSM_REF_FREQ_38_4_MHZ: u32 = 0b010 << 29;
                const DSSM_REF_FREQ_MASK: u32 = 0b111 << 29;
                ref_freq = match dssm.read() & DSSM_REF_FREQ_MASK {
                    DSSM_REF_FREQ_24_MHZ => 24_000_000,
                    DSSM_REF_FREQ_19_2_MHZ => 19_200_000,
                    DSSM_REF_FREQ_38_4_MHZ => 38_400_000,
                    unknown => {
                        log::error!("unknown DSSM reference frequency {}", unknown);
                        return Err(Error::new(EIO));
                    }
                };

                int = Interrupter {
                    change_detects: vec![
                        ChangeDetect::new("de_hpd_interrupt", unsafe { gttmm.mmio(0x44470)? }),
                        ChangeDetect::new("de_port_interrupt", unsafe { gttmm.mmio(0x44440)? }),
                        ChangeDetect::new("shotplug_ctl_ddi", unsafe { gttmm.mmio(0xC4030)? }),
                        ChangeDetect::new("shotplug_ctl_tc", unsafe { gttmm.mmio(0xC4034)? }),
                        ChangeDetect::new("tbt_hotplug_ctl", unsafe { gttmm.mmio(0x44030)? }),
                        ChangeDetect::new("tc_hotplug_ctl", unsafe { gttmm.mmio(0x44038)? }),
                    ],
                    display_int_ctl: unsafe { gttmm.mmio(0x44200)? },
                    display_int_ctl_enable: 1 << 31,
                    display_int_ctl_sde: 1 << 23,
                    gfx_mstr_intr: Some(unsafe { gttmm.mmio(0x190010)? }),
                    gfx_mstr_intr_display: 1 << 16,
                    gfx_mstr_intr_enable: 1 << 31,
                    sde_interrupt: InterruptRegs {
                        isr: unsafe { gttmm.mmio(0xC4000)? },
                        imr: unsafe { gttmm.mmio(0xC4004)? },
                        iir: unsafe { gttmm.mmio(0xC4008)? },
                        ier: unsafe { gttmm.mmio(0xC400C)? },
                    },
                };
            }
        }

        let ddis;
        let dplls;
        let pipes;
        let power_wells;
        let transcoders;
        match kind {
            DeviceKind::KabyLake => {
                ddis = Ddi::kabylake(&gttmm)?;
                //TODO: kaby lake dplls
                dplls = Vec::new();
                pipes = Pipe::kabylake(&gttmm)?;
                power_wells = PowerWells::kabylake(&gttmm)?;
                transcoders = Transcoder::kabylake(&gttmm)?;
            }
            DeviceKind::TigerLake => {
                ddis = Ddi::tigerlake(&gttmm)?;
                dplls = Dpll::tigerlake(&gttmm)?;
                pipes = Pipe::tigerlake(&gttmm)?;
                power_wells = PowerWells::tigerlake(&gttmm)?;
                transcoders = Transcoder::tigerlake(&gttmm)?;
            }
            DeviceKind::Alchemist => {
                // Many registers are identical to tigerlake
                dplls = Dpll::tigerlake(&gttmm)?;
                pipes = Pipe::alchemist(&gttmm)?;
                // FIXME transcoders are probably different too
                transcoders = Transcoder::tigerlake(&gttmm)?;
                // Power wells are distinct
                ddis = Ddi::alchemist(&gttmm)?;
                power_wells = PowerWells::alchemist(&gttmm)?;
            }
        }

        //TODO: get number of available buffers
        let buffers = 1024;
        Ok(Self {
            kind,
            alloc_buffers: RangeAllocator::new(0..buffers),
            bios,
            ddis,
            dpclka_cfgcr0,
            dplls,
            events: VecDeque::new(),
            framebuffers: Vec::new(),
            int,
            gttmm,
            ggtt,
            gm,
            gmbus,
            pipes,
            power_wells,
            ref_freq,
            transcoders,
        })
    }

    pub fn init_inner(&mut self) {
        // Discover current framebuffers
        self.alloc_buffers.reset();
        self.framebuffers.clear();
        for pipe in self.pipes.iter() {
            for plane in pipe.planes.iter() {
                if plane.ctl.readf(PLANE_CTL_ENABLE) {
                    plane.fetch_modeset(&mut self.alloc_buffers);

                    self.framebuffers
                        .push(plane.fetch_framebuffer(&self.gm, &mut self.ggtt));
                }
            }
        }

        // Probe all DDIs
        let ddi_names: Vec<&str> = self.ddis.iter().map(|ddi| ddi.name).collect();
        for ddi_name in ddi_names {
            self.probe_ddi(ddi_name).expect("failed to probe DDI");
        }

        self.dump();

        log::info!(
            "device initialized with {} framebuffers",
            self.framebuffers.len()
        );

        // Enable SDE interrupts
        {
            let mut mask = 0;
            for ddi in self.ddis.iter() {
                if let Some(sde_interrupt_hotplug) = ddi.sde_interrupt_hotplug {
                    mask |= sde_interrupt_hotplug;
                }
            }
            let sde_int = &mut self.int.sde_interrupt;
            // Enable DDI hotplug interrupts
            sde_int.ier.write(mask);
            // Clear identity register
            sde_int.iir.write(sde_int.iir.read());
            // Unmask all interrupts
            sde_int.imr.write(0);
        }
        // Enable display interrupts
        self.int
            .display_int_ctl
            .write(self.int.display_int_ctl_enable);
        if let Some(gfx_mstr_intr) = &mut self.int.gfx_mstr_intr {
            // Enable graphics interrupts
            gfx_mstr_intr.write(self.int.gfx_mstr_intr_enable);
        }
        for change_detect in self.int.change_detects.iter_mut() {
            change_detect.log();
        }
    }

    pub fn dump(&self) {
        for ddi in self.ddis.iter() {
            if ddi.buf_ctl.readf(DDI_BUF_CTL_ENABLE) {
                ddi.dump();
            }
        }

        if let Some(dpclka_cfgcr0) = &self.dpclka_cfgcr0 {
            eprintln!("dpclka_cfgcr0 {:08X}", dpclka_cfgcr0.read());
        }
        for dpll in self.dplls.iter() {
            if dpll.enable.readf(DPLL_ENABLE_ENABLE) {
                dpll.dump();
            }
        }

        for (transcoder, pipe) in self.transcoders.iter().zip(self.pipes.iter()) {
            if transcoder.conf.readf(TRANS_CONF_ENABLE) {
                transcoder.dump();
                pipe.dump();
                for plane in pipe.planes.iter() {
                    if plane.index == 0 || plane.ctl.readf(PLANE_CTL_ENABLE) {
                        eprint!("  ");
                        plane.dump();
                    }
                }
            }
        }
    }

    pub fn probe_ddi(&mut self, name: &str) -> Result<bool> {
        let Some(ddi) = self.ddis.iter_mut().find(|ddi| ddi.name == name) else {
            log::warn!("DDI {} not found", name);
            return Err(Error::new(EIO));
        };

        // Enable DDI power well
        self.power_wells.enable_well_by_ddi(ddi.name)?;

        let Some((source, edid_data)) =
            ddi.probe_edid(&mut self.power_wells, &self.gttmm, &mut self.gmbus)?
        else {
            return Ok(false);
        };

        let edid = match edid::parse(&edid_data).to_full_result() {
            Ok(edid) => {
                log::info!("DDI {} EDID from {}: {:?}", ddi.name, source, edid);
                edid
            }
            Err(err) => {
                log::warn!(
                    "DDI {} failed to parse EDID from {}: {:?}",
                    ddi.name,
                    source,
                    err
                );
                // Will try again but not fail the driver
                return Ok(false);
            }
        };

        let timing_opt = edid.descriptors.iter().find_map(|desc| match desc {
            edid::Descriptor::DetailedTiming(timing) => Some(timing),
            _ => None,
        });
        let Some(timing) = timing_opt else {
            log::warn!(
                "DDI {} EDID from {} missing detailed timing",
                ddi.name,
                source
            );
            // Will try again but not fail the driver
            return Ok(false);
        };

        let mut modeset = |ddi: &mut Ddi, input: VideoInput| -> Result<()> {
            // IHD-OS-TGL-Vol 12-1.22-Rev2.0 "Sequences for HDMI and DVI"

            // Power wells should already be enabled

            //TODO: Type-C needs aux power enabled and max lanes set

            // Enable port PLL without SSC. Not required on Type-C ports
            if let Some(clock_shift) = ddi.dpclka_cfgcr0_clock_shift {
                // Find free DPLL
                let dpll = self
                    .dplls
                    .iter_mut()
                    .find(|dpll| !dpll.enable.readf(DPLL_ENABLE_ENABLE))
                    .ok_or_else(|| {
                        log::error!("failed to find free DPLL");
                        Error::new(EIO)
                    })?;

                // DPLL power guard
                let mut dpll_enable = unsafe { MmioPtr::new(dpll.enable.as_mut_ptr()) };
                let dpll_power_guard = CallbackGuard::new(
                    &mut dpll_enable,
                    |dpll_enable| {
                        // Enable DPLL power
                        dpll_enable.writef(DPLL_ENABLE_POWER_ENABLE, true);
                        //TODO: timeout not specified in docs, should be very fast
                        let timeout = Timeout::from_micros(1);
                        while !dpll_enable.readf(DPLL_ENABLE_POWER_STATE) {
                            timeout.run().map_err(|()| {
                                log::debug!("timeout while enabling DPLL {} power", dpll.name);
                                Error::new(EIO)
                            })?;
                        }
                        Ok(())
                    },
                    |dpll_enable| {
                        // Disable DPLL power
                        dpll_enable.writef(DPLL_ENABLE_POWER_ENABLE, false);
                    },
                )?;

                match input {
                    VideoInput::Hdmi => {
                        // Set SSC enable/disable. For HDMI, always disable
                        dpll.ssc.writef(DPLL_SSC_ENABLE, false);

                        // Configure DPLL frequency
                        dpll.set_freq_hdmi(self.ref_freq, &timing)?;
                    }
                    VideoInput::Dp => {
                        log::warn!("DPLL for DisplayPort not implemented");
                        return Err(Error::new(EIO));
                    }
                }

                //TODO: "Sequence Before Frequency Change"

                // Enable DPLL
                //TODO: use guard?
                {
                    dpll.enable.writef(DPLL_ENABLE_ENABLE, true);
                    let timeout = Timeout::from_micros(50);
                    while !dpll.enable.readf(DPLL_ENABLE_LOCK) {
                        timeout.run().map_err(|()| {
                            log::debug!("timeout while enabling DPLL {}", dpll.name);
                            Error::new(EIO)
                        })?;
                    }
                }

                //TODO: "Sequence After Frequency Change"

                // Update DPLL mapping
                if let Some(dpclka_cfgcr0) = &mut self.dpclka_cfgcr0 {
                    const DPCLKA_CFGCR0_CLOCK_MASK: u32 = 0b11;

                    let mut v = dpclka_cfgcr0.read();
                    v &= !(DPCLKA_CFGCR0_CLOCK_MASK << clock_shift);
                    v |= dpll.dpclka_cfgcr0_clock_value << clock_shift;
                    dpclka_cfgcr0.write(v);
                }

                // Continue to allow DPLL power
                mem::forget(dpll_power_guard);
            }

            // Enable DPLL clock (must be done separately from PLL mapping)
            if let Some(dpclka_cfgcr0) = &mut self.dpclka_cfgcr0 {
                if let Some(clock_off) = ddi.dpclka_cfgcr0_clock_off {
                    dpclka_cfgcr0.writef(clock_off, false);
                }
            }

            // Enable IO power
            //TODO: the request can be shared by multiple DDIs
            //TODO: skip if TBT
            let pwr_well_ctl_ddi_request = ddi.pwr_well_ctl_ddi_request;
            let pwr_well_ctl_ddi_state = ddi.pwr_well_ctl_ddi_state;
            let mut pwr_well_ctl_ddi =
                unsafe { MmioPtr::new(self.power_wells.ctl_ddi.as_mut_ptr()) };
            let pwr_guard = CallbackGuard::new(
                &mut pwr_well_ctl_ddi,
                |pwr_well_ctl_ddi| {
                    // Enable IO power
                    pwr_well_ctl_ddi.writef(pwr_well_ctl_ddi_request, true);
                    let timeout = Timeout::from_micros(30);
                    while !pwr_well_ctl_ddi.readf(pwr_well_ctl_ddi_state) {
                        timeout.run().map_err(|()| {
                            log::debug!("timeout while requesting DDI {} IO power", ddi.name);
                            Error::new(EIO)
                        })?;
                    }
                    Ok(())
                },
                |pwr_well_ctl_ddi| {
                    // Disable IO power
                    pwr_well_ctl_ddi.writef(pwr_well_ctl_ddi_request, false);
                },
            )?;

            //TODO: Type-C DP_MODE

            // Enable planes, pipe, and transcoder
            {
                // Find free transcoder with free pipe
                let mut transcoder_pipe = None;
                for (transcoder, pipe) in self.transcoders.iter_mut().zip(self.pipes.iter_mut()) {
                    if transcoder.conf.readf(TRANS_CONF_ENABLE) {
                        continue;
                    }
                    //TODO: how would we know if pipe is in use?
                    transcoder_pipe = Some((transcoder, pipe));
                    break;
                }
                let Some((transcoder, pipe)) = transcoder_pipe else {
                    log::error!("free transcoder and pipe not found");
                    return Err(Error::new(EIO));
                };

                // Enable pipe and transcoder power wells
                self.power_wells.enable_well_by_pipe(pipe.name)?;
                self.power_wells
                    .enable_well_by_transcoder(transcoder.name)?;

                // Configure transcoder clock select
                if let Some(transcoder_index) = ddi.transcoder_index {
                    transcoder
                        .clk_sel
                        .write(transcoder_index << transcoder.clk_sel_shift);
                }

                // Set pipe bottom color to blue for debugging
                pipe.bottom_color.write(0x3FF);

                // Configure and enable planes
                //TODO: THIS IS HACKY
                if let Some(plane) = pipe.planes.first_mut() {
                    let width = timing.horizontal_active_pixels as u32;
                    let height = timing.vertical_active_lines as u32;

                    let fb = DeviceFb::alloc(&self.gm, &mut self.ggtt, width, height)?;

                    plane.modeset(&mut self.alloc_buffers)?;
                    plane.set_framebuffer(&fb);

                    self.framebuffers.push(fb);
                }

                //TODO: VGA and panel fitter steps?

                // Configure transcoder timings and other pipe and transcoder settings
                transcoder.modeset(pipe, &timing);

                // Configure and enable TRANS_DDI_FUNC_CTL
                {
                    let mut ddi_func_ctl = TRANS_DDI_FUNC_CTL_ENABLE |
                        //TODO: allow different bits per color
                        TRANS_DDI_FUNC_CTL_BPC_8 |
                        //TODO: correct port width selection
                        TRANS_DDI_FUNC_CTL_PORT_WIDTH_4;

                    if let Some(transcoder_index) = ddi.transcoder_index {
                        ddi_func_ctl |= transcoder_index << transcoder.ddi_func_ctl_ddi_shift;
                    }

                    match input {
                        VideoInput::Hdmi => {
                            ddi_func_ctl |= TRANS_DDI_FUNC_CTL_MODE_HDMI;

                            // Set HDMI scrambling and high TMDS char rate based on symbol rate > 340 MHz
                            if timing.pixel_clock > 340_000 {
                                ddi_func_ctl |= transcoder.ddi_func_ctl_hdmi_scrambling
                                    | transcoder.ddi_func_ctl_high_tmds_char_rate;
                            }
                        }
                        VideoInput::Dp => {
                            //TODO: MST
                            ddi_func_ctl |= TRANS_DDI_FUNC_CTL_MODE_DP_SST;
                        }
                    }

                    match (timing.features >> 3) & 0b11 {
                        // Digital sync, separate
                        0b11 => {
                            if (timing.features & (1 << 2)) != 0 {
                                ddi_func_ctl |= TRANS_DDI_FUNC_CTL_SYNC_POLARITY_VSHIGH;
                            }
                            if (timing.features & (1 << 1)) != 0 {
                                ddi_func_ctl |= TRANS_DDI_FUNC_CTL_SYNC_POLARITY_HSHIGH;
                            }
                        }
                        unsupported => {
                            log::warn!("unsupported sync {:#x}", unsupported);
                        }
                    }

                    transcoder.ddi_func_ctl.write(ddi_func_ctl);
                }

                // Configure and enable TRANS_CONF
                let mut conf = transcoder.conf.read();
                // Set mode to progressive
                conf &= !TRANS_CONF_MODE_MASK;
                // Enable transcoder
                conf |= TRANS_CONF_ENABLE;
                transcoder.conf.write(conf);
                //TODO: what is the correct timeout?
                let timeout = Timeout::from_millis(100);
                while !transcoder.conf.readf(TRANS_CONF_STATE) {
                    timeout.run().map_err(|()| {
                        log::error!(
                            "timeout on DDI {} transcoder {} enable",
                            ddi.name,
                            transcoder.name
                        );
                        Error::new(EIO)
                    })?;
                }
            }

            // Enable port
            {
                // Configure voltage swing and related IO settings
                match input {
                    VideoInput::Hdmi => {
                        ddi.voltage_swing_hdmi(&self.gttmm, &timing)?;
                    }
                    VideoInput::Dp => {
                        //TODO ddi.voltage_swing_dp(&self.gttmm)?;
                        log::error!("voltage swing for DP not implemented");
                        return Err(Error::new(EIO));
                    }
                }

                // Configure PORT_CL_DW10 static power down to power up all lanes
                //TODO: only power up required lanes
                if let Some(mut port_cl_dw10) = ddi.port_cl(PortClReg::Dw10) {
                    port_cl_dw10.writef(0b1111 << 4, false);
                }

                // Configure and enable DDI_BUF_CTL
                //TODO: more DDI_BUF_CTL bits?
                ddi.buf_ctl.writef(DDI_BUF_CTL_ENABLE, true);

                // Wait for DDI_BUF_CTL IDLE = 0, timeout after 500 us
                let timeout = Timeout::from_micros(500);
                while ddi.buf_ctl.readf(DDI_BUF_CTL_IDLE) {
                    timeout.run().map_err(|()| {
                        log::warn!("timeout while waiting for DDI {} active", ddi.name);
                        Error::new(EIO)
                    })?;
                }
            }

            // Keep IO power on if finished
            mem::forget(pwr_guard);

            Ok(())
        };

        if ddi.buf_ctl.readf(DDI_BUF_CTL_IDLE) {
            log::info!("DDI {} idle, will attempt mode setting", ddi.name);
            const EDID_VIDEO_INPUT_UNDEFINED: u8 = (1 << 7) | 0b0000;
            const EDID_VIDEO_INPUT_DVI: u8 = (1 << 7) | 0b0001;
            const EDID_VIDEO_INPUT_HDMI_A: u8 = (1 << 7) | 0b0010;
            const EDID_VIDEO_INPUT_HDMI_B: u8 = (1 << 7) | 0b0011;
            const EDID_VIDEO_INPUT_DP: u8 = (1 << 7) | 0b0101;
            const EDID_VIDEO_INPUT_MASK: u8 = (1 << 7) | 0b1111;
            let input = match edid_data[20] & EDID_VIDEO_INPUT_MASK {
                //TODO: how to accurately discover input type?
                //TODO: HDMI often shows up as undefined, do others?
                EDID_VIDEO_INPUT_UNDEFINED
                | EDID_VIDEO_INPUT_DVI
                | EDID_VIDEO_INPUT_HDMI_A
                | EDID_VIDEO_INPUT_HDMI_B => VideoInput::Hdmi,
                EDID_VIDEO_INPUT_DP => VideoInput::Dp,
                unknown => {
                    log::warn!("EDID video input 0x{:02X} not supported", unknown);
                    return Err(Error::new(EIO));
                }
            };
            //TODO: DisplayPort modeset not complete
            match modeset(ddi, input) {
                Ok(()) => {
                    log::info!("DDI {} modeset {:?} finished", ddi.name, input);
                }
                Err(err) => {
                    log::warn!("DDI {} modeset {:?} failed: {}", ddi.name, input, err);
                    // Will try again but not fail the driver
                    return Ok(false);
                }
            }
        } else {
            log::info!("DDI {} already active", ddi.name);
        }

        Ok(true)
    }

    pub fn handle_display_irq(&mut self) -> bool {
        let display_ints = self.int.display_int_ctl.read() & !self.int.display_int_ctl_enable;
        if display_ints != 0 {
            log::info!("  display ints {:08X}", display_ints);
            if display_ints & self.int.display_int_ctl_sde != 0 {
                let sde_ints = self.int.sde_interrupt.iir.read();
                self.int.sde_interrupt.iir.write(sde_ints);
                log::info!("    south display engine ints {:08X}", sde_ints);
                for ddi in self.ddis.iter() {
                    if let Some(sde_interrupt_hotplug) = ddi.sde_interrupt_hotplug {
                        if sde_ints & sde_interrupt_hotplug == sde_interrupt_hotplug {
                            self.events.push_back(Event::DdiHotplug(ddi.name));
                        }
                    }
                }
            }
            true
        } else {
            false
        }
    }

    pub fn handle_irq(&mut self) -> bool {
        let had_irq = if let Some(gfx_mstr_intr) = &mut self.int.gfx_mstr_intr {
            let gfx_ints = gfx_mstr_intr.read() & !self.int.gfx_mstr_intr_enable;
            if gfx_ints != 0 {
                log::info!("gfx ints {:08X}", gfx_ints);
                gfx_mstr_intr.write(gfx_ints | self.int.gfx_mstr_intr_enable);

                if gfx_ints & self.int.gfx_mstr_intr_display != 0 {
                    self.handle_display_irq();
                }

                true
            } else {
                false
            }
        } else {
            self.handle_display_irq()
        };

        if had_irq {
            for change_detect in self.int.change_detects.iter_mut() {
                change_detect.check();
            }
        }

        had_irq
    }

    pub fn handle_events(&mut self) {
        while let Some(event) = self.events.pop_front() {
            match event {
                Event::DdiHotplug(ddi_name) => {
                    log::info!("DDI {} plugged", ddi_name);
                    for _attempt in 0..4 {
                        //TODO: gmbus times out!
                        match self.probe_ddi(ddi_name) {
                            Ok(true) => {
                                break;
                            }
                            Ok(false) => {
                                log::warn!("timeout probing {}", ddi_name);
                            }
                            Err(err) => {
                                log::warn!("failed to probe {}: {}", ddi_name, err);
                            }
                        }
                        //TODO: do this asynchronously so scheme events can be handled
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }
                }
            }
        }
    }
}
