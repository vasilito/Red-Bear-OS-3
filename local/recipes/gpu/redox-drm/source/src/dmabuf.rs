use std::collections::BTreeMap;

use log::{debug, warn};

use crate::driver::{DriverError, Result};
use crate::gem::GemHandle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmabufInfo {
    pub phys_addr: usize,
    pub size: u64,
    pub gem_handle: GemHandle,
}

#[derive(Clone, Debug)]
struct DmabufEntry {
    #[allow(dead_code)]
    info: DmabufInfo,
    #[allow(dead_code)]
    scheme_path: String,
    #[allow(dead_code)]
    refcount: usize,
}

pub struct DmabufManager {
    #[allow(dead_code)]
    next_fd: i32,
    #[allow(dead_code)]
    exported: BTreeMap<i32, GemHandle>,
    #[allow(dead_code)]
    entries: BTreeMap<GemHandle, DmabufEntry>,
}

impl DmabufManager {
    pub fn new() -> Self {
        Self {
            next_fd: 10_000,
            exported: BTreeMap::new(),
            entries: BTreeMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn export(&mut self, handle: GemHandle) -> Result<i32> {
        self.export_with_info(handle, 0, 0)
    }

    #[allow(dead_code)]
    pub fn export_with_info(
        &mut self,
        handle: GemHandle,
        phys_addr: usize,
        size: u64,
    ) -> Result<i32> {
        if handle == 0 {
            return Err(DriverError::InvalidArgument(
                "DMA-BUF export requires a non-zero GEM handle",
            ));
        }

        let fd = self.allocate_fd()?;
        let scheme_path = Self::scheme_path(handle);

        if let Some(entry) = self.entries.get_mut(&handle) {
            entry.info.phys_addr = Self::merge_phys_addr(entry.info.phys_addr, phys_addr)?;
            entry.info.size = Self::merge_size(entry.info.size, size)?;
            entry.refcount = entry.refcount.checked_add(1).ok_or_else(|| {
                DriverError::Buffer(format!(
                    "DMA-BUF refcount overflow for GEM handle {}",
                    handle
                ))
            })?;

            debug!(
                "redox-drm: dup() DMA-BUF export fd {} -> {} (GEM handle {}, refs={})",
                entry.scheme_path, fd, handle, entry.refcount
            );
        } else {
            self.entries.insert(
                handle,
                DmabufEntry {
                    info: DmabufInfo {
                        phys_addr,
                        size,
                        gem_handle: handle,
                    },
                    scheme_path: scheme_path.clone(),
                    refcount: 1,
                },
            );

            warn!(
                "redox-drm: exported DMA-BUF {} as synthetic fd {} for GEM handle {} \
                 (phys={:#x}, size={})",
                scheme_path, fd, handle, phys_addr, size
            );
        }

        self.exported.insert(fd, handle);
        Ok(fd)
    }

    pub fn import(&self, fd: i32) -> Result<GemHandle> {
        let info = self
            .lookup(fd)
            .ok_or_else(|| DriverError::NotFound(format!("unknown synthetic dma-buf fd {fd}")))?;

        debug!(
            "redox-drm: imported DMA-BUF fd {} -> GEM handle {} (phys={:#x}, size={})",
            fd, info.gem_handle, info.phys_addr, info.size
        );

        Ok(info.gem_handle)
    }

    pub fn close(&mut self, fd: i32) -> Result<()> {
        let handle = self
            .exported
            .remove(&fd)
            .ok_or_else(|| DriverError::NotFound(format!("unknown synthetic dma-buf fd {fd}")))?;

        let remove_entry = {
            let entry = self.entries.get_mut(&handle).ok_or_else(|| {
                DriverError::NotFound(format!(
                    "DMA-BUF bookkeeping missing for GEM handle {}",
                    handle
                ))
            })?;

            if entry.refcount == 0 {
                return Err(DriverError::Buffer(format!(
                    "DMA-BUF refcount underflow for GEM handle {}",
                    handle
                )));
            }

            entry.refcount -= 1;
            debug!(
                "redox-drm: closed DMA-BUF fd {} for {} (GEM handle {}, refs={})",
                fd, entry.scheme_path, handle, entry.refcount
            );
            entry.refcount == 0
        };

        if remove_entry {
            let _ = self.entries.remove(&handle);
            warn!(
                "redox-drm: released final DMA-BUF export for GEM handle {}",
                handle
            );
        }

        Ok(())
    }

    pub fn lookup(&self, fd: i32) -> Option<DmabufInfo> {
        let handle = self.exported.get(&fd)?;
        self.entries.get(handle).map(|entry| entry.info)
    }

    pub fn dup(&mut self, fd: i32) -> Result<i32> {
        let info = self
            .lookup(fd)
            .ok_or_else(|| DriverError::NotFound(format!("unknown synthetic dma-buf fd {fd}")))?;
        self.export_with_info(info.gem_handle, info.phys_addr, info.size)
    }

    fn allocate_fd(&mut self) -> Result<i32> {
        let fd = self.next_fd;
        self.next_fd = self.next_fd.checked_add(1).ok_or_else(|| {
            DriverError::Buffer("synthetic DMA-BUF fd space exhausted".to_string())
        })?;
        Ok(fd)
    }

    fn scheme_path(handle: GemHandle) -> String {
        format!("drm:card0/dmabuf/{handle}")
    }

    fn merge_phys_addr(current: usize, incoming: usize) -> Result<usize> {
        if current == 0 || incoming == 0 || current == incoming {
            return Ok(current.max(incoming));
        }

        Err(DriverError::Buffer(format!(
            "conflicting DMA-BUF physical addresses: existing={:#x}, incoming={:#x}",
            current, incoming
        )))
    }

    fn merge_size(current: u64, incoming: u64) -> Result<u64> {
        if current == 0 || incoming == 0 || current == incoming {
            return Ok(current.max(incoming));
        }

        Err(DriverError::Buffer(format!(
            "conflicting DMA-BUF sizes: existing={}, incoming={}",
            current, incoming
        )))
    }
}
