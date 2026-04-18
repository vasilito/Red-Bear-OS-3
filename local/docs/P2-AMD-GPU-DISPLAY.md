# Historical Phase P2: AMD GPU Display Output

## Status: Historical implementation milestone complete — hardware validation pending

> **Planning authority note (2026-04-18):** this file is an AMD display implementation/status
> reference. For current GPU/DRM execution order, Intel/AMD parity criteria, and future task
> sequencing, use `local/docs/DRM-MODERNIZATION-EXECUTION-PLAN.md`.

The original P2 implementation task was completed and compile-validated for its scoped deliverables.
This file is now a historical AMD display implementation/status reference rather than the canonical
GPU planning document. Hardware validation remains a separate milestone requiring physical AMD GPU
hardware.

## Goal
Enable AMD GPU display output (modesetting) on Redox OS via a DRM scheme daemon
that ports the AMD Display Core (DC) from Linux kernel 7.0-rc7.

## Architecture

Userspace apps → scheme:drm → redox-drm daemon → AMD DC (C code, linux-kpi) → MMIO

## Components

### redox-drm (local/recipes/gpu/redox-drm/)
DRM scheme daemon. Registers scheme:drm/card0.
- PCI enumeration for AMD GPUs (vendor 0x1002)
- MMIO register mapping via redox-driver-sys
- KMS: connector detection, mode getting, CRTC programming
- GEM: buffer object create/mmap/close
- Dispatches to AMD driver backend

### amdgpu C port (local/recipes/gpu/amdgpu/source/)
AMD GPU driver source extracted from Linux 7.0-rc7:
- drivers/gpu/drm/amd/ — full AMD driver (269k lines)
- drivers/gpu/drm/ttm/ — TTM memory manager
- include/drm/ — DRM core headers
- include/linux/ — Linux kernel headers (reference)

### amdgpu build recipe (local/recipes/gpu/amdgpu/)
Compiles AMD DC display code against linux-kpi headers with -D__redox__:
- recipe.toml — custom build template
- redox_glue.h — type compatibility, function stubs, macro replacements
- redox_stubs.c — C implementations of Linux kernel API stubs
- amdgpu_redox_main.c — daemon entry point replacing module_init

## Build Integration

Config: config/redbear-desktop.toml (includes desktop.toml + Red Bear GPU packages)
- Includes redox-drm and amdgpu packages
- filesystem_size = 8196 (8GB, needs space for firmware blobs)

pcid: local/config/pcid.d/amd_gpu.toml
- Auto-detects AMD GPU (vendor 0x1002, class 0x03)
- Launches redox-drm with PCI device location

## Boot Sequence (P2)

1. Kernel boots, initializes PCI subsystem
2. pcid detects AMD GPU (vendor 0x1002)
3. pcid-spawner launches: redox-drm $BUS $DEV $FUNC
4. redox-drm opens PCI device, verifies AMD GPU
5. redox-drm maps MMIO BAR0 (GPU registers)
6. redox-drm loads PSP firmware via scheme:firmware
7. redox-drm initializes AMD DC (Display Core)
8. AMD DC detects connectors, reads EDID
9. scheme:drm/card0 registered
10. Userspace can begin bounded display-side probing (for example `modetest`) once runtime validation is available; this is not yet a hardware-backed support claim by itself

## Verification

### Code Complete (P2 implementation task)
- [x] scheme:drm/card0 daemon compiles and registers scheme
- [x] KMS ioctl dispatch handles all 15 DRM ioctls
- [x] GEM buffer lifecycle: create/mmap/close with ownership tracking
- [x] FB lifecycle: ADDFB/RMFB with size validation, per-fd ownership
- [x] Page flip: one outstanding per CRTC, vblank-gated retirement
- [x] Firmware: Rust cache validates blob availability at startup; C code loads via request_firmware() from scheme:firmware at runtime
- [x] GTT page tables: free-list reuse, TLB-safe error rollback
- [x] Original implementation pass received repeated review of resource lifecycle, ownership, GTT, and page-flip behavior
- [x] All 4 Rust crates build with zero errors, zero warnings
- [x] C glue files pass gcc -fsyntax-only
- [x] Build symlinks and config files in place

### Hardware Validation (requires physical AMD GPU)
- [ ] modetest -M amd shows connector info and modes
- [ ] modetest -M amd -s 0:1920x1080 sets mode and shows test pattern
- [ ] Works on real AMD hardware (RDNA2/RDNA3)

Current bounded runtime harness:
- `redbear-drm-display-check` is now the in-guest bounded display checker.
- `local/scripts/test-amd-gpu.sh` now wraps the shared `local/scripts/test-drm-display-runtime.sh` harness.
- The checker now proves connector/mode enumeration directly and can perform a bounded direct modeset proof.
- A successful harness run is still display-only evidence, not render proof.

## Key Files

| File | Purpose |
|------|---------|
| local/recipes/gpu/redox-drm/ | DRM scheme daemon |
| local/recipes/gpu/amdgpu/ | Build recipe + integration glue |
| local/recipes/gpu/amdgpu/source/ | AMD driver C port source (from Linux 7.0-rc7) |
| config/redbear-desktop.toml | Build config |
| local/config/pcid.d/amd_gpu.toml | PCI auto-detection (AMD) |
| local/recipes/gpu/redox-drm/source/src/drivers/interrupt.rs | MSI-X/legacy interrupt abstraction |
| local/config/pcid.d/intel_gpu.toml | Intel GPU PCI auto-detection |
| local/patches/base/P0-pcid-config-endpoint.patch | pcid /config file endpoint |
| local/scripts/build-redbear.sh | Canonical build wrapper |
| local/scripts/test-amd-gpu.sh | Test script |

## Dependencies (P1)

| Crate | Status | Provides |
|-------|--------|----------|
| redox-driver-sys | ✅ | MmioRegion, PciDevice, IrqHandle, DmaBuffer |
| linux-kpi | ✅ | C headers, FFI stubs (kmalloc, mutex, spinlock...) |
| firmware-loader | ✅ | scheme:firmware daemon |

## P1/P2 Changes Since Initial Implementation

### pcid /config endpoint (T1)
- Added `Config { addr: PciAddress }` handle to pcid scheme
- Routes `/scheme/pci/{addr}/config` to raw PCI config space read/write
- Enables redox-driver-sys PciDevice to access config space for MSI-X, BAR parsing

### MSI-X interrupt support (T2-T4)
- Created shared `InterruptHandle` enum in `redox-drm/src/drivers/interrupt.rs`
- Tries MSI-X first (find capability → parse → map table → mask_all → enable → request_vector)
- Falls back through MSI and then legacy IRQ, with `NO_MSIX`, `NO_MSI`, and `FORCE_LEGACY_IRQ` quirk gates applied before transport selection
- Both AMD and Intel drivers use `InterruptHandle::setup()`

### Dynamic PCI device info (T6)
- Replaced hardcoded `redox_pci_find_amd_gpu()` stub with `redox_pci_set_device_info()`
- Rust side passes real PciDeviceInfo (vendor, device, revision, IRQ, BAR0/BAR2) to C via FFI
- C layer validates the struct is populated before `amdgpu_redox_init()` uses it

### linux-kpi quirk consumption (current)
- `redox-drm` now also passes the real PCI BDF into the amdgpu C glue so linux-kpi quirk lookups resolve against the actual GPU, not a guessed location
- `amdgpu_redox_main.c` now calls `pci_get_quirk_flags()` / `pci_has_quirk()` in the live Redox init path
- firmware gating now stays at the Rust-side GPU driver boundary, while the AMD C backend logs quirk-driven IRQ expectations (`NO_MSI`, `NO_MSIX`, `FORCE_LEGACY`) via linux-kpi lookups on the real GPU BDF

### Intel GPU support (T4-T5)
- Intel driver switched to shared `InterruptHandle` (MSI-X / MSI / legacy with quirk-aware fallback)
- Added `local/config/pcid.d/intel_gpu.toml` for auto-detection (vendor 0x8086, class 0x03)
