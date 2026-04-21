# Red Bear OS Desktop Stack — Current Status

**Last updated:** 2026-04-19
**Canonical plan:** `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` (v2.0)

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

- **Track A (Phase 1–2):** Runtime Substrate → Software Compositor — **Phase 1 is the current target**
- **Track B (Phase 3–4):** KWin Session → KDE Plasma — **blocked on Track A**
- **Track C (Phase 5):** Hardware GPU — **can start after Phase 1**

**Current position:** Build-side gates are crossed. Phase 1 (Runtime Substrate Validation) is still
the next broad desktop target, but the repo now also carries an experimental Red Bear-native
greeter/auth/session-launch stack on the `redbear-full` desktop path.

## Active Target Surface and Evidence Boundary

- The supported compile targets are `redbear-mini`, `redbear-live-mini`, `redbear-full`, and `redbear-live-full`.
- Desktop/graphics are available only on `redbear-full` and `redbear-live-full`.
- Older names such as `redbear-kde`, `redbear-wayland`, and `redbear-minimal*` still appear in
  historical or staging material, but they are not the supported compile-target surface.
- The greeter/login path is currently an **experimental build/integration surface** on `redbear-full`;
  it is not yet a runtime-validated end-to-end desktop-login claim.

## Status Matrix

| Area | Evidence class | Detail |
|---|---|---|
| `libwayland` | **builds** | relibc/Wayland-facing compatibility is materially better than before |
| Qt6 core stack | **builds** | `qtbase` (7 libs + 12 plugins), `qtdeclarative`, `qtsvg`, `qtwayland` |
| KF6 frameworks | **builds** | All 32/32; some higher-level pieces use bounded/reduced recipes (kf6-kio heavy shim, kirigami stub-only) |
| KWin | **experimental** | Recipe exists; current reduced path now links honest `libudev.so` and `libdisplay-info.so` provider paths alongside real `libepoxy` and `lcms2`; 11 feature switches remain disabled and runtime/session proof is still missing |
| plasma-workspace | **experimental** | Recipe exists; stub deps (kf6-knewstuff, kf6-kwallet) unresolved |
| plasma-desktop | **experimental** | Recipe exists; depends on plasma-workspace |
| Mesa EGL+GBM+GLES2 | **builds** | Software path via LLVMpipe proven in QEMU; hardware path not proven |
| libdrm amdgpu | **builds** | Package-level success only |
| Input stack | **builds, enumerates** | evdevd, libevdev, libinput, seatd present; evdevd registers scheme at boot |
| D-Bus | **builds, usable (bounded)** | System bus wired in `redbear-full`; D-Bus plan + sessiond complete (DB-1), Qt 6.11 D-Bus coverage documented (Section 14), DB-2/3/4 service daemons implemented as stubs (notifications, upower, udisks, polkit) |
| redbear-sessiond | **builds, scaffold** | org.freedesktop.login1 D-Bus session broker — Rust daemon (zbus 5), wired on the `redbear-full` desktop path; now includes runtime control updates used by the greeter/auth session handoff |
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
| DRM/KMS | **builds** | redox-drm scheme daemon; shared contract hardened (GEM, PRIME, bounded private CS surface, honest fsync, shared driver-event groundwork for B3 across Intel and AMD); no hardware runtime validation |
| GPU acceleration | **blocked** | PRIME/DMA-BUF ioctls and bounded private CS surface implemented; real vendor render CS/fence path still missing |
| validation compositor runtime | **experimental** | Reaches early init in QEMU; no complete session |
| validation profile | **builds, boots** | Bounded Wayland runtime profile |
| `redbear-full` profile | **builds, boots** | Active desktop/graphics compile surface; now owns the experimental greeter/auth/session-launch integration path |
| `redbear-live-full` profile | **builds** | Live image following the active desktop/graphics target |
| `redbear-mini` profile | **builds** | Minimal non-desktop compile target |
| `redbear-live-mini` profile | **builds** | Minimal live image target |

## Profile View

### `redbear-full`

- **Role:** Active desktop/graphics compile target and current greeter-integration surface
- **Current truth:** Carries D-Bus, sessiond, broader integration pieces, and the experimental Red Bear-native greeter/auth/session-launch stack; VirtIO networking works in QEMU, the bounded Phase 5 network/session checker is evidence-backed there, and the repo now includes a bounded greeter checker/harness for the login surface. `redbear-validation-session` remains staged only as a bounded helper, not the active `20_display.service` owner on this target. TUI fallback (`30_console.service`/`31_debug_console.service`) is now triggered through `29_activate_console.service` and is decoupled from greeter success.
- **Use for:** Desktop integration testing, greeter/login bring-up, and bounded desktop/network plumbing validation
- **Do not overclaim:** This profile proves bounded QEMU desktop/network plumbing only. It does not by itself close the Wi-Fi implementation plan's later real-hardware Phase W5 reporting/recovery gate.

### `redbear-live-full`

- **Role:** Live/demo/recovery image layered on the active desktop target
- **Current truth:** Follows `redbear-full`; desktop/graphics-capable live image, but the greeter/login surface remains experimental until end-to-end proof exists
- **Use for:** Demo, install, and bounded live-media validation on the current desktop surface

### `redbear-mini`

- **Role:** Minimal non-desktop target
- **Current truth:** No desktop/graphics path; recovery and non-desktop integration surface only. TUI recovery is bound to VT activation through `29_activate_console.service` followed by `30_console.service`/`31_debug_console.service`.
- **Use for:** Minimal runtime bring-up, subsystem validation, and non-desktop packaging checks

### `redbear-live-mini`

- **Role:** Minimal live image target
- **Current truth:** No desktop/graphics path; live/recovery-oriented minimal image surface
- **Use for:** Minimal live boot and recovery workflows

## Current Blockers

### 1. Runtime trust trails build success (Phase 1 gate)

The repo has real build-visible desktop progress, but build success exceeds runtime confidence.
Phase 1 exists specifically to close this gap.

### 2. No complete compositor session (Phase 2 gate)

A bounded compositor initialization reaches early startup but does not complete a usable Wayland compositor session.
This blocks all desktop session work.

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
- Four supported compile targets exist, with desktop/graphics on `redbear-full` and `redbear-live-full`
- the Red Bear-native greeter/login path now has a bounded passing QEMU proof (`GREETER_HELLO=ok`, `GREETER_INVALID=ok`, `GREETER_VALID=ok`)
- relibc compatibility is materially stronger than before

The remaining work is **broader runtime validation, compositor/session stability, and the remaining KDE session/runtime proof work**.
Phase 1 (Runtime Substrate Validation) remains the immediate broad target. The key current boundary is now explicit: the greeter/login slice has crossed its bounded proof gate, the old `kwin_wayland` page-fault path has been removed, and current QEMU now fails lower in the desktop/runtime layer with a clean no-usable-DRM limitation rather than with a compositor crash.
