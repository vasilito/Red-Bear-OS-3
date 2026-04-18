# Red Bear OS Desktop Stack — Current Status

**Last updated:** 2026-04-18
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
For historical design rationale, see `docs/03-WAYLAND-ON-REDOX.md` and `docs/05-KDE-PLASMA-ON-REDOX.md`.

## Where We Are in the Plan

The canonical desktop plan uses a three-track model:

- **Track A (Phase 1–2):** Runtime Substrate → Software Compositor — **Phase 1 is the current target**
- **Track B (Phase 3–4):** KWin Session → KDE Plasma — **blocked on Track A**
- **Track C (Phase 5):** Hardware GPU — **can start after Phase 1**

**Current position:** Build-side gates are crossed. Phase 1 (Runtime Substrate Validation) is the
next work target. The repo has not yet started systematic runtime validation.

## Tracked Default and Evidence Boundary

- The tracked default build now resolves to `CONFIG_NAME?=redbear-kde`.
- the bounded Phase 2 validation profile remains available.
- `redbear-kde` is the tracked KWin Wayland desktop direction, but runtime-proven compositor/
  session claims still remain incomplete.

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
| D-Bus | **builds, usable (bounded)** | System bus wired in `redbear-full` and `redbear-kde`; D-Bus plan + sessiond complete (DB-1), Qt 6.11 D-Bus coverage documented (Section 14), DB-2/3/4 service daemons implemented as stubs (notifications, upower, udisks, polkit) |
| redbear-sessiond | ✅ Scaffold | org.freedesktop.login1 D-Bus session broker — Rust daemon (zbus 5), config wired in redbear-kde.toml, acpi_watcher with edge detection |
| redbear-notifications | ✅ Scaffold | org.freedesktop.Notifications — logs to stderr, no display integration yet |
| redbear-upower | ✅ bounded real | org.freedesktop.UPower — enumerates real AC adapters/batteries from `/scheme/acpi/power`; desktop machines with no battery report line power only |
| redbear-udisks | ✅ bounded real | org.freedesktop.UDisks2 — enumerates real `disk.*` schemes and partitions into read-only D-Bus objects; no fabricated mount/serial metadata |
| Phase 5 D-Bus runtime proof | ✅ implemented (bounded QEMU proof) | `redbear-phase5-network-check` + `test-phase5-network-qemu.sh` assert bounded-real UPower/UDisks2 registration and runtime-backed enumeration on `redbear-full`; this is a desktop/network plumbing proof, not a claim that the Wi-Fi plan's later Phase W5 hardware/runtime-reporting exit criteria are complete |
| Phase 6 Solid readiness proof | ✅ implemented, blocked | `redbear-phase6-kde-check` + `test-phase6-kde-qemu.sh` now distinguish real Solid validation from blocked states; `kf6-solid` remains disabled until runtime proof + tooling are present |
| redbear-polkit | ✅ Scaffold | org.freedesktop.PolicyKit1 — always-permit authorization; KAuth still uses FAKE backend because PolkitQt6-1 is not packaged yet |
| redbear-dbus-services | ✅ Created | D-Bus activation files + policies staged |
| DRM/KMS | **builds** | redox-drm scheme daemon; shared contract hardened (GEM, PRIME, bounded private CS surface, honest fsync, shared driver-event groundwork for B3 across Intel and AMD); no hardware runtime validation |
| GPU acceleration | **blocked** | PRIME/DMA-BUF ioctls and bounded private CS surface implemented; real vendor render CS/fence path still missing |
| validation compositor runtime | **experimental** | Reaches early init in QEMU; no complete session |
| validation profile | **builds, boots** | Bounded Wayland runtime profile |
| `redbear-full` profile | **builds, boots** | Broader desktop plumbing profile |
| `redbear-kde` profile | **builds** | Tracked KWin desktop-direction profile |
| `redbear-live` profile | **builds** | Live image following the tracked KWin desktop target |

## Profile View

### Validation profile

- **Role:** Phase 2 Wayland compositor validation target
- **Current truth:** Builds and boots in QEMU; bounded compositor initialization reaches early init but no complete session
- **Use for:** Compositor/runtime regression testing, not broad desktop claims

### `redbear-full`

- **Role:** Broader desktop/network/session plumbing
- **Current truth:** Carries D-Bus and broader integration pieces; VirtIO networking works in QEMU, and the bounded Phase 5 network/session checker is evidence-backed there
- **Use for:** Desktop integration testing beyond the narrow Wayland slice
- **Do not overclaim:** This profile proves bounded QEMU desktop/network plumbing only. It does not by itself close the Wi-Fi implementation plan's later real-hardware Phase W5 reporting/recovery gate.

### `redbear-kde`

- **Role:** Phase 3–4 KDE/Plasma session bring-up
- **Current truth:** Carries KWin/session wiring and KDE-facing package set; experimental but selected as the tracked default desktop target
- **Use for:** KDE session surface testing once Phase 2 completes

### `redbear-live`

- **Role:** Live/demo/recovery image layered on the tracked desktop profile
- **Current truth:** Inherits `redbear-kde`, so live media now follows the tracked KWin desktop target
- **Use for:** Demo, install, and recovery workflows based on the current shipped desktop surface

## Current Blockers

### 1. Runtime trust trails build success (Phase 1 gate)

The repo has real build-visible desktop progress, but build success exceeds runtime confidence.
Phase 1 exists specifically to close this gap.

### 2. No complete compositor session (Phase 2 gate)

A bounded compositor initialization reaches early startup but does not complete a usable Wayland compositor session.
This blocks all desktop session work.

### 3. KWin reduced build is now dependency-honest, but runtime proof is still missing (Phase 3 gate)

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

### 4. Hardware acceleration missing GPU CS ioctl (Phase 5 gate)

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
| `local/docs/QT6-PORT-STATUS.md` | Qt/KF6/KWin package-level build status |
| `local/docs/AMD-FIRST-INTEGRATION.md` | AMD-specific hardware/driver detail |
| `docs/03-WAYLAND-ON-REDOX.md` | Historical Wayland design rationale |
| `docs/05-KDE-PLASMA-ON-REDOX.md` | Historical KDE design rationale |
| `local/docs/PROFILE-MATRIX.md` | Profile roles and support-language reference |

## Bottom Line

The Red Bear desktop stack has crossed major build-side gates:
- All Qt6 core modules, all 32 KF6 frameworks, Mesa EGL/GBM/GLES2, and D-Bus build
- Three tracked desktop profiles exist and at least boot in QEMU
- relibc compatibility is materially stronger than before

The remaining work is **runtime validation, session assembly, and the remaining KDE session/runtime proof work**.
Phase 1 (Runtime Substrate Validation) remains the immediate next target, while the KWin reduced path still lacks runtime compositor/session proof.
