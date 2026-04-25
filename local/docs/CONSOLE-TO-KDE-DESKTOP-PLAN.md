# Red Bear OS: Console to Hardware-Accelerated KDE Desktop on Wayland

**Version:** 2.1 (2026-04-25)
**Updated:** Phase 1 test coverage complete; refined Phase 2–4 work items and blocker detail
**Replaces:** All prior console-to-KDE roadmap documents
**Status:** Canonical desktop path plan

## Purpose

This is the single authoritative plan for the Red Bear OS path from console boot to a
hardware-accelerated KDE Plasma desktop running on Wayland.

It consolidates and replaces the top-level planning role previously held by:

- `docs/05-KDE-PLASMA-ON-REDOX.md` (historical KDE rationale)
- `local/docs/AMD-FIRST-INTEGRATION.md` (AMD-specific hardware detail)
- Prior revisions of this document (v1, which used a different Phase 1–5 breakdown)

`local/docs/WAYLAND-IMPLEMENTATION-PLAN.md` now serves as the canonical Wayland subsystem plan
beneath this top-level desktop path.

Those documents remain useful for subsystem detail, porting history, and design rationale.
The earlier reassessment bridge is now retired, and its reconciliation role is covered here together
with `local/docs/DESKTOP-STACK-CURRENT-STATUS.md` and `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md`.
The DRM-specific execution detail beneath this desktop path now lives in
`local/docs/DRM-MODERNIZATION-EXECUTION-PLAN.md`.
This document answers the higher-level question: **what must happen, in what order, before
Red Bear OS can honestly claim a usable KDE Plasma desktop on Wayland — first in software,
then with real hardware acceleration.**

This plan is grounded in the current repo state, not greenfield assumptions. The project
has substantial build-side progress across relibc, driver infrastructure, Wayland, Mesa,
Qt6, KF6, D-Bus, and desktop-facing profiles. The remaining problem is not package absence.
It is the gap between what **builds** and what is **runtime-trusted**.

Scope: console boot → first Wayland compositor proof → software-rendered Qt6 on Wayland →
hardware GPU validation → KWin session bring-up → KDE Plasma session bring-up.

Out of scope: USB, Wi-Fi, Bluetooth (covered by their own subsystem plans).

Tracked-default truth: this document is the canonical desktop-path plan, and the tracked desktop-
capable surface is `redbear-full`. Older names such as `redbear-wayland` and `redbear-kde`
should be read as historical or staging labels, not supported compile targets.

---

## Evidence Model

This plan uses strict evidence classes. They are not interchangeable.

| Class | Meaning | Safe to say | Not safe to say |
|---|---|---|---|
| **builds** | Package compiles and stages | "builds" | "works" |
| **boots** | Image reaches prompt or known runtime surface | "boots" | "desktop works" |
| **enumerates** | Scheme/device node appears and answers basic queries | "enumerates" | "usable end to end" |
| **usable** | Bounded runtime path performs its intended task | "usable for this path" | "broadly stable" |
| **validated** | Repeated proof on the intended target class | "validated" | "complete everywhere" |
| **experimental** | Partial, scaffolded, or unproven | "experimental" | "done" |

Rules:
- Compiles-only → called **builds**
- Boots but doesn't complete a session → called **boots**
- Daemon registers a scheme → called **enumerates**
- Only QEMU proof → claim stays bounded to QEMU
- Dependencies still shimmed/stubbed → layer remains **experimental**
- Nothing is **validated** without repeated runtime proof on the intended target class

---

## Current State Baseline

### Honest capability matrix

| Area | State | Evidence | Notes |
|---|---|---|---|
| AMD bare-metal boot | validated | Boot, ACPI, SMP, x2APIC all work | Bounded to current tested hardware |
| relibc Wayland/Qt unblockers | builds + targeted runtime proof | signalfd, timerfd, eventfd, open_memstream, F_DUPFD_CLOEXEC, MSG_NOSIGNAL, bounded waitid, bounded RLIMIT, bounded eth0 networking, shm_open, bounded sem_open | Strict relibc Redox-target runtime proof now exists for the fd-event slice; broader real-consumer semantics still need confirmation |
| redox-driver-sys | builds | Driver substrate | |
| linux-kpi | builds | Linux kernel API compatibility layer | |
| firmware-loader | builds, boots | scheme:firmware registers at boot; 24 unit tests (mmap lifecycle, openat validation, read/fstat) | |
| redox-drm (AMD + Intel) | builds | DRM scheme daemon; 68 unit tests passing (KMS, GEM, PRIME, wire structs, scheme pure logic) | No hardware runtime validation |
| amdgpu retained C path | builds | Red Bear display glue retained path + linux-kpi compat; imported Linux AMD DC/TTM/core remain under compile triage | No hardware runtime validation |
| evdevd | builds, boots | scheme:evdev registers at boot; 65 unit tests (device classification, capability bitmaps, input translation) | |
| udev-shim | builds, boots | scheme:udev registers at boot; 15 unit tests (device database, subsystem naming, property formatting) | |
| redbear-hwutils | builds | lspci/lsusb tools; 19 unit tests (PCI location parsing, USB device description, argument handling) | |
| libwayland 1.24.0 | builds | No compositor proof yet | |
| wayland-protocols | builds | Build blocker removed | |
| Mesa EGL + GBM + GLES2 | builds | Software rendering via LLVMpipe proven | Hardware path not proven |
| libdrm + libdrm_amdgpu | builds | Package-level success only | |
| Qt6 qtbase 6.11.0 | builds | Core, Gui, Widgets, DBus, Wayland, OpenGL, EGL | |
| qtdeclarative | builds | QML JIT disabled | |
| qtsvg | builds | | |
| qtwayland | builds | | |
| D-Bus 1.16.2 | builds, bounded runtime | System bus wired in redbear-full | |
| libinput 1.30.2 | builds | Runtime integration open | |
| libevdev 1.13.2 | builds | Runtime integration open | |
| seatd | builds | Session-management runtime proof open | |
| All 32 KF6 frameworks | builds | Major build milestone; some higher-level pieces use bounded/reduced recipes (kirigami stub-only, kf6-kio heavy shim, kf6-knewstuff/kwallet stubs) | |
| kdecoration | builds | | |
| plasma-wayland-protocols | builds | | |
| kf6-kwayland | builds | | |
| kf6-kcmutils | builds | Widget-only build (QML stripped) | |
| `redbear-wayland` profile | historical / staging | Bounded Wayland validation profile | Not a supported compile target |
| `redbear-full` profile | builds, boots | Broader desktop plumbing profile | Session/network/runtime integration slice |
| `redbear-kde` profile | historical / staging | Older KDE session-surface profile | Not a supported compile target; use `redbear-full` for the tracked desktop-capable surface |
| bounded compositor validation path | experimental | Reaches xkbcommon init + EGL platform selection in QEMU | No complete session |
| qt6-wayland-smoke | builds, partial | Creates QWindow with colored background, runs 3 seconds | |
| QEMU graphics | usable (bounded) | Renderer is llvmpipe | Not hardware acceleration |
| D-Bus system bus (redbear-full) | usable (bounded) | Not full session integration | |
| VirtIO networking (QEMU) | usable | | |
| KWin | experimental, blocked | Recipe exists, blocked by shimmed/stubbed deps | |
| plasma-workspace | experimental | Recipe exists, incomplete deps | |
| plasma-desktop | experimental | Recipe exists, incomplete deps | |
| QtNetwork | blocked | Intentionally disabled — relibc networking too narrow | |
| Hardware GPU acceleration | blocked | PRIME/DMA-BUF scheme ioctls and a bounded private CS surface exist, but no real vendor GPU render CS/fence path | |

The current bounded runtime entrypoint for display-path evidence is the in-guest
`redbear-drm-display-check` tool, with shell wrappers in `local/scripts/test-drm-display-runtime.sh`,
`local/scripts/test-amd-gpu.sh`, and `local/scripts/test-intel-gpu.sh`. It now covers direct
connector/mode enumeration and bounded direct modeset proof, but successful runs from that surface are
still display-only evidence, not render proof.
| Working Wayland compositor session | blocked | Runtime not proven | |
| KWin compositor runtime | blocked | Runtime not proven | |
| KDE Plasma session | blocked | Runtime not proven | |

### What is DONE (build-side)

The repo has crossed major build-side gates:

1. **relibc surface** — signalfd, timerfd, eventfd, open_memstream, F_DUPFD_CLOEXEC, MSG_NOSIGNAL, bounded waitid, bounded RLIMIT, bounded eth0 networking, shm_open, bounded sem_open
2. **Driver substrate** — redox-driver-sys, linux-kpi, firmware-loader, redox-drm (AMD+Intel), amdgpu C port, evdevd, udev-shim
3. **Wayland/graphics packages** — libwayland, wayland-protocols, Mesa EGL+GBM+GLES2, libdrm, libdrm_amdgpu
4. **Qt6 + D-Bus** — qtbase (7 libs + 12 plugins), qtdeclarative (11 libs), qtsvg, qtwayland, D-Bus 1.16.2
5. **KF6 + KDE-facing** — All 32 KF6 frameworks, kdecoration, plasma-wayland-protocols, kf6-kwayland, kf6-kcmutils
6. **Tracked profiles** — redbear-mini, redbear-full, redbear-grub
7. **Phase 1 test coverage** — 300+ unit tests across evdevd (65), udev-shim (15), firmware-loader (24), redox-drm (68), redbear-hwutils (19), and bluetooth/wifi daemons

### What is runtime-proven (limited scope)

- AMD bare-metal boot with ACPI, SMP, x2APIC
- the bounded runtime validation surface boots in QEMU and reaches early initialization
- QEMU graphics via llvmpipe (software)
- D-Bus system bus wired in `redbear-full`
- VirtIO networking in QEMU
- firmware-loader, evdevd, udev-shim register schemes at boot

### What is NOT DONE

**Runtime not proven:**
- No GPU hardware-accelerated rendering
- No working Wayland compositor session
- No KWin compositor runtime
- No KDE Plasma session
- Qt6 OpenGL/EGL only have software-path proof

**Builds still blocked/scaffolded:**
- KWin does not build with fully real dependencies (4 stub deps: libepoxy, libudev, lcms2, libdisplay-info)
- kirigami is stub-only
- kf6-kio is a heavy shim
- 11 KWin feature switches remain disabled (BUILD_WITH_QML=OFF, KWIN_BUILD_KCMS=OFF, KWIN_BUILD_EFFECTS=OFF, KWIN_BUILD_TABBOX=OFF, KWIN_BUILD_GLOBALSHORTCUTS=OFF, KWIN_BUILD_NOTIFICATIONS=OFF, KWIN_BUILD_SCREENLOCKING=OFF, KWIN_BUILD_SCREENLOCKER=OFF, legacy backend disabled, KWIN_BUILD_RUNNING_IN_KDE=OFF, KWIN_BUILD_ELECTRONICALLY_SIGNING_DOCS=OFF)
- QtNetwork disabled (relibc networking incomplete)
- No compositor session proof exists — KWin builds but has zero runtime session evidence
- Qt6Quick/QML runtime not proven — JIT disabled, no QML client test exists

### Baseline conclusion

The repo is no longer stuck at package availability. It is limited by **runtime trust,
hardware validation, and KWin/Plasma session assembly**. That is the real starting point.

---

## Dependency Stack

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│                           KDE Plasma Session                                 │
│       plasma-workspace, plasma-desktop, shell, panels, launcher, apps        │
├──────────────────────────────────────────┬───────────────────────────────────┤
│                    KWin desktop-session layer                               │
│            KWin, kdecoration, seat and session wiring                       │
├──────────────────────────────────────────┬───────────────────────────────────┤
│                 Qt6 and KDE frameworks                                       │
│       Qt6 Widgets, QtWayland, QtDBus, QML, KF6, KDE support libs            │
├──────────────────────────────────────────┬───────────────────────────────────┤
│              Wayland compositor and protocols                                │
│      bounded validation compositor work, then KWin as the desktop path       │
│        NOTE: KWin owns the compositor and session layers in Phase 3.         │
├──────────────────────────────────────────┬───────────────────────────────────┤
│             Mesa, GBM, EGL, GLES2, libdrm                                   │
│       software path first, hardware path after DMA-BUF                       │
├──────────────────────────────────────────┬───────────────────────────────────┤
│          DRM, KMS, firmware, input, device enumeration                       │
│       redox-drm, amdgpu, Intel path, evdevd, udev-shim                      │
├──────────────────────────────────────────┬───────────────────────────────────┤
│           Kernel and libc substrate for desktop                              │
│        relibc, fd passing, DMA-BUF, IRQ, PCI, schemes                        │
├──────────────────────────────────────────┬───────────────────────────────────┤
│              Hardware and boot substrate                                     │
│       AMD64 boot, ACPI, SMP, x2APIC, AMD and Intel GPUs                     │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Layer-by-layer status

| Layer | State | What's proven | What's missing |
|---|---|---|---|
| Hardware + boot | partly runtime-proven | AMD boot, ACPI, SMP, x2APIC | Desktop-path validation on real AMD/Intel GPUs |
| Kernel + libc | strong build-side, runtime incomplete | relibc surfaces, driver substrate | Real Wayland/Qt event-loop pressure, GPU CS ioctl |
| DRM/firmware/input | build + boot visible, not runtime-trusted | Scheme registration at boot | Real firmware loading, real input flow, real DRM/KMS queries |
| Graphics userland | software builds, hardware blocked | Mesa EGL/GBM/GLES2, libdrm, Qt6 OpenGL | Hardware renderer path, GBM/EGL on hardware |
| Wayland compositor | partial runtime, not complete | bounded compositor initialization reached in QEMU | Complete compositor session, input routing, Qt6 client display |
| Qt6 + KF6 | build milestone, runtime thin | All packages build | Real Qt6 Wayland client behavior, QML without JIT |
| KWin session | experimental, blocked | Recipes exist, some features re-enabled | Honest deps, KWin runtime, session services |
| KDE Plasma | not yet proven | Recipe surfaces exist | plasma-workspace, plasma-desktop, shell, panel, apps |

### Conclusion

The shortest honest path is not "port more packages". It is:
1. **Validate the substrate** (turn builds into runtime trust)
2. **Finish one software compositor validation path**
3. **Finish one KWin session path** (on software renderer)
4. **Finish one Plasma session path** (on software renderer)
5. **Land real hardware acceleration** (in parallel with steps 3–4)

---

## Phased Work Plan

This plan uses a three-track model:

- **Track A: Runtime Substrate → Compositor** (sequential, blocking)
- **Track B: Desktop Session Assembly** (sequential after Track A, Phase 2)
- **Track C: Hardware GPU Enablement** (parallel with Track B)

```
Track A (Phases 1–2): Substrate → Software Compositor
    Phase 1: Runtime Substrate Validation (4–6 weeks)
    Phase 2: Wayland Compositor Proof (4–6 weeks)

Track B (Phases 3–4): Desktop Session Assembly
    Phase 3: KWin Desktop Session (6–10 weeks, starts after Phase 2)
    Phase 4: KDE Plasma Session (8–12 weeks, starts after Phase 3)

Track C (parallel): Hardware GPU Enablement
    Phase 5: Hardware GPU Enablement (12–20 weeks, starts after Phase 1)
```

### Phase 1: Runtime Substrate Validation

**Duration:** 4–6 weeks
**Goal:** Turn the lowest desktop-facing layers from build-visible into runtime-trusted.
**Why it matters most:** Without this phase, all later failures will be impossible to diagnose correctly.

#### Work items

| # | Task | Acceptance criteria |
|---|---|---|
| 1.1 | Validate relibc POSIX APIs against real consumers (libwayland, Qt6) | signalfd/timerfd/eventfd pass libwayland event-loop smoke test; shm_open/sem_open pass Qt6 shared-memory path; waitid passes Qt6 process exit detection |
| 1.2 | Validate evdevd path: input schemes → `/dev/input/eventX` | Keyboard/mouse events arrive with correct semantics |
| 1.3 | Validate udev-shim device enumeration | libinput can enumerate at least one keyboard and one pointer device through udev-shim; DRM devices are visible to Mesa |
| 1.4 | Validate firmware-loader with real blobs + real consumer | Blob is requestable, loadable, consumable at runtime |
| 1.5 | Validate `scheme:drm/card0` registration + bounded KMS queries in QEMU | Scheme registers, answers basic queries, no startup-class failures |
| 1.6 | Produce repeatable runtime-service health check for `redbear-wayland` | `redbear-info` or equivalent shows all Phase 1 services as functional |

#### Exit criteria

**Test coverage progress (Phase 1 substrate):** 300+ unit tests now cover all Phase 1 daemon pure-logic surfaces. Runtime validation of these tests in a live environment remains the exit criterion.

- [ ] `redbear-wayland` boots in validation environment
- [ ] All Phase 1 runtime services register without startup errors
- [ ] relibc runtime checks pass for desktop-facing consumers
- [ ] Input path reaches evdevd and yields expected event nodes + bounded test events
- [ ] udev-shim exposes expected bounded device view
- [ ] firmware-loader serves at least one real consumer path with real blobs
- [ ] `scheme:drm/card0` registers and answers bounded basic queries

#### Exit statement

> The desktop substrate is no longer only a build artifact. It is runtime-trusted enough
> to support a compositor completion pass.

---

### Phase 2: Wayland Compositor Runtime Proof

**Duration:** 4–6 weeks
**Goal:** Produce the first working Wayland compositor session using software rendering.
**Profile target:** tracked validation profile
**Renderer:** LLVMpipe (software) — acceptable for correctness proof.

#### Why a bounded validation compositor comes before full session bring-up

Jumping straight to full session bring-up combines too many unknowns: compositor runtime, input,
QML, session services, and dependency scaffolding. A bounded validation compositor isolates
compositor + input + Qt client issues before session-shell complexity.

#### Work items

| # | Task | Acceptance criteria | Technical notes |
|---|---|---|---|
| 2.1 | Complete bounded runtime path to usable session | Compositor launches, creates a Wayland surface, survives 60 seconds in QEMU; `WAYLAND_DISPLAY` is set and a client can connect | Must use KWin reduced path; start with headless/framebuffer output |
| 2.2 | Wire evdevd input into compositor | Keyboard + mouse events arrive through evdevd → libinput → compositor chain | libinput recipe has udev disabled; evdevd must serve as input source; libevdev is available |
| 2.3 | Wire Mesa software rendering through GBM + EGL | Software rendering works through Mesa/GBM/EGL | LLVMpipe already proven; GBM/EGL must connect to redox-drm buffer path |
| 2.4 | Get Qt6 widget app to display through compositor | `qt6-wayland-smoke` shows a window inside compositor in QEMU | qt6-wayland-smoke already exists as bounded client proof |
| 2.5 | Validate seatd for compositor seat access | seatd grants compositor process graphics+input seat; DRM lease works | seatd-redox needs redox-drm scheme for DRM lease path |

#### Exit criteria

- [ ] the compositor launches into a working session in QEMU
- [ ] Keyboard and mouse work through the current input stack
- [ ] Mesa software rendering works through GBM and EGL
- [ ] `qt6-wayland-smoke` shows a window inside the compositor in QEMU

#### Exit statement

> Red Bear OS has a working software-rendered Wayland compositor path with a visible
> Qt6 client.

---

### Phase 3: KWin Desktop Session

**Duration:** 6–10 weeks (starts after Phase 2)
**Goal:** Turn compositor proof into a real desktop-session substrate centered on KWin.
**Profile target:** `redbear-full`
**Renderer:** LLVMpipe (software) — KWin inherits accelerated renderer once Phase 5 lands.

#### Blocked dependency set that must be closed

**Honest reduced-build dependency state** in the current KWin path:

| Dependency | Current state | Remaining limit |
|---|---|---|
| libepoxy | Real dependency | none in this slice |
| lcms2 | Real dependency | none in this slice |
| libudev | Honest scheme-backed provider | hotplug monitoring remains bounded |
| libdisplay-info | Honest bounded provider | base-EDID only; CTA / DisplayID / HDR metadata still unsupported |

**Stub-only/heavily shimmed packages:**

| Package | Current state | Path forward |
|---|---|---|
| kirigami | Stub-only for dep resolution | Real build needed for QML-dependent Plasma shell |
| kf6-kio | Heavy shim build | Must become honest build for session claims |

**KWin feature switches** (11 still disabled in the current reduced path):

| Switch | Why disabled | Re-enable condition |
|---|---|---|
| BUILD_WITH_QML=OFF | QML-dependent paths | QML runtime proof in Phase 2 |
| KWIN_BUILD_KCMS=OFF | Requires QML | After BUILD_WITH_QML |
| KWIN_BUILD_EFFECTS=OFF | Desktop effects | After basic compositor works |
| KWIN_BUILD_TABBOX=OFF | Alt-tab switcher | After basic window management works |
| KWIN_BUILD_GLOBALSHORTCUTS=OFF | Global shortcut integration | After the reduced KWin path is otherwise honest |
| KWIN_BUILD_NOTIFICATIONS=OFF | Notification integration | After the reduced KWin path is otherwise honest |
| KWIN_BUILD_SCREENLOCKING=OFF | Screen locking | Late session polish |
| KWIN_BUILD_SCREENLOCKER=OFF | Screenlocker binary | Late session polish |
| legacy windowing backend disabled | legacy windowing backend | Intentional: Wayland-only |
| KWIN_BUILD_RUNNING_IN_KDE=OFF | KDE runtime detection | After KWin runs as compositor |
| KWIN_BUILD_ELECTRONICALLY_SIGNING_DOCS=OFF | Document signing | Low priority |

**3 switches already re-enabled** in the current reduced path: DECORATIONS, RUNNERS, USE_DBUS.

#### Work items

| # | Task | Acceptance criteria | Technical path |
|---|---|---|---|
| 3.1 | Keep KWin reduced path dependency-honest | cmake configure succeeds without fake stub imported fallbacks | Current honest deps: libepoxy, lcms2, libudev (scheme-backed), libdisplay-info (bounded EDID) |
| 3.2 | Launch KWin as Wayland compositor | KWin starts, registers WAYLAND_DISPLAY, owns display 60+ seconds | 11 feature groups disabled; re-enable incrementally after basic compositor works |
| 3.3 | Validate libinput backend | Key/mouse events arrive via libinput + evdevd | libinput udev disabled; must use evdevd path |
| 3.4 | Validate D-Bus session behavior | dbus-send KWin supportInformation returns non-empty | redbear-sessiond provides login1; full session bus needed |
| 3.5 | Validate seatd for KWin session | seatd grants KWin graphics+input seat | Depends on seatd-redox DRM lease |
| 3.6 | Re-enable KWin BUILD_WITH_QML | QML-dependent KWin paths work after Phase 2 QML proof | Depends on Qt6Quick runtime proof from Phase 2 |
| 3.7 | Make kf6-kio build honest | kf6-kio cmake succeeds without QtNetwork stubs | QtNetwork blocked on relibc; may need bounded network path |

#### Exit criteria

- [ ] KWin cmake configure succeeds without any `-stub` INTERFACE IMPORTED targets
- [ ] KWin process starts and registers `WAYLAND_DISPLAY`
- [ ] KWin owns display output for at least 60 seconds without crash
- [ ] Keyboard and mouse events arrive at KWin-managed windows
- [ ] D-Bus session bus responds to KWin supportInformation query
- [ ] seatd grants graphics+input seat access to KWin

#### Exit statement

> Red Bear OS has a working Wayland desktop session substrate centered on KWin.

---

### Phase 4: KDE Plasma Session

**Duration:** 8–12 weeks (starts after Phase 3)
**Goal:** Boot into a KDE Plasma session with essential desktop shell and session services.
**Profile target:** `redbear-full`

#### Work items

| # | Task | Acceptance criteria | Technical path |
|---|---|---|---|
| 4.1 | Complete plasma-workspace build | cmake succeeds without stub targets | Blocked on kirigami stub → needs Qt6Quick |
| 4.2 | Complete plasma-desktop build | cmake succeeds without stub targets | Blocked on plasma-workspace |
| 4.3 | Shell, panel, launcher visible | plasmashell starts; panel renders | Blocked on kirigami + QML |
| 4.4 | File-manager and settings paths | dolphin opens directory; systemsettings opens module | Blocked on kf6-kio honest build |
| 4.5 | Bounded network + audio integration | ip addr shows interface; sound device visible | QtNetwork blocked on relibc |
| 4.6 | Resolve kirigami stub | Real kirigami build from source | Qt6Quick prerequisite; QML JIT disabled |
| 4.7 | Resolve kf6-knewstuff/kwallet stubs | Real or bounded builds replace stubs | plasma-workspace dependencies |

#### Dependency chain to close

```
plasma-desktop
  └── plasma-workspace
        ├── kf6-knewstuff (currently stub) → Phase 4: must become real or bounded real build
        ├── kf6-kwallet (currently stub) → Phase 4: must become real or bounded real build
        ├── kf6-prison (real recipe, needs compilation) → Phase 4: compile + validate
        └── other unresolved deps → identify during Phase 3
```

#### Cross-phase blocker ownership

| Blocker | Named in "NOT DONE" | Owned by phase |
|---|---|---|
| kirigami stub-only | Yes | **Phase 4** — real build needed for QML-dependent Plasma shell components |
| kf6-kio heavy shim | Yes | **Phase 3** — KWin uses kf6-kio for runners; honest KWin claim requires honest kio |
| QtNetwork disabled | Yes | **Post-Phase 4** — not a desktop session blocker; network clients will use it after relibc networking matures |
| kf6-knewstuff/kwallet stubs | Yes | **Phase 4** — plasma-workspace dependency |

#### Exit criteria

- [ ] `redbear-full` boots into a KDE Plasma session (plasmashell process is running)
- [ ] KWin is the active compositor (`WAYLAND_DISPLAY` owned by KWin)
- [ ] Plasma panel renders and is interactive (launcher opens, clock visible)
- [ ] An application can be launched from the session and displays a window
- [ ] A file-manager path opens a directory view
- [ ] A settings module opens from systemsettings
- [ ] Network interface is visible inside Plasma session

#### Exit statement

> If Phase 5 incomplete: Red Bear OS has a **software-rendered** KDE Plasma session on Wayland.
> If Phase 5 complete: Red Bear OS has a **hardware-accelerated** KDE Plasma session on Wayland.

---

### Phase 5: Hardware GPU Enablement

**Duration:** 12–20 weeks (starts after Phase 1, runs in parallel with Phases 3–4)
**Goal:** Replace software-only graphics with real hardware-accelerated display + rendering.
**Why separate:** Hardware acceleration is a different class of systems work. It should not
block KWin/Plasma session assembly, which can proceed on the software renderer.
**Dependency note:** Phase 5 can start after Phase 1 (substrate trust), but its final acceptance
criterion ("compositor runs through hardware path") requires a working compositor from Phase 2
or Phase 3. In practice, Track C's final validation gate depends on Track A completing first.

#### Work items

| # | Task | Acceptance criteria |
|---|---|---|
| 5.1 | Implement GPU command submission (CS ioctl) | PRIME buffer sharing already implemented; CS ioctl is the gating missing piece |
| 5.2 | Validate redox-drm AMD driver on real hardware | Device detection, MMIO mapping, firmware loading, connector detection, mode enumeration, bounded modeset proof |
| 5.3 | Validate redox-drm Intel driver on real hardware | Same validation surface as AMD |
| 5.4 | Validate Mesa hardware rendering path | Real renderer (radeonsi for AMD, iris/anv for Intel), not llvmpipe |
| 5.5 | Validate GBM buffer allocation through hardware path | GBM allocates through real DRM/GPU, not software fallback |

#### Exit criteria

- [ ] GPU command submission exists with focused proof coverage
- [ ] `modetest -M amd` shows display modes on real AMD hardware
- [ ] Equivalent Intel DRM query shows display modes on real Intel hardware
- [ ] Compositor runs through hardware path on at least one AMD + one Intel class
- [ ] Runtime evidence shows hardware-backed renderer, not software fallback

#### Exit statement

> Red Bear OS can drive real display hardware and run the compositor on a
> hardware-accelerated path.

---

## Critical Path

### Primary path to software-rendered KDE session

```
Phase 1 (runtime substrate validation)
  → Phase 2 (software Wayland compositor proof)
    → Phase 3 (KWin desktop-session assembly)
      → Phase 4 (KDE Plasma session)
```

This is the shortest honest path. **~22–34 weeks with 2 developers.**

### Parallel hardware path

```
Phase 1 (runtime substrate validation)
  → Phase 5 (hardware GPU enablement, parallel with Phases 3–4)

Phase 5 + Phase 3 + Phase 4
  → hardware-accelerated KDE Plasma desktop
```

**~34–54 weeks total with 2 developers** for hardware-accelerated KDE.

### Why Phase 1 is the real gate

Phase 1 converts lower-layer package progress into runtime trust. Without it, Phase 2+ failures
will be misdiagnosed as compositor bugs when they're actually substrate bugs.

### Why bounded validation comes before KWin session proof

This is the smallest environment to isolate compositor + input + Qt client issues. KWin adds
session services, QML, dependency scaffolding, and desktop-shell behavior on top.

### Why hardware doesn't block session assembly

KWin and Plasma have their own blockers (dependency cleanup, session services, compositor
integration). Those can be solved on software renderer while hardware path matures.

---

## Risk Register

| ID | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | relibc runtime gaps worse than build evidence suggests | Medium | High | Validate with real consumers in Phase 1 |
| R2 | GPU CS ioctl scope is uncertain | High | High | Isolate design + proof early in Phase 5 |
| R3 | Real-hardware validation reveals fundamental driver issues | High | High | Validate AMD and Intel separately |
| R4 | KWin needs significantly more patches than estimated | Medium | High | Finish the bounded validation proof first for cleaner lower-layer evidence |
| R5 | QML-heavy pieces behave badly with JIT disabled | Medium | Medium-High | Keep QML runtime proof explicit in Phases 3–4 |
| R6 | Mesa hardware rendering needs Redox-specific winsys work | Medium | High | Separate display proof from renderer proof |
| R7 | linux-kpi gaps only surface during real-hardware execution | High | Medium-High | Budget for hardware-driven compat fixes in Phase 5 |
| R8 | kirigami/stub deps cannot be resolved without full QML stack | Medium | High | Evaluate early in Phase 3; may need alternative approach |
| R9 | Phase 2 compositor proof reveals deeper relibc/glibc gaps in Wayland event loop | Medium | High | Use bounded validation compositor first; isolate event-loop pressure from session complexity |

---

## Timeline

### Planning assumptions

- 2 developers with access to representative AMD and Intel hardware
- No major regression from upstream refresh during desktop push
- Estimates do not assume perfect first-pass success on real hardware

### Phase estimates

| Phase | Weeks | Notes |
|---|---|---|
| Phase 1: Runtime Substrate Validation | 4–6 | Must finish honestly before claiming runtime trust |
| Phase 2: Wayland Compositor Proof | 4–6 | Can overlap with late Phase 1 cleanup |
| Phase 3: KWin Desktop Session | 6–10 | Starts after Phase 2; **lower bound is optimistic — assumes stub/shim cleanup stays bounded** |
| Phase 4: KDE Plasma Session | 8–12 | Starts after Phase 3; **lower bound assumes kirigami/knewstuff stubs resolve without major rework** |
| Phase 5: Hardware GPU Enablement | 12–20 | Starts after Phase 1, parallel with 3–4 |

### Total duration (2 developers)

| Target | Weeks | Months |
|---|---|---|
| Software-rendered KDE Plasma on Wayland | 22–34 | 6–8 |
| Hardware-accelerated KDE Plasma on Wayland | 34–54 | 8–13 |

### One-developer estimate

| Target | Months |
|---|---|
| Software-rendered KDE | 9–16 |
| Hardware-accelerated KDE | 12–27 |

### Rough overlap model

```
Weeks  1– 6: Phase 1 (runtime substrate validation)
Weeks  4–12: Phase 2 (software compositor proof)
Weeks  7–26: Phase 5 (hardware GPU enablement, parallel)
Weeks 13–22: Phase 3 (KWin session assembly)
Weeks 23–34: Phase 4 (KDE Plasma session)
```

---

## Relationship to Other Plans

This is the canonical document for the desktop path. It does not replace subsystem-specific plans.

### Primary supporting plans

| Plan | What it covers |
|---|---|
| `local/docs/AMD-FIRST-INTEGRATION.md` | AMD-specific GPU/driver detail (equal-priority AMD+Intel policy) |
| `local/docs/QT6-PORT-STATUS.md` | Qt6, KF6, KWin blocker/shim/stub status detail |
| `local/docs/DESKTOP-STACK-CURRENT-STATUS.md` | Short current-state desktop truth summary |
| `local/docs/RELIBC-COMPLETENESS-AND-ENHANCEMENT-PLAN.md` | relibc completeness detail + patch ownership |
| `local/docs/INPUT-SCHEME-ENHANCEMENT.md` | Input-path design if structural cleanup needed |
| `local/docs/AMDGPU-DC-COMPILE-TRIAGE-PLAN.md` | AMD DC compile-triage + bounded source-set strategy |
| `local/docs/DMA-BUF-IMPROVEMENT-PLAN.md` | DMA-BUF scheme detail |
| `local/docs/IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md` | Controller/IRQ/IOMMU quality work |
| `local/docs/PROFILE-MATRIX.md` | Profile roles + support-language reference |

### How to use this plan

1. Read this document first for execution order, claim language, completion criteria, critical path
2. Read subsystem plans for exact relibc, driver, package, or input details behind those phases
3. Use `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md` for the repo-wide workstream ordering

---

## Pre-Phase Work (Already Complete)

The following work was completed before this plan was written. It is listed here for
continuity, not as future work.

| Work | Status | When |
|---|---|---|
| AMD bare-metal boot (ACPI, SMP, x2APIC) | ✅ Boot-baseline complete | Prior to this plan; see `local/docs/ACPI-IMPROVEMENT-PLAN.md` for ongoing ownership and robustness work |
| Driver infrastructure (redox-driver-sys, linux-kpi, firmware-loader) | ✅ Builds complete | Prior to this plan |
| AMD GPU display (redox-drm + bounded amdgpu retained path) | 🚧 Partial build completion | Imported Linux AMD DC/TTM/core remain under compile triage; no hardware runtime validation yet |
| relibc POSIX unblockers (signalfd, timerfd, eventfd, etc.) | ✅ Builds + targeted runtime proof complete | Prior to this plan |
| Qt6 base stack (qtbase, qtdeclarative, qtsvg, qtwayland) | ✅ Builds complete | Prior to this plan |
| D-Bus 1.16.2 | ✅ Builds + bounded runtime | Prior to this plan |
| All 32 KF6 frameworks | ✅ Builds complete | Prior to this plan |
| Input stack (libevdev, libinput, evdevd, udev-shim) | ✅ Builds complete | Prior to this plan |
| Mesa EGL/GBM/GLES2 + libdrm amdgpu | ✅ Builds complete | Prior to this plan |
| Desktop profiles (`redbear-mini`, `redbear-full`, `redbear-grub`) | ✅ Builds complete | Prior to this plan |
| `local/docs/DBUS-INTEGRATION-PLAN.md` | D-Bus architecture, service dependency map, and phased implementation |
| PRIME/DMA-BUF scheme ioctls | ✅ Implemented | Prior to this plan |
| KWin recipe with 5 re-enabled features | ✅ Partial build | Prior to this plan |
| kdecoration, plasma-wayland-protocols, kf6-kwayland | ✅ Builds complete | Prior to this plan |
