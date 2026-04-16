# Red Bear OS: Console to Hardware-Accelerated KDE Desktop on Wayland

## Purpose

This document is the single authoritative implementation plan for the Red Bear OS path from
console boot to a hardware-accelerated KDE Plasma desktop on Wayland.

It consolidates and replaces the roadmap role previously spread across:

- `docs/03-WAYLAND-ON-REDOX.md`
- `docs/05-KDE-PLASMA-ON-REDOX.md`
- `docs/02-GAP-ANALYSIS.md`
- `local/docs/AMD-FIRST-INTEGRATION.md`
- `local/docs/DESKTOP-STACK-CURRENT-STATUS.md`
- `local/docs/QT6-PORT-STATUS.md`

Those documents still matter for subsystem detail, package status, and implementation history.
This document is the place to answer the higher-level question: what still has to happen, in the
right order, before Red Bear OS can honestly claim a usable KDE Plasma desktop on Wayland, first
on the software path and then on real hardware acceleration.

This plan is grounded in the current repo state, not in older greenfield assumptions. The project
already has substantial build-side progress across relibc, driver infrastructure, Wayland, Mesa,
Qt6, KF6, D-Bus, and desktop-facing profiles. The remaining problem is mostly not package absence.
The remaining problem is the gap between what builds and what is runtime-trusted.

Scope here covers console boot to first working Wayland compositor proof, software-rendered Qt6 on
Wayland, hardware GPU validation for AMD and Intel, KWin session bring-up, and KDE Plasma session
bring-up. It does not cover USB, Wi-Fi, Bluetooth, tutorial-style examples, or repo structure and
build-command reference material already documented elsewhere.

This document uses the current Red Bear hardware policy. AMD and Intel GPUs are equal-priority
desktop targets.

## Current State Baseline

### Evidence model

This plan uses six evidence classes. They are intentionally strict and are not treated as equal.

| Evidence class | Meaning | Safe wording | Not safe wording |
|---|---|---|---|
| **builds** | package compiles and stages | builds | works |
| **boots** | image reaches prompt or known runtime surface | boots | desktop works |
| **enumerates** | scheme, device node, or service surface appears and answers basic queries | enumerates | usable end to end |
| **usable** | a bounded runtime path performs its intended task | usable for this path | broadly stable |
| **validated** | repeated proof on the intended target class with explicit checks | validated | complete everywhere |
| **experimental** | partial, scaffolded, or unproven despite visible progress | experimental | done |

Interpretation rules used throughout this document:

- if something only compiles, it is called **builds**
- if something boots but does not complete a session, it is called **boots**
- if a daemon registers a scheme or device node, it is called **enumerates**
- if the only proof is a bounded QEMU path, the claim stays bounded to that path
- if dependencies are still shimmed or stubbed, the layer remains **experimental**
- nothing is called **validated** without repeated runtime proof on the intended target class

### Honest capability matrix

| Area | Current state | Evidence | Notes |
|---|---|---|---|
| AMD bare-metal boot | present | validated for bounded current claim | ACPI, SMP, x2APIC all work |
| relibc Wayland and Qt unblockers | present | builds | signalfd, timerfd, eventfd, open_memstream, F_DUPFD_CLOEXEC, MSG_NOSIGNAL, bounded waitid, bounded RLIMIT, bounded eth0 networking, shm_open, bounded sem_open, bounded sys/ipc.h, bounded sys/shm.h |
| redox-driver-sys | present | builds | driver substrate |
| linux-kpi | present | builds | compatibility layer for Linux-style drivers |
| firmware-loader | present | builds, boots | scheme registers at boot |
| redox-drm with AMD and Intel | present | builds | runtime hardware validation still open |
| amdgpu C port | present | builds | AMD DC + TTM + linux-kpi compat compiles |
| evdevd | present | builds, boots | scheme registers at boot |
| udev-shim | present | builds, boots | scheme registers at boot |
| libwayland 1.24.0 | present | builds | no full compositor proof yet |
| wayland-protocols | present | builds | build-side blocker removed |
| Mesa EGL + GBM + GLES2 | present | builds | proven runtime path is still software via LLVMpipe |
| libdrm + libdrm_amdgpu | present | builds | package-level success only |
| Qt6 qtbase 6.11.0 | present | builds | Core, Gui, Widgets, DBus, Wayland, OpenGL, EGL |
| qtdeclarative | present | builds | QML JIT disabled |
| qtsvg | present | builds | build-visible |
| qtwayland | present | builds | build-visible |
| D-Bus 1.16.2 | present | builds, bounded runtime wiring | system bus wired in `redbear-full` |
| libinput 1.30.2 | present | builds | runtime integration still open |
| libevdev 1.13.2 | present | builds | runtime integration still open |
| linux-input-headers | present | builds | support package |
| seatd | present | builds | session-management runtime proof still open |
| All 32 KF6 frameworks | present | builds | major build milestone complete |
| kdecoration | present | builds | build-visible |
| plasma-wayland-protocols | present | builds | build-visible |
| kf6-kwayland | present | builds | build-visible |
| kf6-kcmutils | present | builds, reduced | widget-only build |
| `redbear-wayland` | present | builds, boots | bounded Wayland runtime profile |
| `redbear-full` | present | builds, boots | broader desktop plumbing profile |
| `redbear-kde` | present | builds | KDE session-surface profile |
| smallvil path | partial | boots, experimental | reaches xkbcommon init and EGL platform selection in QEMU |
| QEMU graphics truth | present | usable for bounded path | current renderer is llvmpipe, not hardware acceleration |
| D-Bus system bus in `redbear-full` | present | usable for bounded path | not equal to full session integration completeness |
| VirtIO networking in QEMU | present | usable | useful for bounded test environment |
| firmware-loader, evdevd, udev-shim scheme registration | present | enumerates | register during boot |
| KWin | blocked | experimental | recipe exists, blocked by remaining shimmed and stubbed dependencies |
| plasma-workspace | partial | experimental | recipe exists, still experimental |
| plasma-desktop | partial | experimental | recipe exists, still experimental |
| QtNetwork | blocked | intentionally disabled | relibc networking completeness still too narrow |
| hardware GPU acceleration | blocked | not runtime-proven | kernel DMA-BUF fd passing required |
| working Wayland compositor session | blocked | runtime not proven | smallvil does not complete a usable session |
| KWin compositor runtime | blocked | runtime not proven | no working KWin session |
| KDE Plasma session | blocked | runtime not proven | no full Plasma session |

### What is DONE, build-side

The repo has already crossed several major build-side gates.

#### relibc surface that now builds downstream consumers

The current build-visible relibc surface includes signalfd, timerfd, eventfd, open_memstream,
F_DUPFD_CLOEXEC, MSG_NOSIGNAL, bounded waitid, bounded RLIMIT behavior, bounded eth0 networking,
shm_open, bounded sem_open, bounded `sys/ipc.h`, and bounded `sys/shm.h`.

#### driver and runtime-service substrate

redox-driver-sys, linux-kpi, firmware-loader, redox-drm with AMD and Intel paths, the amdgpu C
port, evdevd, and udev-shim all build successfully.

#### Wayland and graphics packages

libwayland 1.24.0, wayland-protocols, Mesa EGL + GBM + GLES2 with `libEGL.so`, `libgbm.so`,
`libGLESv2.so`, `swrast_dri.so`, plus libdrm and libdrm_amdgpu all build.

#### Qt6 and D-Bus

D-Bus 1.16.2 builds. qtbase 6.11.0 builds with Core, Gui, Widgets, DBus, Wayland, OpenGL, and
EGL. qtdeclarative, qtsvg, and qtwayland also build.

#### KF6 and KDE-facing build surfaces

All 32 KF6 frameworks build. The completed set spans ecm, core and widget foundations, config,
internationalization, codecs, GUI add-ons, color and notification layers, job and archive support,
item models and views, Solid, D-Bus and service layers, package and crash handling, text and icon
layers, global shortcuts, KDE declarative support, XML GUI, bookmarks, idle time, KIO, and
KCMUtils. Additional KDE-facing packages that already build include kdecoration,
plasma-wayland-protocols, kf6-kwayland, and kf6-kcmutils.

#### tracked desktop profiles

The tracked desktop-facing profiles are `redbear-wayland`, `redbear-full`, and `redbear-kde`.

These are real achievements and should be presented as such. They are not yet desktop-runtime proof.

### What is runtime-proven, limited scope

The current desktop-related runtime proof is bounded, but real.

#### Boot and machine substrate

Red Bear boots on AMD bare metal, and ACPI, SMP, and x2APIC work for the current bounded claim.

#### bounded Wayland bring-up path

`redbear-wayland` boots in QEMU, and smallvil reaches xkbcommon initialization plus EGL platform
selection on Redox.

#### bounded graphics truth

Current QEMU graphics are software-rendered, the renderer evidence is llvmpipe, QEMU is useful for
compositor and Qt bring-up, and QEMU is not proof of the final hardware-accelerated desktop path.

#### bounded runtime services

D-Bus system bus is wired in `redbear-full`, VirtIO networking works in QEMU, and firmware-loader,
evdevd, and udev-shim register schemes at boot.

### What is NOT DONE

This list must stay explicit.

#### runtime not proven

No GPU hardware-accelerated rendering is proven, no kernel DMA-BUF support exists for the required
desktop path, no working Wayland compositor session is proven, no KWin compositor runtime is
proven, no KDE Plasma session is proven, and Qt6 OpenGL and EGL still have only software-path
runtime proof.

#### builds still blocked or scaffolded

KWin does not build end to end with fully real dependencies. Kirigami is still stub-only, KIO is
still a heavy shim build, libepoxy, libudev, lcms2, and libdisplay-info remain real blockers,
plasma-workspace and plasma-desktop remain experimental, and QtNetwork remains disabled due to
incomplete relibc networking semantics.

### Baseline conclusion

The repo is no longer stuck at package availability. It is now limited by runtime trust, hardware
validation, and KWin or Plasma session assembly. That is the real starting point for the plan below.

## Dependency Stack

### ASCII layer diagram

```text
+--------------------------------------------------------------------------------+
|                                KDE Plasma Session                              |
|        plasma-workspace, plasma-desktop, shell, panels, launcher, apps         |
+---------------------------------------^----------------------------------------+
                                        |
+---------------------------------------|----------------------------------------+
|                             KWin desktop-session layer                         |
|                     KWin, kdecoration, seat and session wiring                 |
+---------------------------------------^----------------------------------------+
                                        |
+---------------------------------------|----------------------------------------+
|                              Qt6 and KDE frameworks                            |
|            Qt6 Widgets, QtWayland, QtDBus, QML, KF6, KDE support libs          |
+---------------------------------------^----------------------------------------+
                                        |
+---------------------------------------|----------------------------------------+
|                           Wayland compositor and protocols                     |
|                     smallvil first, then KWin, plus libwayland                 |
+---------------------------------------^----------------------------------------+
                                        |
+---------------------------------------|----------------------------------------+
|                            Mesa, GBM, EGL, GLES2, libdrm                       |
|                  software path first, hardware path after DMA-BUF              |
+---------------------------------------^----------------------------------------+
                                        |
+---------------------------------------|----------------------------------------+
|                     DRM, KMS, firmware, input, device enumeration              |
|                  redox-drm, amdgpu, Intel path, evdevd, udev-shim              |
+---------------------------------------^----------------------------------------+
                                        |
+---------------------------------------|----------------------------------------+
|                      Kernel and libc substrate for desktop bring-up            |
|                     relibc, fd passing, DMA-BUF, IRQ, PCI, schemes             |
+---------------------------------------^----------------------------------------+
                                        |
+---------------------------------------|----------------------------------------+
|                            Hardware and boot substrate                         |
|                    AMD64 boot, ACPI, SMP, x2APIC, AMD and Intel GPUs           |
+--------------------------------------------------------------------------------+
```

### Reading the dependency stack correctly

This stack has two kinds of blockers.

#### runtime substrate blockers

These sit low in the stack and poison all higher work if they are not validated:

- relibc runtime correctness
- input event correctness
- udev-like device enumeration correctness
- firmware loading correctness
- basic DRM and KMS correctness
- kernel DMA-BUF support for the accelerated path

#### session-assembly blockers

These sit higher and matter after the lower layers are trusted:

- smallvil completion
- Qt6 client display on Wayland
- KWin dependency cleanup
- KWin runtime session wiring
- Plasma shell and workspace integration

The plan must handle these in order. Otherwise failures in KWin or Plasma will really be lower-layer
failures in disguise.

### Layer-by-layer status

#### Layer 0, hardware and boot

Status: **partly runtime-proven**

What is true now:

- AMD bare-metal boot works for the current bounded claim
- ACPI, SMP, and x2APIC work
- AMD and Intel are equal-priority GPU targets

What still needs proof:

- real desktop-path validation on AMD GPUs
- real desktop-path validation on Intel GPUs

#### Layer 1, kernel and libc substrate

Status: **strong build-side, runtime incomplete**

What is true now:

- relibc exposes the build-visible Wayland and Qt unblockers
- redox-driver-sys and linux-kpi exist as the current driver substrate

What still needs proof:

- relibc behavior under real Wayland and Qt event-loop pressure
- kernel DMA-BUF fd passing for the hardware path

#### Layer 2, DRM, firmware, input, enumeration

Status: **build-visible and boot-visible, not runtime-trusted**

What is true now:

- redox-drm builds with AMD and Intel drivers
- amdgpu builds
- firmware-loader, evdevd, and udev-shim register at boot

What still needs proof:

- actual firmware loading by a real consumer
- actual input flow from Redox input sources into compositor-visible event devices
- actual scheme:drm registration and basic KMS query behavior in runtime
- actual AMD and Intel hardware-driver behavior on target machines

#### Layer 3, graphics userland interface

Status: **software path builds, hardware path blocked**

What is true now:

- Mesa EGL, GBM, and GLES2 build
- libdrm and libdrm_amdgpu build
- Qt6 OpenGL and EGL build
- QEMU proof still uses llvmpipe

What still needs proof:

- hardware renderer path through real DRM and real GPU drivers
- GBM allocation on the hardware path
- EGL and GLES stability on the hardware path

#### Layer 4, Wayland protocol and compositor

Status: **partial runtime proof, not complete**

What is true now:

- libwayland and wayland-protocols build
- smallvil is the bounded first runtime target
- smallvil reaches early initialization in QEMU

What still needs proof:

- a complete compositor session
- input routed into the compositor
- Qt6 client display in that compositor

#### Layer 5, Qt6 and KF6

Status: **major build milestone complete, runtime still thin**

What is true now:

- Qt6 builds across core, widgets, DBus, Wayland, OpenGL, and EGL
- qtdeclarative and qtwayland build
- all 32 KF6 frameworks build

What still needs proof:

- real Qt6 Wayland client behavior on Redox
- behavior of QML-heavy pieces under the no-JIT path
- broader networking semantics needed before QtNetwork can be enabled

#### Layer 6, KWin session shell

Status: **experimental and blocked**

What is true now:

- recipes exist
- `redbear-kde` exists
- several KWin-adjacent packages already build

What still needs proof:

- replacement of shimmed and stubbed blockers with real enough dependencies
- KWin compile success against honest dependencies
- KWin runtime as the compositor

#### Layer 7, KDE Plasma session

Status: **not yet proven**

What is true now:

- Plasma recipe surfaces exist
- the stack is far enough along that this is now a session-assembly problem, not a package-startup problem

What still needs proof:

- plasma-workspace integration
- plasma-desktop integration
- panel, launcher, file manager, settings, and session service behavior

### Dependency-stack conclusion

The shortest honest path is not "port more packages". The shortest honest path is "validate the
substrate, finish one software compositor path, finish one KWin session path, finish one Plasma
session path, then land the real hardware renderer path in parallel".

## Phased Work Plan

This plan uses fresh Phase 1 through Phase 5 numbering and does not reuse the old P0 through P6
scheme.

### Phase 1: Runtime Substrate Validation

**Duration:** 4 to 6 weeks

**Goal:** turn the lowest desktop-facing layers from build-visible into runtime-trusted.

This phase matters more than any other because it removes ambiguity from the entire stack. Without
it, later compositor or KDE failures will be impossible to classify correctly.

#### Core work

1. Validate relibc POSIX APIs against real consumers, especially libwayland and Qt6 runtime paths.
2. Validate the evdevd path from Redox input schemes through to `/dev/input/eventX` behavior.
3. Validate udev-shim device enumeration semantics for the current compositor and input stack.
4. Validate firmware-loader and `scheme:firmware` with real firmware blobs and a real consumer path.
5. Validate `scheme:drm/card0` registration and bounded KMS queries in QEMU.
6. Produce a repeatable runtime-service health check for the `redbear-wayland` slice.

#### Why this phase exists

The repo already compiles the lower desktop stack. What it lacks is evidence that the lower stack
behaves correctly under real use. Phase 1 is where builds become runtime-trusted enough to support
the first serious compositor pass.

#### Deliverables

##### relibc runtime validation set

Validate the relibc surfaces already present in-tree:

- signalfd
- timerfd
- eventfd
- open_memstream
- F_DUPFD_CLOEXEC
- MSG_NOSIGNAL
- bounded waitid
- bounded shared-memory and semaphore paths used by Qt6

The standard here is not just "the symbol exists". The standard is that real consumers can use the
API without hidden workarounds, hangs, or broken semantics.

##### evdev input validation set

Validate the current input chain end to end:

- input source emits events
- evdevd exposes expected event devices
- keyboard events arrive with correct semantics
- mouse events arrive with correct semantics

##### udev-shim validation set

Validate that current consumers can discover and classify the devices they need. Full Linux parity
is not required. Sufficient enumeration for the current desktop path is required.

##### firmware-loader validation set

Validate firmware loading with real blobs and a real consumer path. Scheme registration alone is not
enough. The blob must be requestable, discoverable, loadable, and consumable at runtime.

##### redox-drm runtime-surface validation set

Validate bounded runtime behavior first in QEMU:

- scheme registration for `scheme:drm/card0`
- basic KMS queries
- no startup-class failures in the redox-drm path

#### Acceptance criteria

Phase 1 is complete when all of the following are true:

- `redbear-wayland` boots in the bounded validation environment
- Phase 1 runtime services register without startup errors
- relibc runtime checks pass for the selected desktop-facing consumers
- the input path reaches evdevd and yields expected event nodes and bounded test events
- udev-shim exposes the expected bounded device view
- firmware-loader successfully serves at least one real consumer path with real blobs
- `scheme:drm/card0` registers and answers bounded basic queries

#### Exit statement

At the end of Phase 1, the repo should be able to say: the desktop substrate is no longer only a
build artifact. It is runtime-trusted enough to support a compositor completion pass.

### Phase 2: Wayland Compositor Runtime Proof

**Duration:** 4 to 6 weeks

**Goal:** produce the first working Wayland compositor session using software rendering.

This phase stays intentionally narrow. The first complete compositor proof should happen in the
smallest runtime target available, which is still smallvil.

#### Core work

1. Complete the current smallvil runtime path.
2. Wire evdevd input into the compositor.
3. Wire Mesa software rendering through GBM and EGL.
4. Get a Qt6 widget application to display through the compositor.

#### Why smallvil remains the right target

Jumping straight to KWin would combine too many unknowns: compositor runtime, input, QML, session
services, dependency scaffolding, and desktop-shell behavior. smallvil is smaller, easier to debug,
and already present. It is the right place to finish the first software compositor proof.

#### Deliverables

##### complete smallvil runtime path

The current proof stops during early initialization. This phase completes the path into a usable
session.

##### input wired into compositor

Keyboard and mouse must work through the current Redox input stack, not through an artificial bypass.

##### software rendering path confirmed

The proven renderer for this phase is LLVMpipe through Mesa, GBM, and EGL. That is acceptable. The
goal is correctness of compositor and client behavior, not hardware acceleration yet.

##### Qt6 smoke client on Wayland

The first meaningful desktop-facing end-to-end proof is a real Qt6 Wayland client window appearing
inside the compositor.

#### Acceptance criteria

Phase 2 is complete when all of the following are true:

- smallvil launches into a working session in QEMU
- keyboard and mouse work through the current input stack
- Mesa software rendering works through GBM and EGL
- `qt6-wayland-smoke` shows a window inside the compositor in QEMU

#### Exit statement

At the end of Phase 2, the repo should be able to say: Red Bear OS has a working software-rendered
Wayland compositor path with a visible Qt6 client.

### Phase 3: Hardware GPU Enablement

**Duration:** 12 to 20 weeks

**Goal:** replace the software-only graphics proof with real hardware-accelerated display output and
rendering.

This is the highest-uncertainty phase. It includes new kernel work, real hardware-driver proof, and
the first true Mesa hardware path on Redox.

#### Core work

1. Add kernel DMA-BUF fd passing.
2. Validate redox-drm AMD and Intel drivers on real hardware.
3. Validate Mesa hardware rendering path, including real renderer identity.
4. Validate GBM buffer allocation through the hardware path.

#### Why this phase is separate

The software compositor path is the fastest honest route to proving compositor and client behavior.
The hardware path is a different class of systems work. It should run in parallel with later KWin
and Plasma assembly instead of blocking everything else.

#### Deliverables

##### kernel DMA-BUF fd passing

This is the gating feature for the accelerated desktop path. Without it, hardware-accelerated KDE
is not a credible target.

##### real AMD hardware validation

Validate on representative AMD hardware:

- device detection
- MMIO mapping
- firmware loading
- connector detection
- mode enumeration
- bounded modeset proof

##### real Intel hardware validation

Validate on representative Intel hardware:

- device detection
- MMIO mapping
- connector detection
- mode enumeration
- bounded modeset proof

##### Mesa hardware rendering proof

Validate the actual hardware renderer path rather than llvmpipe fallback. For AMD the target is the
radeonsi path. For Intel the target is the intended real Intel hardware path available in the stack.

#### Acceptance criteria

Phase 3 is complete when all of the following are true:

- kernel DMA-BUF fd passing exists and has focused proof coverage
- `modetest -M amd` shows display modes on real AMD hardware
- the equivalent Intel DRM query path shows display modes on real Intel hardware
- the compositor runs through the hardware path rather than llvmpipe on at least one real AMD class
  and one real Intel class
- runtime evidence shows a hardware-backed renderer rather than software fallback

#### Exit statement

At the end of Phase 3, the repo should be able to say: Red Bear OS can drive real display hardware
and run the compositor on a hardware-accelerated path.

### Phase 4: Desktop Session Assembly

**Duration:** 6 to 10 weeks

**Goal:** turn the compositor proof into a real desktop-session substrate centered on KWin.

This phase starts after Phase 2. It does not need to wait for the full hardware path. KWin can come
up first on the software renderer and later inherit the accelerated renderer once Phase 3 lands.

#### Core work

1. Resolve KWin shimmed and stubbed blockers.
2. Get KWin to compile with real enough dependencies.
3. Launch KWin as the Wayland compositor.
4. Validate libinput backend behavior.
5. Validate D-Bus session behavior.
6. Validate seatd for the bounded KWin session model.

#### blocked dependency set that must be closed

- kirigami stub-only state
- heavy kio shim state where it blocks honest session claims
- libepoxy
- libudev
- lcms2
- libdisplay-info

#### Deliverables

##### honest KWin build

The milestone is not just that a recipe exists. The milestone is that KWin builds without fake
dependency satisfaction for core runtime behavior.

##### KWin runtime as compositor

KWin must launch as the compositor and own the display path.

##### session services for bounded desktop use

D-Bus session behavior and seatd behavior must be good enough for the bounded KWin target this plan
claims. Linux parity is not required. Correct bounded behavior is required.

#### Acceptance criteria

Phase 4 is complete when all of the following are true:

- KWin builds against real enough dependencies to support honest runtime claims
- KWin launches as the compositor
- KWin takes over display output in the bounded session path
- keyboard and mouse work through the KWin session path
- required D-Bus session behavior for the bounded KWin path works
- seatd behavior is validated for the bounded KWin session model

#### Exit statement

At the end of Phase 4, the repo should be able to say: Red Bear OS has a working Wayland desktop
session substrate centered on KWin.

### Phase 5: KDE Plasma Session

**Duration:** 8 to 12 weeks

**Goal:** boot into a KDE Plasma session with the essential desktop shell and session services
working.

This is the final desktop product phase. By this point the remaining work should mostly be session
assembly, application integration, and shell behavior.

#### Core work

1. Complete plasma-workspace compilation and integration.
2. Complete plasma-desktop compilation and integration.
3. Get the shell, panel, and launcher visible and usable.
4. Get settings and file-manager paths working.
5. Provide bounded network and audio integration suitable for the session claim.

#### Deliverables

##### Plasma shell

The minimum target is not a screenshot. The session must show the shell, panel, and launcher and be
stable through basic interaction.

##### application and settings path

At least one real file-manager path and one settings path must work. Otherwise the session is still
too incomplete to count as a desktop.

##### bounded desktop-service integration

For this phase the question is narrow: can the Plasma session boot into a usable desktop with bounded
network and audio integration. The long-term subsystem plans remain separate.

#### Acceptance criteria

Phase 5 is complete when all of the following are true:

- `redbear-kde` boots into a KDE Plasma session
- KWin is the active compositor
- the Plasma shell, panel, and launcher appear
- an application can be launched from the session
- a file-manager path works through the current kio integration
- a settings path works
- bounded network and audio integration exist for the claimed session profile

#### Exit statement

At the end of Phase 5, the repo should be able to say one of two things:

- if Phase 3 is still incomplete: Red Bear OS has a software-rendered KDE Plasma session on Wayland
- if Phase 3 is complete: Red Bear OS has a hardware-accelerated KDE Plasma session on Wayland

## Critical Path

### primary path to a software-rendered KDE session

```text
Phase 1, runtime substrate validation
  -> Phase 2, software Wayland compositor proof
    -> Phase 4, KWin desktop-session assembly
      -> Phase 5, KDE Plasma session
```

This is the shortest honest path to a KDE desktop claim.

### parallel hardware path

```text
Phase 1, runtime substrate validation
  -> Phase 3, hardware GPU enablement

Phase 3 proceeds in parallel with Phase 4 where possible

Phase 3 + Phase 4 + Phase 5
  -> hardware-accelerated KDE Plasma desktop
```

### why Phase 1 is the real gate

Phase 1 is the true gateway because it converts lower-layer package progress into runtime trust.
Without it, Phase 2, Phase 4, and Phase 5 failures will be misdiagnosed.

### why Phase 2 comes before KWin

The first complete compositor proof should happen in the smallest environment. smallvil is smaller,
already present, and easier to debug than KWin. It isolates compositor, input, and Qt client issues
before session-shell complexity is added.

### why Phase 3 should not block Phase 4

Hardware acceleration is critical, but KWin and Plasma also have their own blockers: dependency
cleanup, session services, and compositor integration. Those can be solved on the software renderer
while the hardware path matures.

### critical-path summary

The execution order this repo should present is:

1. validate the runtime substrate
2. prove one software compositor path
3. assemble one KWin session path
4. assemble one Plasma session path
5. land hardware acceleration in parallel

## Risk Register

| ID | Risk | Likelihood | Impact | Why it matters | Mitigation |
|---|---|---|---|---|---|
| R1 | relibc runtime gaps are worse than build evidence suggests | Medium | High | Qt6 and Wayland may still fail at runtime even though they build | validate with real consumers in Phase 1 |
| R2 | kernel DMA-BUF fd passing is a new feature with uncertain scope | High | High | hardware acceleration depends on it | isolate design and proof early in Phase 3 |
| R3 | AMD or Intel real-hardware validation reveals fundamental driver issues | High | High | compile success may not survive real modesetting or rendering | validate AMD and Intel separately on representative hardware |
| R4 | KWin porting needs significantly more patches than estimated | Medium | High | KWin sits on the desktop critical path | finish smallvil proof first, then attack KWin with cleaner lower-layer evidence |
| R5 | Kirigami and other QML-heavy pieces do not behave acceptably with QML JIT disabled | Medium | Medium to High | Plasma shell may build but behave badly | keep QML-heavy runtime proof explicit in Phase 4 and Phase 5 |
| R6 | Mesa hardware rendering needs Redox-specific winsys work beyond current estimates | Medium | High | hardware acceleration may stall after modesetting starts working | separate display proof from renderer proof |
| R7 | linux-kpi compatibility gaps only appear during real-hardware execution | High | Medium to High | compile-only success can hide runtime failures | budget for hardware-driven compatibility fixes in Phase 3 |

## Timeline

### planning assumptions

These estimates assume 2 developers, usable access to representative AMD and Intel hardware, and no
major regression from unrelated upstream refresh during the desktop push. They do not assume perfect
first-pass success on real hardware.

### phase estimates with 2 developers

| Phase | Estimate | Notes |
|---|---|---|
| Phase 1, Runtime Substrate Validation | 4 to 6 weeks | must finish honestly before claiming runtime trust |
| Phase 2, Wayland Compositor Runtime Proof | 4 to 6 weeks | can overlap with late Phase 1 cleanup |
| Phase 3, Hardware GPU Enablement | 12 to 20 weeks | parallel track after Phase 1 |
| Phase 4, Desktop Session Assembly | 6 to 10 weeks | starts after Phase 2 |
| Phase 5, KDE Plasma Session | 8 to 12 weeks | starts after Phase 4 |

### total duration with 2 developers

#### to software-rendered KDE Plasma on Wayland

- **22 to 34 weeks**
- roughly **6 to 8 months**

This path is Phase 1 + Phase 2 + Phase 4 + Phase 5.

#### to hardware-accelerated KDE Plasma on Wayland

- **34 to 54 weeks**
- roughly **8 to 13 months**

This path is Phase 1 + Phase 2 + Phase 3 in parallel + Phase 4 + Phase 5.

### rough overlap model

```text
Weeks 1 to 6
  Phase 1, runtime substrate validation

Weeks 4 to 12
  Phase 2, software compositor proof

Weeks 7 to 26
  Phase 3, hardware GPU enablement

Weeks 13 to 22
  Phase 4, KWin session assembly

Weeks 23 to 34
  Phase 5, KDE Plasma session
```

This is an intended overlap shape, not a guaranteed calendar.

### one-developer estimate

With 1 developer, the overall timeline is roughly 1.5x to 2x the two-developer estimates.

Practical meaning:

- software-rendered KDE path: about 9 to 16 months
- hardware-accelerated KDE path: about 12 to 27 months

The wider range reflects the loss of useful parallelism between hardware work and session work.

### timeline conclusion

The software-rendered KDE target is no longer a greenfield multi-year fantasy. The hardware-
accelerated KDE target is still a serious systems milestone because Phase 3 carries the widest
uncertainty band.

## Relationship to Other Plans

This is the canonical document for the desktop path from console boot to KDE Plasma on Wayland. It
does not replace every subsystem-specific plan. It sets the ordering, scope, and acceptance language
for the desktop path while deeper subsystem documents retain their detailed ownership.

### primary supporting plans

- `local/docs/RELIBC-COMPLETENESS-AND-ENHANCEMENT-PLAN.md` for relibc completeness detail,
  ownership of patch-carried behavior, and deeper evidence tracking
- `local/docs/AMD-FIRST-INTEGRATION.md` for deeper GPU-driver and firmware detail, with the caveat
  that this desktop plan uses equal-priority AMD and Intel targeting
- `local/docs/QT6-PORT-STATUS.md` for Qt6, KF6, KWin blocker, shim, and stub status
- `local/docs/DESKTOP-STACK-CURRENT-STATUS.md` for the short current-state desktop truth summary
- `local/docs/IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md` for controller, IRQ, MSI, MSI-X,
  and IOMMU quality work that supports later hardware desktop validation
- `local/docs/INPUT-SCHEME-ENHANCEMENT.md` for deeper input-path design if the current chain needs
  structural cleanup beyond Phase 1 validation
- `local/docs/P2-AMD-GPU-DISPLAY.md` for the code-complete AMD display status and concrete AMD
  validation targets such as `modetest -M amd`

### how to use this plan with the supporting plans

Read this document first for execution order, current claim language, completion criteria, and the
critical path. Read the subsystem plans for the exact relibc, driver, package, or input details
behind those higher-level phases.
