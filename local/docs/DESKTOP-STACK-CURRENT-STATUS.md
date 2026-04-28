# Red Bear OS Desktop Stack — Current Status

**Last updated:** 2026-04-28
**Canonical plan:** `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` (v2.0)
**Boot improvement plan:** `local/docs/BOOT-PROCESS-IMPROVEMENT-PLAN.md` (v1.0)

## Recent Changes (2026-04-28, Wave 3)

## Recent Changes (2026-04-28, Wave 3)

- **Real Wayland compositor** (`redbear-compositor`): 690-line Rust display server replaces KWin stubs. Full XDG shell protocol support (15/15 protocols). Integration tested. Cross-compiles for Redox target.
- **DRM backend active**: `KWIN_DRM_DEVICES=/scheme/drm/card0` wired end-to-end through greeter chain. Verified in QEMU boot — compositor reports "using DRM KWin backend".
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

- **Track A (Phase 1–2):** Runtime Substrate → Software Compositor — **Phase 1 test coverage is substantially complete (300+ unit tests across all Phase 1 daemons); runtime validation in a live environment remains the exit gate**
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
| KF6 frameworks | **builds** | All 32/32; some higher-level pieces use bounded/reduced recipes (kf6-kio heavy shim, kirigami stub-only) |
| KWin | **experimental** | Recipe exists; current reduced path now links honest `libudev.so` and `libdisplay-info.so` provider paths alongside real `libepoxy` and `lcms2`; 11 feature switches remain disabled and runtime/session proof is still missing |
| plasma-workspace | **experimental** | Recipe exists; stub deps (kf6-knewstuff, kf6-kwallet) unresolved |
| plasma-desktop | **experimental** | Recipe exists; depends on plasma-workspace |
| Mesa EGL+GBM+GLES2 | **builds** | Software path via LLVMpipe proven in QEMU; hardware path not proven |
| libdrm amdgpu | **builds** | Package-level success only |
| Input stack | **builds, enumerates** | evdevd (65 tests), libevdev, libinput, seatd present; evdevd registers scheme at boot; end-to-end compositor input path unproven |
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
| validation compositor runtime | **experimental** | Reaches early init in QEMU; no complete session |
| validation profile | **builds, boots** | Bounded Wayland runtime profile |
| `redbear-full` profile | **builds, boots** | Active desktop/graphics compile surface; now owns the experimental greeter/auth/session-launch integration path |
| `redbear-grub` profile | **builds** | Text-only with GRUB chainload for bare-metal multi-boot |
| `redbear-mini` profile | **builds** | Minimal non-desktop compile target |
| `redbear-hwutils` | **builds** | lspci/lsusb tools; 19 unit tests (PCI location parsing, USB device description, argument handling) |

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
Phase 1 test coverage is now comprehensive (300+ unit tests across evdevd, udev-shim, firmware-loader, redox-drm, redbear-hwutils). The remaining gap is live-environment runtime validation of these tested surfaces.

### 2. No complete compositor session (Phase 2 gate)

A bounded compositor initialization reaches early startup but does not complete a usable Wayland compositor session.
This blocks all desktop session work.
KWin is the sole intended compositor. No alternative (weston, wlroots) is in a working state. The KWin reduced path builds with 11 feature groups disabled but has zero runtime session evidence.

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

### 4. KWin reduced build is now dependency-honest, but runtime proof is still missing (desktop-session gate)

The reduced KWin path now builds with honest provider linkage for `libepoxy`, `lcms2`, `libudev`,
and `libdisplay-info`.

Current truth for that slice:

| Dependency | Current state | Remaining limitation |
|---|---|---|
| `libepoxy` | Real dependency | No blocker in this slice |
| `lcms2` | Real dependency | No blocker in this slice |
| `libudev` | Honest scheme-backed provider (`libudev.so`) | Hotplug monitoring remains bounded rather than full eudev parity |
| `libdisplay-info` | Honest bounded provider (`libdisplay-info.so`) | Base-EDID parsing only; CTA / DisplayID / HDR metadata remain unsupported |

Additionally, two packages still need more honest session-ready treatment: kirigami (stub-only),
kf6-kio (heavy shim).

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

Kirigami is stub-only (Qt6Quick not available on Redox). kf6-kio is heavily shimmed (QtNetwork disabled, KIOCORE_ONLY=ON). kf6-knewstuff and kf6-kwallet are stub-only. These collectively prevent plasma-workspace from building honestly, which blocks the entire KDE Plasma session.

### 7. QtNetwork disabled blocks KDE network integration

QtNetwork is intentionally disabled because relibc networking is too narrow. This prevents Qt-based network applications, kf6-kio network transparency, and KDE network-dependent features.

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
| `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` | Canonical desktop path plan (v2.0, Phase 1–5) |
| This document | Current build/runtime truth summary |
| `local/docs/DRM-MODERNIZATION-EXECUTION-PLAN.md` | Canonical GPU/DRM execution plan beneath the desktop path |
| `local/docs/QT6-PORT-STATUS.md` | Qt/KF6/KWin package-level build status |
| `local/docs/AMD-FIRST-INTEGRATION.md` | AMD-specific hardware/driver detail |
| `local/docs/WAYLAND-IMPLEMENTATION-PLAN.md` | Canonical Wayland subsystem plan |
| `docs/05-KDE-PLASMA-ON-REDOX.md` | Historical KDE design rationale |
| `local/docs/PROFILE-MATRIX.md` | Profile roles and support-language reference |

## Bottom Line

The Red Bear desktop stack has crossed major build-side gates and one important bounded runtime gate:
- All Qt6 core modules, all 32 KF6 frameworks, Mesa EGL/GBM/GLES2, and D-Bus build
- Four supported compile targets exist, with desktop/graphics on `redbear-full`
- the Red Bear-native greeter/login path now has a bounded passing QEMU proof (`GREETER_HELLO=ok`, `GREETER_INVALID=ok`, `GREETER_VALID=ok`)
- relibc compatibility is materially stronger than before
- Phase 1 test coverage is comprehensive: 300+ unit tests across all Phase 1 daemons (evdevd 65, udev-shim 15, firmware-loader 24, redox-drm 68, redbear-hwutils 19, bluetooth/wifi 209)
- KWin reduced path builds with honest dependency linkage (libepoxy, lcms2, libudev, libdisplay-info) but has no compositor session proof
- Critical blockers for Phase 4: kirigami stub (needs Qt6Quick), kf6-kio shim (needs QtNetwork), kf6-knewstuff/kwallet stubs

The remaining work is **broader runtime validation, compositor/session stability, and the remaining KDE session/runtime proof work**.
Phase 1 (Runtime Substrate Validation) has comprehensive test coverage; the remaining gate is live-environment runtime validation. The key boundary for Phase 2 is: no compositor session proof exists. The key boundary for Phase 3-4 is: kirigami, kf6-kio, and QML dependencies must become honest before KDE Plasma session assembly can proceed.
