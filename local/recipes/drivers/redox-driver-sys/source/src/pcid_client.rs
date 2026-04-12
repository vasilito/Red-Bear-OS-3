use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{FromRawFd, IntoRawFd, RawFd};
use std::path::Path;

use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct PciFunction {
    pub segment: u16,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub irq: Option<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PcidClientRequest {
    EnableDevice,
    RequestConfig,
    ReadConfig(u16),
    WriteConfig(u16, u32),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PcidClientResponse {
    EnabledDevice,
    Config(PciFunction),
    ReadConfig(u32),
    WriteConfig,
    Error(String),
}

pub struct PcidClient {
    channel: File,
}

impl PcidClient {
    pub fn connect_default() -> Option<Self> {
        let fd_str = std::env::var("PCID_CLIENT_CHANNEL").ok()?;
        let fd: RawFd = fd_str.parse().ok()?;
        Some(Self::connect_common(fd))
    }

    pub fn connect_by_path(device_path: &Path) -> Result<Self, std::io::Error> {
        let channel_path = device_path.join("channel");
        let fd = libredox::call::open(
            channel_path.to_str().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid path")
            })?,
            libredox::flag::O_RDWR,
            0,
        )
        .map_err(|e| std::io::Error::from_raw_os_error(e.errno()))?;
        Ok(Self::connect_common(fd as RawFd))
    }

    fn connect_common(channel_fd: RawFd) -> Self {
        let channel = unsafe { File::from_raw_fd(channel_fd) };
        Self { channel }
    }

    fn send<T: Serialize>(&mut self, msg: &T) -> Result<(), std::io::Error> {
        let data = bincode::serialize(msg)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        let len = data.len() as u64;
        self.channel.write_all(&len.to_le_bytes())?;
        self.channel.write_all(&data)?;
        Ok(())
    }

    fn recv<T: DeserializeOwned>(&mut self) -> Result<T, std::io::Error> {
        let mut len_buf = [0u8; 8];
        self.channel.read_exact(&mut len_buf)?;
        let len = u64::from_le_bytes(len_buf) as usize;
        if len > 0x100_000 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "response too large",
            ));
        }
        let mut data = vec![0u8; len];
        self.channel.read_exact(&mut data)?;
        bincode::deserialize_from(&data[..])
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    pub fn request_config(&mut self) -> Result<PciFunction, std::io::Error> {
        self.send(&PcidClientRequest::RequestConfig)?;
        match self.recv()? {
            PcidClientResponse::Config(func) => Ok(func),
            other => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unexpected response: {other:?}"),
            )),
        }
    }

    pub fn enable_device(&mut self) -> Result<(), std::io::Error> {
        self.send(&PcidClientRequest::EnableDevice)?;
        match self.recv()? {
            PcidClientResponse::EnabledDevice => Ok(()),
            other => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unexpected response: {other:?}"),
            )),
        }
    }

    pub fn read_config(&mut self, offset: u16) -> Result<u32, std::io::Error> {
        self.send(&PcidClientRequest::ReadConfig(offset))?;
        match self.recv()? {
            PcidClientResponse::ReadConfig(val) => Ok(val),
            other => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unexpected response: {other:?}"),
            )),
        }
    }

    pub fn write_config(&mut self, offset: u16, value: u32) -> Result<(), std::io::Error> {
        self.send(&PcidClientRequest::WriteConfig(offset, value))?;
        match self.recv()? {
            PcidClientResponse::WriteConfig => Ok(()),
            other => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unexpected response: {other:?}"),
            )),
        }
    }

    pub fn into_raw_fd(self) -> RawFd {
        self.channel.into_raw_fd()
    }
}
