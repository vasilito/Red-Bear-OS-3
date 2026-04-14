# AMD-FIRST REDOX OS — MASTER INTEGRATION PLAN

**Target**: Modern AMD64 bare metal machine with AMD GPU (RDNA2/RDNA3)
**Secondary**: Intel GPU machines
**Date**: 2026-04-11

## CRITICAL FINDINGS

### amdgpu is 18x larger than Intel i915

| Driver | Lines of Code | Complexity |
|--------|--------------|------------|
| amdgpu (AMD) | **6,048,151** | Largest driver in Linux kernel |
| i915 (Intel) | ~341,000 | Well-documented, simpler |
| nouveau (NVIDIA) | ~400,000 | Community driver |

**Implication**: AMD-first is HARDER but has larger market impact. We MUST use
the LinuxKPI compatibility approach — a clean Rust rewrite would take 5+ years.

### AMD Bare Metal Status on Redox

| Component | Status | Detail |
|-----------|--------|--------|
| UEFI boot | ✅ Works | x86_64 UEFI bootloader functional |
| AMD CPUs | ✅ Works | AMD 32/64-bit supported, Ryzen Threadripper verified |
| ACPI | ✅ Complete | RSDP/SDT checksums, MADT types 0x4/0x5/0x9/0xA, LVT NMI, FADT shutdown/reboot |
| x2APIC | ✅ Works | Auto-detected via CPUID, APIC/SMP functional |
| HPET | ✅ Works | Timer initialized from ACPI |
| IOMMU | 🚧 Buildable, unvalidated | `iommu` daemon now builds, but no VT-d/AMD-Vi hardware validation yet |
| AMD GPU | 🚧 In progress | MMIO mapped, DC port compiles, MSI-X wired, no hardware validation yet |
| Wi-Fi/BT | ❌ Missing | No wireless support |
| USB | ⚠️ Variable | Some USB controllers work, others don't |

### Known AMD-Specific Issues

1. **Framework Laptop 16 (AMD Ryzen 7040)**: CRASHES — unimplemented ACPI function (jackpot51/acpi#3)
2. **ASUS PRIME B350M-E**: Partial PS/2 keyboard, mouse broken
3. **Zen3+ page alignment**: Potential memory corruption with 16k-aligned pages
4. **I2C on AMD platforms**: Touchpad may fail

---

## PHASE 0: BARE METAL BOOT ON AMD (4-6 weeks)

Before any GPU or desktop work, Redox must boot reliably on modern AMD hardware.

### P0-1: Fix ACPI for AMD

**Problem**: Framework AMD Ryzen 7040 crashes. ACPI is incomplete.

**What to do**:
- Identify which ACPI function is unimplemented (see jackpot51/acpi#3)
- Implement missing ACPI table parsers (FACP, DSDT, SSDT)
- Test on: Framework 16, ASUS B350M-E, any modern AMD board

**Where**: 
- Kernel: `recipes/core/kernel/source/src/acpi/`
- acpid: `recipes/core/base/source/drivers/acpid/`
- Patches: `local/patches/kernel/`

### P0-2: AMD-Specific Boot Hardening

**What to do**:
- Fix CPUID validation (FIXME in cpuid.rs)
- Fix Zen3+ page alignment issue (16k-aligned page smashing)
- Ensure trampoline page permissions are correct
- Validate memory map parsing on AMD systems with >4GB

**Where**: `recipes/core/kernel/source/src/arch/x86_64/`

### P0-3: Hardware Testing Matrix

**Required test hardware**:
- AMD Ryzen desktop (B550/X570 motherboard)
- AMD Ryzen laptop (Framework 16 or similar)
- AMD APU system (Ryzen 5xxxG series)

**Test procedure**: Write to `local/scripts/test-baremetal.sh`

---

## PHASE 1: DRIVER INFRASTRUCTURE (8-12 weeks)

### P1-1: redox-driver-sys Crate

**Purpose**: Safe Rust wrappers around Redox scheme-based hardware access.

```
local/recipes/drivers/redox-driver-sys/
├── Cargo.toml
├── src/
│   ├── lib.rs          # Re-exports
│   ├── memory.rs       # Physical memory mapping (scheme:memory)
│   ├── irq.rs          # Interrupt handling (scheme:irq)
│   ├── pci.rs          # PCI device access (scheme:pci / pcid)
│   ├── io.rs           # Port I/O (iopl syscall)
│   └── dma.rs          # DMA buffer management
```

**API design**: See `docs/04-LINUX-DRIVER-COMPAT.md` §Crate 1.

### P1-2: Firmware Loading Infrastructure

**Purpose**: Load AMD GPU firmware blobs from filesystem.

```
local/recipes/system/firmware-loader/
├── Cargo.toml
├── src/
│   ├── main.rs          # Daemon: registers scheme:firmware
│   ├── scheme.rs        # "firmware" scheme handler
│   └── blob.rs          # Firmware blob management
```

**Firmware blobs needed for amdgpu** (from linux-firmware):

| Block | Purpose | File Pattern |
|-------|---------|-------------|
| PSP | Security processor | `psp_*_sos.bin`, `psp_*_ta.bin` |
| GC | Graphics/shader engine | `gc_*_me.bin`, `gc_*_pfp.bin`, `gc_*_ce.bin` |
| SDMA | DMA engine | `sdma_*_bin.bin` |
| VCN | Video encode/decode | `vcn_*_bin.bin` |
| SMC | Power management | `smu_*_bin.bin` |
| DMCUB | Display controller | `dcn_*_dmcub.bin` |

**Storage**: `local/firmware/amdgpu/` (fetched via `local/scripts/fetch-firmware.sh`)

### P1-3: linux-kpi Compatibility Headers

**Purpose**: C headers translating Linux kernel APIs → redox-driver-sys Rust calls.

```
local/recipes/drivers/linux-kpi/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── c_headers/linux/
│   │   ├── slab.h       # → malloc/kfree
│   │   ├── mutex.h      # → pthread mutex
│   │   ├── spinlock.h   # → atomic lock
│   │   ├── pci.h        # → redox-driver-sys::pci
│   │   ├── io.h         # → port I/O
│   │   ├── irq.h        # → redox-driver-sys::irq
│   │   ├── device.h     # → struct device wrapper
│   │   ├── workqueue.h  # → thread pool
│   │   ├── dma-mapping.h # → bus DMA
│   │   └── firmware.h   # → firmware_loader scheme
│   ├── c_headers/drm/
│   │   ├── drm.h
│   │   ├── drm_crtc.h
│   │   ├── drm_gem.h
│   │   └── drm_ioctl.h
│   └── rust_impl/
│       ├── memory.rs    # kmalloc, kzalloc, kfree
│       ├── sync.rs      # mutex, spinlock, completion
│       ├── pci.rs       # pci_register_driver
│       ├── firmware.rs  # request_firmware
│       └── drm_shim.rs  # DRM core → scheme:drm
```

---

## PHASE 2: AMD GPU DISPLAY OUTPUT (12-16 weeks)

### P2-1: redox-drm Daemon

**Purpose**: DRM scheme daemon — registers `scheme:drm/card0`.

```
local/recipes/gpu/redox-drm/
├── Cargo.toml
├── src/
│   ├── main.rs           # Daemon entry, PCI enumeration for AMD GPUs
│   ├── scheme.rs         # Registers "drm" scheme
│   ├── kms/              # KMS core
│   │   ├── crtc.rs       # CRTC state machine
│   │   ├── connector.rs  # Hotplug, EDID
│   │   ├── encoder.rs    # Encoder management
│   │   └── plane.rs      # Primary/cursor planes
│   ├── gem.rs            # GEM buffer objects
│   ├── dmabuf.rs         # DMA-BUF export/import
│   └── drivers/
│       ├── mod.rs         # trait GpuDriver
│       └── amd/
│           ├── mod.rs     # AMD driver entry
│           ├── display.rs # Display Core (DC) port
│           ├── gtt.rs     # Graphics Translation Table
│           └── ring.rs    # Command ring buffer
```

### P2-2: AMD Display Core Port (Mode A — C port)

**The critical decision**: amdgpu's display code (AMD DC) is ~1.5M lines. We port
ONLY the display/modesetting portion first, using linux-kpi headers.

**Approach**:
1. Extract `drivers/gpu/drm/amd/display/` from Linux kernel
2. Compile against linux-kpi headers with `-D__redox__`
3. Run as userspace daemon under redox-drm
4. Start with basic modesetting (no acceleration)

**Estimated patches**: ~3000-5000 lines of `#ifdef __redox__`

### P2-3: Firmware Loading for AMD

**Sequence on boot**:
```
1. pcid detects AMD GPU (vendor 0x1002)
2. pcid-spawner launches redox-drm with PCI device info
3. redox-drm maps MMIO registers via scheme:memory
4. redox-drm loads PSP firmware via scheme:firmware
5. PSP firmware loads GC, SDMA, SMC, DMCUB sub-firmwares
6. AMD DC initializes display pipeline
7. scheme:drm/card0 registered
8. modetest -M amd shows display modes
```

### Verification (Phase 2 complete when):
- `scheme:drm/card0` exists
- `modetest -M amd` shows connector info and modes
- `modetest -M amd -s 0:1920x1080` sets mode and shows test pattern
- Works on real AMD hardware (not just QEMU)

---

## P1/P2 IMPLEMENTATION STATUS (2026-04-12)

### P1: Driver Infrastructure — COMPLETE (compiles)

| Component | Status | Files |
|-----------|--------|-------|
| redox-driver-sys | ✅ | `local/recipes/drivers/redox-driver-sys/source/` — PCI, IRQ (MSI-X), MMIO, DMA |
| linux-kpi | ✅ | `local/recipes/drivers/linux-kpi/source/` — C compat headers + Rust shims |
| firmware-loader | ✅ | `local/recipes/system/firmware-loader/source/` — scheme:firmware daemon |
| pcid /config endpoint | ✅ | `local/patches/base/P0-pcid-config-endpoint.patch` — raw PCI config space via scheme:pci |
| MSI-X interrupt support | ✅ | `local/recipes/gpu/redox-drm/source/src/drivers/interrupt.rs` — shared MSI-X/legacy abstraction |
| Intel pcid-spawner config | ✅ | `local/config/pcid.d/intel_gpu.toml` — auto-detect Intel GPUs |

### P2: AMD GPU Display — COMPLETE (compiles, no HW validation)

| Component | Status | Files |
|-----------|--------|-------|
| redox-drm daemon | ✅ | `local/recipes/gpu/redox-drm/source/` — DRM scheme daemon |
| AMD driver (Rust) | ✅ | `local/recipes/gpu/redox-drm/source/src/drivers/amd/mod.rs` |
| AMD DisplayCore (FFI) | ✅ | `local/recipes/gpu/redox-drm/source/src/drivers/amd/display.rs` |
| AMD PCI stubs (dynamic) | ✅ | `local/recipes/gpu/amdgpu/source/redox_stubs.c` — populated from Rust via FFI |
| AMD DC init (C) | ✅ | `local/recipes/gpu/amdgpu/source/amdgpu_redox_main.c` — modesetting, connector detect |
| AMD glue headers | ✅ | `local/recipes/gpu/amdgpu/source/redox_glue.h` — Linux compat surface |
| GTT manager | ✅ | `local/recipes/gpu/redox-drm/source/src/drivers/amd/gtt.rs` |
| Ring buffer | ✅ | `local/recipes/gpu/redox-drm/source/src/drivers/amd/ring.rs` |
| GEM buffer mgmt | ✅ | `local/recipes/gpu/redox-drm/source/src/gem.rs` |
| DMA-BUF | ✅ | `local/recipes/gpu/redox-drm/source/src/dmabuf.rs` |
| Intel driver | ✅ | `local/recipes/gpu/redox-drm/source/src/drivers/intel/mod.rs` + `display.rs` |

### Build Verification

All crates compile with `cargo check` (0 errors):
- `redox-driver-sys` ✅
- `linux-kpi` ✅
- `redox-drm` ✅
- `firmware-loader` ✅
- `evdevd` ✅
- `udev-shim` ✅
- `ext4d` ✅

### Next Steps (P2 → P3)

P2 code compiles but has NOT been validated on real hardware. Remaining:
1. Flash Red Bear OS image to USB, boot on AMD hardware with RDNA2/RDNA3 GPU
2. Verify pcid exposes `/scheme/pci/{addr}/config` and MSI-X vectors allocate
3. Verify redox-drm detects GPU, maps MMIO, initializes DC
4. Test connector detection and modesetting via scheme:drm
5. Begin P3 (POSIX gaps + evdevd) in parallel with hardware validation

---

## PHASE 3: INPUT + POSIX (4-8 weeks, parallel with Phase 2)

### P3-1: relibc POSIX Gaps (2-4 weeks)

7 APIs needed by libwayland. Same as before regardless of GPU vendor.

| API | Effort | File to create/modify |
|-----|--------|----------------------|
| signalfd/signalfd4 | ~200 lines | `relibc/src/header/signal/` |
| timerfd_create/settime/gettime | ~300 lines | `relibc/src/header/sys_timerfd/` (NEW) |
| eventfd | ~100 lines | `relibc/src/header/sys_eventfd/` (NEW) |
| F_DUPFD_CLOEXEC | ~20 lines | `relibc/src/header/fcntl/` |
| MSG_CMSG_CLOEXEC, MSG_NOSIGNAL | ~50 lines | `relibc/src/header/sys_socket/` |
| open_memstream | ~200 lines | `relibc/src/header/stdio/` |

**Patches go in**: `local/patches/relibc/`

### P3-2: evdevd Input Daemon (4-6 weeks)

Same as before. GPU vendor doesn't affect input path.

```
local/recipes/system/evdevd/
├── src/
│   ├── main.rs       # Read Redox input schemes, expose /dev/input/eventX
│   ├── scheme.rs     # "evdev" scheme
│   ├── device.rs     # Translate Redox events → input_event
│   └── ioctl.rs      # EVIOCG* ioctls
```

---

## PHASE 4: WAYLAND COMPOSITOR (4-6 weeks after P2+P3)

### P4-1: Smithay Redox Backends

```
smithay/src/backend/
├── input/redox.rs    # Input backend (reads evdev via evdevd)
├── drm/redox.rs      # DRM backend (uses scheme:drm)
└── egl/redox.rs      # EGL display (uses Mesa)
```

### P4-2: libdrm AMD Backend

Currently libdrm has `-Damdgpu=disabled`. Enable it once redox-drm exists.

**Patches**: `local/patches/libdrm/`

---

## PHASE 5: AMD GPU ACCELERATION (16-24 weeks, parallel with P4)

### P5-1: Full amdgpu Port via LinuxKPI

This is the big one. Port the full amdgpu driver using linux-kpi headers.

**Scope**: ~666k lines of actual C code (excluding auto-generated headers)

**Approach**:
1. Port TTM memory manager first (needed by amdgpu VM)
2. Port AMD GPU VM (page table management)
3. Port command submission (ring buffers, fences)
4. Port display features beyond basic modesetting
5. Port power management (SMU interface)
6. Port video decode (VCN) — optional, later

**Estimated effort**: 
- TTM: ~4 weeks
- VM + command submission: ~6 weeks
- Full driver: ~12-16 weeks
- Total with linux-kpi: **16-24 weeks**

---

## PHASE 6: KDE PLASMA (12-16 weeks after P4)

Same as previous plan (docs/05). GPU vendor doesn't affect Qt/KDE path.

1. Qt6 base + qtwayland (6-8 weeks)
2. KDE Frameworks tier 1-3 (6-8 weeks)
3. KWin + Plasma Shell (4-6 weeks)

---

## REVISED TIMELINE (AMD-FIRST)

```
Week 1-6:     P0 — Fix ACPI, boot on AMD bare metal
Week 3-14:    P1 — redox-driver-sys + firmware-loader + linux-kpi (parallel)
Week 15-30:   P2 — redox-drm + AMD DC display port (parallel)
Week 3-10:    P3 — POSIX gaps + evdevd (parallel with P1)
Week 31-36:   P4 — Smithay Wayland compositor (needs P2+P3)
Week 15-38:   P5 — Full amdgpu via LinuxKPI (parallel with P3-P4)
Week 37-52:   P6 — KDE Plasma (needs P4)
```

**With 2 developers**: ~52 weeks (~12 months) to KDE Plasma on AMD bare metal.
**With 1 developer**: ~18-24 months.

### Critical Path

```
P0 (ACPI boot)
  → P1 (driver infra) → P2 (AMD display) → P4 (Wayland) → P6 (KDE)
                         P3 (POSIX+input) ──┘
                         P5 (full amdgpu, parallel)
```

---

## WHAT NEEDS TO BE DOCUMENTED

### New Documents to Create

| Document | Location | Purpose |
|----------|----------|---------|
| This file | `local/docs/AMD-FIRST-INTEGRATION.md` | Master plan |
| ACPI fix guide | `local/docs/ACPI-FIXES.md` | What ACPI functions are missing |
| Firmware loading spec | `local/docs/FIRMWARE-LOADING.md` | How AMD firmware loading works |
| AMD GPU register notes | `local/docs/AMD-GPU-NOTES.md` | Hardware programming notes |
| Bare metal testing log | `local/docs/BAREMETAL-LOG.md` | Hardware test results |
| Build guide (AMD) | `local/docs/BUILD-GUIDE-AMD.md` | How to build for AMD hardware |
| Overlay usage guide | `local/AGENTS.md` | How to use local/ overlay |

### Existing Documents to Update

| Document | Change |
|----------|--------|
| `AGENTS.md` (root) | Add AMD-first strategy, local/ overlay refs |
| `recipes/core/AGENTS.md` | Add AMD boot requirements, IOMMU note |
| `recipes/wip/AGENTS.md` | Add AMD GPU driver WIP section |
| `docs/AGENTS.md` | Add reference to local/docs/ |
| `docs/04-LINUX-DRIVER-COMPAT.md` | Add AMD-specific porting notes |
| `docs/02-GAP-ANALYSIS.md` | Add P0 bare metal boot layer |

### Config Files to Create

| File | Purpose |
|------|---------|
| `local/config/my-amd-desktop.toml` | AMD desktop build config |
| `local/scripts/fetch-firmware.sh` | Download AMD firmware blobs |
| `local/scripts/build-amd.sh` | Build wrapper for AMD target |
| `local/scripts/test-baremetal.sh` | Burn + test on real hardware |

---

## ANTI-PATTERNS FOR AMD-FIRST

- **DO NOT** attempt a clean Rust rewrite of amdgpu — 6M lines, 5+ years
- **DO NOT** skip ACPI fixes — AMD machines WILL NOT BOOT without complete ACPI
- **DO NOT** forget firmware blobs — amdgpu CANNOT FUNCTION without PSP/GC/SDMA firmware
- **DO NOT** test only in QEMU — AMD GPU behavior differs significantly from VirtIO
- **DO NOT** assume Intel patterns work for AMD — AMD uses different register maps, different firmware flow
- **DO NOT** port old GCN GPUs — target RDNA2+ only (reduces scope by ~40%)
