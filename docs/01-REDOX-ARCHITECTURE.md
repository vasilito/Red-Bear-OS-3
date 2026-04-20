# 01 — Redox OS Architecture Overview

> **Status note (2026-04-15):** This file is primarily an architecture reference, not the canonical
> current-state status document for Red Bear OS. Use `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md`
> and the current subsystem plans under `local/docs/` for project execution order and current
> implementation truth.

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
- fork/exec goes to userspace via `thisproc:` scheme
- signal handling goes to userspace with a kernel-shared page for low-cost `sigprocmask`
- process manager is planned as a userspace daemon

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

**Network**: e1000d (Intel GigE), rtl8168d (Realtek 8168/8169/8125 path), ixgbed (Intel 10G), virtio-netd (VirtIO)

The native wired stack in this tree is userspace end to end: `pcid-spawner` autoloads NIC daemons,
drivers expose `network.*` schemes through `driver-network`, `smolnetd` provides the `ip`/`tcp`/
`udp`/`icmp`/`netcfg` schemes, and `dhcpd` plus config files under `/etc/net/` supply runtime
addressing. Red Bear additionally ships a small native `netctl` compatibility command for profile-
driven wired setup.

**Audio**: ac97d, ihdad (Intel HD Audio), sb16d (Sound Blaster)

**Display**: vesad (VESA framebuffer), virtio-gpud (VirtIO 2D)

**Other**: pcid (PCI enumeration), acpid (ACPI / AML daemon: FADT parsing, shutdown/reboot,
`kstop` eventing, and provisional `/scheme/acpi/power` exposure; known gaps remain around the
userspace `RSDP_ADDR` handoff contract, PCI-gated AML readiness, and broader shutdown robustness),
usbhidd (USB HID), inputd (input multiplexor)

### GPU Driver Status

- Broad hardware-validated GPU acceleration is not yet available as a general support claim.
- BIOS VESA and UEFI GOP framebuffers remain the default proven display path.
- Experimental Intel modesetting exists.
- Red Bear also carries compile/integration-oriented AMD and Intel DRM work in its local overlay,
  but that should not be read as broad hardware-validated acceleration support yet.

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

These were the specific missing features originally identified from libwayland's `redox.patch`.
Today, most are provided by the active relibc recipe patch chain rather than by plain upstream-only
source convergence, and downstream Wayland consumers still carry compatibility patches, so this
table is a bounded current-state summary rather than an untouched historical claim.

| Missing API | Used By | Status |
|-------------|---------|--------|
| `signalfd` / `SFD_CLOEXEC` | libwayland event loop | Active relibc recipe-applied surface; downstream libwayland still patched around usage |
| `timerfd` / `TFD_CLOEXEC` / `TFD_TIMER_ABSTIME` | libwayland timers | Active relibc recipe-applied surface; downstream libwayland still patched around usage |
| `eventfd` / `EFD_CLOEXEC` | libwayland server | Active relibc recipe-applied surface; downstream libwayland still patched around usage |
| `F_DUPFD_CLOEXEC` | libwayland fd management | Active relibc recipe-applied surface |
| `MSG_CMSG_CLOEXEC` | libwayland socket recv | Active relibc recipe-applied surface |
| `MSG_NOSIGNAL` | libwayland connection | Active relibc recipe-applied surface; downstream libwayland still omits flag |
| `open_memstream` | libdrm, libwayland | Active relibc recipe-applied surface; downstream libwayland still bypasses usage |

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

Each config selects packages and overrides init scripts. The tracked Red Bear desktop direction now
centers on the KWin Wayland target, while bounded validation configs remain separate from that
forward desktop path.

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

### Compatibility and Validation Surfaces

- legacy compatibility configs remain in-tree as references
- the tracked Red Bear desktop direction is KWin Wayland
- bounded validation configs remain separate from the forward desktop target

### Key Blockers for Wayland

1. **Downstream Wayland compatibility patches remain** (`libwayland/redox.patch` still bypasses some interfaces even though relibc-side APIs now exist in-tree)
2. **No GPU acceleration** (only software rendering)
3. **Input/runtime integration remains incomplete** (`evdevd`, `udev-shim`, and libinput exist, but compositor input is not fully validated)
4. **DRM/KMS runtime validation remains incomplete** (`redox-drm` exists in-tree, but full Wayland/driver runtime integration is still open)
5. **runtime compositor/session proof** remains incomplete
