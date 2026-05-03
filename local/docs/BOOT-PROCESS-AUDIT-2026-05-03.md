# Red Bear OS — Boot Process Audit & Improvement Plan

**Date**: 2026-05-03
**Scope**: Power-on → login prompt; all daemons, services, hardware initialization

## 1. Boot Sequence (Current)

```
Bootloader (UEFI)
  → kernel (microkernel, scheme-based)
  → bootstrap (kernel → userspace bridge)
  → init (TOML service manager)
    → INITFS phase:
      00_logd       — scheme:log (kernel-level logging)
      00_nulld      — /dev/null
      00_randd      — scheme:rand (entropy)
      00_rtcd       — RTC driver  
      00_zerod      — scheme:zero
      10_inputd     — scheme:input (VT/keyboard/mouse multiplexer)
      10_lived      — live disk support
      20_fbbootlogd — framebuffer boot log
      20_fbcond     — scheme:fbcon (text console on VT2)
      20_vesad      — VESA framebuffer driver
      40_hwd        — ACPI/DTB hardware manager
      40_pcid-*     — PCI driver spawner (initfs mode)
      40_ps2d       — PS/2 keyboard/mouse
      50_rootfs     — redoxfs mount (/)
    → SWITCHROOT to /usr
    → USERLAND phase:
      00_ipcd           — IPC daemon
      00_pcid-spawner   — full PCI driver spawner  
      00_ptyd           — scheme:pty
      00_sudo           — privilege escalation
      10_dhcpd          — DHCP
      10_smolnetd       — network stack
      20_audiod         — audio
      29_activate_console — VT2 activation
      30_console        — getty on VT2 → login prompt
```

## 2. Daemon-by-Daemon Assessment

### 2.1 Critical Path Daemons (P0 - boot-blocking)

| Daemon | Status | Issues |
|--------|--------|--------|
| **kernel** | Stable | Scheme-based, userspace drivers. Kernel syscall surface is fixed. |
| **bootstrap** | Stable | First userspace code, spawns init. No issues. |
| **init** | Improved | Now with colored ANSI output. Reads TOML service files. No multi-user.target support yet. |
| **logd** | Basic | scheme:log, console output only. No persistent logging, no log rotation, no structured logs. |
| **rootfs (redoxfs)** | Stable | Default filesystem. ext4/fat support exists but redoxfs is primary. |

### 2.2 Input Stack (P1)

| Daemon | Status | Issues |
|--------|--------|--------|
| **inputd** | Good | Named producers via InputProducer enum (P3). Multiplexes keyboard/mouse/graphics. |
| **ps2d** | Good | LED feedback (caps/num/scroll). InputProducer migration done. |
| **usbhidd** | Good (hardened) | HID descriptor validation (P3). Static lookup table. 8-button support. Retry with backoff. |
| **Gap** | Missing | No touchpad gesture support beyond basic mouse. No gamepad/joystick. |

### 2.3 Display Stack (P1)

| Daemon | Status | Issues |
|--------|--------|--------|
| **vesad** | Basic | VESA BIOS only. No GPU acceleration. 1280x720 default. |
| **fbcond** | Basic | Text console on framebuffer. No unicode beyond ASCII. No scrollback buffer. |
| **fbbootlogd** | Minimal | Boot log overlay. Basic. |
| **Gap** | Missing | No GPU driver active at boot (redox-drm/amdgpu not in initfs). No Wayland in initfs. |

### 2.4 Hardware Enumeration (P1)

| Daemon | Status | Issues |
|--------|--------|--------|
| **hwd** | Partial | ACPI table parsing. RSDP forwarding from bootloader. AML-backed enumeration but bootstrap contract weak. |
| **pcid-spawner** | Good | PCI device discovery + driver spawning. Works for storage, network, USB. |
| **rtcd** | Basic | RTC read only. No RTC write, no NTP sync. |
| **Gap** | Missing | No SMBIOS/DMI parsing for hardware quirks at boot. No IOMMU init. |

### 2.5 Storage Stack (P1)

| Daemon | Status | Issues |
|--------|--------|--------|
| **ahcid** | Stable | SATA AHCI driver. |
| **ided** | Stable | Legacy PATA driver. |
| **nvmed** | Stable | NVMe driver. |
| **usbscsid** | Partial | USB mass storage. Read verified. Write not validated. |

### 2.6 Network Stack (P2)

| Daemon | Status | Issues |
|--------|--------|--------|
| **smolnetd** | Basic | Minimal network stack. |
| **dhcpd** | Basic | DHCP client. |
| **e1000d/rtl8168d** | Stable | Ethernet drivers. |
| **Gap** | Missing | No WiFi (iwlwifi not active). No Bluetooth. No firewall. No DNS resolver daemon. |

### 2.7 Audio Stack (P2)

| Daemon | Status | Issues |
|--------|--------|--------|
| **audiod** | Basic | Audio multiplexer. |
| **ac97d/ihdad/sb16d** | Partial | Audio codec drivers. Intel HDA partially works. |

### 2.8 User Interface (P2)

| Binary | Status | Issues |
|--------|--------|--------|
| **getty** | Basic | Opens TTY, runs login. No PAM. Simple password check via /etc/passwd. |
| **login** | Basic | Authenticates user, spawns shell. No session management. |
| **ion** | Basic | Fast but minimal. No job control, limited scripting, no tab completion, no history search. |

### 2.9 System Services (P3)

| Service | Status | Issues |
|---------|--------|--------|
| **ipcd** | Stable | IPC channel daemon. |
| **ptyd** | Stable | Pseudo-terminal multiplexer. |
| **sudo** | Basic | Simple privilege escalation. No policy file. |
| **randd** | Stable | Entropy from kernel. |
| **zerod/nulld** | Stable | /dev/zero and /dev/null. |

## 3. Hardware Initialization Completeness

| Subsystem | Boot Stage | Completeness |
|-----------|-----------|-------------|
| CPU / x2APIC / SMP | Kernel | ✅ Multi-core works |
| Memory (paging) | Bootloader | ✅ UEFI memory map |
| ACPI / RSDP | Bootloader → hwd | 🟡 RSDP forwarded, AML partial, shutdown weak |
| PCI enumeration | pcid-spawner | ✅ Enumeration + driver spawning |
| Storage (AHCI/NVMe) | initfs drivers | ✅ Block devices available |
| USB (xHCI) | initfs drivers | 🟡 xhcid loaded, usbhidd in initfs but no USB storage in initfs |
| Display (VESA) | initfs vesad | ✅ Basic framebuffer |
| PS/2 input | initfs ps2d | ✅ Keyboard + mouse |
| USB HID | initfs usbhidd | ✅ Keyboard + mouse (hardened P3) |
| Ethernet | userland | ✅ e1000d/rtl8168d |
| WiFi | userland | ❌ Not active |
| Bluetooth | userland | ❌ Not implemented |
| Audio | userland | 🟡 Partial |
| GPU (DRM/KMS) | userland | 🟡 redox-drm compiled, not in boot path |
| IOMMU | kernel | 🟡 QEMU proof passes, HW unvalidated |
| TPM / Secure Boot | bootloader | ❌ Not implemented |

## 4. Console Shell Analysis (ion)

### Strengths
- Fast startup (Rust, no legacy cruft)
- Basic POSIX-like commands work
- Pipeline support (|)
- Redirect support (>, <, >>)

### Gaps
- No job control (fg/bg/Ctrl-Z)
- No tab completion
- No command history search (Ctrl-R)
- Limited scripting (no if/for/while in shell syntax)
- No alias support
- No environment variable editing
- No prompt customization
- No signal handling (SIGINT/SIGTERM properly passed to children)

### Comparison: ion vs bash/dash
| Feature | ion | bash | dash |
|---------|-----|------|------|
| Startup time | ~5ms | ~15ms | ~3ms |
| Job control | ❌ | ✅ | ✅ |
| Tab completion | ❌ | ✅ | ❌ |
| Scripting | Basic | Full | Full |
| History | Linear | Searchable | Linear |
| Size | ~500KB | ~1MB | ~150KB |

## 5. Stale Documentation

35 files in `local/docs/`. Many are historical plans/analyses that were written but never fully executed. Files that appear stale or superseded:

| File | Status | Recommendation |
|------|--------|----------------|
| `ACPI-I2C-HID-IMPLEMENTATION-PLAN.md` | Stale | Archive or delete |
| `AMD-FIRST-INTEGRATION.md` | Superseded | AMD/Intel now equal-priority; archive |
| `BOOT-PROCESS-IMPROVEMENT-PLAN.md` | Superseded | This document supersedes it |
| `DEVICE-INIT-COMPREHENSIVE-IMPROVEMENT-PLAN.md` | Stale | Archive |
| `GREETER-LOGIN-ANALYSIS.md` | Stale | Superseded by GREETER-LOGIN-IMPLEMENTATION-PLAN |
| `INTEL-HDA-IMPLEMENTATION-PLAN.md` | Stale | Archive |
| `HARDWARE-3D-ASSESSMENT.md` | Stale | Archive |
| `WIFI-PASSTHROUGH-VALIDATION.md` | Stale | Archive |
| `boot-logs/` | Directory | Keep recent, archive old |

## 6. Improvement Plan

### Phase A — P0: Boot Reliability (Week 1-2)

| Task | Priority | Effort |
|------|----------|--------|
| Fix ACPI shutdown robustness | Critical | 3d |
| Verify SMBIOS/DMI parsing in hwd | High | 2d |
| Add RTC write support to rtcd | Medium | 1d |
| Add persistent logging to logd (file + rotation) | High | 2d |

### Phase B — P1: Driver Completeness (Week 2-4)

| Task | Priority | Effort |
|------|----------|--------|
| Enable redox-drm in boot path (not just compile) | High | 3d |
| Add USB storage (usbscsid) to initfs drivers | High | 1d |
| Verify USB HID hotplug (xhcid re-enumeration) | Medium | 2d |
| Add IOMMU init to boot path (DMA remapping) | Medium | 3d |
| Implement thermal daemon (CPU temp monitoring) | Low | 2d |

### Phase C — P2: User Experience (Week 3-6)

| Task | Priority | Effort |
|------|----------|--------|
| Improve ion shell (tab completion, job control, history search) | High | 5d |
| Add scrollback buffer to fbcond | Medium | 2d |
| Add unicode font support to fbcond | Medium | 3d |
| Improve getty security (rate limiting, secure attention key) | Medium | 1d |
| Add network config persistence (netctl profiles) | Medium | 2d |
| Enable WiFi driver in boot path | High | 5d |

### Phase D — P3: Documentation Cleanup (Week 1)

| Task | Priority | Effort |
|------|----------|--------|
| Archive/delete 8 stale doc files | Medium | 1d |
| Consolidate boot-related docs into this audit | Medium | 1d |
| Update AGENTS.md with boot process diagram | Low | 0.5d |

### Phase E — P3: Security Hardening

| Task | Priority | Effort |
|------|----------|--------|
| Add PAM-like authentication to getty/login | High | 3d |
| Add audit logging (syscall tracing) | Medium | 3d |
| Implement secure boot chain verification | Low | 5d |
| Add filesystem encryption support (LUKS-like) | Low | 5d |

## 7. Summary

The boot process is functional — the system reaches a login prompt reliably. The architecture is clean (microkernel + userspace drivers via schemes). However, there are significant gaps:

- **Hardware initialization is incomplete**: USB storage not in initfs, no GPU driver at boot, ACPI power management weak
- **User experience is basic**: ion shell lacks job control/completion, console is ASCII-only with no scrollback
- **Security is primitive**: no PAM, no audit logging, no secure boot
- **Documentation is bloated**: 35 docs in local/docs/, many stale

The most impactful improvements are:
1. Fix ACPI shutdown (stability)
2. Improve ion shell (user experience)
3. Enable DRM/GPU in boot (display)
4. Archive stale docs (maintainability)
