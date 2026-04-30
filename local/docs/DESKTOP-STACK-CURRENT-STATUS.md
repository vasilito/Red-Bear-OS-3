# Red Bear OS Desktop Stack — Current Status

**Last updated:** 2026-04-29
**Canonical plan:** `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` (v3.0)
**Boot improvement plan:** `local/docs/BOOT-PROCESS-IMPROVEMENT-PLAN.md` (v1.1)
**Source archival policy:** `local/docs/SOURCE-ARCHIVAL-POLICY.md` (v1.0)

## Recent Changes (2026-04-29, Wave C)

- **Validation infrastructure consistency refresh**:
  - `redbear-phase4-kde-check` now detects resolved KF6 shared-library version suffixes in addition to checking library presence, session entry points, and KDE runtime markers.
  - `redbear-phase5-gpu-check` now detects GPU PCI vendor/device identity from `/scheme/pci` before reporting firmware / Mesa / DRM preflight state.
  - `redbear-boot-check` now provides a bounded boot-surface checker for greeter-facing service ordering, DRM readiness, compositor socket creation, and greeter hello-protocol health.

## Recent Changes (2026-04-29, Wave 7)

- **KF6 surface made more honest for Phase 3/4**:
  - `kf6-kdeclarative` is enabled in `config/redbear-full.toml` (real reduced cmake build with `BUILD_WITH_QML=OFF`).
  - `kf6-kio` is blocked (commented out — source-incompatible).
- `kf6-knewstuff` and `kf6-kwallet` have real cmake builds; `kf6-kwallet` enabled, `kf6-knewstuff` blocked (empty package).
- Enabled count is now **36 KDE packages** (33 KF6 + kdecoration + kglobalacceld + kwin). See Current KDE Package Status table for full breakdown.

## Recent Changes (2026-04-29, Wave 6)

- **Phase 2/3 validation infrastructure**: Added bounded runtime checkers and harnesses for the next two desktop plan gates.
  - `redbear-phase2-wayland-check`: Validates Wayland socket visibility, compositor process presence, bounded `wl_display.get_registry` connectivity, `/usr/lib/libEGL.so`, `/usr/lib/libGBM.so`, software-renderer evidence, and the optional `qt6-wayland-smoke` binary.
  - `test-phase2-runtime.sh`: Automated `--guest` and `--qemu` Phase 2 harness using explicit binary checks plus exit-code-based expect markers.
  - `redbear-phase3-kwin-check`: Validates `kwin_wayland`/`redbear-compositor` presence, `DBUS_SESSION_BUS_ADDRESS`, `dbus-send --session`, `/run/seatd.sock`, active `WAYLAND_DISPLAY`, and a bounded `wl_display.sync` roundtrip.
  - `test-phase3-runtime.sh`: Automated `--guest` and `--qemu` Phase 3 harness using explicit binary checks plus exit-code-based expect markers.
  - Both binaries are wired into `redbear-hwutils` Cargo packaging and the recipe staged-file list.

## Recent Changes (2026-04-29, Wave 5)

- **Phase 1 runtime validation infrastructure**: Added service presence probes and check binaries for the Phase 1 desktop substrate.
  - `redbear-info --probe`: New output mode that probes Phase 1 service presence (evdevd, udev-shim, firmware-loader, redox-drm, time) via scheme enumeration. Reports per-service presence + summary line ("ALL PHASE 1 SERVICES PRESENT", "MOSTLY PRESENT, SOME GAPS", "SIGNIFICANT GAPS REMAIN").
  - `redbear-phase1-evdev-check`: Validates evdev scheme enumeration, event device readability, EV_KEY/EV_REL event semantics.
  - `redbear-phase1-udev-check`: Validates udev-shim device enumeration, keyboard/pointer/DRM device counts.
  - `redbear-phase1-firmware-check`: Validates firmware scheme registration, blob listing, blob reading with fallback key attempts.
  - `redbear-phase1-drm-check`: Validates DRM scheme registration, card enumeration, connector/mode queries.
  - `relibc-phase1-tests`: Six C POSIX test programs (signalfd, timerfd, eventfd, shm_open, sem_open, waitid) exercising relibc compatibility layers as real consumers would.
  - `test-phase1-runtime.sh`: Automated QEMU validation script with --guest and --qemu modes.
  - All changes in local/ (durable, survives upstream refresh).
  - `relibc-phase1-tests` wired into `config/redbear-full.toml`.
- **relibc-phase1-tests recipe**: Cross-compiles with Redox toolchain, installs to `/home/user/relibc-phase1-tests/` in guest filesystem.

## Recent Changes (2026-04-29, Wave 4)

- **Daemon INIT_NOTIFY panic fixed**: `P0-daemon-fix-init-notify-unwrap.patch` — replaced two `unwrap()` calls in base `daemon/src/lib.rs` (`get_fd` and `ready`) with graceful error handling. Fix survives clean source re-fetch via recipe `patches = [...]`.
- **Bootstrap workspace fix**: `P0-workspace-add-bootstrap.patch` — added `bootstrap` to workspace members in base `Cargo.toml` so initfs builds succeed.
- **Broken P2 base patches removed**: 23 broken upstream P2 patches removed from `recipes/core/base/recipe.toml` — they could not apply to current source revision and blocked fresh fetches.
- **Compositor protocol fix**: Fixed swapped size/opcode field parsing in `redbear-compositor` `dispatch()` — Wayland wire format `[size:u16][opcode:u16]` was reversed. Compositor now correctly identifies message types.
- **Compositor binary finding fix**: Wrapper script now uses `/usr/bin/redbear-compositor` (full path) to avoid PATH issues when running as `greeter` user.
- **messagebus group**: Added `[groups.messagebus]` to `config/redbear-full.toml` (gid=100, members=["messagebus"]) — D-Bus was failing to find the messagebus group.
- **Live ISO built**: `build/x86_64/redbear-full.iso` (4.0G) and `build/x86_64/redbear-full.img` built successfully with full D-Bus + Qt6 + greeter stack.
- **Source archival policy**: `local/docs/SOURCE-ARCHIVAL-POLICY.md` — new canonical policy requiring versioned filenames and fully-patched sources in archives.
- **Sources export**: 152 patches (29 recipe + 123 local) plus 40 source tarballs exported to `sources/x86_64-unknown-redox/`.

### Known Remaining Issues (Wave 4)

- **Qt Wayland shell integration**: Compositor correctly parses protocol now, but Qt6's Wayland plugin reports "Loading shell integration failed" and falls back to redox platform plugin. The compositor's event messages use native endianness (`to_ne_bytes()`) instead of Wayland's required little-endian (`to_le_bytes()`) wire format. Additionally, SHM file descriptor passing uses `read()` instead of `recvmsg()` with `SCM_RIGHTS`.
- **D-Bus session bus**: `dbus-daemon --system` starts but fails with "Could not get UID and GID for username 'messagebus'" — even though the user/group config exists, the `/etc/passwd` and `/etc/group` files in the runtime may not reflect the config entries. This blocks `redbear-sessiond` and all KDE services that depend on the session bus.
- **KF6 enablement**: superseded — see Current KDE Package Status table above. 36 KDE packages enabled, 12 blocked with documented reasons.

## Recent Changes (2026-04-28, Wave 3)

- **Bounded Wayland compositor proof** (`redbear-compositor`): 788-line Rust compositor replaces KWin stubs. Self-consistent protocol dispatch (wl_display, wl_compositor, wl_shm, wl_shell, xdg_wm_base, wl_seat). Zero warnings. 3/3 tests pass. Known limitations: SHM fd passing uses payload bytes (not Unix SCM_RIGHTS), framebuffer compositing uses private memory (not real vesad), wire encoding uses NUL-terminated strings (not padded Wayland format). Cross-compiles for Redox target. Not yet a real compositor runtime proof — bounded scaffold only.
- **DRM backend wired**: `KWIN_DRM_DEVICES=/scheme/drm/card0` wired through greeter chain in config. Runtime verification pending.
- **Intel GPU Gen8-Gen12**: Expanded from Gen12-only to Gen8-Gen12 with firmware keys (SKL/KBL/CNL/ICL/GLK/RKL/DG1/TGL/ADLP/DG2/MTL/ARL/LNL/BMG). 200+ device IDs from Linux 7.0 i915.
- **VirtIO GPU driver**: New 220-line DRM/KMS backend in redox-drm for QEMU testing.
- **Kernel 4GB RAM fix**: MEMORY_MAP overflow at 512 entries → 1024. Verified with canary chain.
- **Live ISO preload**: Capped at 1 GiB with partial preload messaging.
- **Boot daemons**: dhcpd auto-detects interface. I2C decode hardened with retry.
- **Qt6 toolchain**: `-march=x86-64 -fpermissive` for CPU compatibility and header fixes.
- **Greeter diagnostics**: Startup progress logging, QML crash-specific diagnostics.

## Purpose

This document is the **current build/runtime truth summary** for the Red Bear desktop stack.

Its job is to answer:
- what the desktop stack actually builds,
- what the tracked profiles currently expose,
- what is only build-visible,
- what is runtime-proven,
- and what still blocks a trustworthy Wayland/KDE session claim.

For the execution plan (phases, timelines, acceptance criteria), see the canonical plan above.
For subsystem planning detail, see `local/docs/WAYLAND-IMPLEMENTATION-PLAN.md`; for historical KDE rationale, see `docs/05-KDE-PLASMA-ON-REDOX.md`.

## Where We Are in the Plan

The canonical desktop plan uses a three-track model:

- **Track A (Phase 1–2):** Runtime Substrate → Software Compositor — **Phase 1 active probe and check binaries are now implemented; runtime validation in a live environment remains the exit gate**
- **Track B (Phase 3–4):** KWin Session → KDE Plasma — **blocked on Track A**
- **Track C (Phase 5):** Hardware GPU — **can start after Phase 1**

**Current position:** Build-side gates are crossed. Phase 1 (Runtime Substrate Validation) is still
the next broad desktop target, but the repo now also carries an experimental Red Bear-native
greeter/auth/session-launch stack on the `redbear-full` desktop path.

## Active Target Surface and Evidence Boundary

- The supported compile targets are `redbear-mini`, `redbear-full`, and `redbear-grub`.
- Desktop/graphics are available only on `redbear-full`.
- Older names such as `redbear-kde`, `redbear-wayland`, `redbear-minimal*`, `redbear-live-mini`,
  and `redbear-live-full` still appear in historical or staging material, but they are not the
  supported compile-target surface.
- The greeter/login path is currently an **experimental build/integration surface** on `redbear-full`;
  it is not yet a runtime-validated end-to-end desktop-login claim.

## Status Matrix

| Area | Evidence class | Detail |
|---|---|---|
| `libwayland` | **builds** | relibc/Wayland-facing compatibility is materially stronger; 33 patches verified (was 25): signalfd, timerfd, eventfd, pthread_yield, secure_getenv, getentropy, dup3, vfork, clock_nanosleep, named-semaphores, tls-get-addr-panic-fix, fcntl-dupfd-cloexec, ipc-tests, socket-flags, syscall-0.7.4-procschemeattrs-ens-to-prio, sysv-ipc, sysv-sem-impl, sysv-shm-impl, waitid-header, open_memstream, F_DUPFD_CLOEXEC, MSG_NOSIGNAL, waitid, RLIMIT, eth0 networking, shm_open, sem_open, select-not-epoll-timeout, exec-root-bypass, tcp-nodelay, netdb-lookup-retry-fix, eventfd-mod, fd-event-tests, ifaddrs-net_if, signalfd-header, elf64-types, socket-cred, strtold-cpp-linkage, semaphore-fixes |
| Qt6 core stack | **builds** | `qtbase` (7 libs + 12 plugins), `qtdeclarative`, `qtsvg`, `qtwayland`; Qt6Quick/JIT not runtime-proven |
## Current KDE Package Status (2026-04-30)

| Category | Count | Detail |
|----------|-------|--------|
| **Building + in repo** | 13 | PKGAR artifacts: attica, ECM, karchive, kauth, kconfig, kcoreaddons, kcrash, kdbusaddons, kglobalaccel, ki18n, kwidgetsaddons, kwindowsystem, kwin |
| **Building (stage only)** | 23 | kdecoration, kwallet + 21 other KF6 frameworks — compiled during cook, need full make all to push to repo |
| **Attica (new)** | — | Minimal core library build, KF6::Attica cmake target (counted in Building+repo above) |
| **Blocked: QML gate** | 1 | kirigami — source includes QQuickWindow/QQmlEngine unconditionally |
| **Blocked: compilation** | 2 | breeze, kf6-kio — upstream source incompatibilities with Redox toolchain |
| **Blocked: transitive** | 3 | plasma-framework (needs kirigami), plasma-workspace (needs kf6-knewstuff payload), plasma-desktop (needs plasma-workspace) |
| **Blocked: Qt6::Sensors** | 1 | kwin real build (current stub delegates to redbear-compositor) |
| **Blocked: source-incompatible** | 1 | kde-cli-tools (depends on kf6-kio) |

**Total: 48 recipes. 36 build (13 in repo + 23 stage-only). 12 blocked (documented).**

Recipe versions: KF6 frameworks v6.10.0, Plasma v6.3.4, Attica v6.10.0, KWin v6.3.4 (stub). All versions are current upstream releases as of 2026-04-30.
| KWin | **stub** | cmake config stub + wrapper scripts delegating to redbear-compositor; real build requires Qt6Quick/QML downstream proof |
| plasma-workspace | **blocked (builds real)** | Real cmake build, but commented out in config; depends on kf6-knewstuff + kwin |
| plasma-desktop | **blocked (builds real)** | Recipe exists; depends on plasma-workspace; commented out in config |
| Mesa EGL+GBM+GLES2 | **builds** | Software path via LLVMpipe proven in QEMU; hardware path not proven |
| libdrm amdgpu | **builds** | Package-level success only |
| Input stack | **builds, enumerates** | evdevd (65 tests), udev-shim, seatd present; libinput builds but suppressed in config (`libinput = "ignore"`); libevdev commented out; end-to-end compositor input path unproven |
| D-Bus | **builds, bounded (in improvement)** | System bus wired in `redbear-full`; session bus incomplete; Phase 3/4 improvement plan active; completeness: login1.Manager ~10%, login1.Session ~47%, login1.Seat ~20%, Notifications ~80%, UPower ~60%, UDisks2 ~50%, PolicyKit1 ~50%; `StatusNotifierWatcher` is the new service being added in Phase 4 |
| redbear-sessiond | **builds, scaffold (Phase 3/4 improvement active)** | org.freedesktop.login1 D-Bus session broker — Rust daemon (zbus 5), wired on the `redbear-full` desktop path; Phase 3 hard gate is TakeDevice FD passing plus PauseDevice/ResumeDevice signal emission; Priority 1 in Phase 3/4 improvement plan |
| redbear-authd | **builds** | Privileged local-user auth daemon; `/etc/passwd`/`/etc/shadow`/`/etc/group` parsing, SHA-256/SHA-512 crypt verification, bounded lockout, target-side recipe build proven |
| redbear-session-launch | **builds** | User-session bootstrap tool; runtime-dir/env setup, uid/gid handoff, dbus-run-session → `redbear-kde-session`, target-side recipe build proven |
| redbear-greeterd | **builds, experimental** | Root-owned greeter orchestrator; UI/auth socket protocol, bounded restart policy, return-to-greeter daemon logic, crate tests pass; end-to-end runtime proof still pending |
| redbear-greeter UI | **builds, experimental** | Qt6/QML unprivileged login surface now ships in-tree; bounded runtime proof remains narrower than a full trusted KDE desktop-login claim |
| TUI login fallback | **builds, boots** | `29_activate_console.service` now owns VT3 activation for `30_console.service` and `31_debug_console.service`, keeping VT2/ debug fallback consoles independent of `20_greeter.service` success |
| redbear-validation-session | **builds, bounded helper** | Still staged as a validation launcher/helper, but no longer the primary `redbear-full` display-service owner |
| Greeter runtime checker | ✅ implemented (bounded checker) | `redbear-greeter-check` asserts greeter binaries, assets, service files, socket reachability, hello protocol, invalid-login handling, and a validation-only successful-login/session-return loop inside the guest |
| Greeter QEMU harness | ✅ implemented (bounded harness) | `test-greeter-qemu.sh` boots `redbear-full`, logs in on the fallback console, and now passes the in-guest greeter checker for hello, invalid-login, and bounded successful-login return-to-greeter proof |
| redbear-notifications | ✅ Scaffold | org.freedesktop.Notifications — logs to stderr, no display integration yet |
| redbear-upower | ⚠️ scaffold / experimental | org.freedesktop.UPower — service exists, and the backing `/scheme/acpi/power` surface now performs real AML-backed enumeration, but its bootstrap preconditions and runtime proof are still too weak to call release-grade or consumer-validated; treat current enumeration as provisional until Wave 3 in `local/docs/ACPI-IMPROVEMENT-PLAN.md` closes |
| redbear-udisks | ✅ bounded real | org.freedesktop.UDisks2 — enumerates real `disk.*` schemes and partitions into read-only D-Bus objects; no fabricated mount/serial metadata |
| Phase 5 D-Bus runtime proof | ✅ implemented (bounded QEMU proof) | `redbear-phase5-network-check` + `test-phase5-network-qemu.sh` assert bounded QEMU service registration and current runtime plumbing on `redbear-full`; treat UPower as provisional until the ACPI power surface is made honest in `local/docs/ACPI-IMPROVEMENT-PLAN.md` Wave 3 |
| Phase 6 Solid readiness proof | ✅ implemented, blocked | `redbear-phase6-kde-check` + `test-phase6-kde-qemu.sh` now distinguish real Solid validation from blocked states; `kf6-solid` remains disabled until runtime proof + tooling are present |
| redbear-polkit | ✅ Scaffold | org.freedesktop.PolicyKit1 — always-permit authorization; KAuth still uses FAKE backend because PolkitQt6-1 is not packaged yet |
| redbear-dbus-services | ✅ Created | D-Bus activation files + policies staged |
| DRM/KMS | **builds** | redox-drm scheme daemon; 68 unit tests (KMS, GEM, PRIME, wire structs, scheme pure logic); no hardware runtime validation |
| GPU acceleration | **blocked** | PRIME/DMA-BUF ioctls and bounded private CS surface implemented; real vendor render CS/fence path still missing |
| validation compositor runtime | **experimental bounded scaffold** | Self-consistent protocol dispatch; 3/3 tests pass; known gaps: SHM fd passing, wire encoding, framebuffer compositing; not a real client-compatible compositor runtime proof |
| validation profile | **builds, boots** | Bounded Wayland runtime profile |
| `redbear-full` profile | **builds, boots** | Active desktop/graphics compile surface; now owns the experimental greeter/auth/session-launch integration path |
| `redbear-grub` profile | **builds** | Text-only with GRUB chainload for bare-metal multi-boot |
| `redbear-mini` profile | **builds** | Minimal non-desktop compile target |
| `redbear-hwutils` | **builds** | lspci/lsusb + Phase 1 check tools; 79 unit tests (12 cfg-gated Redox-only); zero warnings; 4 Phase 1 check binaries (evdev, udev, firmware, DRM) |
| `redbear-info --probe` | **builds** | Phase 1 service presence probes (evdevd, udev-shim, firmware-loader, redox-drm, time); reports PRESENT/ABSENT with summary; exits non-zero on gaps; 5 unit tests; bidirectional `--probe`/`--json`/`--test`/`--quirks` mutual exclusivity |
| `relibc-phase1-tests` | **builds** | Six C POSIX tests (signalfd, timerfd, eventfd, shm_open, sem_open, waitid); cross-compiled for Redox |
| `test-phase1-runtime.sh` | **builds** | Automated QEMU validation (--guest/--qemu modes) for Phase 1 substrate |
| `redbear-phase2-wayland-check` | **builds** | Phase 2 compositor proof checker: socket/process visibility, bounded `wl_display.get_registry`, EGL/GBM presence, software-renderer evidence, and optional `qt6-wayland-smoke` presence |
| `test-phase2-runtime.sh` | **builds** | Automated guest/QEMU Phase 2 harness using explicit binary checks and exit-code-only pass/fail markers |
| `redbear-phase3-kwin-check` | **builds** | Phase 3 desktop session preflight: compositor binary presence, session-bus address + `dbus-send`, seatd socket, active `WAYLAND_DISPLAY`, and bounded `wl_display.sync` roundtrip (does not validate real KWin behavior) |
| `test-phase3-runtime.sh` | **builds** | Automated guest/QEMU Phase 3 harness using explicit binary checks and exit-code-only pass/fail markers |
| | | |
| **Phase 4 (KDE Plasma) — see canonical status table above** | | |
| KF6 frameworks | **36 build / 12 blocked** | See Current KDE Package Status table (lines 122-132) for exact breakdown |
| `plasma-workspace` | **blocked (transitive)** | Recipe exists, blocked by kf6-knewstuff empty package + kwin stub |
| `plasma-desktop` | **blocked (transitive)** | Recipe exists, blocked by plasma-workspace |
| `plasma-framework` | **blocked (QML gate)** | Recipe exists, blocked by kirigami |
| `kdecoration` | **builds** | Window decoration library — in repo |
| `kf6-kwayland` | **builds (stage)** | Qt/C++ Wayland protocol wrapper |
| `plasma-wayland-protocols` | **builds (stage)** | XML protocol definitions for kwayland/KWin |
| `kirigami` | **blocked: QML gate** | QQuickWindow/QQmlEngine headers don't exist on Redox |
| `kwin` | **stub (by design)** | cmake configs + kwin_wayland shim → redbear-compositor; real build needs Qt6::Sensors |
| `kf6-attica` | **builds (in repo)** | Minimal core library (2.4MB pkgar) — NEW |
| `test-phase4-runtime.sh` | **exists** | Phase 4 KDE Plasma preflight harness (guest + QEMU modes) |
| `test-phase5-gpu-runtime.sh` | **exists** | Phase 5 hardware GPU preflight harness (guest + QEMU modes) |
| `redbear-phase4-kde-check` | **builds** | Phase 4 KDE preflight: KF6 libraries plus resolved KF6 version suffixes, plasma binaries, session entry points, and kirigami status |
| `redbear-phase5-gpu-check` | **builds** | Phase 5 GPU preflight: DRM device, GPU PCI vendor/device detection, GPU firmware, Mesa DRI drivers, and display modes |
| `redbear-boot-check` | **builds** | Boot-surface preflight: `pcid-spawner`→greeter ordering contract, DRM device readiness, compositor socket creation, and greeter hello-protocol health |
| | | |
| **Phase 5 (Hardware GPU) — driver scaffold** | | |
| `redox-drm` | **builds** | DRM scheme daemon with Intel Gen8-Gen12 + AMD device support and quirk tables; no hardware validation |
| `mesa` | **builds** | Software llvmpipe renderer; hardware renderers (radeonsi/iris) not cross-compiled |
| `amdgpu` | **compile triage** | Imported Linux AMD DC/TTM/core C port; bounded path compiles |
| `test-phase5-network-qemu.sh` | **exists** | Legacy Phase 5 network/session QEMU launcher (pre-v2.0 plan) |

## Profile View

### `redbear-full`

- **Role:** Active desktop/graphics compile target and current greeter-integration surface
- **Current truth:** Carries D-Bus, sessiond, broader integration pieces, and the experimental Red Bear-native greeter/auth/session-launch stack; VirtIO networking works in QEMU, the bounded Phase 5 network/session checker is evidence-backed there, and the repo now includes a bounded greeter checker/harness for the login surface. `redbear-validation-session` remains staged only as a bounded helper, not the active `20_display.service` owner on this target. TUI fallback (`30_console.service`/`31_debug_console.service`) is now triggered through `29_activate_console.service` and is decoupled from greeter success.
- **Use for:** Desktop integration testing, greeter/login bring-up, and bounded desktop/network plumbing validation
- **Do not overclaim:** This profile proves bounded QEMU desktop/network plumbing only. It does not by itself close the Wi-Fi implementation plan's later real-hardware Phase W5 reporting/recovery gate.

### `redbear-grub`

- **Role:** Text-only target with GRUB boot manager for bare-metal multi-boot
- **Current truth:** Follows `redbear-mini`; text-only with GRUB chainload ESP layout, no desktop/graphics
- **Use for:** Bare-metal multi-boot, recovery with GRUB menu, and install workflows requiring GRUB

### `redbear-mini`

- **Role:** Minimal non-desktop target
- **Current truth:** No desktop/graphics path; recovery and non-desktop integration surface only. TUI recovery is bound to VT activation through `29_activate_console.service` followed by `30_console.service`/`31_debug_console.service`.
- **Use for:** Minimal runtime bring-up, subsystem validation, and non-desktop packaging checks

## Current Blockers

### 1. Runtime trust trails build success (Phase 1 gate)

The repo has real build-visible desktop progress, but build success exceeds runtime confidence.
Phase 1 exists specifically to close this gap.
Phase 1 code implementation is build-verified complete (zero warnings, zero test failures, zero LSP errors). Active service probes and check binaries are in place (`redbear-info --probe`, `redbear-phase1-{evdev,udev,firmware,drm}-check`). Six C POSIX test programs for relibc compatibility layers are wired in the `relibc-phase1-tests` recipe. The automated QEMU test harness (`test-phase1-runtime.sh`) is complete and syntax-verified. Live-environment runtime validation remains pending (requires QEMU/bare metal, not available in current environment).

### 2. No complete compositor session (Phase 2 gate)

A bounded compositor initialization reaches early startup but does not complete a usable Wayland compositor session.
This blocks all desktop session work.
KWin is the sole intended compositor direction. No alternative (weston, wlroots) is in a working state. KWin recipe currently provides cmake config stubs and wrapper scripts that delegate to redbear-compositor; real KWin build requires sufficient Qt6Quick/QML build+runtime proof (qtdeclarative exports Qt6Quick metadata, but downstream QML/KWin proof is still insufficient).

### 3. Greeter/login path now exists, but runtime proof is still missing (desktop-login gate)

The repo now carries the main non-visual pieces of the Red Bear-native greeter/login plan:

- `redbear-authd`
- `redbear-session-launch`
- `redbear-greeterd`
- `redbear-greeter-services.toml`
- `redbear-greeter-check`
- `test-greeter-qemu.sh`

Current truth for that slice:

| Piece | Current state | Remaining limitation |
|---|---|---|
| `redbear-authd` | Target-side recipe build proven; unit tests cover passwd/shadow parsing, SHA-crypt and Argon2 verification, lockout, approval checks | Remaining risk is no longer auth-format handling, but broader desktop-session stability below the greeter slice |
| `redbear-session-launch` | Target-side recipe build proven; unit tests cover env/runtime-dir/argument handling, including current session environment contract | Remaining limitation is broader compositor/session stability, not the basic session-launch boundary |
| `redbear-greeterd` | Crate tests cover protocol-facing state strings, installed asset paths, bounded restart policy, and now own successful-login session launch directly after response delivery | Full desktop-login trust still depends on wider KDE runtime proof; the remaining instability is KWin compositor startup, not greeter/auth protocol wiring |
| Greeter validation helpers | `redbear-greeter-check` + `test-greeter-qemu.sh` exist and are wired for bounded runtime proof | The successful-login path is validation-only and does not replace broader KDE session proof, but the bounded QEMU greeter proof now passes |
| `redbear-greeter` packaging | Builds in-tree | Qt/QML UI binary, compositor wrapper, branded assets, and a shared login-protocol crate are present; Qt shared-plugin loading now works in the guest, while broader KWin runtime stability still remains experimental |

This means Red Bear now has a credible **bounded runtime-visible login boundary**, but not yet a
runtime-trusted general-purpose graphical login surface.

### 4. KWin recipe is a cmake stub; real KWin desktop-session proof requires Qt6Quick/QML

KWin recipe provides cmake config stubs and wrapper scripts (kwin_wayland, kwin_wayland_wrapper) that delegate to redbear-compositor. Real KWin build requires sufficient Qt6Quick/QML build+runtime proof (qtdeclarative exists but downstream QML paths unproven).

Current truth for that slice:

| Dependency | Current state | Remaining limitation |
|---|---|---|
| `libepoxy` | Real dependency | No blocker in this slice |
| `lcms2` | Real dependency | No blocker in this slice |
| `libudev` | Honest scheme-backed provider (`libudev.so`) | Hotplug monitoring remains bounded rather than full eudev parity |
| `libdisplay-info` | Honest bounded provider (`libdisplay-info.so`) | Base-EDID parsing only; CTA / DisplayID / HDR metadata remain unsupported |

Additionally, kirigami still needs more honest session-ready treatment. `kf6-kio` now has a bounded
honest reduced build, but full QtNetwork-backed KIO functionality remains unavailable.

### 5. Hardware acceleration missing GPU CS ioctl (Phase 5 gate)

PRIME/DMA-BUF buffer sharing is implemented at the scheme level, and a bounded private CS
surface now exists for shared-contract work. Real vendor render command submission and shared
fence semantics still do not exist. This still blocks hardware-accelerated rendering.

The repo now also carries a bounded in-guest display checker, `redbear-drm-display-check`, with
shell wrappers at `local/scripts/test-drm-display-runtime.sh`, `test-amd-gpu.sh`, and
`test-intel-gpu.sh`. It now covers direct connector/mode enumeration and bounded direct modeset
proof over the Red Bear DRM ioctl surface, but it is still only a runtime evidence tool until it is
exercised on real Intel and AMD hardware.

### 6. KDE Plasma session assembly blocked on QML stack (Phase 4 gate)

Kirigami is QML-gated (source unconditionally includes QQuickWindow/QQmlEngine). `kf6-kwallet` builds (API-only). `kf6-knewstuff` blocked (empty package — cmake succeeds, no libs produced). All status in Current KDE Package Status table above.
the KDE Plasma session. `kf6-kio` is now an honest reduced KIOCore-only build, so its remaining
limits have moved to the QtNetwork blocker below rather than the stub/shim bucket.

### 7. QtNetwork re-enabled for KDE network integration (2026-04-29)

QtNetwork has been re-enabled in the qtbase recipe (`-DFEATURE_network=ON`) following DNS resolver
hardening (use-after-free fix, FD leak fix, transaction ID validation, RCODE/TC handling, typed
error mapping via `P3-dns-resolver-hardening.patch`). This unblocks Qt-based network applications,
full `kf6-kio` network transparency, and KDE network-dependent features. Full build validation is
in progress.

### 8. Build system improvements completed

The build system has received targeted fixes that improve reliability:

| Component | Fix | Status |
|---|---|---|
| OnceLock panic | `get_or_init` pattern now used instead of direct `once_cell` access that could panic | Fixed |
| disk.mk error suppression | Meaningful error messages now surface instead of suppressed failures | Fixed |
| prefix.mk wget retry | Retry logic added: 3 tries with 30-second timeout | Fixed |

### 9. Init/config cleanup completed

Init service configuration has been streamlined:

- 10 unnecessary `ion -c` wrappers removed from `redbear-mini.toml` and `redbear-full.toml` (sessiond, upower, udisks, polkit, authd, echo, and others)
- D-Bus service retains `ion -c` wrapper (justified: requires shell chaining for proper daemonization)
- `redbear-login-protocol` recipe.toml created and symlinked into recipe search path
- `redbear-statusnotifierwatcher` symlinked into `recipes/system/`

## Canonical Document Roles

| Document | Role |
|---|---|
| `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` | Canonical desktop path plan (v3.0, full chain reassessment) |
| This document | Current build/runtime truth summary |
| `local/docs/DRM-MODERNIZATION-EXECUTION-PLAN.md` | Canonical GPU/DRM execution plan beneath the desktop path |
| `local/docs/QT6-PORT-STATUS.md` | Qt/KF6/KWin package-level build status |
| `local/docs/AMD-FIRST-INTEGRATION.md` | AMD-specific hardware/driver detail |
| `local/docs/WAYLAND-IMPLEMENTATION-PLAN.md` | Canonical Wayland subsystem plan |
| `docs/05-KDE-PLASMA-ON-REDOX.md` | Historical KDE design rationale |
| `local/docs/PROFILE-MATRIX.md` | Profile roles and support-language reference |

## Bottom Line

The Red Bear desktop stack has crossed major build-side gates and one important bounded runtime gate:
- All Qt6 core modules, all 32 KF6 recipes, Mesa EGL/GBM/GLES2, and D-Bus build — 36 KDE packages enabled in config (13 in repo with .pkgar, 23 stage-only)
- Three supported compile targets exist, with desktop/graphics on `redbear-full`
- the Red Bear-native greeter/login path now has a bounded passing QEMU proof (`GREETER_HELLO=ok`, `GREETER_INVALID=ok`, `GREETER_VALID=ok`) — but the greeter service is currently **disabled** in config (runs `/usr/bin/true` instead of `redbear-greeterd`)
- relibc compatibility is materially stronger than before
- Phase 1 test coverage is comprehensive: 300+ unit tests across all Phase 1 daemons (evdevd 65, udev-shim 15, firmware-loader 24, redox-drm 68, redbear-hwutils 79 host + 12 Redox-cfg-gated, bluetooth/wifi 209); service presence probes (`redbear-info --probe`) and 4 check binaries (`redbear-phase1-{evdev,udev,firmware,drm}-check`) validate Phase 1 substrate; 6 C POSIX tests (`relibc-phase1-tests`) exercise relibc compatibility layers
- KWin recipe is a **stub** — downloads real KWin v6.3.4 source but build script never compiles it; delegates to redbear-compositor via wrapper
- Critical blockers for Phase 4: KWin remains stub (needs Qt6::Sensors + libinput); kirigami QML-gated; 12 packages blocked total (see canonical status table above)

The remaining work is **platform prerequisite resolution** (QML JIT, Qt6::Sensors, libinput ports) before full KDE Plasma session can be assembled. Phase 1-2 runtime validation continues via QEMU.
Phase 1 (Runtime Substrate Validation) has comprehensive test coverage; the remaining gate is live-environment runtime validation. The key boundary for Phase 2 is: no compositor session proof exists. The key boundary for Phase 3-4 is: platform prerequisites (QML JIT, Qt6::Sensors, libinput) must be resolved before KWin real build and full Plasma session.
