use std::alloc::{self, Layout};
use std::convert::TryInto;
use std::ptr::{self, NonNull};
use std::sync::Mutex;

use driver_graphics::kms::connector::{KmsConnectorDriver, KmsConnectorStatus};
use driver_graphics::kms::objects::{KmsCrtc, KmsCrtcState, KmsObjectId, KmsObjects};
use driver_graphics::{Buffer, CursorPlane, Damage, GraphicsAdapter};
use drm_sys::{
    DRM_CAP_DUMB_BUFFER, DRM_CAP_DUMB_PREFER_SHADOW, DRM_CLIENT_CAP_CURSOR_PLANE_HOTSPOT,
};
use syscall::{EINVAL, PAGE_SIZE};

#[derive(Debug)]
pub struct FbAdapter {
    pub framebuffers: Vec<FrameBuffer>,
}

#[derive(Debug)]
pub struct Connector {
    width: u32,
    height: u32,
    framebuffer_id: usize,
}

impl KmsConnectorDriver for Connector {
    type State = ();
}

impl GraphicsAdapter for FbAdapter {
    type Connector = Connector;
    type Crtc = ();

    type Buffer = GraphicScreen;
    type Framebuffer = ();

    fn name(&self) -> &'static [u8] {
        b"vesad"
    }

    fn desc(&self) -> &'static [u8] {
        b"VESA"
    }

    fn init(&mut self, objects: &mut KmsObjects<Self>) {
        for (framebuffer_id, framebuffer) in self.framebuffers.iter().enumerate() {
            let crtc = objects.add_crtc((), ());

            objects.add_connector(
                Connector {
                    width: framebuffer.width as u32,
                    height: framebuffer.height as u32,
                    framebuffer_id,
                },
                (),
                &[crtc],
            );
        }
    }

    fn get_cap(&self, cap: u32) -> syscall::Result<u64> {
        match cap {
            DRM_CAP_DUMB_BUFFER => Ok(1),
            DRM_CAP_DUMB_PREFER_SHADOW => Ok(0),
            _ => Err(syscall::Error::new(EINVAL)),
        }
    }

    fn set_client_cap(&self, cap: u32, _value: u64) -> syscall::Result<()> {
        match cap {
            DRM_CLIENT_CAP_CURSOR_PLANE_HOTSPOT => Ok(()),
            _ => Err(syscall::Error::new(EINVAL)),
        }
    }

    fn probe_connector(&mut self, objects: &mut KmsObjects<Self>, id: KmsObjectId) {
        let mut connector = objects.get_connector(id).unwrap().lock().unwrap();
        let connector = &mut *connector;
        connector.connection = KmsConnectorStatus::Connected;
        connector.update_from_size(connector.driver_data.width, connector.driver_data.height);
    }

    fn create_dumb_buffer(&mut self, width: u32, height: u32) -> (Self::Buffer, u32) {
        (
            GraphicScreen::new(width as usize, height as usize),
            width * 4,
        )
    }

    fn map_dumb_buffer(&mut self, framebuffer: &Self::Buffer) -> *mut u8 {
        framebuffer.ptr.as_ptr().cast::<u8>()
    }

    fn create_framebuffer(&mut self, _buffer: &Self::Buffer) -> Self::Framebuffer {
        ()
    }

    fn set_crtc(
        &mut self,
        objects: &KmsObjects<Self>,
        crtc: &Mutex<KmsCrtc<Self>>,
        state: KmsCrtcState<Self>,
        damage: Damage,
    ) -> syscall::Result<()> {
        let mut crtc = crtc.lock().unwrap();
        let buffer = state
            .fb_id
            .map(|fb_id| objects.get_framebuffer(fb_id))
            .transpose()?;
        crtc.state = state;

        for connector in objects.connectors() {
            let connector = connector.lock().unwrap();

            if connector.state.crtc_id != objects.crtc_ids()[crtc.crtc_index as usize] {
                continue;
            }

            let framebuffer_id = connector.driver_data.framebuffer_id;
            let framebuffer = &mut self.framebuffers[framebuffer_id];

            if let Some(buffer) = buffer {
                buffer.buffer.sync(framebuffer, damage)
            } else {
                let onscreen_ptr = framebuffer.onscreen as *mut u32; // FIXME use as_mut_ptr once stable
                for row in 0..framebuffer.height {
                    unsafe {
                        ptr::write_bytes(
                            onscreen_ptr.add(row * framebuffer.stride),
                            0,
                            framebuffer.width,
                        );
                    }
                }
            }
        }

        Ok(())
    }

    fn hw_cursor_size(&self) -> Option<(u32, u32)> {
        None
    }

    fn handle_cursor(&mut self, _cursor: &CursorPlane<Self::Buffer>, _dirty_fb: bool) {
        unimplemented!("Vesad does not support this function");
    }
}

#[derive(Debug)]
pub struct FrameBuffer {
    pub onscreen: *mut [u32],
    pub phys: usize,
    pub width: usize,
    pub height: usize,
    pub stride: usize,
}

impl FrameBuffer {
    pub unsafe fn new(phys: usize, width: usize, height: usize, stride: usize) -> Self {
        let size = stride * height;
        let virt = common::physmap(
            phys,
            size * 4,
            common::Prot {
                read: true,
                write: true,
            },
            common::MemoryType::WriteCombining,
        )
        .expect("vesad: failed to map framebuffer") as *mut u32;

        let onscreen = ptr::slice_from_raw_parts_mut(virt, size);

        Self {
            onscreen,
            phys,
            width,
            height,
            stride,
        }
    }

    pub unsafe fn parse(var: &str) -> Option<Self> {
        fn parse_number(part: &str) -> Option<usize> {
            let (start, radix) = if part.starts_with("0x") {
                (2, 16)
            } else {
                (0, 10)
            };
            match usize::from_str_radix(&part[start..], radix) {
                Ok(ok) => Some(ok),
                Err(err) => {
                    eprintln!("vesad: failed to parse '{}': {}", part, err);
                    None
                }
            }
        }

        let mut parts = var.split(',');
        let phys = parse_number(parts.next()?)?;
        let width = parse_number(parts.next()?)?;
        let height = parse_number(parts.next()?)?;
        let stride = parse_number(parts.next()?)?;
        Some(Self::new(phys, width, height, stride))
    }
}

#[derive(Debug)]
pub struct GraphicScreen {
    width: usize,
    height: usize,
    ptr: NonNull<[u32]>,
}

impl GraphicScreen {
    fn new(width: usize, height: usize) -> GraphicScreen {
        let len = width * height;
        let layout = Self::layout(len);
        let ptr = unsafe { alloc::alloc_zeroed(layout) };
        let ptr = ptr::slice_from_raw_parts_mut(ptr.cast(), len);
        let ptr = NonNull::new(ptr).unwrap_or_else(|| alloc::handle_alloc_error(layout));

        GraphicScreen { width, height, ptr }
    }

    #[inline]
    fn layout(len: usize) -> Layout {
        // optimizes to an integer mul
        Layout::array::<u32>(len)
            .unwrap()
            .align_to(PAGE_SIZE)
            .unwrap()
    }
}

impl Drop for GraphicScreen {
    fn drop(&mut self) {
        let layout = Self::layout(self.ptr.len());
        unsafe { alloc::dealloc(self.ptr.as_ptr().cast(), layout) };
    }
}

impl Buffer for GraphicScreen {
    fn size(&self) -> usize {
        self.width * self.height * 4
    }
}

impl GraphicScreen {
    fn sync(&self, framebuffer: &mut FrameBuffer, sync_rect: Damage) {
        let sync_rect = sync_rect.clip(
            self.width.try_into().unwrap(),
            self.height.try_into().unwrap(),
        );

        let start_x: usize = sync_rect.x.try_into().unwrap();
        let start_y: usize = sync_rect.y.try_into().unwrap();
        let w: usize = sync_rect.width.try_into().unwrap();
        let h: usize = sync_rect.height.try_into().unwrap();

        let offscreen_ptr = self.ptr.as_ptr() as *mut u32;
        let onscreen_ptr = framebuffer.onscreen as *mut u32; // FIXME use as_mut_ptr once stable

        for row in start_y..start_y + h {
            unsafe {
                ptr::copy(
                    offscreen_ptr.add(row * self.width + start_x),
                    onscreen_ptr.add(row * framebuffer.stride + start_x),
                    w,
                );
            }
        }
    }
}
