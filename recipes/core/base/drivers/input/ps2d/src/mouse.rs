use crate::controller::Ps2;
use std::time::Duration;

pub const RESET_RETRIES: usize = 10;
pub const RESET_TIMEOUT: Duration = Duration::from_millis(1000);
pub const COMMAND_TIMEOUT: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
#[allow(dead_code)]
enum MouseCommand {
    SetScaling1To1 = 0xE6,
    SetScaling2To1 = 0xE7,
    StatusRequest = 0xE9,
    GetDeviceId = 0xF2,
    EnableReporting = 0xF4,
    SetDefaultsDisable = 0xF5,
    SetDefaults = 0xF6,
    Reset = 0xFF,
}

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
enum MouseCommandData {
    SetResolution = 0xE8,
    SetSampleRate = 0xF3,
}

#[derive(Debug)]
struct MouseTx {
    write: &'static [u8],
    write_i: usize,
    read: Vec<u8>,
    read_bytes: usize,
}

impl MouseTx {
    fn new(write: &'static [u8], read_bytes: usize, ps2: &mut Ps2) -> Result<Self, ()> {
        let mut this = Self {
            write,
            write_i: 0,
            read: Vec::with_capacity(read_bytes),
            read_bytes,
        };
        this.try_write(ps2)?;
        Ok(this)
    }

    fn try_write(&mut self, ps2: &mut Ps2) -> Result<(), ()> {
        if let Some(write) = self.write.get(self.write_i) {
            if let Err(err) = ps2.mouse_command_async(*write) {
                log::error!("failed to write {:02X} to mouse: {:?}", write, err);
                return Err(());
            }
        }
        Ok(())
    }

    fn handle(&mut self, data: u8, ps2: &mut Ps2) -> Result<bool, ()> {
        if self.write_i < self.write.len() {
            if data == 0xFA {
                self.write_i += 1;
                self.try_write(ps2)?;
            } else {
                log::error!("unknown mouse response {:02X}", data);
                return Err(());
            }
        } else {
            self.read.push(data);
        }
        Ok(self.write_i >= self.write.len() && self.read.len() >= self.read_bytes)
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
#[allow(dead_code)]
enum MouseId {
    /// Mouse sends three bytes
    Base = 0x00,
    /// Mouse sends fourth byte with scroll
    Intellimouse1 = 0x03,
    /// Mouse sends fourth byte with scroll, button 4, and button 5
    //TODO: support this mouse type
    Intellimouse2 = 0x04,
}

// From Synaptics TouchPad Interfacing Guide
#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum TouchpadCommand {
    Identify = 0x00,
}

#[derive(Debug)]
pub enum MouseState {
    /// No mouse found
    None,
    /// Ready to initialize mouse
    Init,
    /// Reset command is sent
    Reset,
    /// BAT completion code returned
    Bat,
    /// Identify touchpad
    IdentifyTouchpad { tx: MouseTx },
    /// Enable intellimouse features
    EnableIntellimouse { tx: MouseTx },
    /// Status request
    Status { index: usize },
    /// Device ID update
    DeviceId,
    /// Enable reporting command sent
    EnableReporting { id: u8 },
    /// Mouse is streaming
    Streaming { id: u8 },
}

#[derive(Debug)]
#[must_use]
pub enum MouseResult {
    None,
    Packet(u8, bool),
    Timeout(Duration),
}

impl MouseState {
    pub fn reset(&mut self, ps2: &mut Ps2) -> MouseResult {
        if ps2.mouse_resets < RESET_RETRIES {
            ps2.mouse_resets += 1;
        } else {
            log::error!("tried to reset mouse {} times, giving up", ps2.mouse_resets);
            *self = MouseState::None;
            return MouseResult::None;
        }
        match ps2.mouse_command_async(MouseCommand::Reset as u8) {
            Ok(()) => {
                *self = MouseState::Reset;
                MouseResult::Timeout(RESET_TIMEOUT)
            }
            Err(err) => {
                log::error!("failed to send mouse reset command: {:?}", err);
                //TODO: retry reset?
                *self = MouseState::None;
                MouseResult::None
            }
        }
    }

    fn enable_reporting(&mut self, id: u8, ps2: &mut Ps2) -> MouseResult {
        match ps2.mouse_command_async(MouseCommand::EnableReporting as u8) {
            Ok(()) => {
                *self = MouseState::EnableReporting { id };
                MouseResult::Timeout(COMMAND_TIMEOUT)
            }
            Err(err) => {
                log::error!("failed to enable mouse reporting: {:?}", err);
                //TODO: reset mouse?
                *self = MouseState::None;
                MouseResult::None
            }
        }
    }

    fn request_status(&mut self, ps2: &mut Ps2) -> MouseResult {
        match ps2.mouse_command_async(MouseCommand::StatusRequest as u8) {
            Ok(()) => {
                *self = MouseState::Status { index: 0 };
                MouseResult::Timeout(COMMAND_TIMEOUT)
            }
            Err(err) => {
                log::error!("failed to request mouse status: {:?}", err);
                //TODO: reset mouse instead?
                self.request_id(ps2)
            }
        }
    }

    fn request_id(&mut self, ps2: &mut Ps2) -> MouseResult {
        match ps2.mouse_command_async(MouseCommand::GetDeviceId as u8) {
            Ok(()) => {
                *self = MouseState::DeviceId;
                MouseResult::Timeout(COMMAND_TIMEOUT)
            }
            Err(err) => {
                log::error!("failed to request mouse id: {:?}", err);
                //TODO: reset mouse instead?
                self.enable_reporting(MouseId::Base as u8, ps2)
            }
        }
    }

    fn identify_touchpad(&mut self, ps2: &mut Ps2) -> MouseResult {
        let cmd = TouchpadCommand::Identify as u8;
        match MouseTx::new(
            &[
                // Ensure command alignment
                MouseCommand::SetScaling1To1 as u8,
                // Send special identify touchpad command
                MouseCommandData::SetResolution as u8,
                0,
                MouseCommandData::SetResolution as u8,
                0,
                MouseCommandData::SetResolution as u8,
                0,
                MouseCommandData::SetResolution as u8,
                0,
                // Status request
                MouseCommand::StatusRequest as u8,
            ],
            3,
            ps2,
        ) {
            Ok(tx) => {
                *self = MouseState::IdentifyTouchpad { tx };
                MouseResult::Timeout(COMMAND_TIMEOUT)
            }
            Err(()) => self.enable_intellimouse(ps2),
        }
    }

    fn enable_intellimouse(&mut self, ps2: &mut Ps2) -> MouseResult {
        match MouseTx::new(
            &[
                MouseCommandData::SetSampleRate as u8,
                200,
                MouseCommandData::SetSampleRate as u8,
                100,
                MouseCommandData::SetSampleRate as u8,
                80,
            ],
            0,
            ps2,
        ) {
            Ok(tx) => {
                *self = MouseState::EnableIntellimouse { tx };
                MouseResult::Timeout(COMMAND_TIMEOUT)
            }
            Err(()) => self.request_id(ps2),
        }
    }

    pub fn handle(&mut self, data: u8, ps2: &mut Ps2) -> MouseResult {
        match *self {
            MouseState::None | MouseState::Init => {
                //TODO: enable port in this case, mouse hotplug may send 0xAA 0x00
                log::error!(
                    "received mouse byte {:02X} when mouse not initialized",
                    data
                );
                MouseResult::None
            }
            MouseState::Reset => {
                if data == 0xFA {
                    log::debug!("mouse reset ok");
                    MouseResult::Timeout(RESET_TIMEOUT)
                } else if data == 0xAA {
                    log::debug!("BAT completed");
                    *self = MouseState::Bat;
                    MouseResult::Timeout(COMMAND_TIMEOUT)
                } else {
                    log::warn!("unknown mouse response {:02X} after reset", data);
                    self.reset(ps2)
                }
            }
            MouseState::Bat => {
                if data == MouseId::Base as u8 {
                    // Enable intellimouse features
                    log::debug!("BAT mouse id {:02X} (base)", data);
                    self.identify_touchpad(ps2)
                } else if data == MouseId::Intellimouse1 as u8 {
                    // Extra packet already enabled
                    log::debug!("BAT mouse id {:02X} (intellimouse)", data);
                    self.enable_reporting(data, ps2)
                } else {
                    log::warn!("unknown mouse id {:02X} after BAT", data);
                    MouseResult::Timeout(RESET_TIMEOUT)
                }
            }
            MouseState::IdentifyTouchpad { ref mut tx } => {
                match tx.handle(data, ps2) {
                    Ok(done) => {
                        if done {
                            //TODO: handle touchpad identification
                            // If tx.read[1] == 0x47, this is a synaptics touchpad
                            self.request_status(ps2)
                        } else {
                            MouseResult::Timeout(COMMAND_TIMEOUT)
                        }
                    }
                    Err(()) => self.enable_intellimouse(ps2),
                }
            }
            MouseState::EnableIntellimouse { ref mut tx } => match tx.handle(data, ps2) {
                Ok(done) => {
                    if done {
                        self.request_status(ps2)
                    } else {
                        MouseResult::Timeout(COMMAND_TIMEOUT)
                    }
                }
                Err(()) => self.request_status(ps2),
            },
            MouseState::Status { index } => {
                match index {
                    0 => {
                        //TODO: check response
                        *self = MouseState::Status { index: 1 };
                        MouseResult::Timeout(COMMAND_TIMEOUT)
                    }
                    1 => {
                        *self = MouseState::Status { index: 2 };
                        MouseResult::Timeout(COMMAND_TIMEOUT)
                    }
                    2 => {
                        *self = MouseState::Status { index: 3 };
                        MouseResult::Timeout(COMMAND_TIMEOUT)
                    }
                    _ => self.request_id(ps2),
                }
            }
            MouseState::DeviceId => {
                if data == 0xFA {
                    // Command OK response
                    //TODO: handle this separately?
                    MouseResult::Timeout(COMMAND_TIMEOUT)
                } else if data == MouseId::Base as u8 || data == MouseId::Intellimouse1 as u8 {
                    log::debug!("mouse id {:02X}", data);
                    self.enable_reporting(data, ps2)
                } else {
                    log::warn!("unknown mouse id {:02X} after requesting id", data);
                    self.reset(ps2)
                }
            }
            MouseState::EnableReporting { id } => {
                log::debug!("mouse id {:02X} enable reporting {:02X}", id, data);
                //TODO: handle response ok/error
                *self = MouseState::Streaming { id };
                MouseResult::None
            }
            MouseState::Streaming { id } => {
                MouseResult::Packet(data, id == MouseId::Intellimouse1 as u8)
            }
        }
    }

    pub fn handle_timeout(&mut self, ps2: &mut Ps2) -> MouseResult {
        match *self {
            MouseState::None | MouseState::Streaming { .. } => MouseResult::None,
            MouseState::Init => {
                // The state uses a timeout on init to request a reset
                self.reset(ps2)
            }
            MouseState::Reset => {
                log::warn!("timeout waiting for mouse reset");
                self.reset(ps2)
            }
            MouseState::Bat => {
                log::warn!("timeout waiting for BAT completion");
                self.reset(ps2)
            }
            MouseState::IdentifyTouchpad { .. } => {
                //TODO: retry?
                log::warn!("timeout identifying touchpad");
                self.request_status(ps2)
            }
            MouseState::EnableIntellimouse { .. } => {
                //TODO: retry?
                log::warn!("timeout enabling intellimouse");
                self.request_status(ps2)
            }
            MouseState::Status { index } => {
                log::warn!("timeout waiting for mouse status {}", index);
                self.request_id(ps2)
            }
            MouseState::DeviceId => {
                log::warn!("timeout requesting mouse id");
                self.enable_reporting(0, ps2)
            }
            MouseState::EnableReporting { id } => {
                log::warn!("timeout enabling reporting");
                //TODO: limit number of retries
                self.enable_reporting(id, ps2)
            }
        }
    }
}
