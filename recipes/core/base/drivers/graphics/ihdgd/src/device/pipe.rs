use common::io::{Io, MmioPtr};
use range_alloc::RangeAllocator;
use syscall::error::Result;
use syscall::{Error, EIO};

use super::buffer::GpuBuffer;
use super::{GlobalGtt, MmioRegion};

pub const PLANE_CTL_ENABLE: u32 = 1 << 31;

pub const PLANE_WM_ENABLE: u32 = 1 << 31;
pub const PLANE_WM_LINES_SHIFT: u32 = 14;

#[derive(Debug)]
pub struct DeviceFb {
    pub buffer: GpuBuffer,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
}

impl DeviceFb {
    pub unsafe fn new(
        gm: &MmioRegion,
        surf: u32,
        width: u32,
        height: u32,
        stride: u32,
        clear: bool,
    ) -> Self {
        Self {
            buffer: unsafe { GpuBuffer::new(gm, surf, stride * height, clear) },
            width,
            height,
            stride,
        }
    }

    pub fn alloc(
        gm: &MmioRegion,
        ggtt: &mut GlobalGtt,
        width: u32,
        height: u32,
    ) -> syscall::Result<Self> {
        let (buffer, stride) = GpuBuffer::alloc_dumb(gm, ggtt, width, height)?;

        Ok(DeviceFb {
            buffer,
            width,
            height,
            stride,
        })
    }
}

pub struct Plane {
    pub name: &'static str,
    pub index: usize,
    pub buf_cfg: MmioPtr<u32>,
    pub color_ctl: Option<MmioPtr<u32>>,
    pub color_ctl_gamma_disable: u32,
    pub ctl: MmioPtr<u32>,
    pub ctl_source_rgb_8888: u32,
    pub ctl_source_mask: u32,
    pub offset: MmioPtr<u32>,
    pub pos: MmioPtr<u32>,
    pub size: MmioPtr<u32>,
    pub stride: MmioPtr<u32>,
    pub surf: MmioPtr<u32>,
    pub wm: [MmioPtr<u32>; 8],
    pub wm_trans: MmioPtr<u32>,
}

impl Plane {
    pub fn fetch_modeset(&self, alloc_buffers: &mut RangeAllocator<u32>) {
        let buf_cfg = self.buf_cfg.read();
        let buffer_start = buf_cfg & 0x7FF;
        let buffer_end = (buf_cfg >> 16) & 0x7FF;
        alloc_buffers
            .allocate_exact_range(buffer_start..(buffer_end + 1))
            .unwrap_or_else(|err| {
                panic!(
                    "failed to allocate pre-existing buffer blocks {} to {}: {:?}",
                    buffer_start, buffer_end, err
                );
            });
    }

    pub fn modeset(&mut self, alloc_buffers: &mut RangeAllocator<u32>) -> syscall::Result<()> {
        // FIXME handle runtime buffer reconfiguration
        //TODO: enable DBUF if more buffers needed
        //TODO: more blocks would mean better power usage
        // Minimum is 8 blocks for linear planes, 160 blocks is recommended for pre-OS init
        let buffer_size = 160;
        let buffer = alloc_buffers.allocate_range(buffer_size).map_err(|err| {
            log::warn!(
                "failed to allocate {} buffer blocks: {:?}",
                buffer_size,
                err
            );
            Error::new(EIO)
        })?;
        self.buf_cfg.write(buffer.start | (buffer.end << 16));

        //TODO: correct watermark calculation
        self.wm[0].write(PLANE_WM_ENABLE | (2 << PLANE_WM_LINES_SHIFT) | buffer.len() as u32);
        for i in 1..self.wm.len() {
            self.wm[i].writef(PLANE_WM_ENABLE, false);
        }
        self.wm_trans.writef(PLANE_WM_ENABLE, false);

        Ok(())
    }

    pub fn fetch_framebuffer(&self, gm: &MmioRegion, ggtt: &mut GlobalGtt) -> DeviceFb {
        let size = self.size.read();
        let width = (size & 0xFFFF) + 1;
        let height = ((size >> 16) & 0xFFFF) + 1;
        let stride_64 = self.stride.read() & 0x7FF;
        //TODO: this will be wrong for tiled planes
        let stride = stride_64 * 64;
        let surf = self.surf.read() & 0xFFFFF000;
        //TODO: read bits per pixel
        let surf_size = (stride * height).next_multiple_of(4096);
        ggtt.reserve(surf, surf_size);

        unsafe { DeviceFb::new(gm, surf, width, height, stride, true) }
    }

    pub fn set_framebuffer(&mut self, fb: &DeviceFb) {
        //TODO: documentation on this is not great
        let stride_64 = fb.stride / 64;

        self.size.write((fb.width - 1) | ((fb.height - 1) << 16));
        self.stride.write(stride_64);

        self.surf.write(fb.buffer.gm_offset);

        // Disable gamma
        if let Some(color_ctl) = &mut self.color_ctl {
            color_ctl.write(self.color_ctl_gamma_disable);
        }

        //TODO: more PLANE_CTL bits
        self.ctl.write(PLANE_CTL_ENABLE | self.ctl_source_rgb_8888);
    }

    pub fn dump(&self) {
        eprint!("Plane {}", self.name);
        eprint!(" buf_cfg {:08X}", self.buf_cfg.read());
        if let Some(reg) = &self.color_ctl {
            eprint!(" color_ctl {:08X}", reg.read());
        }
        eprint!(" ctl {:08X}", self.ctl.read());
        eprint!(" offset {:08X}", self.offset.read());
        eprint!(" pos {:08X}", self.offset.read());
        eprint!(" size {:08X}", self.size.read());
        eprint!(" stride {:08X}", self.stride.read());
        eprint!(" surf {:08X}", self.surf.read());
        for i in 0..self.wm.len() {
            eprint!(" wm_{} {:08X}", i, self.wm[i].read());
        }
        eprint!(" wm_trans {:08X}", self.wm_trans.read());
        eprintln!();
    }
}

pub struct Pipe {
    pub name: &'static str,
    pub index: usize,
    pub planes: Vec<Plane>,
    pub bottom_color: MmioPtr<u32>,
    pub misc: MmioPtr<u32>,
    pub srcsz: MmioPtr<u32>,
}

impl Pipe {
    pub fn dump(&self) {
        eprint!("Pipe {}", self.name);
        eprint!(" bottom_color {:08X}", self.bottom_color.read());
        eprint!(" misc {:08X}", self.misc.read());
        eprint!(" srcsz {:08X}", self.srcsz.read());
        eprintln!();
    }

    pub fn kabylake(gttmm: &MmioRegion) -> Result<Vec<Self>> {
        let mut pipes = Vec::with_capacity(3);
        for (i, name) in ["A", "B", "C"].iter().enumerate() {
            let mut planes = Vec::new();
            //TODO: cursor plane
            for (j, name) in ["1", "2", "3"].iter().enumerate() {
                planes.push(Plane {
                    name,
                    index: j,
                    // IHD-OS-KBL-Vol 2c-1.17 PLANE_BUF_CFG
                    buf_cfg: unsafe { gttmm.mmio(0x7027C + i * 0x1000 + j * 0x100)? },
                    // N/A
                    color_ctl: None,
                    color_ctl_gamma_disable: 0,
                    // IHD-OS-KBL-Vol 2c-1.17 PLANE_CTL
                    ctl: unsafe { gttmm.mmio(0x70180 + i * 0x1000 + j * 0x100)? },
                    ctl_source_rgb_8888: 0b0100 << 24,
                    ctl_source_mask: 0b1111 << 24,
                    // IHD-OS-KBL-Vol 2c-1.17 PLANE_OFFSET
                    offset: unsafe { gttmm.mmio(0x701A4 + i * 0x1000 + j * 0x100)? },
                    // IHD-OS-KBL-Vol 2c-1.17 PLANE_POS
                    pos: unsafe { gttmm.mmio(0x7018C + i * 0x1000 + j * 0x100)? },
                    // IHD-OS-KBL-Vol 2c-1.17 PLANE_SIZE
                    size: unsafe { gttmm.mmio(0x70190 + i * 0x1000 + j * 0x100)? },
                    // IHD-OS-KBL-Vol 2c-1.17 PLANE_STRIDE
                    stride: unsafe { gttmm.mmio(0x70188 + i * 0x1000 + j * 0x100)? },
                    // IHD-OS-KBL-Vol 2c-1.17 PLANE_SURF
                    surf: unsafe { gttmm.mmio(0x7019C + i * 0x1000 + j * 0x100)? },
                    // IHD-OS-KBL-Vol 2c-1.17 PLANE_WM
                    wm: [
                        unsafe { gttmm.mmio(0x70240 + i * 0x1000 + j * 0x100)? },
                        unsafe { gttmm.mmio(0x70244 + i * 0x1000 + j * 0x100)? },
                        unsafe { gttmm.mmio(0x70248 + i * 0x1000 + j * 0x100)? },
                        unsafe { gttmm.mmio(0x7024C + i * 0x1000 + j * 0x100)? },
                        unsafe { gttmm.mmio(0x70250 + i * 0x1000 + j * 0x100)? },
                        unsafe { gttmm.mmio(0x70254 + i * 0x1000 + j * 0x100)? },
                        unsafe { gttmm.mmio(0x70258 + i * 0x1000 + j * 0x100)? },
                        unsafe { gttmm.mmio(0x7025C + i * 0x1000 + j * 0x100)? },
                    ],
                    wm_trans: unsafe { gttmm.mmio(0x70268 + i * 0x1000 + j * 0x100)? },
                });
            }
            pipes.push(Pipe {
                name,
                index: i,
                planes,
                // IHD-OS-KBL-Vol 2c-1.17 PIPE_BOTTOM_COLOR
                bottom_color: unsafe { gttmm.mmio(0x70034 + i * 0x1000)? },
                // IHD-OS-KBL-Vol 2c-1.17 PIPE_MISC
                misc: unsafe { gttmm.mmio(0x70030 + i * 0x1000)? },
                // IHD-OS-KBL-Vol 2c-1.17 PIPE_SRCSZ
                srcsz: unsafe { gttmm.mmio(0x6001C + i * 0x1000)? },
            })
        }
        Ok(pipes)
    }

    pub fn tigerlake(gttmm: &MmioRegion) -> Result<Vec<Self>> {
        let mut pipes = Vec::with_capacity(4);
        for (i, name) in ["A", "B", "C", "D"].iter().enumerate() {
            let mut planes = Vec::new();
            //TODO: cursor plane
            for (j, name) in ["1", "2", "3", "4", "5", "6", "7"].iter().enumerate() {
                planes.push(Plane {
                    name,
                    index: j,
                    // IHD-OS-TGL-Vol 2c-12.21 PLANE_BUF_CFG
                    buf_cfg: unsafe { gttmm.mmio(0x7027C + i * 0x1000 + j * 0x100)? },
                    // IHD-OS-TGL-Vol 2c-12.21 PLANE_COLOR_CTL
                    color_ctl: Some(unsafe { gttmm.mmio(0x701CC + i * 0x1000 + j * 0x100)? }),
                    color_ctl_gamma_disable: 1 << 13,
                    // IHD-OS-TGL-Vol 2c-12.21 PLANE_CTL
                    ctl: unsafe { gttmm.mmio(0x70180 + i * 0x1000 + j * 0x100)? },
                    ctl_source_rgb_8888: 0b01000 << 23,
                    ctl_source_mask: 0b11111 << 23,
                    // IHD-OS-TGL-Vol 2c-12.21 PLANE_OFFSET
                    offset: unsafe { gttmm.mmio(0x701A4 + i * 0x1000 + j * 0x100)? },
                    // IHD-OS-TGL-Vol 2c-12.21 PLANE_POS
                    pos: unsafe { gttmm.mmio(0x7018C + i * 0x1000 + j * 0x100)? },
                    // IHD-OS-TGL-Vol 2c-12.21 PLANE_SIZE
                    size: unsafe { gttmm.mmio(0x70190 + i * 0x1000 + j * 0x100)? },
                    // IHD-OS-TGL-Vol 2c-12.21 PLANE_STRIDE
                    stride: unsafe { gttmm.mmio(0x70188 + i * 0x1000 + j * 0x100)? },
                    // IHD-OS-TGL-Vol 2c-12.21 PLANE_SURF
                    surf: unsafe { gttmm.mmio(0x7019C + i * 0x1000 + j * 0x100)? },
                    // IHD-OS-TGL-Vol 2c-12.21 PLANE_WM
                    wm: [
                        unsafe { gttmm.mmio(0x70240 + i * 0x1000 + j * 0x100)? },
                        unsafe { gttmm.mmio(0x70244 + i * 0x1000 + j * 0x100)? },
                        unsafe { gttmm.mmio(0x70248 + i * 0x1000 + j * 0x100)? },
                        unsafe { gttmm.mmio(0x7024C + i * 0x1000 + j * 0x100)? },
                        unsafe { gttmm.mmio(0x70250 + i * 0x1000 + j * 0x100)? },
                        unsafe { gttmm.mmio(0x70254 + i * 0x1000 + j * 0x100)? },
                        unsafe { gttmm.mmio(0x70258 + i * 0x1000 + j * 0x100)? },
                        unsafe { gttmm.mmio(0x7025C + i * 0x1000 + j * 0x100)? },
                    ],
                    wm_trans: unsafe { gttmm.mmio(0x70268 + i * 0x1000 + j * 0x100)? },
                });
            }
            pipes.push(Pipe {
                name,
                index: i,
                planes,
                // IHD-OS-TGL-Vol 2c-12.21 PIPE_BOTTOM_COLOR
                bottom_color: unsafe { gttmm.mmio(0x70034 + i * 0x1000)? },
                // IHD-OS-TGL-Vol 2c-12.21 PIPE_MISC
                misc: unsafe { gttmm.mmio(0x70030 + i * 0x1000)? },
                // IHD-OS-TGL-Vol 2c-12.21 PIPE_SRCSZ
                srcsz: unsafe { gttmm.mmio(0x6001C + i * 0x1000)? },
            })
        }
        Ok(pipes)
    }

    pub fn alchemist(gttmm: &MmioRegion) -> Result<Vec<Self>> {
        let mut pipes = Vec::with_capacity(4);
        for (i, name) in ["A", "B", "C", "D"].iter().enumerate() {
            let mut planes = Vec::new();
            //TODO: cursor plane
            for (j, name) in ["1", "2", "3", "4", "5"].iter().enumerate() {
                planes.push(Plane {
                    name,
                    index: j,
                    // IHD-OS-ACM-Vol 2c-3.23 PLANE_BUF_CFG
                    buf_cfg: unsafe { gttmm.mmio(0x7057C + i * 0x1000 + j * 0x100)? },
                    // IHD-OS-ACM-Vol 2c-3.23 PLANE_COLOR_CTL
                    color_ctl: Some(unsafe { gttmm.mmio(0x704CC + i * 0x1000 + j * 0x100)? }),
                    color_ctl_gamma_disable: 1 << 13,
                    // IHD-OS-ACM-Vol 2c-3.23 PLANE_CTL
                    ctl: unsafe { gttmm.mmio(0x70480 + i * 0x1000 + j * 0x100)? },
                    ctl_source_rgb_8888: 0b01000 << 23,
                    ctl_source_mask: 0b11111 << 23,
                    // IHD-OS-ACM-Vol 2c-3.23 PLANE_OFFSET
                    offset: unsafe { gttmm.mmio(0x704A4 + i * 0x1000 + j * 0x100)? },
                    // IHD-OS-ACM-Vol 2c-3.23 PLANE_POS
                    pos: unsafe { gttmm.mmio(0x7048C + i * 0x1000 + j * 0x100)? },
                    // IHD-OS-ACM-Vol 2c-3.23 PLANE_SIZE
                    size: unsafe { gttmm.mmio(0x70490 + i * 0x1000 + j * 0x100)? },
                    // IHD-OS-ACM-Vol 2c-3.23 PLANE_STRIDE
                    stride: unsafe { gttmm.mmio(0x70488 + i * 0x1000 + j * 0x100)? },
                    // IHD-OS-ACM-Vol 2c-3.23 PLANE_SURF
                    surf: unsafe { gttmm.mmio(0x7049C + i * 0x1000 + j * 0x100)? },
                    // IHD-OS-ACM-Vol 2c-3.23 PLANE_WM
                    wm: [
                        unsafe { gttmm.mmio(0x70540 + i * 0x1000 + j * 0x100)? },
                        unsafe { gttmm.mmio(0x70544 + i * 0x1000 + j * 0x100)? },
                        unsafe { gttmm.mmio(0x70548 + i * 0x1000 + j * 0x100)? },
                        unsafe { gttmm.mmio(0x7054C + i * 0x1000 + j * 0x100)? },
                        unsafe { gttmm.mmio(0x70550 + i * 0x1000 + j * 0x100)? },
                        unsafe { gttmm.mmio(0x70554 + i * 0x1000 + j * 0x100)? },
                        unsafe { gttmm.mmio(0x70558 + i * 0x1000 + j * 0x100)? },
                        unsafe { gttmm.mmio(0x7055C + i * 0x1000 + j * 0x100)? },
                    ],
                    wm_trans: unsafe { gttmm.mmio(0x70568 + i * 0x1000 + j * 0x100)? },
                });
            }
            pipes.push(Pipe {
                name,
                index: i,
                planes,
                // IHD-OS-ACM-Vol 2c-3.23 PIPE_BOTTOM_COLOR
                bottom_color: unsafe { gttmm.mmio(0x70034 + i * 0x1000)? },
                // IHD-OS-ACM-Vol 2c-3.23 PIPE_MISC
                misc: unsafe { gttmm.mmio(0x70030 + i * 0x1000)? },
                // IHD-OS-ACM-Vol 2c-3.23 PIPE_SRCSZ
                srcsz: unsafe { gttmm.mmio(0x6001C + i * 0x1000)? },
            })
        }
        Ok(pipes)
    }
}
