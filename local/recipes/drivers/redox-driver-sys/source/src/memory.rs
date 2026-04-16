use core::ptr;
use core::sync::atomic::{AtomicPtr, Ordering};

use redox_syscall::data::Map;
use redox_syscall::flag::{
    MAP_SHARED, O_CLOEXEC, O_RDONLY, O_RDWR, O_WRONLY, PROT_READ, PROT_WRITE,
};
use redox_syscall::PAGE_SIZE;
use syscall as redox_syscall;

use crate::{DriverError, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheType {
    WriteBack,
    Uncacheable,
    WriteCombining,
    DeviceMemory,
}

impl CacheType {
    pub fn suffix(&self) -> &'static str {
        match self {
            Self::WriteBack => "wb",
            Self::Uncacheable => "uc",
            Self::WriteCombining => "wc",
            Self::DeviceMemory => "dev",
        }
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct MmioProt: u8 {
        const READ = 0b01;
        const WRITE = 0b10;
        const READ_WRITE = 0b11;
    }
}

// SAFETY: The memory scheme root FD is cached for the process lifetime.
// This is valid because:
// 1. scheme:memory is a kernel-built-in scheme that never terminates.
// 2. The FD is opened with O_CLOEXEC — children after exec(2) do not inherit it.
// 3. This code MUST NOT be used in processes that fork() without exec() —
//    the child would share the same FD table slot, risking double-close.
static MEMORY_ROOT_FD: AtomicPtr<()> = AtomicPtr::new(ptr::null_mut());

fn ensure_memory_root() -> Result<libredox::Fd> {
    let current = MEMORY_ROOT_FD.load(Ordering::Acquire);
    if !current.is_null() {
        let raw_fd = current as usize;
        let dup_fd = libredox::call::dup(raw_fd, b"")
            .map_err(|e| std::io::Error::from_raw_os_error(e.errno()))?;
        return Ok(libredox::Fd::new(dup_fd));
    }

    let fd = libredox::Fd::open("/scheme/memory/scheme-root", O_CLOEXEC as i32, 0)?;
    let raw = fd.raw();

    match MEMORY_ROOT_FD.compare_exchange(
        ptr::null_mut(),
        raw as *mut (),
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(_) => {
            std::mem::forget(fd);
            let dup_fd = libredox::call::dup(raw, b"")
                .map_err(|e| std::io::Error::from_raw_os_error(e.errno()))?;
            return Ok(libredox::Fd::new(dup_fd));
        }
        Err(existing) => {
            let dup_fd = libredox::call::dup(existing as usize, b"")
                .map_err(|e| std::io::Error::from_raw_os_error(e.errno()))?;
            return Ok(libredox::Fd::new(dup_fd));
        }
    }
}

pub struct MmioRegion {
    ptr: *mut u8,
    size: usize,
}

impl MmioRegion {
    pub fn map(phys_addr: u64, size: usize, cache: CacheType, prot: MmioProt) -> Result<Self> {
        if phys_addr == 0 {
            return Err(DriverError::InvalidAddress(phys_addr));
        }

        let aligned_size = size.next_multiple_of(PAGE_SIZE);
        let path = format!("physical@{}", cache.suffix());

        let mode = if prot.contains(MmioProt::READ | MmioProt::WRITE) {
            O_RDWR
        } else if prot.contains(MmioProt::WRITE) {
            O_WRONLY
        } else {
            O_RDONLY
        };

        let mut mmap_prot = redox_syscall::MapFlags::empty();
        if prot.contains(MmioProt::READ) {
            mmap_prot |= PROT_READ;
        }
        if prot.contains(MmioProt::WRITE) {
            mmap_prot |= PROT_WRITE;
        }

        let root_fd = ensure_memory_root()?;
        let mem_fd = root_fd.openat(&path, (O_CLOEXEC | mode) as i32, 0)?;

        let map = Map {
            offset: phys_addr as usize,
            size: aligned_size,
            flags: mmap_prot | redox_syscall::MapFlags::from_bits_truncate(MAP_SHARED.bits()),
            address: 0,
        };

        let ptr = unsafe { redox_syscall::call::fmap(mem_fd.raw(), &map) }.map_err(|e| {
            DriverError::MappingFailed {
                phys: phys_addr,
                size,
                reason: format!("{e:?}"),
            }
        })?;

        Ok(Self {
            ptr: ptr as *mut u8,
            size: aligned_size,
        })
    }

    #[inline]
    pub fn read8(&self, offset: usize) -> u8 {
        if offset.checked_add(1).map_or(true, |end| end > self.size) {
            log::error!(
                "MMIO read8 out of bounds: offset={:#x}, size={:#x}",
                offset,
                self.size
            );
            return 0;
        }
        unsafe { core::ptr::read_volatile(self.ptr.add(offset)) }
    }

    #[inline]
    pub fn write8(&self, offset: usize, val: u8) {
        if offset.checked_add(1).map_or(true, |end| end > self.size) {
            log::error!(
                "MMIO write8 out of bounds: offset={:#x}, size={:#x}",
                offset,
                self.size
            );
            return;
        }
        unsafe { core::ptr::write_volatile(self.ptr.add(offset), val) }
    }

    #[inline]
    pub fn read16(&self, offset: usize) -> u16 {
        if offset.checked_add(2).map_or(true, |end| end > self.size) {
            log::error!(
                "MMIO read16 out of bounds: offset={:#x}, size={:#x}",
                offset,
                self.size
            );
            return 0;
        }
        unsafe { core::ptr::read_volatile(self.ptr.add(offset) as *const u16) }
    }

    #[inline]
    pub fn write16(&self, offset: usize, val: u16) {
        if offset.checked_add(2).map_or(true, |end| end > self.size) {
            log::error!(
                "MMIO write16 out of bounds: offset={:#x}, size={:#x}",
                offset,
                self.size
            );
            return;
        }
        unsafe { core::ptr::write_volatile(self.ptr.add(offset) as *mut u16, val) }
    }

    #[inline]
    pub fn read32(&self, offset: usize) -> u32 {
        if offset.checked_add(4).map_or(true, |end| end > self.size) {
            log::error!(
                "MMIO read32 out of bounds: offset={:#x}, size={:#x}",
                offset,
                self.size
            );
            return 0;
        }
        unsafe { core::ptr::read_volatile(self.ptr.add(offset) as *const u32) }
    }

    #[inline]
    pub fn write32(&self, offset: usize, val: u32) {
        if offset.checked_add(4).map_or(true, |end| end > self.size) {
            log::error!(
                "MMIO write32 out of bounds: offset={:#x}, size={:#x}",
                offset,
                self.size
            );
            return;
        }
        unsafe { core::ptr::write_volatile(self.ptr.add(offset) as *mut u32, val) }
    }

    #[inline]
    pub fn read64(&self, offset: usize) -> u64 {
        if offset.checked_add(8).map_or(true, |end| end > self.size) {
            log::error!(
                "MMIO read64 out of bounds: offset={:#x}, size={:#x}",
                offset,
                self.size
            );
            return 0;
        }
        unsafe { core::ptr::read_volatile(self.ptr.add(offset) as *const u64) }
    }

    #[inline]
    pub fn write64(&self, offset: usize, val: u64) {
        if offset.checked_add(8).map_or(true, |end| end > self.size) {
            log::error!(
                "MMIO write64 out of bounds: offset={:#x}, size={:#x}",
                offset,
                self.size
            );
            return;
        }
        unsafe { core::ptr::write_volatile(self.ptr.add(offset) as *mut u64, val) }
    }

    pub fn read_bytes(&self, offset: usize, buf: &mut [u8]) {
        if offset
            .checked_add(buf.len())
            .map_or(true, |end| end > self.size)
        {
            log::error!(
                "MMIO read_bytes out of bounds: offset={:#x}, len={:#x}, size={:#x}",
                offset,
                buf.len(),
                self.size
            );
            return;
        }
        // Volatile byte-by-byte read for MMIO correctness (compiler may
        // optimise away or reorder copy_nonoverlapping).
        for (i, byte) in buf.iter_mut().enumerate() {
            *byte = unsafe { core::ptr::read_volatile(self.ptr.add(offset + i)) };
        }
    }

    pub fn write_bytes(&self, offset: usize, buf: &[u8]) {
        if offset
            .checked_add(buf.len())
            .map_or(true, |end| end > self.size)
        {
            log::error!(
                "MMIO write_bytes out of bounds: offset={:#x}, len={:#x}, size={:#x}",
                offset,
                buf.len(),
                self.size
            );
            return;
        }
        // Volatile byte-by-byte write for MMIO correctness.
        for (i, byte) in buf.iter().enumerate() {
            unsafe { core::ptr::write_volatile(self.ptr.add(offset + i), *byte) };
        }
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.ptr
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr
    }

    pub fn size(&self) -> usize {
        self.size
    }
}

impl Drop for MmioRegion {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            let _ = unsafe { libredox::call::munmap(self.ptr as *mut (), self.size) };
        }
    }
}

unsafe impl Send for MmioRegion {}
unsafe impl Sync for MmioRegion {}
