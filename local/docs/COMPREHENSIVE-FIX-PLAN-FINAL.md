# Red Bear OS — Comprehensive Fix Plan (Final)

**Date**: 2026-05-03
**Status**: 13 patches, redbear-mini boots, redbear-full KDE chain broken
**QEMU verified**: ✅ text console boot, ❌ graphical desktop build

---

## 0. Current State

```
Build:      13 patches → base ✅  base-initfs ✅  userutils ✅
Boot:       redbear-mini → UEFI → 25+ services → console login ✅
            redbear-full → build fails at kf6-kitemviews (pkgar race)
Hardware:   QEMU x86_64. VESA, PS/2, USB HID, PCI, ACPI — all functional.
```

### Completed (all sessions)

| # | Item | Status |
|---|------|--------|
| 1 | Build system atomicity (staging + rollback) | ✅ |
| 2 | Patch normalization (diff --git → ---/+++) | ✅ |
| 3 | Workspace pollution cleanup | ✅ |
| 4 | --allow-protected CLI flag | ✅ |
| 5 | PS/2 LED feedback + InputProducer | ✅ |
| 6 | USB HID hardening (validation, retry, lookup table) | ✅ |
| 7 | Init colored ANSI output | ✅ |
| 8 | XKB bridge (redbear-keymapd) | ✅ |
| 9 | ACPI shutdown hardening | ✅ |
| 10 | Persistent logging (logd → /var/log/system.log) | ✅ |
| 11 | DRM + USB initfs service files | ✅ |
| 12 | Network drivers in initfs (e1000d, rtl8168d, smolnetd, dhcpd) | ✅ |
| 13 | Login rate limiting | ✅ |
| 14 | Documentation (4 audit docs, 9 stale archived) | ✅ |

---

## 1. P0 — Blocker: KDE Build Chain

### Problem
`make live CONFIG_NAME=redbear-full` fails:
```
cook kf6-kitemviews - failed
failed to install 'libwayland/stage.pkgar' in 'kf6-kitemviews/sysroot.tmp':
No such file or directory
```

`libwayland` builds successfully but its `stage.pkgar` is missing when `kf6-kitemviews` needs it.

### Root Cause Analysis

The cookbook tool (`src/cook/`) has a dependency staging race:
1. `libwayland` builds → publishes pkgar to `repo/`
2. `kf6-kitemviews` depends on `libwayland`
3. Cookbook installs dependencies into `sysroot.tmp` before building
4. The pkgar file is looked up at `recipes/wip/wayland/libwayland/target/.../stage.pkgar`
5. This path is incorrect — pkgar should be looked up in `repo/` not `target/`

### Fix

**File**: `src/cook/` — investigate `pkgar` push/install logic.

| Step | Action |
|------|--------|
| 1 | Read `src/cook/package.rs` — `package_source_paths()` function |
| 2 | Read `src/cook/cook_build.rs` — how sysroot.tmp is populated |
| 3 | Trace the pkgar lookup path for `kf6-kitemviews` → `libwayland` |
| 4 | Fix the path lookup to use `repo/` directory instead of `target/` |
| 5 | Rebuild: `make live CONFIG_NAME=redbear-full` |
| 6 | Verify: kf6-kitemviews builds, ISO created |

**Estimated effort**: 4-8 hours (investigation + fix + rebuild)

---

## 2. P1 — Graphical Boot Path

After fixing the KDE build chain, the graphical boot needs runtime validation.

### Components to Test

| Component | Binary | Expected |
|-----------|--------|----------|
| dbus-daemon | /usr/bin/dbus-daemon | System bus starts, responds to `dbus-send` |
| redbear-sessiond | /usr/bin/redbear-sessiond | Registers `org.freedesktop.login1`, responds to ListSessions |
| seatd | /usr/bin/seatd | Seat management |
| redbear-compositor | /usr/bin/redbear-compositor | Wayland compositor starts |
| KWin | /usr/bin/kwin_wayland | KWin connects to compositor |
| redbear-greeter | /usr/bin/redbear-greeter | Graphical login screen on framebuffer |

### Test Procedure

```bash
# Build
make live CONFIG_NAME=redbear-full

# Boot with VNC (for remote graphical access)
qemu-system-x86_64 -m 4096 \
  -drive file=build/x86_64/redbear-full/harddrive.img,format=raw \
  -drive if=pflash,file=/usr/share/edk2/x64/OVMF_CODE.4m.fd,readonly=on \
  -drive if=pflash,file=/tmp/OVMF_VARS.fd \
  -vnc :0

# Connect via VNC viewer and observe graphical boot
# Login via VNC greeter or switch to VT2 (Ctrl+Alt+F2) for text console
```

### Acceptance Criteria

| Gate | Requirement |
|------|-------------|
| G1 | dbus-daemon starts without errors |
| G2 | redbear-sessiond registers on D-Bus system bus |
| G3 | `dbus-send --system --dest=org.freedesktop.login1 /org/freedesktop/login1 org.freedesktop.login1.Manager.ListSessions` returns valid data |
| G4 | Wayland compositor initializes (no crash) |
| G5 | Greeter displays on framebuffer (or text login on VT2 as fallback) |

---

## 3. P2 — Remaining Gaps (from previous audits)

| # | Item | Priority | Effort | Status |
|---|------|----------|--------|--------|
| P2-1 | ion shell job control (fg/bg/Ctrl-Z/&) | High | 3d | Not started |
| P2-2 | ion shell tab completion | High | 2d | Not started |
| P2-3 | /etc/shadow support | High | 4h | Blocked (redox_users crate) |
| P2-4 | polkit enforcement | Medium | 3h | Blocked (needs D-Bus runtime) |
| P2-5 | fbcond scrollback buffer | Medium | 4h | Not started |
| P2-6 | ACPI sleep states (S3/S4) | Low | 2d | Not started |
| P2-7 | Thermal daemon | Low | 2d | Not started |

---

## 4. Implementation Order

```
DAY 1-2:  P0 — Fix KDE build chain (pkgar staging race)
          → Rebuild redbear-full
          → Boot graphical image

DAY 3:    P1 — Test graphical boot components
          → D-Bus validation
          → sessiond/Listsessions test
          → Greeter/console verification

DAY 4-5:  P2-1 — ion job control
          → Background process table
          → fg/bg/jobs builtins
          → Ctrl-Z / SIGTSTP handling

DAY 6:    P2-2 — ion tab completion
          → PATH command completion
          → File path completion

DAY 7:    P2-3/P2-5 — Shadow support + fbcond scrollback
          (if redox_users permits shadow; else document limitation)
```

---

## 5. Cookbook Tool — Specific Areas to Investigate

### pkgar path issue

```rust
// src/cook/package.rs — likely location
fn package_source_paths(pkg_name: &str, ...) -> Vec<PathBuf> {
    // Returns target/<triplet>/stage.pkgar paths
    // Bug: returns target/ path when recipe is in wip/wayland/
    // Fix: should return repo/<triplet>/<pkg>.pkgar path
}
```

### Dependency staging order

```rust
// src/cook/cook_build.rs — sysroot.tmp population
fn build_deps_sysroot(deps: &[CookRecipe], sysroot: &Path) {
    for dep in deps {
        // Should check repo/ for pkgar, not target/
        let pkgar = dep.repo_pkgar_path(); // propose: new method
        install_pkgar(pkgar, sysroot);
    }
}
```

---

## 6. Total Effort

| Phase | Items | Effort |
|-------|-------|--------|
| P0 — KDE build fix | 1 item | 4-8h |
| P1 — Graphical boot test | 5 components | 4h |
| P2 — Remaining gaps | 7 items | ~80h |
| **Total** | **13 items** | **~12 days (1 dev)** |
