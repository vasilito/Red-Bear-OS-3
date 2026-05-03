# Red Bear OS — Comprehensive Fix & Improvement Plan

**Date**: 2026-05-03
**Scope**: All subsystems, boot to desktop
**Previous audits**: `BOOT-PROCESS-AUDIT-2026-05-03.md`, `BOOT-PROCESS-SECOND-AUDIT-2026-05-03.md`

---

## 0. Current State

```
Build:      12/12 patches → base ✅ → base-initfs ✅
Boot:       UEFI → kernel → init → services → getty/login → ion shell
Targets:    redbear-mini (console), redbear-full (desktop), redbear-grub (GRUB boot)
Hardware:   x86_64 only. QEMU-tested. Bare metal untested.
```

### Completed (this session)

| Phase | Item | Status |
|-------|------|--------|
| A1 | ACPI shutdown hardening (PM1a validation, timeout, PM1b retry, keyboard reset) | ✅ |
| A2 | Persistent logging (/var/log/system.log, 5MB rotation) | ✅ |
| B1 | DRM service file in initfs | ✅ |
| B2 | USB mass storage service file in initfs | ✅ |
| D | Documentation cleanup (9 stale docs archived) | ✅ |
| — | Build system atomicity (staging + rollback, normalize_patch, workspace cleanup) | ✅ |
| — | Input stack hardening (usbhidd validation, keymapd XKB bridge, init colored output) | ✅ |

---

## 1. Priority Matrix

| Priority | Definition |
|----------|-----------|
| **P0 — Blocking** | System cannot reach login prompt or crashes during boot |
| **P1 — Critical** | Core functionality missing; blocks desktop path or basic usability |
| **P2 — High** | Significant UX/security gap; required for production readiness |
| **P3 — Medium** | Quality-of-life improvement; can be deferred |
| **P4 — Low** | Nice-to-have; deferred indefinitely |

---

## 2. P0 — Blocking Issues

**None currently.** The system reaches a login prompt reliably on redbear-mini. Redbear-full builds but has not been boot-tested this session.

| # | Issue | Fix | Effort |
|---|-------|-----|--------|
| P0-1 | **Boot redbear-full in QEMU** and verify it reaches login/desktop | Run `make qemu CONFIG_NAME=redbear-full`, collect logs, fix any boot failures | 2h |
| P0-2 | **Verify 12-patch chain on clean checkout** | `make distclean && make all CONFIG_NAME=redbear-mini` | 1h |

---

## 3. P1 — Critical Gaps

### P1-1: D-Bus Runtime Validation
**Impact**: KWin/Plasma cannot start without working D-Bus. All D-Bus code is "build-verified" only.
**Files**: `local/recipes/system/redbear-sessiond/source/`, `config/redbear-full.toml`

| Step | Action | Effort |
|------|--------|--------|
| 1 | Boot redbear-full in QEMU | 30min |
| 2 | Verify `dbus-daemon` starts (`ps | grep dbus`) | 15min |
| 3 | Verify `redbear-sessiond` starts and registers on bus | 15min |
| 4 | Test `dbus-send --system --dest=org.freedesktop.login1 ... ListSessions` | 30min |
| 5 | Test `ListSeats`, `GetUser`, `CreateSession` | 1h |
| 6 | Test `PowerOff` (now backed by hardened ACPI shutdown) | 30min |
| 7 | Fix any startup/runtime failures found | 4h |

**Acceptance**: `dbus-send` to login1 returns valid session/seat/user data. `PowerOff` triggers ACPI shutdown sequence.

### P1-2: ion Shell — Job Control
**Impact**: Cannot background processes, cannot Ctrl-Z suspend. Every Unix user expects this.
**Files**: `recipes/core/ion/source/src/`

| Step | Action | Effort |
|------|--------|--------|
| 1 | Implement signal handling for SIGTSTP/SIGCONT in ion_shell | 1d |
| 2 | Add background job table (track PIDs, job numbers) | 1d |
| 3 | Implement `fg`, `bg`, `jobs` builtins | 4h |
| 4 | Implement `&` operator for backgrounding at command line | 2h |
| 5 | Wire Ctrl-Z to send SIGTSTP to foreground process group | 2h |

**Acceptance**: `sleep 60 &`, `jobs`, `fg %1`, `Ctrl-Z` → `bg` works. `ps` shows proper process states.

### P1-3: ion Shell — Tab Completion
**Impact**: Must type every path and command fully. Painful on any filesystem.
**Files**: `recipes/core/ion/source/src/`

| Step | Action | Effort |
|------|--------|--------|
| 1 | Add `liner::Completer` trait implementation to ion | 4h |
| 2 | Implement command completion (scan $PATH) | 2h |
| 3 | Implement file path completion | 2h |
| 4 | Implement partial match + common prefix completion | 1h |

**Acceptance**: Tab completes commands from $PATH. Tab completes file paths. Double-tab shows options.

### P1-4: DRM/KMS in Boot Path
**Impact**: Only VESA framebuffer available at boot. No GPU acceleration.
**Files**: `recipes/core/base-initfs/recipe.toml`

| Step | Action | Effort |
|------|--------|--------|
| 1 | Add `redox-drm` to base-initfs BINS array | 15min |
| 2 | Verify service file exists (added in Phase B1) | ✅ done |
| 3 | Build and boot redbear-full | 1h |
| 4 | Verify framebuffer switches from VESA to DRM at boot | 1h |
| 5 | Fix any GPU-specific issues (AMD DC or Intel display) | 4h |

**Acceptance**: `lspci` shows GPU. `/scheme/drm/card0` exists. Framebuffer output works via redox-drm.

---

## 4. P2 — High Priority

### P2-1: Login /etc/shadow Support
**Impact**: Passwords stored in /etc/passwd (not hashed separately). Security gap.
**Files**: `recipes/core/userutils/source/src/bin/login.rs`, `redox_users` crate

| Step | Action | Effort |
|------|--------|--------|
| 1 | Read /etc/shadow for password hash (fall back to /etc/passwd) | 2h |
| 2 | Verify SHA-crypt hash verification works (sha-crypt crate already in use) | 1h |
| 3 | Update passwd command to write to /etc/shadow | 1h |

**Acceptance**: Password in /etc/shadow, not /etc/passwd. Login verifies against shadow.

### P2-2: Login Rate Limiting
**Impact**: Unlimited brute-force attempts.
**Files**: `recipes/core/userutils/source/src/bin/login.rs`

| Step | Action | Effort |
|------|--------|--------|
| 1 | Track consecutive failures per TTY | 30min |
| 2 | Sleep 5 seconds after 3 failures | 15min |
| 3 | Log failures to syslog | 15min |

**Acceptance**: 3 wrong passwords → 5-second delay. Delay doubles for each subsequent failure.

### P2-3: Network in Initfs
**Impact**: No network during early boot. DHCP/networking only available after switch_root.
**Files**: `recipes/core/base/source/init.initfs.d/`, `recipes/core/base-initfs/recipe.toml`

| Step | Action | Effort |
|------|--------|--------|
| 1 | Add `e1000d`, `rtl8168d` to base-initfs BINS | 15min |
| 2 | Create `60_smolnetd.service` for initfs | 15min |
| 3 | Create `61_dhcpd.service` for initfs | 15min |
| 4 | Verify netctl boot profile loading works in initfs | 1h |

**Acceptance**: Network available before switch_root. `ifconfig` shows IP. `ping` works.

### P2-4: D-Bus Polkit Enforcement
**Impact**: redbear-polkit is a facade — no actual privilege checks. KAuth expects real polkit.
**Files**: `local/recipes/system/redbear-polkit/source/`

| Step | Action | Effort |
|------|--------|--------|
| 1 | Implement `CheckAuthorization` method with actual policy lookup | 3h |
| 2 | Define default policies (allow root, ask for user password for admin actions) | 2h |
| 3 | Test with KAuth-dependent KDE actions | 2h |

**Acceptance**: `pkcheck --action-id org.freedesktop.login1.power-off` returns auth result.

---

## 5. P3 — Medium Priority

### P3-1: ion Shell — History Search (Ctrl-R)
**Effort**: 1d. Implement incremental reverse search using `liner` library.

### P3-2: ion Shell — Aliases
**Effort**: 2h. Add `alias` builtin, resolve aliases before command lookup.

### P3-3: fbcond Scrollback Buffer
**Effort**: 4h. Add 1000-line ring buffer to framebuffer console. PgUp/PgDn to scroll.

### P3-4: ACPI Sleep States (S3/S4)
**Effort**: 2d. Implement `_S3`/`_S4` AML method invocation. Save/restore device state.

### P3-5: Thermal Daemon
**Effort**: 2d. Read CPU temperature via ACPI thermal zone. Log warnings. Throttle on overheat.

### P3-6: Battery Status
**Effort**: 1d. Read ACPI battery info. Expose via D-Bus org.freedesktop.UPower.

---

## 6. P4 — Deferred

| Item | Reason |
|------|--------|
| WiFi driver enablement | Requires iwlwifi kernel module port (LinuxKPI), firmware loading |
| Bluetooth stack | Requires USB maturity, BlueZ port or native stack |
| Secure boot chain | Requires TPM support, measured boot |
| Filesystem encryption | Requires LUKS-like block layer |
| ZSH port | ion is default; zsh is optional |
| RTC write support | Low priority — NTP can adjust kernel clock without hardware RTC write |

---

## 7. Implementation Order

```
Week 1:  P0-1 (boot redbear-full) → P0-2 (clean build verify)
         P1-4 (DRM in boot path)
         P1-1 (D-Bus runtime validation) — parallel with P1-4

Week 2:  P1-2 (ion job control) → P1-3 (ion tab completion)
         P2-1 (shadow support) → P2-2 (rate limiting)

Week 3:  P2-3 (network in initfs)
         P3-1 (ion history search) → P3-2 (ion aliases)

Week 4:  P2-4 (polkit enforcement)
         P3-3 (fbcond scrollback)

Week 5-6: P3-4 (sleep states)
          P3-5 (thermal daemon)
          P3-6 (battery status)
```

### Parallel Opportunities

```
Week 1: [P0-1/P0-2] || [P1-1] || [P1-4]
Week 2: [P1-2 → P1-3] || [P2-1 → P2-2]
Week 3: [P2-3] || [P3-1 → P3-2]
```

---

## 8. Acceptance Gates

| Gate | Requirement |
|------|-------------|
| G1 — Console Boot | redbear-mini reaches login prompt. All 12 patches apply. base + base-initfs build. |
| G2 — Desktop Boot | redbear-full reaches login prompt or greeter. D-Bus daemon + sessiond start. |
| G3 — Shell Usability | ion supports job control, tab completion, history search, aliases. |
| G4 — Security Baseline | Passwords in /etc/shadow. Rate limiting active. Polkit enforces authorization. |
| G5 — Hardware Coverage | DRM/KMS active at boot. Network available in initfs. USB storage in initfs. |

---

## 9. Total Effort Estimate

| Priority | Items | Effort |
|----------|-------|--------|
| P0 | 2 items | 3h |
| P1 | 4 items | ~40h (5 days) |
| P2 | 4 items | ~20h (2.5 days) |
| P3 | 6 items | ~40h (5 days) |
| **Total** | **16 items** | **~103h (~13 days with 1 dev, ~1 week with 2 devs)** |
