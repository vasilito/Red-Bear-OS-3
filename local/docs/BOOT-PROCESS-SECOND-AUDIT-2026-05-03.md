# Red Bear OS — Boot Process Second Audit (D-Bus & Shell Focus)

**Date**: 2026-05-03
**Scope**: D-Bus honesty, console shell quality, login completeness, hardware gaps
**Builds**: base ✅ | base-initfs ✅ | redbear-full (unknown — not tested this session)

## 1. D-Bus Implementation Honesty Assessment

### 1.1 What Exists

| Component | Lines | Status | Notes |
|-----------|-------|--------|-------|
| `dbus-daemon` (v1.16.2) | Upstream | ✅ Builds | 24-line redox.patch, system bus wired in redbear-full |
| `redbear-sessiond` | 2017 | ✅ Builds | Pure Rust, zbus-based login1-compatible daemon |
| `redbear-dbus-services` | Recipe | ✅ Wired | `.service` activation files + XML policies |
| `redbear-polkit` | Recipe | ✅ Builds | Minimal polkit facade |
| `redbear-notifications` | Recipe | ✅ Builds | Notifications D-Bus service |
| `redbear-upower` | Recipe | ✅ Builds | UPower D-Bus facade |
| `redbear-udisks` | Recipe | ✅ Builds | UDisks2 D-Bus facade |

### 1.2 login1 Interface Honesty

| login1 Method | Implemented | Honesty |
|---------------|-------------|---------|
| `ListSessions` | ✅ | Returns real session list |
| `ListSeats` | ✅ | Returns real seat list |
| `ListUsers` | ✅ | Returns user list |
| `GetSession` | ✅ | Returns session by ID |
| `GetSeat` | ✅ | Returns seat by ID |
| `GetUser` | ✅ | Returns user data |
| `CreateSession` | ✅ | Creates sessions |
| `ReleaseSession` | ✅ | Releases/terminates |
| `ActivateSession` | ✅ | Activates on seat |
| `LockSession/UnlockSession` | ✅ | Lock/unlock |
| `PrepareForSleep` | ✅ | Signal emitted |
| `PrepareForShutdown` | ✅ | Signal emitted |
| `Inhibit` | ✅ | Inhibitors with FDs |
| `CanReboot/CanPowerOff` | 🟡 | Returns hardcoded `yes` |
| `PowerOff/Reboot/Suspend` | 🟡 | Calls inner ACPI/kernel — untested at runtime |
| `SetUserSession` | ❌ | Not implemented |
| `SwitchToGreeter` | ❌ | Not implemented (no greeter yet) |
| `AttachDevice` | ❌ | Not implemented (needs udev) |

**Verdict**: The sessiond is a **real implementation**, not a stub. 15/19 login1 methods are implemented. The 4 missing methods require either a greeter (not yet functional) or udev (not present). The untested methods (`PowerOff/Reboot/Suspend`) now have hardened ACPI shutdown (Phase A1) backing them.

### 1.3 D-Bus Integrity Issues

| Issue | Severity | Detail |
|-------|----------|--------|
| No runtime validation | High | All D-Bus code is "build-verified" only. Never tested in QEMU or bare metal. |
| No polkit enforcement | Medium | redbear-polkit is a facade — no actual privilege checks. |
| Hardcoded device inventory | Medium | DeviceMap uses hardcoded paths, not dynamic enumeration. |
| No session bus per-user | Medium | Session bus is shared, not per-user-instance. |
| No .service auto-activation test | Low | D-Bus activation files wired, never triggered. |

## 2. Console Shell Quality (ion)

### 2.1 Feature Matrix

| Feature | ion | bash | dash | POSIX |
|---------|-----|------|------|-------|
| Command execution | ✅ | ✅ | ✅ | ✅ |
| Pipelines (`|`) | ✅ | ✅ | ✅ | ✅ |
| Redirection (`>`, `<`, `>>`) | ✅ | ✅ | ✅ | ✅ |
| Job control (fg/bg/&) | ❌ | ✅ | ✅ | ✅ |
| Ctrl-C / SIGINT | ✅ | ✅ | ✅ | ✅ |
| Ctrl-Z / SIGTSTP | ❌ | ✅ | ✅ | ✅ |
| Tab completion | ❌ | ✅ | ❌ | — |
| History (↑↓) | ✅ | ✅ | ✅ | — |
| History search (Ctrl-R) | ❌ | ✅ | ❌ | — |
| Aliases | ❌ | ✅ | ❌ | — |
| Functions | ❌ | ✅ | ✅ | — |
| If/for/while | ❌ | ✅ | ✅ | ✅ |
| Variables | Basic | Full | Full | ✅ |
| Prompt customization | ❌ | ✅ | ❌ | — |
| ANSI color support | ✅ | ✅ | ❌ | — |
| Unicode | ✅ | ✅ | ❌ | — |
| Startup time | ~5ms | ~15ms | ~3ms | — |
| Binary size | ~500KB | ~1MB | ~150KB | — |

### 2.2 Critical Gaps

1. **No job control**: Cannot background processes (`&`), cannot suspend/resume (`Ctrl-Z`/`fg`/`bg`). This is the single biggest gap — every Unix user expects this.
2. **No tab completion**: Must type every path and command fully. Painful on a filesystem.
3. **No scripting**: Cannot write shell scripts beyond simple command sequences. Cannot use `if`, `for`, `while`.
4. **No aliases**: Cannot create command shortcuts.
5. **No prompt customization**: Prompt is hardcoded, no `PS1` equivalent.

### 2.3 Honesty Assessment

ion is **honest about its limitations** — it advertises as "not POSIX compliant" in its man page. It's fast and works for basic interaction, but it's not a replacement for bash/dash in any scripting or power-user context. For a recovery/mini target it's adequate. For a desktop target, it needs at minimum job control and tab completion.

## 3. Login Prompt — Does It Work?

### 3.1 Service Chain (redbear-mini, console only)

```
29_activate_console.service → inputd -A 2     (activate VT2)
30_console.service          → getty 2         (login prompt on VT2)
31_debug_console.service    → getty 3         (debug console on VT3)
```

### 3.2 Authentication Chain

```
getty → opens TTY → runs login(1)
login(1) → reads /etc/passwd → prompts for password
         → verifies via redox_users::All → spawns ion shell
```

### 3.3 Gaps

| Gap | Severity | Detail |
|-----|----------|--------|
| No /etc/shadow support | Medium | Passwords in /etc/passwd (not hashed separately) |
| No rate limiting | Medium | Unlimited login attempts |
| No secure attention key | Low | No SAK (Ctrl-Alt-Del) handling |
| No session logging | Low | No wtmp/btmp/lastlog |
| No PAM stack | Low | No pluggable auth modules |
| No motd display | Low | /etc/motd exists but may not be shown |

## 4. Hardware Initialization — Per Subsystem

### 4.1 Storage

| Driver | Status | Initfs | Notes |
|--------|--------|--------|-------|
| ahcid | ✅ | ✅ | SATA |
| ided | ✅ | ✅ | Legacy PATA |
| nvmed | ✅ | ✅ | NVMe |
| usbscsid | ✅ | ✅ (new!) | USB mass storage — Phase B2 |
| virtio-blkd | ✅ | ✅ | VirtIO block |

### 4.2 Display

| Driver | Status | Initfs | Notes |
|--------|--------|--------|-------|
| vesad | ✅ | ✅ | VESA only, no acceleration |
| redox-drm | 🟡 | 🟡 (service file added, binary not in BINS) | AMD/Intel DRM — compiled but not in boot path |
| virtio-gpud | ✅ | ✅ | VirtIO GPU |

### 4.3 Input

| Driver | Status | Initfs | Notes |
|--------|--------|--------|-------|
| ps2d | ✅ | ✅ | PS/2 keyboard + mouse |
| usbhidd | ✅ | ✅ | USB HID (hardened P3) |
| inputd | ✅ | ✅ | Multiplexer |

### 4.4 Network

| Driver | Status | Initfs | Notes |
|--------|--------|--------|-------|
| e1000d | ✅ | ❌ | Intel Gigabit — userland only |
| rtl8168d | ✅ | ❌ | Realtek — userland only |
| rtl8139d | ✅ | ❌ | Realtek legacy — userland only |
| ixgbed | ✅ | ❌ | Intel 10GbE — userland only |
| virtio-netd | ✅ | ❌ | VirtIO — userland only |
| smolnetd | ✅ | ❌ | Network stack — userland |
| dhcpd | ✅ | ❌ | DHCP client — userland |
| **WiFi** | ❌ | ❌ | Not implemented |
| **Bluetooth** | ❌ | ❌ | Not implemented |

### 4.5 USB

| Controller | Status | Initfs | Notes |
|------------|--------|--------|-------|
| xhcid | ✅ | ✅ | xHCI USB 3.x |
| ehcid | ✅ | ❌ | USB 2.0 — userland only |
| uhcid | ✅ | ❌ | USB 1.1 — userland only |
| ohcid | ✅ | ❌ | USB 1.1 — userland only |
| usbhubd | ✅ | ✅ | USB hub |

### 4.6 Audio

| Driver | Status | Initfs | Notes |
|--------|--------|--------|-------|
| ac97d | 🟡 | ❌ | AC'97 — partial |
| ihdad | 🟡 | ❌ | Intel HDA — partial |
| sb16d | 🟡 | ❌ | SoundBlaster — partial |
| audiod | 🟡 | ❌ | Audio multiplexer — userland |

### 4.7 ACPI / Power

| Component | Status | Notes |
|-----------|--------|-------|
| ACPI table parsing | ✅ | RSDP, FADT, MADT, DSDT/SSDT |
| AML interpreter | ✅ | Bounded subset |
| Shutdown (S5) | ✅ (hardened!) | PM1a validation, PM1b retry, keyboard reset fallback |
| Reboot | 🟡 | Reset register + keyboard fallback |
| Sleep (S3/S4) | ❌ | Not implemented |
| Thermal | ❌ | No thermal daemon |
| Battery | ❌ | No battery status |

## 5. Implementation Improvement Plan — Second Pass

### Phase F1 — D-Bus Runtime Validation (Week 1)

| Task | Effort |
|------|--------|
| Boot redbear-full in QEMU, check dbus-daemon startup | 1h |
| Verify sessiond D-Bus interface responds to `dbus-send` queries | 2h |
| Fix any startup/runtime issues found | 4h |
| Add D-Bus runtime smoke test to validation scripts | 2h |

### Phase F2 — ion Shell Improvements (Week 2-3)

| Task | Priority | Effort |
|------|----------|--------|
| Job control (fg/bg/Ctrl-Z/&) | Critical | 3d |
| Tab completion (commands + paths) | Critical | 2d |
| History search (Ctrl-R) | High | 1d |
| Aliases (`alias` command) | High | 0.5d |
| Prompt customization (PS1 env var) | Medium | 0.5d |
| Scripting (if/for/while) | Medium | 3d |

### Phase F3 — Credential Hardening (Week 2)

| Task | Effort |
|------|--------|
| Add /etc/shadow support to login/passwd | 4h |
| Add rate limiting (3 failures → 5s delay) | 1h |
| Add motd display in login | 0.5h |

### Phase F4 — DRM in Boot Path (Week 1)

| Task | Effort |
|------|--------|
| Add `redox-drm` to base-initfs BINS array | 15min |
| Build and verify DRM service starts in initfs | 2h |
| Verify framebuffer switch from VESA to DRM at boot | 3h |

### Phase F5 — Network in Initfs (Week 3)

| Task | Effort |
|------|--------|
| Move e1000d/rtl8168d to initfs BINS | 30min |
| Add init network services (dhcpd, smolnetd) to initfs | 1h |
| Enable netctl boot profile loading at initfs | 2h |

### Phase F6 — Documentation Cleanup (Ongoing)

| Task | Effort |
|------|--------|
| Archive GRUB-INTEGRATION-PLAN.md (GRUB already implemented) | 5min |
| Archive VFAT-IMPLEMENTATION-PLAN.md (VFAT already implemented) | 5min |
| Archive USB-BOOT-INPUT-PLAN.md (superseded) | 5min |

## 6. Known Stale Docs

| File | Reason |
|------|--------|
| `GRUB-INTEGRATION-PLAN.md` | GRUB is fully implemented (grub recipe, redbear-grub config, installer support) |
| `VFAT-IMPLEMENTATION-PLAN.md` | VFAT is fully implemented (fatd, fat-mkfs, fat-label, fat-check) |
| `USB-BOOT-INPUT-PLAN.md` | Superseded — USB HID is in initfs, USB storage is now in initfs (Phase B2) |
| `ZSH-PORTING-PLAN.md` | Deferred indefinitely — ion is the default shell |

## 7. Summary

**D-Bus**: The sessiond is a real 2017-line implementation, not a stub. 15/19 login1 methods work. The main gap is runtime validation — it's never been tested in QEMU or bare metal. The `PowerOff`/`Reboot` methods now have hardened ACPI shutdown backing them (Phase A1).

**Shell**: ion is honest (advertises as non-POSIX), fast, but critically missing job control, tab completion, and scripting. Adequate for console/recovery. Needs 3 features for desktop readiness.

**Login**: Reaches prompt via getty→login→ion. Works but lacks /etc/shadow, rate limiting, and session management.

**Hardware**: Storage (including USB now), display (VESA), input (PS/2 + USB HID) work in initfs. Network and audio are userland-only. WiFi, Bluetooth, sleep states, thermal, and battery are not implemented.
