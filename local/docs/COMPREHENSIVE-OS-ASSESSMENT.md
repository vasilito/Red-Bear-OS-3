# Red Bear OS — Comprehensive Desktop Readiness Assessment and Improvement Plan

**Date:** 2026-04-30
**Scope:** Full desktop OS readiness: microkernel, devices, DRM, Wayland, KDE
**Status:** This document is the single source of truth, superseding all earlier individual plans.

## 1. Executive Summary

Red Bear OS has meaningful build-side progress across all major subsystems. The current state is:

| Subsystem | Status | Confidence |
|-----------|--------|------------|
| Kernel (35 syscalls, 12 schemes) | 🟢 Boot-capable | High |
| ACPI boot baseline | 🟢 Complete | High |
| IRQ/LAPIC/x2APIC | 🟢 Kernel IRQ active | High |
| PCI/MSI-X | 🟡 QEMU-proven, no hardware | Medium |
| IOMMU | 🟡 QEMU-proven | Medium |
| USB (xHCI/hub/HID/storage) | 🟡 QEMU-only | Medium |
| Storage/Network drivers | 🟢 Hardened | High |
| Audio/Input drivers | 🟡 Hardened, untested | Medium |
| Wi-Fi | 🔴 Host-tested, no hardware | Low |
| Bluetooth | 🔴 Experimental BLE-only | Low |
| **relibc (POSIX)** | 🟢 ~38 active patches, ~85% coverage | High |
| **DRM stack** | 🟡 Builds, swrast-only | Medium |
| **Qt6/KF6** | 🟡 32/32 KF6 builds, QtNetwork re-enabled | Medium |
| **Wayland** | 🟡 Libs built, compositor incomplete | Low |
| **KDE Plasma** | 🔴 Blocked (kwin stub, no full session) | Very Low |

### Bottom Line

**The OS boots, but a graphical KDE Plasma desktop session is not yet functional.** The blocker chain: ACPI shutdown robustness → hardware validation → Wayland compositor runtime → KWin → full Plasma session.

### Previously Critical Blocker — RESOLVED (2026-04-30)

**Credential syscalls** (`setgroups`, `getgroups`, `setresuid`, `setresgid`) are now implemented via the kernel proc scheme (`auth-{fd}-groups` path). `getrlimit`/`setrlimit` return userspace defaults. See `local/docs/KERNEL-IPC-CREDENTIAL-PLAN.md` for the full implementation detail. Kernel changes: `Context.groups`, `CallerCtx.groups`, Groups proc scheme handle. Relibc changes: `posix_setgroups()`/`posix_getgroups()`, real `setgroups()` impl, RLIMIT stubs. Durable patches: `local/patches/kernel/P4-supplementary-groups.patch`, `local/patches/relibc/P4-setgroups-getgroups.patch`.

---

## 2. Kernel & Core Infrastructure

### 2.1 Syscall Coverage: ~35 handled, catch-all ENOSYS

The kernel handles 35 syscalls explicitly. All others fall through to `ENOSYS`.

**Genuinely missing for desktop:**
- ~~`SYS_SETUID`, `SYS_SETGID`, `SYS_SETGROUPS`, `SYS_GETGROUPS` — credential syscalls~~ ✅ RESOLVED (2026-04-30): implemented via proc scheme `auth-{fd}-groups` path
- ~~`SYS_GETRLIMIT`, `SYS_SETRLIMIT` — resource limits~~ ✅ RESOLVED (2026-04-30): userspace stubs with reasonable defaults
- `SYS_CLOCK_SETTIME` — set system clock, ENOSYS
- `SYS_PTRACE` — debugging, handled via scheme paths

### 2.2 ACPI: Boot-complete, not release-grade

| Working | Not Working |
|---------|------------|
| RSDP/SDT, MADT, APIC/x2APIC | `acpid` startup has panic-grade `expect` paths |
| FADT shutdown via `kstop` | `_S5` derivation gated on PCI timing |
| `power_snapshot()` with AML-backed enumeration | DMAR orphaned in `acpid` source |
| EC byte-transaction access | Sleep-state beyond S5 incomplete |

### 2.3 Drivers: All hardened, no hardware validation

All 24 driver categories have been hardened (panic→error conversion). **Zero drivers have real hardware validation.** All testing is QEMU-only.

### 2.4 USB: QEMU-validated xHCI

- xHCI: 88 Red Bear patches, interrupt-driven, QEMU-only
- hub: Good quality, interrupt-driven change detection  
- HID: Named `InputProducer` with legacy fallback
- Storage: `ReadCapacity16` with SCSI error handling
- **Missing**: Real hardware, EHCI/UHCI/OHCI runtime paths

### 2.5 Wi-Fi: Host-tested transport, no real hardware

- Intel PCIe transport builds, 119 tests pass
- LinuxKPI compat with 17 modules, 93 tests
- `redbear-wifictl` daemon + scheme interface
- **Blocked**: No Intel hardware available; current host has MediaTek MT7921K

### 2.6 Bluetooth: Experimental BLE-first

- Controller probe via USB, HCI init, `scheme:hciN` with full SchemeSync
- GATT client workflow (discover→read), 209 tests
- QEMU validation in progress, not stabilized

---

## 3. Desktop Stack

### 3.1 DRM/Mesa: Builds, software-rendering only

| Component | Status |
|-----------|--------|
| redox-driver-sys | ✅ Builds |
| linux-kpi | ✅ Builds |
| redox-drm | ✅ Builds (68 unit tests) |
| mesa | ✅ Builds EGL/GBM/OSMesa, **swrast only** (`-Dgallium-drivers=swrast`) |
| amdgpu | ✅ Builds + included in redbear-full |
| firmware-loader | ✅ Builds |
| iommu daemon | ✅ Builds |

**Hard blockers**: GPU command submission, fence/completion signaling, Mesa hardware winsys enablement. These require GPU-architecture-specific engineering.

### 3.2 Wayland: Libraries built, compositor incomplete

| Component | Status |
|-----------|--------|
| libwayland | ✅ Builds |
| wayland-protocols | ✅ Builds |
| smallvil (reference) | ✅ Bounded validation proof |
| cosmic-comp | Historical only |
| Full compositor runtime | ❌ Not present |

### 3.3 Qt6/KF6: 32 frameworks built, QtNetwork re-enabled

| Component | Status |
|-----------|--------|
| qtbase (Core+Gui+DBus+Wayland) | ✅ Builds |
| QtNetwork | ✅ Re-enabled (was disabled) — DNS resolver hardened |
| qtdeclarative, qtsvg, qtwayland | ✅ Builds |
| KF6 Frameworks | ✅ 32/32 built |
| kirigami | ⚠️ Stub-only |
| kf6-knewstuff | 🔄 Unblocked by QtNetwork — needs rebuild |
| kf6-kio | 🔄 Source-local QtNetwork compat headers |

### 3.4 KDE Plasma: Not booting

| Component | Status |
|-----------|--------|
| kwin | 🔄 Building (stub → real transition) |
| plasma-workspace | ❌ Blocked by kf6-knewstuff |
| plasma-desktop | ❌ Blocked by plasma-workspace |
| Full Plasma session | ❌ Not functional |

---

## 4. Implementation Action Plan

### Phase 1: Foundation Hardening (Weeks 1-3)

| # | Action | Impact |
|---|--------|--------|
| 1.1 | Fix `acpid` startup panic paths | Remove expect-based crash risks |
| 1.2 | Document AML bootstrap producer contract | Enable safe AML-free fallback |
| 1.3 | Add device driver hardware validation harness | USB, storage, network |
| 1.4 | Complete Qt6 rebuild with QtNetwork enabled | Unblock kf6-knewstuff |

### Phase 2: Core Stack Completion (Weeks 4-8)

| # | Action | Impact |
|---|--------|--------|
| 2.1 | Build KWin as real (not stub) compositor | Wayland compositor runtime |
| 2.2 | Complete Wayland compositor integration | graphical session proof |
| 2.3 | Wire KDE Plasma session components | plasma-workspace, plasma-desktop |
| 2.4 | Hardware USB validation | Real xHCI controller testing |

### Phase 3: Hardware Enablement (Weeks 9-16)

| # | Action | Impact |
|---|--------|--------|
| 3.1 | Wi-Fi real hardware validation | Intel iwlwifi proof |
| 3.2 | Bluetooth real hardware validation | USB-attached controller proof |
| 3.3 | IOMMU real hardware validation | AMD-Vi/Intel VT-d proof |
| 3.4 | ACPI sleep-state support | S3/S4 suspend/resume |

### Phase 4: Desktop Polish (Weeks 12-20)

| # | Action | Impact |
|---|--------|--------|
| 4.1 | GPU hardware rendering (if feasible) | Mesa radeonsi/intel drivers |
| 4.2 | Full KDE Plasma session runtime | Booting into graphical desktop |
| 4.3 | Desktop Wi-Fi API (D-Bus) | NetworkManager-like surface |
| 4.4 | Bluetooth desktop integration | HID, audio, file transfer |

### Kernel Blocker — RESOLVED (2026-04-30)

| # | Action | Impact | Status |
|---|--------|--------|--------|
| K1 | Engage Redox upstream for credential syscall additions in `redox_syscall` | `SYS_SETUID`, `SYS_SETGID`, `SYS_SETGROUPS` | ✅ Done via proc scheme (no crate changes needed) |
| K2 | Add kernel handler for credential syscalls | Remove ENOSYS catch-all gap | ✅ `auth-{fd}-groups` proc scheme path |
| K3 | Add RLIMIT syscalls or formally design them out | Resource limit support | ✅ Userspace stubs with defaults |

**Remaining kernel gaps:** `clock_settime`, ACPI shutdown robustness, hardware validation.

---

## 5. Documentation

### Stale Docs Deleted (this pass)

| File | Reason |
|------|--------|
| `local/docs/AMDGPU-DC-COMPILE-TRIAGE-PLAN.md` | Superseded by DRM-MODERNIZATION-EXECUTION-PLAN.md; amdgpu now builds |

### Authority Chain

| Document | Role |
|----------|------|
| `local/docs/RELIBC-COMPREHENSIVE-ASSESSMENT.md` | Canonical relibc |
| `local/docs/DRM-MODERNIZATION-EXECUTION-PLAN.md` | Canonical GPU/DRM |
| `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` | Canonical desktop path |
| `local/docs/DESKTOP-STACK-CURRENT-STATUS.md` | Current build/runtime truth |
| **This document** | **Canonical full-OS assessment** |
