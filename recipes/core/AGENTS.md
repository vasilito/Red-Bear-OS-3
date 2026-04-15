# RECIPES/CORE — ESSENTIAL SYSTEM COMPONENTS

Kernel, bootloader, C library, init system, and base drivers. Everything needed to boot Redox.

## STRUCTURE

```
recipes/core/
├── kernel/          # Redox microkernel (~20-40k LoC Rust)
│   └── source/      # Kernel source (fetched from gitlab.redox-os.org)
├── bootloader/      # UEFI bootloader (x86_64-uefi, aarch64-uefi)
│   └── source/mk/   # Per-arch bootloader build rules
├── relibc/          # POSIX C library written in Rust
│   └── source/      # relibc source (headers, platform, syscalls)
├── base/            # Core userland + all drivers
│   └── source/      # Base repo (audiod, ipcd, ptyd, drivers, netstack, ramfs)
│       └── drivers/ # ALL drivers (userspace daemons)
│           ├── graphics/  # vesad, virtio-gpud, ihdgd (Intel experimental)
│           ├── net/       # e1000d, rtl8168d, rtl8139d, ixgbed
│           ├── storage/   # ided, ahcid, nvmed, usbscsid
│           ├── audio/     # ac97d, ihdad, sb16d
│           ├── usb/       # usbhidd (USB HID)
│           ├── virtio/    # virtio-blkd, virtio-netd, virtio-gpud
│           └── pci/       # pcid, pcid-spawner (PCI enumeration)
├── installer/       # redox_installer (creates filesystem images)
├── redoxfs/         # RedoxFS (default filesystem)
├── init/            # Init system (TOML-based service manager)
├── ion/             # Ion shell (default)
├── userutils/       # Core user management
├── uutils/          # Coreutils (Rust port)
└── netutils/        # Basic network utilities
```

## WHERE TO LOOK

| Task | Location |
|------|----------|
| Fix kernel crash | `kernel/source/src/` — syscall handling, context switching, memory mgmt |
| Add a syscall | `kernel/source/src/scheme/` — scheme registration, then `libredox` binding |
| Fix a driver | `base/source/drivers/<driver>/src/` |
| Fix POSIX compat | `relibc/source/src/header/` — add missing POSIX headers/functions |
| Add bootloader support | `bootloader/source/mk/<arch>-unknown-uefi.mk` |
| Fix PCI enumeration | `base/source/drivers/pci/pcid-spawner/` |
| Fix display output | `base/source/drivers/graphics/` — vesad, virtio-gpud |
| Fix networking | `base/source/drivers/net/` + `base/source/netstack/` |

## KERNEL SCHEME ARCHITECTURE

Kernel provides minimal schemes: `debug`, `event`, `memory`, `pipe`, `irq`, `time`, `sys`, `proc`, `serio`.
All other schemes are userspace daemons registering via `File::create(":myscheme")`.

```
Driver access pattern:
  1. iopl() syscall → port I/O privilege
  2. Open /scheme/memory/physical → mmap hardware registers
  3. Open /scheme/irq/{num} → receive interrupts as messages
  4. Register scheme → handle requests from user programs
```

## DRIVER MODEL

- ALL drivers are userspace daemons (except serio for PS/2)
- Access hardware via: `scheme:memory`, `scheme:irq`, `iopl` syscall
- Register as scheme: daemon name becomes `/scheme/<name>`
- PCI devices discovered via `pcid` daemon → spawns drivers

## HISTORICAL POSIX GAPS IN RELIBC (Wayland-facing)

| Missing API | Location to implement |
|-------------|----------------------|
| signalfd/signalfd4 | `relibc/source/src/header/signal/` — now source-visible in the current Red Bear tree |
| timerfd_create/settime/gettime | `relibc/source/src/header/sys_timerfd/` — now source-visible in the current Red Bear tree |
| eventfd | `relibc/source/src/header/sys_eventfd/` — now source-visible in the current Red Bear tree |
| F_DUPFD_CLOEXEC | `relibc/source/src/header/fcntl/` — now source-visible in the current Red Bear tree |
| MSG_CMSG_CLOEXEC, MSG_NOSIGNAL | `relibc/source/src/header/sys_socket/` — now source-visible in the current Red Bear tree |
| open_memstream | `relibc/source/src/header/stdio/` — now source-visible in the current Red Bear tree |

The current relibc work is therefore no longer just “add the missing Wayland APIs.” The higher-value
remaining work is completeness depth, downstream cleanup, and runtime validation.

## ANTI-PATTERNS

- **DO NOT** add drivers to kernel — all drivers must be userspace
- **DO NOT** modify syscall ABI — use libredox/relibc wrappers
- **DO NOT** use unwrap() in drivers — handle errors properly with Result
