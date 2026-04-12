# 02 — Gap Analysis & Roadmap

## Overview

This document maps the distance between current Redox OS 0.9.0 and three goals:
1. **Wayland compositor support** → see [03-WAYLAND-ON-REDOX.md](03-WAYLAND-ON-REDOX.md)
2. **KDE Plasma desktop** → see [05-KDE-PLASMA-ON-REDOX.md](05-KDE-PLASMA-ON-REDOX.md)
3. **Linux driver compatibility layer** → see [04-LINUX-DRIVER-COMPAT.md](04-LINUX-DRIVER-COMPAT.md)

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
| `signalfd`/`signalfd4` | **Missing** | `relibc/src/header/signal/mod.rs` + `signal/types.rs` | Medium |
| `timerfd_create/settime/gettime` | **Missing** | `relibc/src/header/sys_timerfd/` (NEW directory) | Medium |
| `eventfd`/`eventfd_read`/`eventfd_write` | **Missing** | `relibc/src/header/sys_eventfd/` (NEW directory) | Low |
| `F_DUPFD_CLOEXEC` | **Missing** | `relibc/src/header/fcntl/mod.rs` (add constant) | Low |
| `MSG_CMSG_CLOEXEC` | **Missing** | `relibc/src/header/sys_socket/mod.rs` | Low |
| `MSG_NOSIGNAL` | **Missing** | `relibc/src/header/sys_socket/mod.rs` | Low |
| `open_memstream` | **Missing** | `relibc/src/header/stdio/src.rs` | Low |
| UDS + FD passing | **Done** | Already implemented | — |
| `epoll` (event scheme) | **Done** | Redox `scheme:event` | — |
| `mmap`/`mprotect` | **Done** | Kernel syscalls | — |
| `fork`/`exec` | **Done** | Userspace via `thisproc:` scheme | — |

**Proof of gap**: See `recipes/wip/wayland/libwayland/redox.patch` — all 7 missing APIs are stubbed there.

### Layer 2: GPU / Display Infrastructure

| Component | Status | Where to implement | Concrete doc |
|-----------|--------|--------------------|-------------|
| DRM/KMS scheme | **Missing** | New daemon: `redox-drm` crate | [04 §3](04-LINUX-DRIVER-COMPAT.md) |
| GPU driver (Intel) | Experimental modeset only | `redox-drm/src/drivers/intel/` | [04 §3](04-LINUX-DRIVER-COMPAT.md) |
| GEM buffers | **Missing** | `redox-drm/src/gem.rs` | [04 §3](04-LINUX-DRIVER-COMPAT.md) |
| DMA-BUF sharing | **Missing** | `redox-drm/src/dmabuf.rs` | [04 §3](04-LINUX-DRIVER-COMPAT.md) |
| Mesa hardware backend | **Missing** | Mesa winsys for Redox DRM | [03 §3.4](03-WAYLAND-ON-REDOX.md) |
| GPU OpenGL | Software only | Blocked on GPU driver | [04](04-LINUX-DRIVER-COMPAT.md) |

### Layer 3: Input Stack

| Component | Status | Where to implement | Concrete doc |
|-----------|--------|--------------------|-------------|
| evdev daemon | **Missing** | New: `recipes/core/evdevd/` | [03 §2](03-WAYLAND-ON-REDOX.md) |
| udev shim | **Missing** | New: `recipes/wip/wayland/udev-shim/` | [03 §2](03-WAYLAND-ON-REDOX.md) |
| libinput | **Missing** | `recipes/wip/wayland/libinput/` (NEW) | [03 §2](03-WAYLAND-ON-REDOX.md) |
| XKB layouts | **Done** | `xkeyboard-config` ported | — |
| seatd | Recipe exists, untested | `recipes/wip/wayland/seatd/` | — |

### Layer 4: Wayland Protocol

| Component | Status | Recipe | Blocker |
|-----------|--------|--------|---------|
| libwayland | Patched, broken timers | `recipes/wip/wayland/libwayland/` | Layer 1 POSIX gaps |
| cosmic-comp | No keyboard input | `recipes/wip/wayland/cosmic-comp/` | Layer 3 libinput |
| smallvil (Smithay) | Basic, slow | `recipes/wip/wayland/smallvil/` | Layer 2+3 for DRM+input |
| wlroots/sway/hyprland | Not tested | `recipes/wip/wayland/wlroots/` | Layer 2+3 |

### Layer 5: KDE Plasma

| Component | Status | Concrete doc |
|-----------|--------|-------------|
| Qt 6 | Not ported | [05 Phase KDE-A](05-KDE-PLASMA-ON-REDOX.md) |
| KDE Frameworks | Not ported | [05 Phase KDE-B](05-KDE-PLASMA-ON-REDOX.md) |
| KWin | Not ported | [05 Phase KDE-C](05-KDE-PLASMA-ON-REDOX.md) |
| Plasma Shell | Not ported | [05 Phase KDE-C](05-KDE-PLASMA-ON-REDOX.md) |
| D-Bus | **Ported** | `config/x11.toml` has it working |

### Layer 6: Linux Driver Compatibility

| Component | Status | Concrete doc |
|-----------|--------|-------------|
| `redox-driver-sys` crate | Not started | [04 §3](04-LINUX-DRIVER-COMPAT.md) |
| `linux-kpi` C headers | Not started | [04 §3](04-LINUX-DRIVER-COMPAT.md) |
| i915 C driver port | Not started | [04 §4](04-LINUX-DRIVER-COMPAT.md) |
| amdgpu C driver port | Not started | [04 §5](04-LINUX-DRIVER-COMPAT.md) |

---

## Concrete Roadmap with Milestones

### Milestone M1: "libwayland works natively" (2-4 weeks)
- Implement 7 POSIX APIs in relibc (see Layer 1 table)
- Remove `redox.patch` from libwayland recipe
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
