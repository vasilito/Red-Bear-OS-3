//! PS/2 controller, see:
//! - https://wiki.osdev.org/I8042_PS/2_Controller
//! - http://www.mcamafia.de/pdf/ibm_hitrc07.pdf

use common::{
    io::{Io, ReadOnly, WriteOnly},
    timeout::Timeout,
};

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use common::io::Pio;

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
use common::io::Mmio;

use log::{debug, error, info, trace, warn};

use std::fmt;

#[derive(Debug)]
pub enum Error {
    CommandRetry,
    NoMoreTries,
    ReadTimeout,
    WriteTimeout,
    CommandTimeout(Command),
    WriteConfigTimeout(ConfigFlags),
    KeyboardCommandFail(KeyboardCommand),
    KeyboardCommandDataFail(KeyboardCommandData),
}

bitflags! {
    pub struct StatusFlags: u8 {
        const OUTPUT_FULL = 1;
        const INPUT_FULL = 1 << 1;
        const SYSTEM = 1 << 2;
        const COMMAND = 1 << 3;
        // Chipset specific
        const KEYBOARD_LOCK = 1 << 4;
        // Chipset specific
        const SECOND_OUTPUT_FULL = 1 << 5;
        const TIME_OUT = 1 << 6;
        const PARITY = 1 << 7;
    }
}

bitflags! {
    #[derive(Clone, Copy, Debug)]
    pub struct ConfigFlags: u8 {
        const FIRST_INTERRUPT = 1 << 0;
        const SECOND_INTERRUPT = 1 << 1;
        const POST_PASSED = 1 << 2;
        // 1 << 3 should be zero
        const CONFIG_RESERVED_3 = 1 << 3;
        const FIRST_DISABLED = 1 << 4;
        const SECOND_DISABLED = 1 << 5;
        const FIRST_TRANSLATE = 1 << 6;
        // 1 << 7 should be zero
        const CONFIG_RESERVED_7 = 1 << 7;
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
#[allow(dead_code)]
enum Command {
    ReadConfig = 0x20,
    WriteConfig = 0x60,
    DisableSecond = 0xA7,
    EnableSecond = 0xA8,
    TestSecond = 0xA9,
    TestController = 0xAA,
    TestFirst = 0xAB,
    Diagnostic = 0xAC,
    DisableFirst = 0xAD,
    EnableFirst = 0xAE,
    WriteSecond = 0xD4,
}

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
#[allow(dead_code)]
enum KeyboardCommand {
    EnableReporting = 0xF4,
    SetDefaultsDisable = 0xF5,
    SetDefaults = 0xF6,
    Reset = 0xFF,
}

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
enum KeyboardCommandData {
    ScancodeSet = 0xF0,
}

// Default timeout in microseconds
const DEFAULT_TIMEOUT: u64 = 50_000;
// Reset timeout in microseconds
const RESET_TIMEOUT: u64 = 1_000_000;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub struct Ps2 {
    data: Pio<u8>,
    status: ReadOnly<Pio<u8>>,
    command: WriteOnly<Pio<u8>>,
    //TODO: keep in state instead
    pub mouse_resets: usize,
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
pub struct Ps2 {
    data: Mmio<u8>,
    status: ReadOnly<Mmio<u8>>,
    command: WriteOnly<Mmio<u8>>,
    //TODO: keep in state instead
    pub mouse_resets: usize,
}

impl Ps2 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    pub fn new() -> Self {
        Ps2 {
            data: Pio::new(0x60),
            status: ReadOnly::new(Pio::new(0x64)),
            command: WriteOnly::new(Pio::new(0x64)),
            mouse_resets: 0,
        }
    }

    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    pub fn new() -> Self {
        unimplemented!()
    }

    fn status(&mut self) -> StatusFlags {
        StatusFlags::from_bits_truncate(self.status.read())
    }

    fn wait_read(&mut self, micros: u64) -> Result<(), Error> {
        let timeout = Timeout::from_micros(micros);
        loop {
            if self.status().contains(StatusFlags::OUTPUT_FULL) {
                return Ok(());
            }
            timeout.run().map_err(|()| Error::ReadTimeout)?
        }
    }

    fn wait_write(&mut self, micros: u64) -> Result<(), Error> {
        let timeout = Timeout::from_micros(micros);
        loop {
            if !self.status().contains(StatusFlags::INPUT_FULL) {
                return Ok(());
            }
            timeout.run().map_err(|()| Error::WriteTimeout)?
        }
    }

    fn command(&mut self, command: Command) -> Result<(), Error> {
        self.wait_write(DEFAULT_TIMEOUT)
            .map_err(|_| Error::CommandTimeout(command))?;
        self.command.write(command as u8);
        Ok(())
    }

    fn read(&mut self) -> Result<u8, Error> {
        self.read_timeout(DEFAULT_TIMEOUT)
    }

    fn read_timeout(&mut self, micros: u64) -> Result<u8, Error> {
        self.wait_read(micros)?;
        let data = self.data.read();
        Ok(data)
    }

    fn write(&mut self, data: u8) -> Result<(), Error> {
        self.wait_write(DEFAULT_TIMEOUT)?;
        self.data.write(data);
        Ok(())
    }

    fn retry<T, F: Fn(&mut Self) -> Result<T, Error>>(
        &mut self,
        name: fmt::Arguments,
        retries: usize,
        f: F,
    ) -> Result<T, Error> {
        trace!("{}", name);
        let mut res = Err(Error::NoMoreTries);
        for retry in 0..retries {
            res = f(self);
            match res {
                Ok(ok) => {
                    return Ok(ok);
                }
                Err(ref err) => {
                    debug!("{}: retry {}/{}: {:?}", name, retry + 1, retries, err);
                }
            }
        }
        res
    }

    fn config(&mut self) -> Result<ConfigFlags, Error> {
        self.retry(format_args!("read config"), 4, |x| {
            x.command(Command::ReadConfig)?;
            x.read()
        })
        .map(ConfigFlags::from_bits_truncate)
    }

    fn set_config(&mut self, config: ConfigFlags) -> Result<(), Error> {
        self.retry(format_args!("write config {:?}", config), 4, |x| {
            x.command(Command::WriteConfig)?;
            x.write(config.bits())
                .map_err(|_| Error::WriteConfigTimeout(config))?;
            Ok(0)
        })?;
        Ok(())
    }

    fn keyboard_command_inner(&mut self, command: u8) -> Result<u8, Error> {
        self.write(command)?;
        match self.read()? {
            0xFE => Err(Error::CommandRetry),
            value => Ok(value),
        }
    }

    fn keyboard_command(&mut self, command: KeyboardCommand) -> Result<u8, Error> {
        self.retry(format_args!("keyboard command {:?}", command), 4, |x| {
            x.keyboard_command_inner(command as u8)
                .map_err(|_| Error::KeyboardCommandFail(command))
        })
    }

    fn keyboard_command_data(
        &mut self,
        command: KeyboardCommandData,
        data: u8,
    ) -> Result<u8, Error> {
        self.retry(
            format_args!("keyboard command {:?} {:#x}", command, data),
            4,
            |x| {
                let res = x
                    .keyboard_command_inner(command as u8)
                    .map_err(|_| Error::KeyboardCommandDataFail(command))?;
                if res != 0xFA {
                    warn!("keyboard incorrect result of set command: {command:?} {res:02X}");
                    return Ok(res);
                }
                x.write(data)?;
                x.read()
            },
        )
    }

    pub fn mouse_command_async(&mut self, command: u8) -> Result<(), Error> {
        self.command(Command::WriteSecond)?;
        self.write(command as u8)
    }

    pub fn next(&mut self) -> Option<(bool, u8)> {
        let status = self.status();
        if status.contains(StatusFlags::OUTPUT_FULL) {
            let data = self.data.read();
            Some((!status.contains(StatusFlags::SECOND_OUTPUT_FULL), data))
        } else {
            None
        }
    }

    pub fn init_keyboard(&mut self) -> Result<(), Error> {
        let mut b;

        {
            // Enable first device
            self.command(Command::EnableFirst)?;
        }

        {
            // Reset keyboard
            b = self.keyboard_command(KeyboardCommand::Reset)?;
            if b == 0xFA {
                b = self.read().unwrap_or(0);
                if b != 0xAA {
                    error!("keyboard failed self test: {:02X}", b);
                }
            } else {
                error!("keyboard failed to reset: {:02X}", b);
            }
        }

        {
            // Set scancode set to 2
            let scancode_set = 2;
            b = self.keyboard_command_data(KeyboardCommandData::ScancodeSet, scancode_set)?;
            if b != 0xFA {
                error!(
                    "keyboard failed to set scancode set {}: {:02X}",
                    scancode_set, b
                );
            }
        }

        Ok(())
    }

    pub fn init(&mut self) -> Result<(), Error> {
        {
            // Disable devices
            self.command(Command::DisableFirst)?;
            self.command(Command::DisableSecond)?;
        }

        // Disable clocks, disable interrupts, and disable translate
        {
            // Since the default config may have interrupts enabled, and the kernel may eat up
            // our data in that case, we will write a config without reading the current one
            let config = ConfigFlags::POST_PASSED
                | ConfigFlags::FIRST_DISABLED
                | ConfigFlags::SECOND_DISABLED;
            self.set_config(config)?;
        }

        // The keyboard seems to still collect bytes even when we disable
        // the port, so we must disable the keyboard too
        self.retry(format_args!("keyboard defaults"), 4, |x| {
            // Set defaults and disable scanning
            let b = x.keyboard_command(KeyboardCommand::SetDefaultsDisable)?;
            if b != 0xFA {
                error!("keyboard failed to set defaults: {:02X}", b);
                return Err(Error::CommandRetry);
            }

            Ok(b)
        })?;

        {
            // Perform the self test
            self.command(Command::TestController)?;
            let r = self.read()?;
            if r != 0x55 {
                warn!("self test unexpected value: {:02X}", r);
            }
        }

        // Initialize keyboard
        if let Err(err) = self.init_keyboard() {
            error!("failed to initialize keyboard: {:?}", err);
            return Err(err);
        }

        // Enable second device
        let enable_mouse = match self.command(Command::EnableSecond) {
            Ok(()) => true,
            Err(err) => {
                error!("failed to initialize mouse: {:?}", err);
                false
            }
        };

        {
            // Enable keyboard data reporting
            // Use inner function to prevent retries
            // Response is ignored since scanning is now on
            if let Err(err) = self.keyboard_command_inner(KeyboardCommand::EnableReporting as u8) {
                error!("failed to initialize keyboard reporting: {:?}", err);
                //TODO: fix by using interrupts?
            }
        }

        // Enable clocks and interrupts
        {
            let config = ConfigFlags::POST_PASSED
                | ConfigFlags::FIRST_INTERRUPT
                | ConfigFlags::FIRST_TRANSLATE
                | if enable_mouse {
                    ConfigFlags::SECOND_INTERRUPT
                } else {
                    ConfigFlags::SECOND_DISABLED
                };
            self.set_config(config)?;
        }

        Ok(())
    }
}
