use console_draw::{Damage, TextScreen, V2DisplayMap};
use drm::buffer::Buffer;
use drm::control::Device;
use graphics_ipc::V2GraphicsHandle;
use inputd::ConsumerHandle;
use std::io;

pub struct Display {
    pub input_handle: ConsumerHandle,
    pub map: Option<V2DisplayMap>,
}

impl Display {
    pub fn open_new_vt() -> io::Result<Self> {
        let mut display = Self {
            input_handle: ConsumerHandle::new_vt()?,
            map: None,
        };

        display.reopen_for_handoff();

        Ok(display)
    }

    /// Re-open the display after a handoff.
    pub fn reopen_for_handoff(&mut self) {
        let display_file = match self.input_handle.open_display_v2() {
            Ok(display_file) => display_file,
            Err(err) => {
                log::error!("fbcond: No display present yet: {err}");
                return;
            }
        };
        let new_display_handle = V2GraphicsHandle::from_file(display_file).unwrap();

        log::debug!("fbcond: Opened new display");

        match V2DisplayMap::new(new_display_handle) {
            Ok(map) => {
                log::debug!(
                    "fbcond: Mapped new display with size {}x{}",
                    map.buffer.buffer().size().0,
                    map.buffer.buffer().size().1,
                );
                self.map = Some(map)
            }
            Err(err) => {
                log::error!("fbcond: failed to map new display: {err}");
                return;
            }
        }
    }

    pub fn handle_resize(map: &mut V2DisplayMap, text_screen: &mut TextScreen) {
        let mode = match map
            .display_handle
            .first_display()
            .and_then(|handle| Ok(map.display_handle.get_connector(handle, true)?.modes()[0]))
        {
            Ok(mode) => mode,
            Err(err) => {
                eprintln!("fbcond: failed to get display size: {}", err);
                return;
            }
        };

        if (u32::from(mode.size().0), u32::from(mode.size().1)) != map.buffer.buffer().size() {
            match text_screen.resize(map, mode) {
                Ok(()) => eprintln!("fbcond: mapped display"),
                Err(err) => {
                    eprintln!("fbcond: failed to create or map framebuffer: {}", err);
                    return;
                }
            }
        }
    }

    pub fn sync_rect(&mut self, damage: Damage) {
        if let Some(map) = &mut self.map {
            map.dirty_fb(damage).unwrap();
        }
    }
}
