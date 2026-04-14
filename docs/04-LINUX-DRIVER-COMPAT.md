# 04 — Linux Driver Compatibility Layer: Concrete Implementation Path

> **Status note (2026-04-14):** This file is now partly historical design material. The repository
> already contains `local/recipes/drivers/redox-driver-sys/`, `local/recipes/drivers/linux-kpi/`,
> `local/recipes/system/firmware-loader/`, `local/recipes/gpu/redox-drm/`, and
> `local/recipes/gpu/amdgpu/`. Treat the sections below as architecture rationale and porting notes,
> not as an accurate statement that those components are still "not started".

## Current State Snapshot

| Component | Current repo state |
|---|---|
| `redox-driver-sys` | Present and compiling in `local/recipes/drivers/redox-driver-sys/` |
| `linux-kpi` | Present and compiling in `local/recipes/drivers/linux-kpi/` |
| `firmware-loader` | Present and compiling in `local/recipes/system/firmware-loader/` |
| `redox-drm` | Present and compiling in `local/recipes/gpu/redox-drm/` |
| Intel path | Compile-oriented, no hardware validation yet |
| AMD path | Compile-oriented via `amdgpu` + AMD DC port, no hardware validation yet |
| IOMMU | Partial — daemon now builds, hardware validation still TODO in `local/recipes/system/iommu/` |

## Goal

Enable running Linux GPU drivers (amdgpu, i915, nouveau) on Redox OS with minimal
changes to the driver source code, by providing a FreeBSD LinuxKPI-style compatibility shim.

## Why This Is Needed

Writing native Rust GPU drivers for every vendor is years of work. Linux has mature,
vendor-supported GPU drivers. A compatibility layer lets us port them with `#ifdef __redox__`
patches instead of full rewrites.

**Target drivers** (in priority order):
1. **i915** (Intel) — best documented, most relevant for laptops
2. **amdgpu** (AMD) — large market share, good open-source driver
3. **nouveau / nvk** (NVIDIA) — community driver, limited performance
4. **Skip**: NVIDIA proprietary (binary-only, impossible without full Linux kernel)

---

## Architecture

### Two-Mode Design

The compat layer operates in two modes:

**Mode A: C Driver Port** — Compile Linux C driver against our headers, run as userspace daemon
**Mode B: Rust Wrapper** — Rust crate provides idiomatic API, internally calls compat layer

Both modes share the same bottom layer: `redox-driver-sys`.

```
┌────────────────────────────────────────────────────────────┐
│                    Mode A: C Driver Port                     │
│  Linux C driver (i915.ko source)                            │
│  compiled with -D__redox__ against linux-kpi headers        │
├────────────────────────────────────────────────────────────┤
│                    Mode B: Rust Wrapper                      │
│  Rust crate (redox-intel-gpu) using compat APIs             │
├────────────────────────────────────────────────────────────┤
│               linux-kpi (C header compatibility)             │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐      │
│  │ linux/    │ │ linux/   │ │ linux/   │ │ linux/   │      │
│  │ slab.h   │ │ mutex.h  │ │ pci.h    │ │ drm*.h   │      │
│  │ (malloc) │ │ (pthread)│ │ (pcid)   │ │ (scheme) │      │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘      │
├────────────────────────────────────────────────────────────┤
│               redox-driver-sys (Rust crate)                  │
│  Provides: memory mapping, IRQ, DMA, PCI, DRM scheme       │
├────────────────────────────────────────────────────────────┤
│               Redox OS                                      │
│  scheme:memory  scheme:irq  scheme:pci  scheme:drm          │
└────────────────────────────────────────────────────────────┘
```

---

## Implementation: Crate and File Layout

### Crate 1: `redox-driver-sys` (Low-level Redox driver primitives)

**Repository**: New crate in the Redox ecosystem.
**Purpose**: Safe Rust wrappers around Redox's scheme-based hardware access.

```
redox-driver-sys/
├── Cargo.toml
├── src/
│   ├── lib.rs              — Re-exports
│   ├── memory.rs           — Physical memory mapping (scheme:memory)
│   ├── irq.rs              — Interrupt handling (scheme:irq)
│   ├── pci.rs              — PCI device access (scheme:pci / pcid)
│   ├── io.rs               — Port I/O (iopl syscall)
│   └── dma.rs              — DMA buffer management
```

**Key implementations:**

```rust
// src/memory.rs
pub fn map_physical(phys: u64, size: usize, flags: MapFlags) -> Result<*mut u8> {
    // Open scheme:memory/physical
    // Use fmap to map physical address range
    // flags: WriteCombine, Uncacheable, WriteBack
    let fd = File::open("scheme:memory/physical")?;
    let ptr = syscall::fmap(fd.as_raw_fd(), &Map {
        offset: phys,
        size,
        flags: flags.to_syscall_flags(),
    })?;
    Ok(ptr as *mut u8)
}

pub fn unmap_physical(ptr: *mut u8, size: usize) -> Result<()> {
    syscall::funmap(ptr as usize, size)?;
    Ok(())
}
```

```rust
// src/irq.rs
pub struct IrqHandle { fd: File }

impl IrqHandle {
    pub fn request(irq_num: u32) -> Result<Self> {
        // Open /scheme/irq/{irq_num}
        // Read blocks until interrupt fires
        let fd = File::open(&format!("scheme:irq/{}", irq_num))?;
        Ok(Self { fd })
    }
    
    pub fn wait(&mut self) -> Result<()> {
        let mut buf = [0u8; 8];
        self.fd.read(&mut buf)?;
        Ok(())
    }
}
```

```rust
// src/pci.rs
pub struct PciDevice {
    bus: u8, dev: u8, func: u8,
    vendor_id: u16, device_id: u16,
    bars: [u64; 6],
    bar_sizes: [usize; 6],
    irq: u32,
}

pub fn enumerate() -> Result<Vec<PciDevice>> {
    // Read from pcid-spawner or scheme:pci
    // Parse PCI configuration space for each device
    // Filter to GPU devices (class 0x030000-0x0302xx)
}
```

### Crate 2: `linux-kpi` (Linux kernel API compatibility)

**Repository**: New crate. Installs C headers for use by Linux C drivers.
**Purpose**: Provides `linux/*.h` headers that translate Linux kernel APIs to `redox-driver-sys`.

```
linux-kpi/
├── Cargo.toml
├── src/
│   ├── lib.rs              — Rust API for Rust drivers
│   ├── c_headers/          — C headers for C driver ports
│   │   ├── linux/
│   │   │   ├── slab.h      → malloc/kfree (redox-driver-sys::memory)
│   │   │   ├── mutex.h     → pthread mutex (redox-driver-sys::sync)
│   │   │   ├── spinlock.h  → atomic lock
│   │   │   ├── pci.h       → redox-driver-sys::pci
│   │   │   ├── io.h        → port I/O (iopl)
│   │   │   ├── irq.h       → redox-driver-sys::irq
│   │   │   ├── device.h    → struct device wrapper
│   │   │   ├── kobject.h   → reference-counted object
│   │   │   ├── workqueue.h → thread pool
│   │   │   ├── idr.h       → ID allocation
│   │   │   └── dma-mapping.h → bus DMA (redox-driver-sys::dma)
│   │   ├── drm/
│   │   │   ├── drm.h       → DRM core types
│   │   │   ├── drm_crtc.h  → KMS types
│   │   │   ├── drm_gem.h   → GEM buffer objects
│   │   │   └── drm_ioctl.h → DRM ioctl definitions
│   │   └── asm/
│   │       └── io.h        → inl/outl port I/O
│   └── rust_impl/          — Rust implementations backing the C headers
│       ├── memory.rs       — kzalloc, kmalloc, kfree
│       ├── sync.rs         — mutex, spinlock, completion
│       ├── workqueue.rs    — work queue thread pool
│       ├── pci.rs          — pci_register_driver, etc.
│       └── drm_shim.rs     — DRM core shim (connects to scheme:drm)
```

**Example C header:**

```c
// c_headers/linux/slab.h
#ifndef _LINUX_SLAB_H
#define _LINUX_SLAB_H

#include <stddef.h>

// GFP flags — on Redox, these are no-ops (userspace allocation)
#define GFP_KERNEL  0
#define GFP_ATOMIC  1
#define GFP_DMA32   2

void *kmalloc(size_t size, unsigned int flags);
void *kzalloc(size_t size, unsigned int flags);
void kfree(const void *ptr);

#endif
```

**Corresponding Rust implementation:**

```rust
// src/rust_impl/memory.rs
use std::alloc::{alloc, alloc_zeroed, dealloc, Layout};

#[no_mangle]
pub extern "C" fn kmalloc(size: usize, _flags: u32) -> *mut u8 {
    unsafe {
        let layout = Layout::from_size_align(size, 64).unwrap(); // cache-line aligned
        alloc(layout)
    }
}

#[no_mangle]
pub extern "C" fn kzalloc(size: usize, _flags: u32) -> *mut u8 {
    unsafe {
        let layout = Layout::from_size_align(size, 64).unwrap();
        alloc_zeroed(layout)
    }
}

#[no_mangle]
pub extern "C" fn kfree(ptr: *const u8) {
    if !ptr.is_null() {
        unsafe {
            // Note: Linux kfree doesn't take size. We need a size-tracking allocator.
            // Use a HashMap<ptr, Layout> for tracking, or switch to a custom allocator.
        }
    }
}
```

### Crate 3: `redox-drm` (DRM scheme implementation)

**Repository**: Part of the Redox base repo or new crate.
**Purpose**: The daemon that registers `scheme:drm` and talks to GPU hardware.

```
redox-drm/
├── Cargo.toml
├── src/
│   ├── main.rs             — Daemon entry, scheme registration
│   ├── scheme.rs           — "drm" scheme handler (processes ioctls)
│   ├── kms/
│   │   ├── mod.rs          — KMS core
│   │   ├── crtc.rs         — CRTC state machine
│   │   ├── connector.rs    — Hotplug detection, EDID reading
│   │   ├── encoder.rs      — Encoder management
│   │   └── plane.rs        — Primary/cursor planes
│   ├── gem.rs              — GEM buffer object management
│   ├── dmabuf.rs           — DMA-BUF export/import via FD passing
│   └── drivers/
│       ├── mod.rs          — trait GpuDriver
│       ├── intel/
│       │   ├── mod.rs      — Intel driver entry
│       │   ├── gtt.rs      — Graphics Translation Table
│       │   ├── display.rs  — Display pipe configuration
│       │   └── ring.rs     — Command ring buffer (for acceleration later)
│       └── amd/
│           ├── mod.rs      — AMD driver entry (from amdgpu port)
│           └── ...         — Wrapped amdgpu C code
```

```rust
// src/drivers/mod.rs
pub trait GpuDriver: Send + Sync {
    fn driver_name(&self) -> &str;
    fn driver_desc(&self) -> &str;
    fn driver_date(&self) -> &str;
    
    // KMS
    fn get_modes(&self, connector: u32) -> Vec<ModeInfo>;
    fn set_crtc(&self, crtc: u32, fb: u32, connectors: &[u32], mode: &ModeInfo) -> Result<()>;
    fn page_flip(&self, crtc: u32, fb: u32, flags: u32) -> Result<u64>;
    fn get_vblank(&self, crtc: u32) -> Result<u64>;
    
    // GEM
    fn gem_create(&self, size: u64) -> Result<GemHandle>;
    fn gem_close(&self, handle: GemHandle) -> Result<()>;
    fn gem_mmap(&self, handle: GemHandle) -> Result<*mut u8>;
    fn gem_export_dmafd(&self, handle: GemHandle) -> Result<RawFd>;
    fn gem_import_dmafd(&self, fd: RawFd) -> Result<GemHandle>;
    
    // Connector info
    fn detect_connectors(&self) -> Vec<ConnectorInfo>;
    fn get_edid(&self, connector: u32) -> Vec<u8>;
}
```

---

## Concrete Porting Example: Intel i915 Driver

### Step 1: Extract i915 from Linux kernel

```bash
# Clone Linux kernel
git clone --depth 1 https://github.com/torvalds/linux.git
# Extract relevant directories
tar cf intel-driver.tar linux/drivers/gpu/drm/i915/ \
    linux/include/drm/ \
    linux/include/linux/ \
    linux/arch/x86/include/
```

### Step 2: Create recipe

```toml
# recipes/wip/drivers/i915/recipe.toml
[source]
tar = "intel-driver.tar"

[build]
template = "custom"
dependencies = [
    "redox-driver-sys",
    "linux-kpi",
    "redox-drm",
]
script = """
DYNAMIC_INIT

# Build i915 driver as a shared library
# linked against linux-kpi and redox-driver-sys
export CFLAGS="-I${COOKBOOK_SYSROOT}/include/linux-kpi -D__redox__"
export LDFLAGS="-lredox_driver_sys -llinux_kpi -lredox_drm"

# Compile the driver source files
find drivers/gpu/drm/i915/ -name '*.c' | while read src; do
    x86_64-unknown-redox-gcc -c $CFLAGS "$src" -o "${src%.c}.o" || true
done

# Link into a single shared library
x86_64-unknown-redox-gcc -shared -o i915_redox.so \
    $(find drivers/gpu/drm/i915/ -name '*.o') \
    $LDFLAGS

mkdir -p ${COOKBOOK_STAGE}/usr/lib/redox/drivers
cp i915_redox.so ${COOKBOOK_STAGE}/usr/lib/redox/drivers/
"""
```

### Step 3: Minimal patches needed

For i915 on Redox, these are the typical `#ifdef __redox__` changes:

```c
// Example patches (conceptual):

// 1. Replace Linux module init with daemon main()
#ifdef __redox__
int main(int argc, char **argv) {
    return i915_driver_init();
}
#else
module_init(i915_init);
module_exit(i915_exit);
#endif

// 2. Replace kernel memory allocation
#ifdef __redox__
#include <linux/slab.h>  // Our compat header
// kzalloc/kfree still work, but go to userspace allocator
#else
#include <linux/slab.h>  // Real Linux
#endif

// 3. Replace PCI access
#ifdef __redox__
// Use redox-driver-sys PCI API instead of linux/pci.h internals
struct pci_dev *pdev = redox_pci_find_device(PCI_VENDOR_ID_INTEL, device_id);
#else
pdev = pci_get_device(PCI_VENDOR_ID_INTEL, device_id, NULL);
#endif

// 4. Replace MMIO mapping
#ifdef __redox__
void __iomem *regs = redox_ioremap(pci_resource_start(pdev, 0), pci_resource_len(pdev, 0));
#else
void __iomem *regs = ioremap(pci_resource_start(pdev, 0), pci_resource_len(pdev, 0));
#endif
```

### Step 4: Run as daemon

```bash
# In Redox init:
i915d  # Registers scheme:drm/card0
```

---

## Concrete Porting Example: AMD amdgpu Driver

AMD's driver is larger and more complex than Intel's. The LinuxKPI approach is essential.

### Key challenges for amdgpu:

1. **Firmware loading**: amdgpu needs proprietary firmware blobs. Redox has no firmware
   loading infrastructure yet. Need to implement:
   ```
   scheme:firmware/amdgpu/  — firmware blob storage
   request_firmware()       — compat function that reads from scheme
   ```

2. **TTM memory manager**: amdgpu uses TTM (Translation Table Maps) for GPU memory.
   Need to port TTM to use Redox's memory scheme:
   ```rust
   // TTM → Redox mapping:
   // ttm_tt → allocated pages via scheme:memory
   // ttm_buffer_object → GemHandle in scheme:drm
   // ttm_bo_move → page table updates via GPU MMIO
   ```

3. **Display Core (DC)**: AMD's display code is ~100K lines. Need to:
   - Port DCN (Display Core Next) hardware programming
   - Adapt to Redox's DRM scheme instead of Linux kernel DRM
   - Keep most code unchanged, just redirect memory/register access

4. **Power management**: amdgpu uses Linux power management APIs. Need stubs:
   ```c
   #ifdef __redox__
   // No power management on Redox yet — always-on
   #define pm_runtime_get_sync(dev) 0
   #define pm_runtime_put_autosuspend(dev) 0
   #define pm_runtime_allow(dev) 0
   #endif
   ```

### Estimated patches for amdgpu: ~2000-3000 lines of `#ifdef __redox__`

---

## evdev Compatibility Layer

In addition to GPU drivers, we need an evdev compat layer for input:

### Crate: `redox-evdev`

```
redox-evdev/
├── src/
│   ├── lib.rs              — evdev API for Rust
│   ├── c_headers/
│   │   └── linux/
│   │       └── input.h     — struct input_event, EV_*, KEY_*, etc.
│   └── daemon/
│       └── main.rs         — evdevd daemon (see doc 03)
```

The C header `linux/input.h` provides:
- `struct input_event` — identical to Linux
- `EV_KEY`, `EV_REL`, `EV_ABS` — event types
- `KEY_*`, `BTN_*`, `REL_*`, `ABS_*` — event codes
- `EVIOCG*` ioctl numbers — same values as Linux

The daemon reads from Redox input schemes and exposes `/dev/input/eventX` nodes.

---

## Implementation Priority and Timeline

| Phase | Component | Effort | Delivers |
|-------|-----------|--------|----------|
| 1 | `redox-driver-sys` crate | 2-3 weeks | Memory, IRQ, PCI, I/O primitives |
| 2 | Intel native driver (in `redox-drm`) | 6-8 weeks | First working GPU driver, modesetting |
| 3 | `linux-kpi` C headers (core subset) | 3-4 weeks | Memory, sync, PCI, workqueue headers |
| 4 | `linux-kpi` DRM headers | 2-3 weeks | DRM/KMS/GEM C API headers |
| 5 | i915 C driver port | 3-4 weeks | Proves LinuxKPI approach works |
| 6 | `linux-kpi` extended (TTM, firmware) | 4-6 weeks | Enables AMD driver |
| 7 | amdgpu C driver port | 6-8 weeks | AMD GPU support |

**Phase 1-2 is the critical path** — a native Rust Intel driver proves the architecture
and provides immediate value. Phases 3-7 can happen in parallel or later.

### With 2 developers:
- **Month 1-2**: redox-driver-sys + Intel native driver → first display output
- **Month 3-4**: linux-kpi core + DRM headers → i915 C port proof of concept
- **Month 5-8**: linux-kpi TTM + amdgpu port → AMD support
- **Total: 6-8 months** to support both Intel and AMD GPUs

### With 1 developer:
- **Month 1-3**: redox-driver-sys + Intel native driver
- **Month 4-6**: linux-kpi core + i915 port
- **Month 7-10**: amdgpu port
- **Total: 8-10 months**
