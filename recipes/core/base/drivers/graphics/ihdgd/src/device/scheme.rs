//TODO: this is copied from vesad and should be adapted

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
use syscall::{error::EINVAL, PAGE_SIZE};

use super::pipe::DeviceFb;
use super::Device;

#[derive(Debug)]
pub struct Connector {
    framebuffer_id: usize,
}

impl KmsConnectorDriver for Connector {
    type State = ();
}

impl GraphicsAdapter for Device {
    type Connector = Connector;
    type Crtc = ();

    type Buffer = DumbFb;
    type Framebuffer = ();

    fn name(&self) -> &'static [u8] {
        b"ihdgd"
    }

    fn desc(&self) -> &'static [u8] {
        b"Intel HD Graphics"
    }

    fn init(&mut self, objects: &mut KmsObjects<Self>) {
        self.init_inner();

        // FIXME enumerate actual connectors
        for (framebuffer_id, _) in self.framebuffers.iter().enumerate() {
            let crtc = objects.add_crtc((), ());

            objects.add_connector(Connector { framebuffer_id }, (), &[crtc]);
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
            // FIXME hide cursor plane unless this client cap is set
            DRM_CLIENT_CAP_CURSOR_PLANE_HOTSPOT => Ok(()),
            _ => Err(syscall::Error::new(EINVAL)),
        }
    }

    fn probe_connector(&mut self, objects: &mut KmsObjects<Self>, id: KmsObjectId) {
        let mut connector = objects.get_connector(id).unwrap().lock().unwrap();
        let framebuffer = &self.framebuffers[connector.driver_data.framebuffer_id];
        connector.connection = KmsConnectorStatus::Connected;
        connector.update_from_size(framebuffer.width as u32, framebuffer.height as u32);
        // FIXME fetch EDID
    }

    fn create_dumb_buffer(&mut self, width: u32, height: u32) -> (Self::Buffer, u32) {
        (DumbFb::new(width as usize, height as usize), width * 4)
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
                let onscreen_ptr = framebuffer.buffer.virt.cast::<u32>();
                for row in 0..framebuffer.height {
                    unsafe {
                        ptr::write_bytes(
                            onscreen_ptr.add((row * framebuffer.stride) as usize),
                            0,
                            framebuffer.width as usize,
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
        unimplemented!("ihdgd does not support this function");
    }
}

#[derive(Debug)]
pub struct DumbFb {
    width: usize,
    height: usize,
    ptr: NonNull<[u32]>,
}

impl DumbFb {
    fn new(width: usize, height: usize) -> DumbFb {
        let len = width * height;
        let layout = Self::layout(len);
        let ptr = unsafe { alloc::alloc_zeroed(layout) };
        let ptr = ptr::slice_from_raw_parts_mut(ptr.cast(), len);
        let ptr = NonNull::new(ptr).unwrap_or_else(|| alloc::handle_alloc_error(layout));

        DumbFb { width, height, ptr }
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

impl Drop for DumbFb {
    fn drop(&mut self) {
        let layout = Self::layout(self.ptr.len());
        unsafe { alloc::dealloc(self.ptr.as_ptr().cast(), layout) };
    }
}

impl Buffer for DumbFb {
    fn size(&self) -> usize {
        self.width * self.height * 4
    }
}

impl DumbFb {
    fn sync(&self, framebuffer: &mut DeviceFb, sync_rect: Damage) {
        let sync_rect = sync_rect.clip(
            self.width.try_into().unwrap(),
            self.height.try_into().unwrap(),
        );

        let start_x: usize = sync_rect.x.try_into().unwrap();
        let start_y: usize = sync_rect.y.try_into().unwrap();
        let w: usize = sync_rect.width.try_into().unwrap();
        let h: usize = sync_rect.height.try_into().unwrap();

        let offscreen_ptr = self.ptr.as_ptr() as *mut u32;
        let onscreen_ptr = framebuffer.buffer.virt.cast::<u32>();

        for row in start_y..start_y + h {
            unsafe {
                ptr::copy(
                    offscreen_ptr.add(row * self.width + start_x),
                    onscreen_ptr.add(row * framebuffer.stride as usize / 4 + start_x),
                    w,
                );
            }
        }
    }
}
