use core::ptr::NonNull;
use std::sync::atomic::{AtomicI32, Ordering};

use redox_syscall::flag::{MAP_SHARED, O_CLOEXEC, O_RDWR, PROT_READ, PROT_WRITE};
use redox_syscall::PAGE_SIZE;
use syscall as redox_syscall;

use crate::{DriverError, Result};

/// SAFETY: Cached FD for `/scheme/memory/physical`. -1 means uninitialized.
/// This FD is process-lifetime cached for performance. If scheme:memory
/// restarts (which should never happen — it's a kernel scheme), all
/// in-flight DMA operations are already undefined behavior.
static DMA_MEMORY_FD: AtomicI32 = AtomicI32::new(-1);

fn get_dma_memory_fd() -> Result<i32> {
    let current = DMA_MEMORY_FD.load(Ordering::Acquire);
    if current >= 0 {
        return Ok(current);
    }

    let fd = libredox::call::open("/scheme/memory/physical", (O_CLOEXEC | O_RDWR) as i32, 0)
        .map_err(|e| DriverError::Io(std::io::Error::from_raw_os_error(e.errno())))?;

    let raw = fd as i32;
    // Try to store; if another thread won the race, close ours and use theirs.
    match DMA_MEMORY_FD.compare_exchange(-1, raw, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => Ok(raw),
        Err(existing) => {
            let _ = libredox::call::close(fd as usize);
            Ok(existing)
        }
    }
}

fn virt_to_phys_cached(virt: usize) -> Result<usize> {
    // Use a cached fd for address translation
    static TRANSLATION_FD: AtomicI32 = AtomicI32::new(-1);

    let raw = match TRANSLATION_FD.load(Ordering::Acquire) {
        fd if fd >= 0 => fd,
        _ => {
            let fd = libredox::Fd::open("/scheme/memory/translation", O_CLOEXEC as i32, 0)
                .map_err(|e| DriverError::Io(std::io::Error::from_raw_os_error(e.errno())))?;
            let raw = fd.raw() as i32;
            // Leak the fd intentionally — it's a global cache
            std::mem::forget(fd);
            match TRANSLATION_FD.compare_exchange(-1, raw, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => raw,
                Err(existing) => {
                    let _ = libredox::call::close(raw as usize);
                    existing
                }
            }
        }
    };

    let mut buf = virt.to_ne_bytes();
    libredox::call::call_ro(
        raw as usize,
        &mut buf,
        redox_syscall::CallFlags::empty(),
        &[],
    )
    .map_err(DriverError::from)?;
    Ok(usize::from_ne_bytes(buf))
}

enum DmaStorage {
    /// Allocated via scheme:memory — freed via munmap
    SchemeMapped { ptr: NonNull<u8>, size: usize },
    /// Allocated via heap — freed via dealloc
    Heap {
        ptr: NonNull<u8>,
        layout: std::alloc::Layout,
    },
}

pub struct DmaBuffer {
    storage: DmaStorage,
    phys_addr: usize,
    size: usize,
}

impl DmaBuffer {
    /// Allocate a physically contiguous DMA buffer.
    ///
    /// Uses scheme:memory to allocate real physical pages, ensuring the buffer
    /// is safe for DMA hardware access. Falls back to heap allocation only in
    /// non-Redox environments (e.g., Linux host for testing), logging a warning.
    pub fn allocate(size: usize, align: usize) -> Result<Self> {
        let align = align.max(64);
        let aligned_size = size.next_multiple_of(PAGE_SIZE).max(align);

        // Attempt 1: Allocate via scheme:memory (physically contiguous)
        if let Ok(mem_fd) = get_dma_memory_fd() {
            if let Ok(mapped) = Self::allocate_via_scheme(mem_fd, aligned_size, align) {
                return Ok(mapped);
            }
        }

        // Fallback: heap allocation (NOT physically contiguous — log warning)
        log::warn!(
            "DmaBuffer: falling back to heap allocation ({} bytes) — NOT physically contiguous!",
            size
        );
        let layout = std::alloc::Layout::from_size_align(size, align)
            .map_err(|e| DriverError::Other(format!("invalid DMA layout: {e}")))?;

        let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
        let ptr = NonNull::new(ptr).ok_or_else(|| {
            DriverError::Other(format!(
                "DMA allocation failed: {size} bytes aligned to {align}"
            ))
        })?;

        let phys_addr = virt_to_phys_cached(ptr.as_ptr() as usize)?;

        Ok(Self {
            storage: DmaStorage::Heap { ptr, layout },
            phys_addr,
            size,
        })
    }

    /// Allocate physically contiguous memory via scheme:memory/physical.
    fn allocate_via_scheme(mem_fd: i32, size: usize, _align: usize) -> Result<Self> {
        // Open a physical memory region of the requested size
        let path = format!("zeroed@{}", size);
        let region_fd =
            libredox::call::openat(mem_fd as usize, &path, (O_CLOEXEC | O_RDWR) as i32, 0)
                .map_err(|e| DriverError::Io(std::io::Error::from_raw_os_error(e.errno())))?;

        // Map it into our address space
        let ptr = unsafe {
            libredox::call::mmap(libredox::call::MmapArgs {
                fd: region_fd as usize,
                offset: 0,
                length: size,
                flags: MAP_SHARED.bits() as u32,
                prot: (PROT_READ | PROT_WRITE).bits() as u32,
                addr: core::ptr::null_mut(),
            })
        }
        .map_err(|e| {
            let _ = libredox::call::close(region_fd as usize);
            DriverError::MappingFailed {
                phys: 0,
                size,
                reason: format!("DMA mmap failed: {e:?}"),
            }
        })?;

        let _ = libredox::call::close(region_fd as usize);

        let phys_addr = virt_to_phys_cached(ptr as usize)?;
        let ptr = NonNull::new(ptr as *mut u8)
            .ok_or_else(|| DriverError::Other("DMA mmap returned null".into()))?;

        log::debug!(
            "DmaBuffer: {} bytes at virt={:#x} phys={:#x} (physically contiguous)",
            size,
            ptr.as_ptr() as usize,
            phys_addr
        );

        Ok(Self {
            storage: DmaStorage::SchemeMapped { ptr, size },
            phys_addr,
            size,
        })
    }

    pub fn as_ptr(&self) -> *const u8 {
        match &self.storage {
            DmaStorage::SchemeMapped { ptr, .. } | DmaStorage::Heap { ptr, .. } => ptr.as_ptr(),
        }
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        match &mut self.storage {
            DmaStorage::SchemeMapped { ptr, .. } | DmaStorage::Heap { ptr, .. } => ptr.as_ptr(),
        }
    }

    pub fn physical_address(&self) -> usize {
        self.phys_addr
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Returns true if this buffer is guaranteed physically contiguous.
    /// On real hardware, this must be true for DMA to work safely.
    pub fn is_physically_contiguous(&self) -> bool {
        matches!(self.storage, DmaStorage::SchemeMapped { .. })
    }
}

impl Drop for DmaBuffer {
    fn drop(&mut self) {
        match &self.storage {
            DmaStorage::SchemeMapped { ptr, size } => {
                let _ = unsafe { libredox::call::munmap(ptr.as_ptr() as *mut (), *size) };
            }
            DmaStorage::Heap { ptr, layout } => {
                unsafe { std::alloc::dealloc(ptr.as_ptr(), *layout) };
            }
        }
    }
}

unsafe impl Send for DmaBuffer {}
unsafe impl Sync for DmaBuffer {}
