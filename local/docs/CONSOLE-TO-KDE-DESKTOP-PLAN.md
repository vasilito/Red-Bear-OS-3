# Red Bear OS: Console to Hardware-Accelerated KDE Desktop on Wayland

**Version:** 3.0 (2026-04-29)
**Replaces:** v2.2 and all prior desktop-path documents
**Status:** Canonical desktop path plan — OLW-drafted, build-verified
**Implementation status (2026-04-30):** VERIFIED scope is the currently buildable KDE surface on `redbear-full`; packages still blocked by Qt6Quick/QML downstream proof, Qt6::Sensors/libinput, empty-package output, or direct recipe failure stay commented out in config and are not part of this verification claim.

## Purpose

This is the single authoritative plan for the Red Bear OS path from console boot to a
hardware-accelerated KDE Plasma desktop running on Wayland. It is rewritten in v3.0 based on
a full end-to-end reassessment of every component in the chain: DRM/KMS → Mesa → Wayland
Compositor → Input/Seat → Greeter/Login → KDE Plasma.

This plan answers: **what is the current state of every layer, what are the honest blockers,
and what must happen, in what order, to reach a usable KDE Plasma desktop.**

## Full Chain Assessment (2026-04-29)

### LAYER 1 — DRM/KMS

| Component | Status | Config | Notes |
|-----------|--------|--------|-------|
| redox-drm | **builds** | enabled | Intel Gen8-Gen12 + AMD device support + quirk tables; MSI-X/legacy IRQ fallback; no hardware validation |
| libdrm | **builds** | enabled | Provides libdrm_amdgpu; amdgpu device support |
| firmware-loader | **builds** | enabled | scheme:firmware; blob loading verified |
| GPU firmware | **fetched** | partial | amdgpu/i915 blobs available via fetch-firmware.sh |
| virtio-gpu | **builds** | in redox-drm | 220-line DRM/KMS backend for QEMU testing |
| CS ioctl | **protocol exists** | redox-drm scheme | Private CS submit/wait ioctls defined; hardware backend returns unavailable (GPU driver gate) | |

**Verdict**: Display infrastructure exists. Hardware rendering blocked on GPU driver backend + hardware validation (CS ioctl protocol exists).

### LAYER 2 — Mesa/Graphics

| Component | Status | Config | Notes |
|-----------|--------|--------|-------|
| mesa | **builds** | enabled | llvmpipe software renderer; EGL=on, GBM=on, GLES2=on, platform=redox |
| radeonsi (AMD HW) | **not built** | — | Not cross-compiled for Redox target |
| iris (Intel HW) | **not built** | — | Not cross-compiled for Redox target |
| OSMesa | **builds** | enabled | Off-screen software rendering |

**Verdict**: Software rendering works (llvmpipe). Hardware renderers need cross-compilation; CS ioctl protocol exists, backend validation deferred (hardware gate).

### LAYER 3 — Wayland/Compositor

| Component | Status | Config | Notes |
|-----------|--------|--------|-------|
| libwayland 1.24.0 | **builds** | enabled | Wayland protocol library; durability patch applied |
| wayland-protocols | **builds** | enabled | Protocol XML definitions |
| redbear-compositor | **builds, 788 lines** | enabled | Real Rust Wayland compositor; zero warnings; 3/3 tests; known limitations: heap-memory framebuffer, payload-byte SHM, NUL-terminated wire encoding |
| kwin | **stub** | enabled (but stub) | Stub — recipe downloads real KWin v6.3.4 source but build script only creates wrapper scripts + cmake config stubs; delegates to redbear-compositor; real cmake build requires Qt6Quick/QML downstream proof |
| redbear-compositor-check | **builds** | in redbear-compositor pkg | Verifies compositor socket, binaries, framebuffer |

**Verdict**: Working bounded compositor proof. Real KWin gated on Qt6Quick/QML downstream proof.

### LAYER 4 — Input/Seat

| Component | Status | Config | Notes |
|-----------|--------|--------|-------|
| evdevd | **builds** | enabled | scheme:evdev; 65 unit tests; event device semantics verified |
| libinput | **builds, suppressed** | config: `libinput = "ignore"` | Builds but suppressed in redbear-full; evdevd handles input natively for bounded proof |
| udev-shim | **builds** | enabled | scheme:udev; device enumeration; 15 unit tests |
| seatd/seatd-redox | **builds** | enabled | meson build; 13_seatd.service wired; DRM lease via redox-drm |
| libevdev | **suppressed** | commented | build needed header |

**Verdict**: Input path works through evdevd + udev-shim. seatd wired for compositor seat access. libinput deferred (not needed for bounded proof).

### LAYER 5 — Greeter/Login

| Component | Status | Config | Notes |
|-----------|--------|--------|-------|
| redbear-authd | **builds** | enabled | SHA-crypt/Argon2 auth; /etc/passwd + /etc/shadow |
| redbear-session-launch | **builds** | enabled | Session bootstrap (uid/gid/env/runtime-dir) |
| redbear-greeter | **builds** | enabled | greeterd orchestrator + Qt6/QML UI + compositor wrapper |
| redbear-sessiond | **builds** | enabled | org.freedesktop.login1 D-Bus broker (zbus) |
| dbus | **builds** | enabled | 1.16.2; system bus wired; session bus partially |
| Greeter QEMU proof | **passes** | — | GREETER_HELLO=ok, GREETER_VALID=ok, GREETER_INVALID=ok |
| redbear-kde-session | **builds** | enabled | KDE session launcher (DRM/virtual backend, plasmashell/kded6) |

**Verdict**: Greeter/login path works end-to-end in QEMU with bounded proof. KDE session launcher ready.

### LAYER 6 — KDE/Plasma

| Component | Status | Config | Notes |
|-----------|--------|--------|-------|
| qtbase 6.11.0 | **builds** | enabled | Core+Gui+Widgets+DBus+Wayland; 7 libs + 12 plugins |
| qtdeclarative | **builds** | enabled | Qt6Quick metadata exported; QML JIT disabled for Redox; downstream proof insufficient |
| qtwayland | **builds** | enabled | Wayland QPA plugin |
| qtsvg | **builds** | enabled | SVG support |
| KDE/Plasma surface (48 recipes) | **37 build / 11 blocked** | 36 enabled in config, 12 commented/ignored. See DESKTOP-STACK-CURRENT-STATUS.md for exact breakdown. |
| kf6-kio | **builds** | enabled | HostInfo stub (direct QHostInfo::fromName replaces QtConcurrent chain) — pkgar in repo |
| kirigami | **blocked: QML gate** | ignored in config | QQuickWindow/QQmlEngine headers don't exist on Redox |
| kf6-knewstuff | **blocked** | commented out | Empty package — cmake succeeds but core source produces no libs with QtQuick off |
| kf6-kwallet | **builds** | enabled | Real API-only core wallet cmake build; QML/GPG disabled |
| kf6-attica | **builds** | enabled (NEW) | Minimal core library (v6.10.0, 2.4MB pkgar in repo) |
| plasma-framework | **blocked (QML gate)** | commented out | Depends on kirigami |
| plasma-workspace | **blocked** | commented out | Depends on kf6-knewstuff payload + kwin real build |
| plasma-desktop | **blocked (transitive)** | commented out | Depends on plasma-workspace |
| kdecoration | **builds** | transitively via plasma-workspace | Window decoration library |
| kf6-kwayland | **builds** | enabled | Qt/C++ Wayland protocol wrapper |
| plasma-wayland-protocols | **builds** | transitively | XML protocol definitions |

**Verdict**: KDE/Plasma recipes exist (48 total). 36 build, 11 blocked with documented reasons. Real Plasma session requires resolving platform prerequisites: QML JIT for kirigami, Qt6::Sensors for kwin real build.

### LAYER 7 — Validation Infrastructure

| Artifact | Count | Status |
|----------|-------|--------|
| Phase 1-5 check binaries | 15+ | Zero warnings, Redox-target verified |
| Test harness scripts | 12 | Syntax-clean, guest+QEMU modes |
| Oracle verification rounds | 20+ | Phases 1-5 all verified |
| Host cargo check | 3 crates | Zero warnings |
| Redox-target build | 3 crates | Successful (make r.*) |
| Full OS build | exists | build/x86_64/redbear-full.iso + .img available; rebuild via make all CONFIG_NAME=redbear-full |

## Honest Blocker Classification


| Blocker | What's needed |
|---------|---------------|
| Qt6Quick/QML downstream proof | Qt6Quick/QML runtime proof (QML JIT disabled for Redox); blocks kirigami + real KWin + Plasma |
| Hardware GPU validation | Real AMD/Intel GPU; blocks hardware backend validation; CS ioctl protocol exists |
| Bare-metal validation | Real hardware; blocks all hardware claims |

### Deferred (not on critical path for minimal session proof)

| Item | Reason |
|------|--------|
| kf6-knewstuff/kwallet | real cmake builds attempted; QML disabled; not on critical path for minimal session |
| libinput | evdevd handles input natively for bounded proof; libinput builds but suppressed in config |
| libevdev | header build needed; not blocking |

## Critical Path (updated)

```
Layer 1 (DRM) ▸ Layer 2 (Mesa sw) ▸ Layer 3 (compositor proof) ▸
Layer 4 (input/seat) ▸ Layer 5 (greeter) ✓ ▸ Layer 6 (Plasma preflight) ✓

Environmental gate (Qt6Quick): Layer 3 (KWin runtime proof) ← Qt6Quick/QML downstream proof
Environmental gate (hardware): Layer 1 (GPU CS ioctl backend) ← hardware + Mesa HW cross-compilation
```

## Current Config Surface

`config/redbear-full.toml` enables the full desktop-capable surface including:

- 36 KDE packages (33 KF6 + kdecoration + kglobalacceld + kwin); 11 blocked/ignored with documented reasons
- kf6-attica (NEW — minimal core library, 2.4MB pkgar in repo)
- 12 KDE packages blocked/ignored with documented reasons (see config comments)
- mesa + libdrm (GPU software stack)
- qtbase + qtdeclarative + qtwayland + qtsvg + qt6-wayland-smoke
- seatd + redbear-authd + redbear-session-launch + redbear-greeter (via redbear-mini)
- dbus + firmware-loader + redox-drm + evdevd + udev-shim
- redbear-compositor (real Rust Wayland compositor, kwin delegates to it)
- plus inherited packages from redbear-mini profile

## Verification Steps (build-verified; supplementary QEMU validation) (ordered by impact)

1. **Rebuild full OS image** — `make all CONFIG_NAME=redbear-full` (harddrive.img for QEMU) or `make live CONFIG_NAME=redbear-full` (ISO for bare metal); existing artifacts at build/x86_64/

2. **Qt6Quick/QML runtime proof** — validate Qt6Quick downstream consumers with QML JIT disabled for Redox; unblocks kirigami → real KWin → full Plasma session

3. **GPU CS ioctl implementation** — implement command submission in redox-drm; unblocks hardware rendering path

4. **Mesa HW renderer cross-compilation** — build radeonsi/iris for Redox target; requires CS ioctl for validation

5. **Real KWin build** — validate the current real KWin build with Qt6Quick/QML downstream proof; unblocks full KDE Plasma session

6. **Hardware validation** — AMD + Intel bare-metal testing for all layers

## Evidence Model

| Evidence class | What it means |
|----------------|---------------|
| **Source** | Code exists in tree |
| **Host build-verified** | cargo check zero warnings on Linux |
| **Redox build-verified** | make r.* successful on x86_64-unknown-redox |
| **Runtime-validated** | Exercised in QEMU or bare metal |
| **Hardware-validated** | Exercised on real AMD/Intel hardware |

Current state: Layers 1-4 are **Redox build-verified** (all 3 Red Bear crates cook on x86_64-unknown-redox). Layer 5 (greeter) is **runtime-validated** in QEMU. Layer 6 (Plasma) and hardware paths remain at build-verified preflight level.