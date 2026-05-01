# Red Bear OS: Console → Hardware-Accelerated KDE Plasma Desktop

**Version:** 4.0 (2026-04-30)
**Replaces:** v3.0 and all prior desktop-path documents
**Status:** Canonical comprehensive implementation plan — supersedes `COMPREHENSIVE-OS-ASSESSMENT.md`, `DESKTOP-STACK-CURRENT-STATUS.md`, and all layer-specific plans.

## Purpose

This is the **single authoritative plan** for Red Bear OS from console boot to a hardware-accelerated
KDE Plasma desktop on Wayland. It consolidates all layer assessments, honest blocker analysis, and
the complete implementation roadmap into one document.

It answers: **what is done, what is the current state of every layer, what are the honest blockers,
and what must happen, in what order, to reach a usable KDE Plasma desktop with hardware acceleration.**

## Executive Summary

| Subsystem | Status | Evidence Class | Blockers |
|-----------|--------|---------------|----------|
| **Kernel / Credentials** | 🟢 Complete | Source + build | — |
| **ACPI boot** | 🟢 Complete | QEMU + bare-metal proof | Shutdown robustness |
| **IRQ / PCI / MSI-X** | 🟡 QEMU-proven | Source + build + QEMU | Hardware validation |
| **relibc POSIX** | 🟢 ~85% coverage | Source + Redox-target tests | Message queues, AF_UNIX |
| **DRM / KMS** | 🟡 Builds, no HW | Source + build | GPU CS ioctl backend |
| **Mesa** | 🟡 swrast only | Build (llvmpipe) | HW renderer cross-compilation |
| **Wayland compositor** | 🟡 Bounded proof | Build + QEMU | Full compositor runtime |
| **Input / Seat** | 🟢 Working | Build + QEMU | libinput deferred |
| **Greeter / Login** | 🟢 QEMU proof | Build + QEMU proof | — |
| **D-Bus** | 🟡 System bus only | Build + partial runtime | Session bus, user lookup |
| **Qt6** | 🟢 Builds | Build (Core+Gui+DBus+Wayland) | QML JIT disabled |
| **KF6 Frameworks** | 🟡 36/48 build | Build | 12 blocked (QML gate) |
| **KDE Plasma** | 🔴 Blocked | Stub + partial builds | QML JIT, KWin real build |
| **Hardware GPU** | 🔴 Not validated | Source (CS ioctl exists) | Hardware + Mesa HW cross-compile |
| **Wi-Fi / Bluetooth** | 🔴 Host-tested | Source + host tests | Hardware + native stack |

### Bottom Line

**The OS boots to a greeter/login screen in QEMU. Software rendering works. A hardware-accelerated
KDE Plasma desktop is gated on three things: (1) Qt6Quick/QML downstream proof, (2) real KWin build,
(3) hardware GPU validation.**

---

## 1. Kernel & Core Infrastructure

### 1.1 Syscall Coverage — 35 handled, credential gaps RESOLVED

The kernel handles 35 syscalls explicitly. Remaining gaps:

| Syscall | Status |
|---------|--------|
| `setgroups`, `getgroups`, `setresuid`, `setresgid` | ✅ RESOLVED — proc scheme `auth-{fd}-groups` path |
| `getrlimit`, `setrlimit` | ✅ RESOLVED — userspace stubs with defaults |
| `clock_settime` | ❌ ENOSYS — needed for NTP |
| `ptrace` | 🟡 Handled via proc scheme paths |

### 1.2 Kernel Credential Model (2026-04-30)

- `Context.groups: Vec<u32>` — supplementary groups per-thread with process-scope propagation
- `CallerCtx.groups` — exposed to scheme handlers for access control
- Groups proc scheme handle — `auth-{fd}-groups` read/write path
- NGROUPS_MAX=65536 enforced in kernel, non-u32-aligned writes rejected
- Fork inheritance: parent groups copied to child
- Process-scope: `setgroups()` fans out to all threads sharing `owner_proc_id`

### 1.3 ACPI — Boot-complete, not release-grade

| Working | Gaps |
|---------|------|
| RSDP/SDT, MADT, APIC/x2APIC | `acpid` startup has panic-grade `expect` paths |
| FADT shutdown via `kstop` | `_S5` derivation gated on PCI timing |
| EC byte-transaction access | DMAR orphaned in `acpid` source |
| AML mutex + widened accesses | Sleep-state beyond S5 incomplete |

### 1.4 IRQ / PCI / MSI-X — QEMU-proven

- Architecturally sound: LAPIC/x2APIC, IOAPIC, MSI-X table mapping
- `redox-driver-sys`: fast PCI enumeration with capability-chain data, quirk-aware interrupt summary
- Bounded QEMU proof: MSI-X, IOMMU, xHCI IRQ
- **Blocker**: real hardware validation for all controllers

### 1.5 relibc POSIX — ~85% coverage, ~38 active patches

| Done | Deferred |
|------|----------|
| eventfd, signalfd, timerfd (recipe-applied) | POSIX message queues |
| SysV shm, sem (activated 2026-04-29) | SysV message queues |
| waitid, named semaphores | AF_UNIX sockets |
| ifaddrs (synthetic 2-entry) | Live interface enumeration |
| fcntl F_DUPFD_CLOEXEC, MSG_CMSG_CLOEXEC | |
| getentropy, secure_getenv | |

---

## 2. Hardware Enablement Stack

### 2.1 DRM / KMS

| Component | Status | Detail |
|-----------|--------|--------|
| redox-drm | 🟡 Builds | Intel Gen8-Gen12 + AMD device support; MSI-X/legacy IRQ fallback; 68 unit tests |
| libdrm | 🟡 Builds | `libdrm_amdgpu`; AMD device support |
| firmware-loader | 🟡 Builds | `scheme:firmware`; blob loading verified |
| GPU firmware | 🟡 Partial | amdgpu/i915 blobs via `fetch-firmware.sh` |
| virtio-gpu | 🟢 Builds | 220-line DRM/KMS backend for QEMU |
| CS ioctl | 🟡 Protocol exists | Private submit/wait ioctls; hardware backend returns unavailable |
| amdgpu | 🟡 Builds | Linux AMD DC/TTM/core imported; in `redbear-full` |

**Blocker**: GPU command submission backend implementation + hardware validation.

### 2.2 Mesa / Graphics

| Component | Status | Detail |
|-----------|--------|--------|
| mesa | 🟡 Builds | llvmpipe software renderer; EGL=on, GBM=on, GLES2=on |
| mesa virgl (QEMU 3D) | 🟢 **BUILDS** — `virtio_gpu_dri.so` in `usr/lib/dri/` | `-Dgallium-drivers=swrast,virgl` compiles and links. Fix: `-Dstatic_assert(...)=` nullifies Linux `drm.h` macro conflict with Mesa `util/macros.h`. Durable patch: `local/patches/mesa/P4-virgl-redox-disk-cache.patch`. 80MB pkgar. Hardware-accelerated 3D testable in QEMU with `-device virtio-vga-gl`. |
| radeonsi (AMD HW) | 🔴 Not built | Not cross-compiled for Redox target |
| iris (Intel HW) | 🔴 Not built | Not cross-compiled for Redox target |
| OSMesa | 🟢 Builds | Off-screen software rendering |

**virgl path**: Mesa `-Dgallium-drivers=swrast,virgl` compilation reaches 932/1104 objects.
Remaining work: (1) fix `virgl_screen.c` int-conversion warnings-as-errors on Redox target,
(2) provide `bits/safamily-t.h` or disable vtest winsys,
(3) integrate virgl drm winsys with redox-drm CS ioctl backend.

**Blocker**: Mesa hardware renderer cross-compilation requires CS ioctl backend + validation hardware.

### 2.3 Hardware GPU — The Big Gap

| What exists | What's missing |
|-------------|---------------|
| CS ioctl protocol in redox-drm | Backend implementation (submit to GPU rings) |
| amdgpu kernel module imported | Fence/completion signaling |
| firmware blobs fetched | Mesa radeonsi/iris cross-compilation |
| MSI-X/IRQ wired | Real AMD/Intel GPU hardware for validation |

**Hardware GPU is the longest-lead item.** Estimated 12-20 weeks with hardware access.

---

## 3. Desktop Stack

### 3.1 Wayland / Compositor

| Component | Status | Detail |
|-----------|--------|--------|
| libwayland 1.24.0 | 🟢 Builds | Wayland protocol library |
| wayland-protocols | 🟢 Builds | Protocol XML definitions |
| redbear-compositor | 🟡 Bounded proof | 788-line Rust compositor; 3/3 tests; zero warnings |
| kwin | 🔴 Stub | Recipe downloads real source but only creates wrappers |

**Known compositor limitations**: SHM fd passing uses payload bytes (not SCM_RIGHTS), framebuffer uses private memory (not real vesad), wire encoding uses NUL-terminated strings (not padded Wayland format).

**Blocker**: Qt6Quick/QML downstream proof → real KWin build → full compositor runtime.

### 3.2 Input / Seat

| Component | Status | Detail |
|-----------|--------|--------|
| evdevd | 🟢 Builds | `scheme:evdev`; 65 unit tests; event semantics verified |
| udev-shim | 🟢 Builds | `scheme:udev`; device enumeration; 15 unit tests |
| seatd/seatd-redox | 🟢 Builds | DRM lease via redox-drm; service wired |
| libinput | 🟡 Deferred | Builds but suppressed; evdevd handles input natively |
| libevdev | 🟡 Deferred | Header build needed |

### 3.3 Greeter / Login — QEMU PROOF PASSES

| Component | Status | Detail |
|-----------|--------|--------|
| redbear-authd | 🟢 Builds | SHA-crypt/Argon2 auth; `/etc/passwd` + `/etc/shadow` |
| redbear-session-launch | 🟢 Builds | Session bootstrap (uid/gid/env/runtime-dir) |
| redbear-greeter | 🟢 Builds | greeterd + Qt6/QML UI + compositor wrapper |
| redbear-sessiond | 🟢 Builds | `org.freedesktop.login1` D-Bus broker (zbus) |
| Greeter QEMU proof | 🟢 Passes | GREETER_HELLO=ok, GREETER_VALID=ok |
| redbear-kde-session | 🟢 Builds | KDE session launcher |

### 3.4 D-Bus

| Component | Status | Detail |
|-----------|--------|--------|
| dbus 1.16.2 | 🟢 Builds | System bus wired; session bus partially |
| redbear-sessiond | 🟢 Builds | login1-compatible session broker |
| redbear-dbus-services | 🟢 Builds | `.service` files + XML policies |

**Known issue**: `dbus-daemon --system` fails user lookup for `messagebus` user in some runtime configurations.

### 3.5 Qt6 / KF6 / KDE Plasma

#### Qt6

| Component | Status |
|-----------|--------|
| qtbase 6.11.0 (Core+Gui+Widgets+DBus+Wayland) | 🟢 Builds — 7 libs + 12 plugins |
| qtdeclarative | 🟡 Builds — QML JIT disabled for Redox |
| qtwayland | 🟢 Builds — Wayland QPA plugin |
| qtsvg | 🟢 Builds |
| Qt6::Sensors | 🟡 Builds (dummy backend, 520KB pkgar) |
| QtNetwork | 🟢 Re-enabled — DNS resolver hardened |

#### KF6 Frameworks — 36 build, 12 blocked

**Building (36 packages):**
`karchive`, `kauth`, `kbookmarks`, `kcodecs`, `kcolorscheme`, `kcompletion`, `kconfig`,
`kconfigwidgets`, `kcoreaddons`, `kcrash`, `kdbusaddons`, `kdeclarative`, `kded6`,
`kglobalaccel`, `kguiaddons`, `ki18n`, `kiconthemes`, `kidletime`, `kio`, `kitemmodels`,
`kitemviews`, `kjobwidgets`, `knotifications`, `kpackage`, `kservice`, `ktextwidgets`,
`kwayland`, `kwidgetsaddons`, `kwindowsystem`, `kxmlgui`, `solid`, `sonnet`,
`kcmutils`, `attica`, `kdecoration`, `kglobalacceld`

**Blocked (12 packages):**
| Package | Reason |
|---------|--------|
| kirigami | QML JIT gate — `QQuickWindow`/`QQmlEngine` headers unavailable |
| plasma-framework | Depends on kirigami |
| plasma-workspace | Depends on kf6-knewstuff payload + real kwin |
| plasma-desktop | Transitive — depends on plasma-workspace |
| kf6-knewstuff | Empty package — cmake succeeds but core source produces no libs with QtQuick off |
| breeze | Build issues |
| kde-cli-tools | Build issues |
| kf6-prison | Source issues |
| kf6-kwallet | QML/GPG disabled; not in current enabled subset |
| kf6-purpose | Not attempted |
| kf6-frameworkintegration | Not attempted |
| kf6-krunner | Not attempted |

#### KWin / Plasma Session

| Component | Status |
|-----------|--------|
| kwin | 🔴 Blocked — real cmake build attempted with QML disabled; QML gate prevents full build. Redbear-compositor provides the kwin_wayland binary as a separate package. |
| kwin real build | 🔄 Attempted — gated on Qt6Quick/QML downstream proof |
| plasma-workspace | 🔴 Blocked |
| plasma-desktop | 🔴 Blocked (transitive) |
| Full Plasma session | 🔴 Not functional |

**The QML JIT gate**: Qt6Quick's QML engine requires a JIT compiler (`QQuickWindow`, `QQmlEngine`),
which is disabled for the Redox target. Without it, kirigami (the KDE UI framework) cannot build.
kirigami blocks plasma-framework, which blocks plasma-workspace, which blocks the full Plasma desktop.
**This is the single biggest desktop blocker.**

---

## 4. Network & Wireless

### 4.1 Wired Networking — Working

- Native Redox net stack present (`pcid-spawner` → NIC daemon → `smolnetd`/`dhcpd`/`netcfg`)
- `redbear-netctl` native command shipped
- RTL8125 autoload wired through Realtek path
- VirtIO networking in QEMU: `DBUS_SYSTEM_BUS=present`

### 4.2 Wi-Fi — Host-tested, no hardware

- Intel PCIe transport builds, 119 tests
- LinuxKPI compat with 17 modules, 93 tests
- `redbear-wifictl` daemon + scheme interface
- Bounded host-tested scan/connect/disconnect/profile flows
- **Blocker**: No Intel hardware available; MediaTek MT7921K on current host

### 4.3 Bluetooth — Experimental BLE-first

- Controller probe via USB, HCI init, `scheme:hciN`
- GATT client workflow (discover→read), 209 tests
- QEMU validation in progress

---

## 5. Honest Blocker Map

### Critical Path (ordered)

```
[1] Qt6Quick/QML downstream proof     → unblocks kirigami → plasma-framework
[2] Real KWin build                     → unblocks plasma-workspace → plasma-desktop
[3] Hardware GPU validation             → unblocks Mesa HW renderers
[4] ACPI shutdown robustness            → release-grade ACPI
[5] Bare-metal validation               → unblocks all hardware claims
```

### Blocker Detail

| # | Blocker | What's needed | Estimated effort | Hardware required |
|---|---------|---------------|-----------------|-------------------|
| 1 | QML JIT gate | Qt6Quick/QML runtime proof with JIT disabled; unblocks kirigami → 12 KDE packages | 4-6 weeks | No |
| 2 | KWin real build | Real cmake build of KWin v6.3.4; requires Qt6Quick + libinput | 2-4 weeks | No |
| 3 | Plasma session | plasma-workspace + plasma-desktop cmake builds; requires kirigami + kwin | 2-4 weeks | No |
| 4 | HW GPU backend | CS ioctl implementation → Mesa HW renderer cross-compile | 12-20 weeks | Yes — AMD/Intel GPU |
| 5 | ACPI shutdown | Remove panic paths, deterministic `_S5` | 2-4 weeks | No |
| 6 | Bare-metal proof | Real AMD/Intel hardware validation for all layers | 4-8 weeks | Yes — AMD + Intel machines |

### Path to Software-Rendered KDE Plasma (Blocks 1-3)

```
Qt6Quick proof (4-6w) → KWin real build (2-4w) → Plasma session (2-4w)
                                                          ↓
                                              Software-rendered KDE Plasma on Wayland
                                              Total: 8-14 weeks
```

### Path to Hardware-Accelerated KDE Plasma (Blocks 1-6)

```
Software-rendered path (8-14w)
    + GPU CS ioctl backend + Mesa HW cross-compile (12-20w, parallel)
    + Hardware validation (4-8w, parallel)
                                                          ↓
                                              Hardware-accelerated KDE Plasma on Wayland
                                              Total: 20-34 weeks
```

---

## 6. What Changed Since v3.0 (2026-04-29 → 2026-04-30)

| Change | Impact |
|--------|--------|
| Credential syscalls implemented | `setgroups`/`getgroups`/`initgroups`/RLIMIT functional. Unblocks polkit, dbus, logind, sudo. |
| Kernel groups process-scoped | `setgroups()` propagates to all process threads. NGROUPS_MAX enforced. |
| `CallerCtx.groups` added | Schemes can now check supplementary group membership for access control. |
| Kernel readback for `getgroups` | Cache is repopulated from kernel after exec/crash. |
| `setrlimit` returns proper errors | EINVAL for unknown resources, EPERM for process limits. |

---

## 7. Configuration Surface

`config/redbear-full.toml` enables the desktop-capable target:
- 36 KDE packages (33 kf6-* + kdecoration + kglobalacceld + kwin); 12 blocked/ignored
- mesa + libdrm (software GPU stack, swrast only)
- qtbase + qtdeclarative + qtwayland + qtsvg + qt6-wayland-smoke
- seatd + redbear-authd + redbear-session-launch + redbear-greeter
- dbus + firmware-loader + redox-drm + evdevd + udev-shim
- redbear-compositor (real Rust Wayland compositor)
- plus inherited packages from redbear-mini profile

---

## 8. Evidence Model

| Evidence Class | What It Means |
|----------------|---------------|
| **Source** | Code exists in tree |
| **Host build-verified** | `cargo check` zero warnings on Linux |
| **Redox build-verified** | `make r.*` successful on `x86_64-unknown-redox` |
| **Runtime-validated** | Exercised in QEMU |
| **Hardware-validated** | Exercised on real AMD/Intel hardware |

**Current highest evidence bar reached**: QEMU runtime proof for greeter/login, bounded compositor,
D-Bus system bus, evdevd/udev-shim, DRM scheme enumeration.

**No component has hardware validation.** All hardware claims remain evidence-qualified.

---

## 9. Subsystem Plans (Reference)

This document is the authority. Subsystem plans remain for deep-dive detail:

| Plan | Covers |
|------|--------|
| `KERNEL-IPC-CREDENTIAL-PLAN.md` | Kernel credential syscalls, IPC, RLIMIT — Phases K1-K2,K4 complete |
| `IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md` | PCI/IRQ/MSI-X/IOMMU quality |
| `ACPI-IMPROVEMENT-PLAN.md` | ACPI shutdown, power, sleep states |
| `RELIBC-IPC-ASSESSMENT-AND-IMPROVEMENT-PLAN.md` | relibc IPC surface |
| `DRM-MODERNIZATION-EXECUTION-PLAN.md` | DRM/KMS modernization |
| `WAYLAND-IMPLEMENTATION-PLAN.md` | Wayland compositor stability |
| `DBUS-INTEGRATION-PLAN.md` | D-Bus architecture |
| `GREETER-LOGIN-IMPLEMENTATION-PLAN.md` | Greeter/login design |

---

## 10. Stale Docs Deleted (This Pass)

| File | Reason |
|------|--------|
| `COMPREHENSIVE-OS-ASSESSMENT.md` | Consolidated into this document |
| `DESKTOP-STACK-CURRENT-STATUS.md` | Consolidated into this document |
| `AMD-FIRST-INTEGRATION.md` | Historical — AMD and Intel are equal-priority targets |
| `HARDWARE-3D-ASSESSMENT.md` | Historical — consolidated into §2 |
| `DMA-BUF-IMPROVEMENT-PLAN.md` | Historical — consolidated into §2 |
| `INPUT-SCHEME-ENHANCEMENT.md` | Historical — consolidated into §3.2 |
| `BOOT-PROCESS-ASSESSMENT.md` | Historical — consolidated into §1 |
| `LINUX-BORROWING-RUST-IMPLEMENTATION-PLAN.md` | Historical — consolidated into §2 |
| `QT6-PORT-STATUS.md` | Historical — consolidated into §3.5 |
| `REDBEAR-INFO-RUNTIME-REPORT.md` | Historical — validation infrastructure now standard |
| `RELIBC-COMPREHENSIVE-ASSESSMENT.md` | Historical — consolidated into §1.5 |
| `RELIBC-COMPLETENESS-AND-ENHANCEMENT-PLAN.md` | Historical — consolidated into §1.5 |
| `RELIBC-IMPLEMENTATION-PLAN.md` | Historical — consolidated into §1.5 |
