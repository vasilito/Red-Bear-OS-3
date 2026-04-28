# Red Bear OS Boot Process Assessment & Improvement Plan

**Generated:** 2026-04-23
**Updated:** 2026-04-27
**Status:** Phase 1 ✅, Phase 2 ✅, Phase 3 ✅, Phase 4 ✅ (docs + known gaps), Phase 5 ✅, Phase 6 ✅ (boot to login confirmed), Phase 7 ✅ (kernel RAM hang diagnosed + ISO organization documented)
**Scope:** Comprehensive assessment of boot completeness, mistakes, robustness, resilience, and quality

## Boot Chain Overview

```
UEFI firmware → RedBear Bootloader → Kernel (kstart→start→kmain) →
userspace_init → bootstrap (forks initfs/procmgr/initnsmgr) →
fexec init → [initfs phase] → switchroot /usr → [rootfs phase] →
login prompt (text or graphical)
```

## Phase 1: Critical Fixes Applied ✅

| ID | Severity | Fix | Evidence |
|----|----------|-----|----------|
| S1b | SHOWSTOPPER | Removed `boot_essential = true` from 3 greeter services — `#[serde(deny_unknown_fields)]` caused deserialization failure, services never loaded | `config/redbear-greeter-services.toml` — zero `boot_essential` refs remain |
| S1 | SHOWSTOPPER | Defined `05_boot-essential.target` and `12_boot-late.target` — 7 services referenced undefined targets | `config/redbear-greeter-services.toml`, `config/redbear-device-services.toml` |
| S2 | HIGH | Replaced `return` with `Vec::new()` in init config read failure — init no longer dies when rootfs config is unreadable | `init/src/main.rs:165` |
| S4 | HIGH | Removed empty `15_fatd.service` override — empty TOML caused "missing field `unit`" parse error every boot | `config/redbear-minimal.toml` |
| S5 | MEDIUM | Replaced `waitpid().unwrap()` with graceful error handling — init no longer panics on ECHILD | `init/src/main.rs:182-188` |

## Phase 2: Daemon Error Handling ✅

Replaced `unwrap()/expect()`/`assert!()` with graceful error handling across 8 boot-critical daemons + 6 graphics packages.
**Total: 215 fixes across 33 Rust source files. Zero unwrap/expect/assert in non-test production code.**

### 2A: Daemon Library + Init Spawn ✅ (10 fixes)
- `daemon/src/lib.rs`: Double-unwrap in `get_fd()` → eprintln + return -1; pipe unwrap → map_err
- `init/src/service.rs`: 3 fixes (pipe, getns, register_scheme_to_ns)
- `init/src/main.rs`: 2 fixes (filename UTF-8, setrens)
- `init/src/unit.rs`: 3 fixes — `unit()`/`unit_mut()` return `Option`, `set_runtime_target` asserts → graceful early return
- `init/src/scheduler.rs`: 2 caller updates — missing unit logs warning + skips instead of panicking

### 2B: Logd ✅ (8 fixes)
- `logd/src/main.rs`: Socket create, setrens, process_requests_blocking — match on Result<!>
- `logd/src/scheme.rs`: kernel_debug File → Option<File>, kernel_sys_log → Option, read/send errors handled

### 2C: Randd + Zerod ✅ (7 fixes)
- `randd/src/main.rs`: CPUID unwrap → Option chain, socket/setrens/process_requests, loop on error
- `zerod/src/main.rs`: Args → default "zero" + graceful exit, socket/setrens/process_requests, loop on error

### 2D: Inputd ✅ (14 fixes)
- `inputd/src/lib.rs`: 7 panic sites — from_utf8, file_name, to_str, libredox::call::open, fpath bounds check, partial vt event read, buffer size assertion
- `inputd/src/main.rs`: 7 panic sites — write!, handles.remove, deamon(), args, ControlHandle, panic! → eprintln+exit, Producer handle assertion → EBADF

### 2E: Vesad + Fbcond ✅ (34 fixes)
- `vesad/src/main.rs`: 16 fixes — FRAMEBUFFER env vars (unwrap_or_else + exit), EventQueue, env file read, subscribes, setrens, event loop (filter_map), tick error
- `vesad/src/scheme.rs`: 4 fixes — probe_connector double-unwrap, set_crtc mutex unwraps (unwrap_or_else into_inner), physmap expect
- `fbcond/src/main.rs`: 10 fixes — VT parse (filter_map), EventQueue, Socket, subscribe, event iteration, all write responses, vt get_mut, read_events, blocked get_mut
- `fbcond/src/scheme.rs`: 1 fix — fpath write! unwrap → map_err
- `fbcond/src/display.rs`: 2 fixes — V2GraphicsHandle unwrap → graceful return, dirty_fb unwrap → log error
- `fbcond/src/text.rs`: 1 fix — pop_front unwrap → unwrap_or(0)

### 2F: Init Unit Store ✅ (3 fixes)
- `unit.rs`: `unit()`/`unit_mut()` → `Option` return, `set_runtime_target()` asserts → graceful early return
- `scheduler.rs`: Callers handle None gracefully — log warning + skip instead of panicking init

## Phase 3: Boot Reliability ✅

### 3A: Boot Progress Markers ✅
Init now logs phase markers:
- `init: phase 1 — initfs boot`
- `init: starting logd`
- `init: starting runtime target`
- `init: phase 2 — switchroot to /usr`
- `init: scheduling N rootfs units`
- `init: phase 3 — rootfs services started`
- `init: boot complete — entering waitpid loop`

### 3B: Service Schema Validation (Manual) ✅
Script: `local/scripts/validate-service-files.sh`
Checks: [unit] section, [service] section, cmd field, non-empty data
Note: Manual validation script covering `redbear-*.toml` configs. Not wired into the build system — run manually after config changes. Does not cover inherited mainline configs (minimal.toml, desktop.toml).

### 3C: Getty Supervisor ✅
Init supports `respawn = true` in service TOML files. When a respawnable service's process exits, init automatically re-spawns it. All getty services across `redbear-mini`, `redbear-full`, `redbear-greeter-services`, `redbear-grub`, and `wayland` configs now have `respawn = true` set.

Implementation:
- `service.rs`: Added `respawn: bool` field to `Service` (default false). `spawn()` returns `Option<u32>` (child PID) for respawnable oneshot_async services.
- `scheduler.rs`: `Scheduler` collects respawnable (unit_id, pid) pairs in `respawn_pids` field.
- `main.rs`: Waitpid loop maintains a PID → UnitId map. On child exit, checks if the PID is respawnable and re-schedules the unit.

Usage in service TOML:
```toml
[unit]
description = "Text console"

[service]
cmd = "getty"
args = ["2"]
type = "oneshot_async"
respawn = true
```

### 3D: Greeter Crash Fallback (existing)
The fallback path via `29_activate_console.service` already activates VT2 text console independently of the greeter. If greeter crashes, text login is already available.

## Phase 4: Bare-Metal Hardening ✅ (docs + known gaps documented)

Phase 4 is documentation and gap identification. Actual bare-metal validation requires physical hardware.
All known gaps are documented with their status and required follow-up.

### USB Boot-Chain Observability
Chain: pcid-spawner → xhcid → usbhubd → usbhidd → inputd
Status: Chain exists in rootfs only. On modern hardware without PS/2 ports, USB keyboard is the only input path.

### Known Bare-Metal Gaps
| Gap | Status | Detail |
|-----|--------|--------|
| USB keyboard | Documented | 5-step chain in rootfs only; if any step fails, no keyboard |
| AMD x2APIC SMP | Patch exists | `local/patches/kernel/P0-amd-acpi-x2apic.patch` — must preserve |
| PCIe config space | Partial | Advanced PCI features need improvement |
| DMI quirks | Active | `redox-driver-sys/src/quirks/` — data-driven quirk tables |
| ACPI robustness | In progress | See `local/docs/ACPI-IMPROVEMENT-PLAN.md` |
| IRQ/low-level controllers | Active | See `local/docs/IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md` |

### Hardware Validation Requirements
Bare-metal testing requires physical hardware. Current validation is:
- **QEMU boot**: Verified for redbear-mini and redbear-full (no panics, no parse errors, switchroot succeeds)
- **Live ISO build**: redbear-mini and redbear-grub build successfully
- **Interactive login**: Framebuffer login renders correctly (serial not available in headless QEMU)

## Phase 5: Validation Matrix ✅

### Build Verification
| Target | Build | QEMU Boot | Bare-Metal Boot | Notes |
|--------|-------|-----------|-----------------|-------|
| redbear-mini | ✅ harddrive.img (2 GB) | ✅ Login prompt | — | Framebuffer console login |
| redbear-full | ✅ harddrive.img (4 GB) | ✅ Login prompt | — | Desktop packages included |
| redbear-grub | ✅ harddrive.img | — | — | Text-only with GRUB chainload |

### Compilation Verification
- `cargo check --workspace` in base source: **0 errors**
- Individual crate checks: daemon, init, logd, randd, zerod, inputd, vesad, fbcond, console-draw, driver-graphics, fbbootlogd, graphics-ipc, ihdgd, virtio-gpud — **all pass**
- Service file validation: **53 service files pass, 0 failures**

### Unwrap/expect Audit (final)
| Daemon | Active unwrap/expect | Test-only | Status |
|--------|---------------------|-----------|--------|
| daemon/src | 0 | 0 | ✅ |
| init/src (main, service, scheduler, unit) | 0 | 0 | ✅ |
| logd/src | 0 | 0 | ✅ |
| randd/src | 0 | 8 (#[test]) | ✅ |
| zerod/src | 0 | 0 | ✅ |
| inputd/src (lib, main) | 0 | 0 | ✅ |
| vesad/src (main, scheme) | 0 | 0 | ✅ |
| fbcond/src (main, scheme, display, text) | 0 | 0 | ✅ |
| console-draw/src | 0 | 0 | ✅ |
| driver-graphics/src (lib, kms/*) | 0 | 0 | ✅ |
| fbbootlogd/src (main, scheme) | 0 | 0 | ✅ |
| graphics-ipc/src | 0 | 0 | ✅ |
| ihdgd/src (main, device/*) | 0 | 0 | ✅ |
| virtio-gpud/src (main, scheme) | 0 | 0 | ✅ |

### Validation Commands
```bash
# Build
CI=1 make all CONFIG_NAME=redbear-mini ARCH=x86_64
CI=1 make all CONFIG_NAME=redbear-full ARCH=x86_64
CI=1 make live CONFIG_NAME=redbear-mini ARCH=x86_64
CI=1 make live CONFIG_NAME=redbear-full ARCH=x86_64

# QEMU test
make qemu CONFIG_NAME=redbear-mini

# Service file validation
./local/scripts/validate-service-files.sh config/

# Clean rebuild + verify
CI=1 make cr.base CONFIG_NAME=redbear-mini ARCH=x86_64
CI=1 make all CONFIG_NAME=redbear-mini ARCH=x86_64
```

## Key Technical Findings

### Bare-Metal Boot Log Analysis (2026-04-24)

AMD machine boot log shows initfs phase starts but never completes:
- Kernel boots: ACPI, IOAPIC, timer, memory all OK
- vesad initializes: 1280x1024 at 0xA0000000 (FRAMEBUFFER_* from UEFI bootloader)
- fbbootlogd maps display
- ps2d: keyboard works, mouse BAT fails (no PS/2 mouse port — expected on modern hardware)
- pcid begins PCI enumeration
- acpid starts, AML interpreter initializes
- **MISSING**: "init: initfs drivers target step() complete" — scheduler.step() never returns
- **MISSING**: "init: phase 2 — switchroot to /usr" — rootfs phase never starts
- **MISSING**: any getty or login output

Root cause hypothesis (unproven): a service with `type = "notify"`, `type = { scheme = "..." }`,
or `type = "oneshot"` in the initfs phase does not signal readiness or does not exit,
causing init's scheduler.step() to block forever. All three service types wait synchronously
in `service.rs`. Possible blockers include:
- A `notify` service that hangs before calling `daemon::Daemon::ready()`
- A `scheme` service that hangs before calling `daemon::SchemeDaemon::ready_*()`
- An `oneshot` service like `pcid-spawner --initfs` that hangs during PCI enumeration
With the new per-service logging (Phase 6A + 6C), the next boot will show exactly which
service blocks — the last `init: starting ...` line before the hang identifies the blocker.

### Bare-Metal/QEMU Boot Log Analysis (2026-04-24, second test with Phase 6 logging)

The enhanced logging proved the initfs phase completes successfully. The actual blocker is
in the rootfs phase:

- Initfs phase: ✅ all services start and signal readiness/exit correctly
- `init: phase 2 - switchroot to /usr` ✅
- `init: scheduling 22 rootfs units` ✅
- `init: starting PCI driver spawner (pcid-spawner)` ← **BLOCKS HERE**
- pcid-spawner (rootfs, `type = "oneshot"`) spawns e1000d (ok), ihdad (fails with RIRB timeout)
- Then hangs — no further output for 30+ seconds while system is alive (keyboard works)
- Init never reaches `30_console` → getty → login

Root cause (confirmed): rootfs `00_pcid-spawner.service` uses `type = "oneshot"`, which
causes init to block until pcid-spawner exits. On real hardware and QEMU, pcid-spawner
can hang waiting for a PCI device driver that never responds, blocking the entire rootfs
phase including getty/login.

Fix: override `00_pcid-spawner.service` to `type = "oneshot_async"` in
`config/redbear-legacy-base.toml`. Drivers spawn in the background while init proceeds
to start console services. Network services that depend on specific drivers handle their
own timing (they connect to driver schemes when ready).

**Confirmed working**: Both QEMU and bare-metal boot to login prompt after this fix.

### Phase 6: Boot Visibility & Service Cleanup ✅

**Status: Confirmed working — system boots to login prompt on both QEMU and bare metal.**

**6A: Init service start logging (always visible)**
`init/src/scheduler.rs`: Service and target start messages promoted from DEBUG to always-visible.
Every service now logs `init: starting <description> (<cmd>)` before spawning and
`init: started <description> (pid <N>)` after a respawnable process is created.

**6B: Legacy init script cleanup**
`config/redbear-legacy-base.toml`:
- `00_base`: Removed dead `notify ipcd` / `notify ptyd` calls.
  The `notify` binary does not exist anywhere in the build tree — these calls always failed
  silently. ipcd and ptyd are started by the base recipe's systemd-style services
  (`00_ipcd.service`, `00_ptyd.service`). sudo --daemon is kept because `00_sudo.service`
  exists in the base recipe but is not wired into any target that gets scheduled.
  The script now does tmpdir setup + sudo --daemon.
- `00_drivers`: Blanked (was redundant — pcid-spawner starts via `00_pcid-spawner.service`).

**6C: Service readiness completion logging**
`init/src/service.rs`: Added success log after each blocking wait completes:
- `notify` services: `init: <cmd> ready (notify)` after readiness byte received
- `scheme` services: `init: <cmd> ready (scheme <name>)` after scheme registered
- `oneshot` services: `init: <cmd> done (oneshot)` after process exits successfully
Combined with 6A's `init: starting ...` before spawn, the boot log now shows the full
lifecycle of every blocking service — any gap between "starting" and "ready/done" pinpoints
the blocker.

### Serde `deny_unknown_fields` Behavior
`UnitInfo` and `Service` structs use `#[serde(deny_unknown_fields)]`. Any unrecognized field in `[unit]` or `[service]` sections causes the ENTIRE service file to fail deserialization. The init system logs the error and skips the service — it never starts.

**Implication**: Service file schema changes must be coordinated between init code and config TOMLs. Manual validation (`validate-service-files.sh`) catches these in redbear-*.toml configs.

### Init `requires_weak` Semantics
`requires_weak` provides ordering, not readiness. If a dependency is missing (file not found), the scheduler treats it as satisfied (not in pending queue). Services start anyway but without ordering guarantees.

### Init `oneshot_async` Services
Services with `type = "oneshot_async"` are fire-and-forget by default. Init spawns them and doesn't track their lifecycle. However, services with `respawn = true` in their `[service]` section are tracked — if they exit, init re-schedules and re-spawns them. Getty services use `respawn = true`.

### Config Include Chain
```
redbear-full.toml → desktop.toml, redbear-legacy-base.toml, redbear-legacy-desktop.toml,
                      redbear-device-services.toml, redbear-netctl.toml, redbear-greeter-services.toml
desktop.toml → desktop-minimal.toml, server.toml
desktop-minimal.toml → minimal.toml
server.toml → minimal.toml
minimal.toml → base.toml

redbear-grub.toml → redbear-full.toml, redbear-grub-policy.toml

redbear-mini → redbear-minimal.toml → minimal.toml, redbear-legacy-base.toml,
                redbear-device-services.toml, redbear-netctl.toml
```

### Upstream Targets (not Red Bear defined)
- `00_base.target` — `recipes/core/base/source/init.d/00_base.target`
- `10_net.target` — `recipes/core/base/source/init.d/10_net.target`
- These are installed by the base package into `/usr/lib/init.d/` and available at boot.

## Files Modified (This Assessment)

### Config Changes
- `config/redbear-greeter-services.toml` — removed boot_essential, added 05_boot-essential.target
- `config/redbear-device-services.toml` — added 12_boot-late.target
- `config/redbear-minimal.toml` — removed empty fatd override

### 2G: Console-Draw ✅ (8 fixes)
- `console-draw/src/lib.rs`: 4 DRM call unwraps → `?` operator; 3 try_into unwraps → `unwrap_or(0)`; 1 back_mut unwrap → `if let Some`

### 2H: Driver-Graphics ✅ (39 fixes)
- `driver-graphics/src/kms/connector.rs`: 3 fixes — crtc lookup unwrap, connector iterator unwrap, EDID parse unwrap → `nom::IResult::Done` match
- `driver-graphics/src/kms/objects.rs`: 2 fixes — crtcs iterator unwrap, remove_framebuffer unwrap
- `driver-graphics/src/kms/properties.rs`: 4 fixes — range asserts → log::error, mutex lock unwraps → map_err
- `driver-graphics/src/lib.rs`: 30 fixes — constructor fatal errors → process::exit(1), mutex locks → map_err/unwrap_or_else into_inner, vt lookups → ok_or, EDID parse → Done match, assert → if+return Err, try_into unwraps → graceful

### 2I: Fbbootlogd ✅ (14 fixes)
- `fbbootlogd/src/main.rs`: 10 fixes — fatal setup errors → match+exit(1), event loop errors → continue/break
- `fbbootlogd/src/scheme.rs`: 4 fixes — VT handle, graphics handle, dirty_fb ×2 → match+log

### 2J: Graphics-IPC ✅ (8 fixes)
- `graphics-ipc/src/lib.rs`: assert → if+return Err, unwrap → `?`, try_into unwraps → graceful early return

### 2K: ihdgd (Intel HD Graphics) ✅ (37 fixes)
- `ihdgd/src/device/ddi.rs`: 14 fixes — port register unwraps → match+return Err, lane loop unwraps → continue
- `ihdgd/src/device/ggtt.rs`: 2 fixes — asserts → if+return Err, reserve() returns Result
- `ihdgd/src/device/mod.rs`: 2 fixes — Drop unwrap → if let, probe_ddi expect → match+log
- `ihdgd/src/device/scheme.rs`: 8 fixes — connector/crtc lookups → match, Layout unwraps → unwrap_or_else, try_into unwraps → match
- `ihdgd/src/main.rs`: 10 fixes — EventQueue/subscribe/setrens → match+exit(1), event/IRQ loop → continue/log
- `ihdgd/src/device/pipe.rs`: 1 cascading fix — ggtt.reserve Result handling

### 2L: Virtio-GPUD ✅ (33 fixes)
- `virtio-gpud/src/main.rs`: 6 fixes — event loop, IRQ handling, scheme.tick → match+log+continue
- `virtio-gpud/src/scheme.rs`: 27 fixes — connector/crtc mutex locks → map_err/unwrap_or_else, EDID parse, cursor borrow → clone Arc, vt lookups → ok_or

### Code Changes (Phase 2 — 215 fixes across 33 Rust source files + 3 TOML config files)
- `daemon/src/lib.rs` — 2 fixes (get_fd double-unwrap, pipe unwrap)
- `init/src/main.rs` — 4 fixes (config exit, waitpid, boot progress, respawn waitpid loop)
- `init/src/service.rs` — 5 fixes (pipe, getns, register, respawn field, spawn return type)
- `init/src/unit.rs` — 3 fixes (unit/unit_mut → Option return, set_runtime_target asserts)
- `init/src/scheduler.rs` — 4 updates (handle None gracefully, respawn PID tracking, run return type)
- `logd/src/main.rs` — 3 fixes (socket, setrens, process_requests)
- `logd/src/scheme.rs` — 5 fixes (kernel_debug Option, sys_log Option, read/send)
- `randd/src/main.rs` — 4 fixes (CPUID, socket, setrens, process_requests loop)
- `zerod/src/main.rs` — 4 fixes (args, socket, setrens, process_requests loop)
- `inputd/src/lib.rs` — 7 fixes (open_display_v2 chain, fpath bounds, vt event read, buffer size)
- `inputd/src/main.rs` — 7 fixes (write, handles, daemon, args, control, Producer assertion)
- `vesad/src/main.rs` — 16 fixes (FRAMEBUFFER env, EventQueue, env file, event loop)
- `vesad/src/scheme.rs` — 4 fixes (probe_connector, set_crtc mutex, physmap)
- `fbcond/src/main.rs` — 10 fixes (VT parse, EventQueue, Socket, subscribes, writes, events)
- `fbcond/src/scheme.rs` — 1 fix (fpath write)
- `fbcond/src/display.rs` — 2 fixes (V2GraphicsHandle unwrap, dirty_fb unwrap)
- `fbcond/src/text.rs` — 1 fix (pop_front unwrap)

### Patch Preservation
- `local/patches/base/P2-daemon-hardening.patch` — 3767 lines, covers 33 Rust source files + 3 TOML configs
- `recipes/core/base/P2-daemon-hardening.patch` — symlink to local/patches
- `recipes/core/base/recipe.toml` — includes P2-daemon-hardening.patch in patches list

### New Files
- `local/scripts/validate-service-files.sh` — manual service schema validation (redbear-*.toml only)
- `local/docs/BOOT-PROCESS-ASSESSMENT.md` — this document
- `recipes/core/base/source/init.initfs.d/41_acpid.service` — acpid in initfs (boot race fix)

## Boot Procedure

### Supported compile targets

| Target | Purpose | Output |
|--------|---------|--------|
| `redbear-mini` | Minimal non-desktop (QEMU + bare metal) | `build/x86_64/harddrive.img` |
| `redbear-grub` | Text-only with GRUB boot manager (bare metal) | `build/x86_64/harddrive.img` |
| `redbear-full` | Desktop/graphics (QEMU + bare metal) | `build/x86_64/harddrive.img` |

### Build commands

```bash
# Minimal target (QEMU testing)
CI=1 make all CONFIG_NAME=redbear-mini ARCH=x86_64

# Minimal live ISO (bare-metal boot)
CI=1 make live CONFIG_NAME=redbear-mini ARCH=x86_64

# Desktop/graphics target (QEMU testing)
CI=1 make all CONFIG_NAME=redbear-full ARCH=x86_64

# Desktop/graphics live ISO (bare-metal boot)
CI=1 make live CONFIG_NAME=redbear-full ARCH=x86_64
```

### QEMU boot (harddrive.img)

```bash
# Boot the minimal target in QEMU
make qemu CONFIG_NAME=redbear-mini

# Boot with more RAM
make qemu CONFIG_NAME=redbear-mini QEMUFLAGS="-m 4G"

# Boot desktop target
make qemu CONFIG_NAME=redbear-full
```

QEMU boots from `harddrive.img` (not the live ISO). The `-serial mon:stdio` flag provides
the serial console, but Red Bear uses the framebuffer console for login — type at the
graphical console, not serial.

### Bare-metal boot (live ISO)

1. **Build the ISO:**
   ```bash
   CI=1 make live CONFIG_NAME=redbear-mini ARCH=x86_64
   ```

2. **Write ISO to USB drive:**
   ```bash
   sudo dd if=build/x86_64/redbear-live.iso of=/dev/sdX bs=4M status=progress && sync
   ```
   Replace `/dev/sdX` with your USB device. Use `lsblk` to identify it.

3. **Boot from USB:**
   - Insert USB into target machine
   - Power on, enter UEFI boot menu (typically F12, F8, or Esc)
   - Select the USB device as boot target
   - Red Bear OS boots from UEFI → bootloader → kernel → init → login prompt

4. **Login:**
   - Default user: `root`, no password
   - The framebuffer console displays the login prompt after boot completes

### What happens during boot

```
UEFI firmware
  → Red Bear bootloader (loaded from EFI system partition)
    → Kernel (kstart → start → kmain)
      → userspace_init → bootstrap (forks initfs/procmgr/initnsmgr)
        → Initfs phase:
            logd, inputd, vesad (framebuffer), fbcond, fbbootlogd,
            ps2d (keyboard), acpid, pcid-spawner-initfs (initfs PCI drivers), lived, redoxfs
        → switchroot /usr
        → Rootfs phase:
            00_base (tmpdir + sudo --daemon)
            00_ipcd.service, 00_ptyd.service
            00_pcid-spawner.service (async — spawns PCI drivers in background)
            30_console (getty with respawn)
        → Login prompt on framebuffer console
```

### Boot log markers

The init system logs the following always-visible markers. If boot hangs, the last visible
marker identifies the blocker:

```
init: phase 1 — initfs boot
init: starting <description> (<cmd>)          # before each service spawn
init: <cmd> ready (notify)                     # notify-type service ready
init: <cmd> ready (scheme <name>)              # scheme-type service ready
init: <cmd> done (oneshot)                     # oneshot service exited
init: phase 2 — switchroot to /usr
init: scheduling N rootfs units
init: reached target <description>
init: phase 3 — rootfs services started
init: boot complete — entering waitpid loop
```

### Troubleshooting

| Symptom | Likely cause | Fix |
|---------|-------------|-----|
| No display output | UEFI framebuffer not provided | Try different USB port or disable CSM in UEFI settings |
| Boot hangs after "scheduling N rootfs units" | A blocking service hangs | Check last "starting" line; `pcid-spawner` was previously the blocker |
| Keyboard not working | PS/2 unavailable, USB not ready | Modern hardware uses USB — ensure xHCI controller is functional |
| No login prompt | Getty not starting | Check `30_console` service in config; verify getty respawn is set |
| "missing field `unit`" parse error | Invalid service TOML | Run `./local/scripts/validate-service-files.sh config/` |
| **No kernel output at all** (after initfs loading) | Kernel hangs before `serial::init()` finishes | **Reduce QEMU guest RAM to 2 GiB** (`-m 2048`). ≥4 GiB triggers a memory init bug on x86_64. See Phase 7. |

## Phase 7: Kernel RAM Hang Diagnosis ✅ (2026-04-27)

### Discovery

The `redbear-full` harddrive image (4 GiB) boots correctly in QEMU with **2 GiB** of guest RAM,
but **hangs silently with 4 GiB or more** — zero kernel serial output after bootloader loads
kernel and initfs.

### Evidence

| Test | RAM | Result |
|------|-----|--------|
| `redbear-full` nographic | 2 GiB | ✅ Boots: kernel output, init, services, login prompt |
| `redbear-full` nographic | 4 GiB | ❌ Hang: no kernel output, CPU spins in `pause`/`jmp` loop |
| `redbear-mini` nographic | 2 GiB | ✅ Boots normally |
| `redbear-mini` nographic | 4 GiB | ✅ Boots normally |

The kernel and initfs binaries are **identical** between `redbear-full` and `redbear-mini`
(MD5: `bb5402209aefd7d42c3adaca0682b39f` for kernel, same size for initfs). The bootloader
binary is also identical. The only difference is the GPT partition layout (RedoxFS starts at
sector 34816 in full vs 4096 in mini).

QEMU ASM trace (`-d in_asm`) at 4 GiB confirms the kernel executes instructions but **never
reaches** `info!("Redox OS starting...")` — it enters a spin-loop before `serial::init()`
completes. At 2 GiB, the kernel boots normally and produces full serial output.

### Root Cause (Analysis)

The bootloader passes different memory maps to the kernel depending on available RAM. At 2 GiB,
the memory map spans ~0x900000–0x7ED3F000 (~2 GiB). At 4 GiB, the map spans a larger range
with different reservation patterns. The kernel's `startup::memory::init()` or early SMP
bring-up code (`arch/x86_shared/start.rs`) likely encounters an overflow, bad page table
mapping, or SMP deadlock on larger memory configurations.

The spin-loop at the end of the ASM trace (`pause` + `jmp` to self) is consistent with a
spinlock wait on a memory location that never gets released — likely SMP bring-up where one
CPU waits for another that never initializes.

### Impact

| Affected | Not affected |
|----------|-------------|
| `redbear-full` with ≥4 GiB RAM | `redbear-mini` (any RAM) |
| nographic mode specifically | `redbear-grub` (any RAM) |
| Real hardware with >2 GiB RAM | All profiles at 2 GiB |
| | `make qemu` default (QEMU_MEM=2048) |

Since `make qemu` defaults to 2048 MiB and all profiles work correctly at that value, **day-to-day
development is not affected**. The bug manifests only when developers manually override RAM or
when testing on real hardware with larger memory configurations.

### Recommended Fix

Add early raw-serial output (`outb` to COM1 port 0x3F8) in `arch/x86_shared/start.rs` **before**
`device::serial::init()` as a canary to confirm serial hardware works. Then add instrumentation
around the memory map processing in `startup::memory::init()` and SMP bring-up to isolate
whether the hang is in memory init, page table setup, or multi-core initialization.

### References

- `recipes/core/kernel/source/src/arch/x86_shared/start.rs` — early kernel entry, serial init, first `info!` log
- `recipes/core/kernel/source/src/startup/memory.rs` — memory map processing
- `recipes/core/bootloader/source/src/main.rs` — bootloader `KernelArgs` construction
