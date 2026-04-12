# 01 — Redox OS Architecture Overview

## 1. Microkernel Design

Redox is a **pure microkernel** written in Rust (~20-40k LoC). Only essential services
live in kernel space:

- Process and thread management
- Memory management (address spaces, page tables, grants)
- IPC via schemes (packet-based, io_uring-like SQE/CQE format)
- Context switching and scheduling
- Minimal kernel schemes: `debug`, `event`, `memory`, `pipe`, `irq`, `time`, `sys`, `proc`, `serio`

Everything else — drivers, filesystems, display server, networking — runs in **userspace**
as separate processes with isolated address spaces. Crashes are contained; no kernel panics
from driver bugs.

### Syscall Interface

The syscall ABI is **intentionally unstable**. Stability is provided by `libredox` and `relibc`.
On x86_64, syscalls use `int 0x80` with registers:

```
eax = syscall number
ebx, ecx, edx, esi, edi = arguments
eax = return value
```

Key syscalls: `open`, `close`, `read`, `write`, `seek`, `fmap`, `funmap`, `dup`, `fork`, `execve`,
`clone`, `mmap`, `munmap`, `mprotect`, `setrens`, `yield`.

### Userspace-ification Trend

Redox is actively moving POSIX functionality out of the kernel:
- **fork/exec** → userspace via `thisproc:` scheme
- **Signal handling** → userspace with kernel-shared page for low-cost `sigprocmask`
- **Process manager** → planned userspace daemon

This reduces TCB and allows faster iteration without kernel changes.

## 2. The Scheme System — Everything is a URL

Inspired by Plan 9. Every resource is accessed through a **scheme** — a named service
providing file-like operations (`open`, `read`, `write`, `fmap`).

### How Schemes Work

```
User program:  open("/scheme/orbital:myapp/800/600/Title")
    ↓
Kernel:        Routes to "orbital" scheme daemon
    ↓
Orbital:       Creates window, returns file handle
    ↓
User program:  write(fd, pixel_data)  // renders to window
```

### Kernel Schemes

| Scheme | Purpose |
|--------|---------|
| `debug` | Debug output |
| `event` | epoll-like event notification |
| `irq` | Interrupt → message conversion |
| `pipe` | Pipe implementation |
| `memory` | Physical memory mapping |
| `time` / `itimer` | Timers |
| `proc` / `thisproc` | Process context |
| `sys` | System information |
| `serio` | PS/2 driver (kernel-space due to protocol constraints) |

### Userspace Schemes (Daemons)

| Category | Schemes | Daemon |
|----------|---------|--------|
| Storage | `disk.*` | ided, ahcid, nvmed |
| Filesystem | `file` | redoxfs |
| Network | `ip`, `tcp`, `udp`, `icmp` | smolnetd |
| Display | `display.vesa`, `display.virtio-gpu`, `orbital` | vesad, virtio-gpud, orbital |
| IPC | `chan`, `shm`, `uds_stream`, `uds_dgram` | ipcd |
| Audio | `audio` | audiorw |
| Input | `input` | inputd |
| USB | `usb.*` | usbhidd |
| Misc | `rand`, `null`, `zero`, `log`, `pty`, `sudo` | various |

### Scheme Registration

A daemon registers a scheme by:
1. `File::create(":myscheme")` — creates root scheme
2. Opens needed resources (`/scheme/irq/{irq}`, `/scheme/event`)
3. `setrens(0, 0)` — moves to null namespace (security sandbox)
4. Registers FDs with event scheme for async I/O
5. Loops: block on event → handle request → respond

### Namespace Isolation

- **Root namespace**: all processes start here
- **Null namespace**: process can only use existing FDs, cannot open new resources
- Namespaces inherited by children
- Enables sandboxing and privilege separation

## 3. Driver Model

All drivers are **userspace daemons** that access hardware through:

- **`iopl` syscall** — sets I/O privilege level for port I/O
- **`/scheme/memory/physical`** — maps physical memory (writeback, uncacheable, write-combining)
- **`/scheme/irq`** — converts hardware interrupts to messages

### Current Drivers

**Storage**: ided (IDE), ahcid (SATA), nvmed (NVMe), usbscsid (USB SCSI)

**Network**: e1000d (Intel GigE), rtl8168d (Realtek), ixgbed (Intel 10G)

**Audio**: ac97d, ihdad (Intel HD Audio), sb16d (Sound Blaster)

**Display**: vesad (VESA framebuffer), virtio-gpud (VirtIO 2D)

**Other**: pcid (PCI enumeration), acpid (ACPI), usbhidd (USB HID), inputd (input multiplexor)

### GPU Driver Status

- **No hardware-accelerated GPU drivers**
- Only BIOS VESA and UEFI GOP framebuffers
- Experimental Intel modesetting (Kaby Lake, Tiger Lake) — no acceleration
- AMD, NVIDIA, ARM, PowerVR: not supported

## 4. Orbital Display Server

Orbital is Redox's display server, window manager, and compositor — all in one userspace daemon.

### Window Creation (via Scheme)

```rust
// Open a window through the orbital scheme
let window = File::create("orbital:myapp/800/600/My Title")?;

// Read input events
let mut event = [0u8; 32];
window.read(&mut event)?;

// Write pixel data (RGBA)
window.write(&pixel_data)?;
window.sync_all()?;
```

### Supported Toolkits

- SDL1.2, SDL2 — games and emulators
- winit — Rust GUI abstraction (has Orbital backend)
- softbuffer — software rendering
- Iced, egui, Slint — via winit/softbuffer

### Graphics Stack

```
Application
    ↓ (SDL2 / winit / liborbital)
Orbital (display server + compositor)
    ↓ (scheme: display.vesa or display.virtio-gpu)
vesad / virtio-gpud (display driver daemon)
    ↓ (scheme: memory + irq)
Hardware (framebuffer / VirtIO GPU)
```

Rendering is software-only via LLVMpipe (Mesa CPU OpenGL emulation).
No GPU acceleration pipeline exists yet.

## 5. relibc — C Library

relibc is a **POSIX-compatible C library written in Rust**. Provides:
- Standard C library functions
- POSIX syscalls (section 2 + 3)
- Linux/BSD extensions

Targets: Redox (via `redox-rt`), Linux (direct syscalls).
Architectures: i586, x86_64, aarch64, riscv64gc.

### Known POSIX Gaps (blocking Wayland)

These are the specific missing features found in libwayland's `redox.patch`:

| Missing API | Used By | Status |
|-------------|---------|--------|
| `signalfd` / `SFD_CLOEXEC` | libwayland event loop | Not implemented |
| `timerfd` / `TFD_CLOEXEC` / `TFD_TIMER_ABSTIME` | libwayland timers | Not implemented |
| `eventfd` / `EFD_CLOEXEC` | libwayland server | Not implemented |
| `F_DUPFD_CLOEXEC` | libwayland fd management | Not implemented |
| `MSG_CMSG_CLOEXEC` | libwayland socket recv | Not implemented |
| `MSG_NOSIGNAL` | libwayland connection | Not implemented |
| `open_memstream` | libdrm, libwayland | Not implemented |

## 6. Build System (This Repository)

This repository is the **build system** — it orchestrates fetching, building, and packaging
components from ~100+ Git repositories into a bootable Redox image.

### Key Directories

| Directory | Purpose |
|-----------|---------|
| `config/` | Build configurations (desktop, server, wayland, x11, minimal) |
| `recipes/` | Package recipes (source + build instructions) |
| `recipes/core/` | Essential: kernel, bootloader, relibc, init |
| `recipes/gui/` | Orbital, orbterm, orbutils |
| `recipes/libs/` | Libraries: mesa, cairo, pango, SDL, etc. |
| `recipes/wip/` | Work-in-progress packages (wayland/, kde/, gnome/, etc.) |
| `mk/` | Makefile infrastructure |
| `src/` | Build system source (cookbook tool in Rust) |

### Config System

Configs are TOML files that include each other:

```
wayland.toml → desktop.toml → desktop-minimal.toml → minimal.toml → base.toml
```

Each config selects packages and overrides init scripts. For example, `wayland.toml`
overrides the orbital init to launch `cosmic-comp` instead of `orblogin`.

### Build Flow

```bash
make all
  → downloads cross-toolchain (Clang/LLVM for x86_64-unknown-redox)
  → fetches recipe sources (git/tar)
  → applies patches (redox.patch files)
  → builds each recipe (cargo, meson, cmake, make, custom)
  → stages into sysroot
  → creates RedoxFS image
  → produces harddrive.img / redox-live.iso
```

## 7. Existing Wayland/X11 Support

### X11 (Working)

Config: `config/x11.toml`
- X.org with dummy video driver inside Orbital
- GTK3, MATE desktop, Mesa EGL
- DRI3 enabled
- Software rendering only

### Wayland (Experimental, WIP)

Config: `config/wayland.toml`
- **21 Wayland recipes** in `recipes/wip/wayland/`
- **cosmic-comp**: partially working, performance issues, no keyboard input
- **smallvil** (Smithay): ported, basic compositor running
- **wlroots**: not compiled or tested
- **sway**: not compiled or tested
- **hyprland**: not compiled or tested
- **niri**: needs Smithay port
- **xwayland**: partially patched, needs wayland-client fixes

### Key Blockers for Wayland

1. **relibc POSIX gaps** (signalfd, timerfd, eventfd, open_memstream)
2. **No GPU acceleration** (only software rendering)
3. **No libinput** (requires evdev + udev)
4. **No DRM/KMS** (libdrm has all GPU drivers disabled)
5. **cosmic-comp**: missing keyboard input, performance issues
