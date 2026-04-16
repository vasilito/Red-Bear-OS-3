# 02 — Gap Analysis & Roadmap

## Overview

This document maps the distance between current Redox OS 0.9.0 and three goals:
1. **Wayland compositor support** → see [03-WAYLAND-ON-REDOX.md](03-WAYLAND-ON-REDOX.md)
2. **KDE Plasma desktop** → see [05-KDE-PLASMA-ON-REDOX.md](05-KDE-PLASMA-ON-REDOX.md)
3. **Linux driver compatibility layer** → see [04-LINUX-DRIVER-COMPAT.md](04-LINUX-DRIVER-COMPAT.md)

## Status Correction (2026-04-14)

Most of this document is a historical roadmap and no longer reflects the repository's current state.
Use the matrix below as the authoritative phase summary before reading the older milestone text.

| Layer / Phase | Current repo state | Evidence |
|---|---|---|
| P0 ACPI / bare-metal boot | Complete in-tree | `local/docs/ACPI-FIXES.md`, `local/patches/kernel/redox.patch`, `local/patches/base/redox.patch` |
| P1 driver infrastructure | Complete in-tree, compile-oriented | `local/recipes/drivers/redox-driver-sys/`, `local/recipes/drivers/linux-kpi/`, `local/recipes/system/firmware-loader/` |
| P2 DRM / AMD+Intel display | Complete in-tree, hardware validation pending | `local/docs/P2-AMD-GPU-DISPLAY.md`, `local/recipes/gpu/redox-drm/`, `local/recipes/gpu/amdgpu/` |
| P3 POSIX + input | Implemented in-tree; consumer-visible `signalfd`/`timerfd`/`eventfd`/`open_memstream` header-export path fixed in this repo pass; runtime validation still pending | `recipes/core/relibc/source/src/header/`, `recipes/core/relibc/source/include/sys/signalfd.h`, `local/patches/relibc/`, `local/recipes/system/evdevd/`, `local/recipes/system/udev-shim/` |
| P4 Wayland stack | Partially complete | `recipes/wip/wayland/`, `recipes/wip/libs/other/libinput/`, `recipes/wip/services/seatd/` |
| P5 AMD acceleration / IOMMU | Partial, but no longer blocked on basic QEMU first-use proof | `local/recipes/gpu/amdgpu/`, `local/recipes/system/iommu/` |
| P6 KDE Plasma | In progress with mixed real builds and stubs/shims | `config/redbear-kde.toml`, `local/recipes/kde/`, `local/docs/QT6-PORT-STATUS.md` |

### Ordered Remaining Gaps

1. **Validate the completed P3→P4 bridge in practice**: `libwayland` now rebuilds with `signalfd`, `timerfd`, `eventfd`, `open_memstream`, `MSG_CMSG_CLOEXEC`, and `MSG_NOSIGNAL` restored, but compositor/runtime validation is still outstanding.
2. **Complete P4 runtime path**: libinput/seatd/GBM/Wayland compositor integration is still incomplete even though the base libraries now build, `seatd` now builds for Redox, and the KDE runtime config now starts a seatd service.
3. **Separate KDE real builds from scaffolding**: parts of the KDE stack are genuine builds, while others are shimmed or stubbed only to satisfy dependency resolution.
4. **Hardware validation remains open** for AMD/Intel DRM and the IOMMU path, even though the IOMMU daemon now builds and its guest-driven QEMU first-use proof passes.

### P7 Note

The repository's tracked phase model currently stops at **P6**. What a user might call "P7"
only appears here as later milestone-style work (for example M7/M8 below), not as a first-class
implemented phase with its own config/recipe/doc boundary.

## Dependency Chain: Hardware → KDE Desktop

```
┌─────────────────────────────────────────────────────────┐
│                    KDE Plasma Desktop                     │
│  (KWin compositor, Plasma Shell, Qt, KDE Frameworks)    │
├─────────────────────────────────────────────────────────┤
│                    Wayland Protocol                       │
│  (libwayland, wayland-protocols, compositor)             │
├─────────────────────────────────────────────────────────┤
│                    Graphics Stack                         │
│  (Mesa3D OpenGL/Vulkan, GBM, libdrm, GPU driver)        │
├─────────────────────────────────────────────────────────┤
│                    Kernel Interfaces                      │
│  (DRM/KMS, GEM/TTM, DMA-BUF, evdev, udev)              │
├─────────────────────────────────────────────────────────┤
│                    Hardware                               │
│  (GPU: AMD/Intel/NVIDIA, Input: keyboard/mouse/touch)   │
└─────────────────────────────────────────────────────────┘
```

## Gap Matrix with Concrete File References

### Layer 1: POSIX Interfaces (relibc)

| API | Status | Where to implement | Effort |
|-----|--------|--------------------|--------|
| `signalfd`/`signalfd4` | **Implemented in-tree** | `relibc/src/header/signal/mod.rs` + `signal/signalfd.rs` | Runtime validation still needed |
| `timerfd_create/settime/gettime` | **Implemented in-tree** | `relibc/src/header/sys_timerfd/` | Runtime validation still needed |
| `eventfd`/`eventfd_read`/`eventfd_write` | **Implemented in-tree** | `relibc/src/header/sys_eventfd/` | Runtime validation still needed |
| `F_DUPFD_CLOEXEC` | **Implemented in-tree** | `relibc/src/header/fcntl/mod.rs` | Verify against downstream consumers |
| `MSG_CMSG_CLOEXEC` | **Implemented in-tree** | `relibc/src/header/sys_socket/mod.rs` | Verify against downstream consumers |
| `MSG_NOSIGNAL` | **Implemented in-tree** | `relibc/src/header/sys_socket/mod.rs` | Verify against downstream consumers |
| `open_memstream` | **Implemented in-tree** | `relibc/src/header/stdio/open_memstream.rs` | Verify against downstream consumers |
| UDS + FD passing | **Done** | Already implemented | — |
| `epoll` (event scheme) | **Done** | Redox `scheme:event` | — |
| `mmap`/`mprotect` | **Done** | Kernel syscalls | — |
| `fork`/`exec` | **Done** | Userspace via `thisproc:` scheme | — |

**Current blocker**: The build-side relibc/libwayland bridge is now in place, but downstream Wayland still needs runtime validation and the wider compositor stack (`evdevd`/`seatd`/DRM/GBM) is still incomplete.

### Layer 2: GPU / Display Infrastructure

| Component | Status | Where to implement | Concrete doc |
|-----------|--------|--------------------|-------------|
| DRM/KMS scheme | **Present in-tree** | `local/recipes/gpu/redox-drm/` | [04 §3](04-LINUX-DRIVER-COMPAT.md) |
| GPU driver (Intel) | Experimental modeset only | `redox-drm/src/drivers/intel/` | [04 §3](04-LINUX-DRIVER-COMPAT.md) |
| GEM buffers | **Present in-tree** | `local/recipes/gpu/redox-drm/source/src/gem.rs` | [04 §3](04-LINUX-DRIVER-COMPAT.md) |
| DMA-BUF sharing | **Present in-tree** | `local/recipes/gpu/redox-drm/source/src/dmabuf.rs` | [04 §3](04-LINUX-DRIVER-COMPAT.md) |
| Mesa hardware backend | **Missing** | Mesa winsys for Redox DRM | [03 §3.4](03-WAYLAND-ON-REDOX.md) |
| GPU OpenGL | Software only | Blocked on GPU driver | [04](04-LINUX-DRIVER-COMPAT.md) |

### Layer 3: Input Stack

> **Interpretation note:** paths under `recipes/wip/` in the matrix below should be read as upstream
> WIP inputs or historical references, not automatically as the current Red Bear shipping source of
> truth. Under the Red Bear WIP policy, upstream WIP may still be mirrored, fixed, and shipped from
> the local overlay instead.

| Component | Status | Where to implement | Concrete doc |
|-----------|--------|--------------------|-------------|
| evdev daemon | **Present in-tree** | `local/recipes/system/evdevd/` | [03 §2](03-WAYLAND-ON-REDOX.md) |
| udev shim | **Present in-tree** | `local/recipes/system/udev-shim/` | [03 §2](03-WAYLAND-ON-REDOX.md) |
| libinput | **Present as WIP port** | `recipes/wip/libs/other/libinput/` | [03 §2](03-WAYLAND-ON-REDOX.md) |
| XKB layouts | **Done** | `xkeyboard-config` ported | — |
| seatd | Builds and is wired into KDE config, runtime unvalidated | `recipes/wip/services/seatd/`, `config/redbear-kde.toml` | — |

### Layer 4: Wayland Protocol

> **Interpretation note:** the `recipes/wip/wayland/*` paths below are still useful references for
> upstream status, but Red Bear should not treat them as automatically preferred shipping sources
> while they remain upstream WIP.

| Component | Status | Recipe | Blocker |
|-----------|--------|--------|---------|
| libwayland | Patched, downstream compatibility workarounds remain | `recipes/wip/wayland/libwayland/` | Reduce/remove `redox.patch` and verify runtime behavior |
| cosmic-comp | No keyboard input | `recipes/wip/wayland/cosmic-comp/` | Layer 3 libinput |
| smallvil (Smithay) | Basic, slow | `recipes/wip/wayland/smallvil/` | Layer 2+3 for DRM+input |
| wlroots/sway/hyprland | Not tested | `recipes/wip/wayland/wlroots/` | Layer 2+3 |

### Layer 5: KDE Plasma

| Component | Status | Concrete doc |
|-----------|--------|-------------|
| Qt 6 | Ported in-tree | [05 Phase KDE-A](05-KDE-PLASMA-ON-REDOX.md) |
| KDE Frameworks | Partially ported in-tree | [05 Phase KDE-B](05-KDE-PLASMA-ON-REDOX.md) |
| KWin | Recipe exists, still incomplete | [05 Phase KDE-C](05-KDE-PLASMA-ON-REDOX.md) |
| Plasma Shell | Recipe exists, still incomplete | [05 Phase KDE-C](05-KDE-PLASMA-ON-REDOX.md) |
| D-Bus | **Ported** | `config/x11.toml` has it working |

### Layer 6: Linux Driver Compatibility

| Component | Status | Concrete doc |
|-----------|--------|-------------|
| `redox-driver-sys` crate | Present in-tree | [04 §3](04-LINUX-DRIVER-COMPAT.md) |
| `linux-kpi` C headers | Present in-tree | [04 §3](04-LINUX-DRIVER-COMPAT.md) |
| i915 C driver port | Not started as Linux C port | [04 §4](04-LINUX-DRIVER-COMPAT.md) |
| amdgpu C driver port | Present in-tree, hardware validation pending | [04 §5](04-LINUX-DRIVER-COMPAT.md) |

---

## Concrete Roadmap with Milestones

### Milestone M1: "libwayland works natively" (2-4 weeks)
- Build-side part now substantially complete: relibc exports the needed consumer-visible POSIX headers/symbols and `libwayland` rebuilds with only residual Redox-specific build tweaks
- Remaining work: runtime validation (`wayland-rs_simple_window`, compositor bring-up)
- **Test**: `wayland-rs_simple_window` runs without crashes
- **Delivers**: libwayland, wayland-protocols, libdrm all build natively

### Milestone M2: "Input works via libinput" (4-6 weeks after M1)
- Build `evdevd` daemon (reads Redox input schemes, exposes /dev/input/eventX)
- Build `udev-shim` for hotplug
- Port libinput with evdev backend
- **Test**: `libinput list-devices` shows keyboard and mouse
- **Delivers**: Full input stack for any Wayland compositor

### Milestone M3: "Display output via DRM" (8-12 weeks, parallel with M2)
- Build `redox-driver-sys` crate
- Build `redox-drm` daemon with Intel native driver
- Register `scheme:drm/card0`
- **Test**: `modetest -M intel` shows display modes
- **Delivers**: KMS modesetting, hardware display control

### Milestone M4: "Wayland compositor with input + display" (2-4 weeks after M2+M3)
- Add Redox backends to Smithay (input + DRM + EGL)
- Build `smallvil` with Redox backends
- **Test**: Compositor takes over display, keyboard/mouse work
- **Delivers**: First fully functional Wayland compositor on Redox

### Milestone M5: "Qt application runs" (6-8 weeks after M4)
- Port `qtbase` with Wayland QPA
- Port `qtwayland`, `qtdeclarative`
- **Test**: Qt widget app shows window on compositor
- **Delivers**: Qt development on Redox

### Milestone M6: "KDE app runs" (6-8 weeks after M5)
- Port KDE Frameworks (25 frameworks)
- Port one KDE app (e.g., Kate)
- **Test**: Kate editor opens and edits a file
- **Delivers**: KDE application ecosystem begins

### Milestone M7: "KDE Plasma desktop" (4-6 weeks after M6)
- Port KWin (DRM/Wayland backend)
- Port Plasma Shell
- Create `config/kde.toml`
- **Test**: Full Plasma session boots
- **Delivers**: KDE Plasma as a usable desktop

### Milestone M8: "Linux GPU drivers" (8-12 weeks, parallel track from M3)
- Build `linux-kpi` C headers
- Port i915 as proof of concept
- Port amdgpu for AMD support
- **Test**: amdgpu drives AMD GPU on Redox
- **Delivers**: Broad GPU hardware support via Linux driver ports

---

## Parallel Execution Plan

```
Week 1-4:     M1 (relibc POSIX gaps)
Week 3-12:    M2 (evdev input) ──── parallel ──── M3 (DRM/KMS)
Week 13-16:   M4 (Wayland compositor = M2 + M3 + M1)
Week 13-24:   M8 (Linux driver compat, parallel with M4-M6)
Week 17-24:   M5 (Qt Foundation)
Week 25-32:   M6 (KDE Frameworks)
Week 33-38:   M7 (Plasma Desktop)
```

**Total to KDE Plasma**: ~38 weeks (~9 months) with 2 developers.
**Total to Linux driver compat**: ~24 weeks (~6 months) in parallel.

## Critical Path

```
M1 (POSIX) ──────────────────────────────────────┐
                                                   │
M3 (DRM/KMS) ─────────── M4 (Compositor) ── M5 (Qt) ── M6 (KDE) ── M7 (Plasma)
       │                  ↑                      │
M2 (Input) ──────────────┘                       M8 (Linux drivers, parallel)
```

**Shortest path to a desktop**: M1 → M2 → M3 (parallel) → M4 → M5 → M6 → M7
**Shortest path to GPU drivers**: M3 → M8 (can start as soon as `redox-driver-sys` exists)
