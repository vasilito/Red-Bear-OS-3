# Red Bear OS Bluetooth Implementation Plan

## Purpose

This document defines the current Bluetooth state in Red Bear OS, assesses what the repo now proves
through its bounded first slice, and lays out the conservative roadmap beyond that slice.

The goal is to describe what the repo currently proves, what it does not prove, what parts of the
Bluetooth stack are credible versus not credible today, and how Red Bear can grow from **one
bounded experimental Bluetooth slice** toward broader support without overstating current runtime
validation.

## Validation States

- **builds** — code exists in-tree and is expected to compile
- **boots** — image or service path reaches a usable runtime state
- **validated** — behavior has been exercised with real evidence for the claimed scope
- **experimental** — available for bring-up, but not support-promised
- **missing** — no in-tree implementation path is currently present

This repo should not treat planned scope as equivalent to implemented support.

## Current Repo State

### Summary

Broad Bluetooth support is still **missing** in Red Bear OS, but the repo now carries one bounded
experimental first slice.

That bounded slice now has a packaged in-guest checker (`redbear-bluetooth-battery-check`) and a
host-side QEMU harness (`./local/scripts/test-bluetooth-qemu.sh --check`). That QEMU validation
path is still being stabilized, so it should currently be described as **QEMU validation in
progress**, not as already validated for its claimed scope.

That first in-tree slice is deliberately narrow:

- standalone profile: `config/redbear-bluetooth-experimental.toml`
- transport daemon: `local/recipes/drivers/redbear-btusb/`
- host/control daemon: `local/recipes/system/redbear-btctl/`
- packaged in-guest checker: `redbear-bluetooth-battery-check`
- host QEMU harness: `local/scripts/test-bluetooth-qemu.sh`
- startup model: explicit startup only
- transport model: USB-attached only
- protocol scope: BLE-first only
- autospawn model: **not** wired to USB-class autospawn yet

This does **not** mean Red Bear has broad Bluetooth support. It means the repo now has one
experimental, profile-scoped bring-up surface instead of zero in-tree Bluetooth components.

What the repo *does* have is enough adjacent infrastructure to make a Bluetooth port plausible:

- userspace drivers and schemes as the standard architectural model
- USB and PCI hardware access patterns
- runtime diagnostics discipline
- D-Bus plumbing for later desktop compatibility work
- an evolving input and hotplug model that could later absorb Bluetooth HID devices

### Feasibility Summary

Implementing Bluetooth from scratch in Red Bear is **possible**, but only in a narrow, staged
sense.

The currently credible interpretation is:

- **feasible**: one experimental USB-attached controller path, one native host daemon, one BLE-first
  workload, one CLI/control surface, one hardware-specific validation slice
- **not yet credible**: broad controller coverage, full classic-Bluetooth parity, Bluetooth audio,
  or a desktop-equivalent BlueZ replacement in the first pass

So the answer is not “Bluetooth from scratch is unrealistic,” but it is also not “Bluetooth is just
one more driver.” The feasible first target is a deliberately small subsystem slice.

### Current Status Matrix

| Area | State | Notes |
|---|---|---|
| Bluetooth controller support | **experimental, USB discovery real** | `redbear-btusb` now probes `/scheme/usb/` for real Bluetooth class devices (USB class 0xE0/0x01/0x01 with vendor-ID fallback), parses descriptor files, assigns `hciN` names deterministically, and writes real adapter metadata into the status file. Daemon re-probes periodically. 8 tests pass including mock-filesystem USB discovery tests. |
| Bluetooth host stack | **experimental bounded slice** | `redbear-btctl` provides a BLE-first CLI/scheme surface with stub-backed scan plus bounded connect/disconnect control for stored bond IDs; the packaged checker and QEMU harness now exist for repeated guest validation, but the end-to-end QEMU proof is still in progress |
| Pairing / bond database | **experimental bounded slice** | `redbear-btctl` now persists conservative stub bond records under `/var/lib/bluetooth/<adapter>/bonds/`; connect/disconnect control targets those records, and the checker now verifies cleanup honesty, but this is still storage/control plumbing only, not real pairing or generic reconnect validation |
| Desktop Bluetooth API | **missing** | D-Bus exists generally, but no Bluetooth API/service exists |
| Bluetooth HID | **missing** | Could later build on input modernization work |
| Bluetooth audio | **missing** | Also blocked by broader desktop audio compatibility work |
| Runtime diagnostics | **partial implemented** | `redbear-info` now reports the bounded Bluetooth transport/control surfaces conservatively |

## Evidence Already In Tree

### Direct negative evidence

- `HARDWARE.md` says broad Wi-Fi and Bluetooth support is still incomplete even though bounded
  in-tree scaffolding now exists
- `local/docs/AMD-FIRST-INTEGRATION.md` treats `Wi-Fi/BT` as in progress with bounded wireless
  scaffolding present but validated connectivity still incomplete

### Positive architectural prerequisites

- `docs/01-REDOX-ARCHITECTURE.md` describes the userspace-daemon and scheme model Red Bear must
  follow for any new hardware subsystem
- `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md` sets the repo-wide rule that support claims must be
  profile-scoped and evidence-backed
- `local/docs/PROFILE-MATRIX.md` defines the validation-language model a future Bluetooth path must
  use
- `local/docs/BLUETOOTH-VALIDATION-RUNBOOK.md` now records the canonical QEMU operator path for the
  current bounded Battery Level slice
- `local/docs/INPUT-SCHEME-ENHANCEMENT.md` shows the direction of travel for per-device, hotplug,
  named input sources, which is relevant to later Bluetooth HID support
- `config/redbear-kde.toml` and related profile wiring already show D-Bus and desktop-session
  plumbing that later Bluetooth desktop integration might rely on

## Feasibility Constraints

### 1. Bluetooth is not one driver

Bluetooth in Red Bear cannot be treated as a single device daemon.

At minimum, Red Bear would need:

- controller transport handling
- adapter state management
- scanning and connection management
- pairing / bonding persistence
- higher protocol layers
- some user-facing control surface

This makes Bluetooth more like networking than like a single peripheral driver.

### 1.1 From-Scratch Scope Reality

Starting from zero, the minimum Red Bear-native Bluetooth stack is still several layers:

- controller transport
- HCI command/event handling
- adapter management
- LE scanning and connection lifecycle
- pairing / bonding policy and persistence
- ATT/GATT client work for the first useful BLE workload
- some observable user control/reporting surface

That means “from scratch” should be read as “new native subsystem assembly from several bounded
components,” not as “write a single daemon and call Bluetooth done.”

### 1.2 Minimum Native Subsystem Shape

The smallest Red Bear-native Bluetooth subsystem should be split into these pieces:

- one **controller transport daemon** for the first supported controller family
- one **host daemon** for adapter state, discovery, connection state, and higher protocol work
- one **user-facing control path** (CLI first, compatibility shim later if needed)
- one **pairing/bond persistence path** with a documented storage location and lifecycle

This should be treated as the minimum subsystem shape, not as optional later cleanup.

### 2. The correct architectural fit is native userspace daemons

The repo's existing system model strongly favors:

- userspace controller daemons
- explicit runtime services
- narrow compatibility shims when desktop software expects them
- profile-scoped support language

That means Bluetooth should be implemented as a native Red Bear subsystem, not described as a
wholesale Linux/BlueZ drop-in.

### 2.1 BlueZ-equivalent replacement is not the first feasible target

Red Bear should not frame the initial work as “reimplement BlueZ.”

That would pull in a much larger surface:

- broad controller/transport coverage
- full classic + BLE host functionality
- stable D-Bus compatibility shape
- profile breadth beyond the first bounded use case
- much more policy and persistence behavior than the repo currently needs for an initial milestone

The first feasible target is instead:

- native Red Bear controller transport daemon
- native Red Bear host daemon
- small native CLI/control path
- later compatibility shim only if real desktop consumers require one

### 2.2 Repo Placement Guidance

Unless upstream Redox grows a first-class Bluetooth path first, the initial Red Bear work should
live under `local/`:

- controller transport daemon recipes under `local/recipes/drivers/`
- host daemon, CLI, and compatibility-surface recipes under `local/recipes/system/`
- Red Bear-specific profile and service wiring under `config/redbear-*.toml`
- validation helpers under `local/scripts/`
- support-language and roadmap updates under `local/docs/`

That keeps the first implementation pass aligned with Red Bear's overlay model and rebase strategy.

### 3. Desktop parity is not the first milestone

The current repo does not justify claiming a full desktop Bluetooth user experience early.

The first realistic milestone is much smaller:

- one controller family
- one transport path
- one limited workload
- experimental support language only

### 3.1 BLE-first is materially more feasible than classic-first

The repo should treat **BLE-first** as the credible from-scratch path.

Why:

- it keeps the first useful workload smaller
- it avoids early pressure for classic-audio and broader profile parity
- it matches the repo's current “bounded experimental slice first” discipline
- it reduces the amount of early compatibility behavior that must be correct before any user value
  appears

Classic Bluetooth should therefore be treated as a later expansion, not as the first milestone.

### 3.2 First-Milestone Dependency

If the first supported controller is USB-attached, then Bluetooth Phase B1 depends directly on the
USB plan's controller and hotplug baseline work.

In practice that means Bluetooth should not claim a validated first controller path until the USB
stack can already support that controller family with stable enumeration, attach/detach behavior,
and honest runtime diagnostics.

### 3.3 Most credible first controller family

The most credible first controller family is:

- one **USB-attached BLE-capable adapter family** with simple host-facing initialization behavior

The least credible early targets are:

- UART-attached laptop-integrated controllers that require new board-specific transport bring-up
- broad “internal laptop Bluetooth” claims across mixed Intel/Realtek/MediaTek controller families
- controller families that immediately force a large firmware and vendor-protocol surface

### 4. Bluetooth scope depends on adjacent subsystems

Bluetooth HID depends on the modernized input path.

Bluetooth audio depends on the broader audio compatibility story that the repo already treats as
unfinished for desktop use.

That means the Bluetooth roadmap must stay sequenced and should not over-promise audio or broad
desktop integration early.

### 4.1 Native host-side Bluetooth is still required even if transport uses compatibility glue

The Wi-Fi plan already establishes an important repo rule: a compatibility layer can be useful below
the subsystem boundary, but it does not remove the need for a native Red Bear control plane.

Bluetooth should follow the same rule.

That means:

- transport-side glue or borrowed implementation ideas are acceptable
- but adapter management, support language, diagnostics, persistence, and user-visible control
  should still be modeled as native Red Bear runtime services

### 4.2 Bluetooth is gated more by USB maturity than by D-Bus presence

The repo already has D-Bus packages, but that does **not** make Bluetooth close to done.

The more important blockers are still:

- low-level controller/runtime trust
- USB controller correctness and hotplug quality
- a first real transport path
- native host-daemon correctness

So Bluetooth feasibility should be tied to controller/runtime credibility first, and only later to
desktop compatibility.

## Recommended From-Scratch Interpretation

The currently recommended interpretation of “implement Bluetooth from scratch” in this repo is:

1. do **not** start by chasing broad desktop Bluetooth parity
2. do **not** start by promising internal laptop Bluetooth across all machines
3. do start with one USB-attached adapter family
4. do build one native controller transport daemon plus one native host daemon
5. do target one BLE-first workflow
6. do keep all support language experimental and hardware-specific until real runtime proof exists

This is the narrowest version of “from scratch” that is still technically meaningful and worth
shipping.

## Recommended First Deliverable

The first deliverable Red Bear should actually target is:

- one standalone `redbear-bluetooth-experimental` profile slice
- one USB Bluetooth transport daemon
- one host/control daemon with bounded scan/status reporting
- one CLI-oriented control path
- one BLE-first workflow boundary
- one validation helper script plus one named runtime surface contract

That is small enough to be plausible and large enough to count as real Bluetooth work.

## Implementation Plan

### Repo-fit note

Some of the implementation targets below refer to upstream-managed trees such as
`recipes/core/base/source/...`.

In Red Bear, changes against those paths should be carried through the relevant patch carrier under
`local/patches/` until intentionally upstreamed. This plan names the technical integration point,
not a recommendation to edit upstream-managed trees outside Red Bear's normal overlay model.

### Phase B0 — Scope Freeze and Support Model

**Goal**: Decide what the first Bluetooth milestone actually is.

**What to do**:

- declare broad Bluetooth support as incomplete today while one bounded experimental slice exists
- define validation labels and support language for future Bluetooth work
- freeze the first milestone as **host-side, experimental, one controller family, one limited use
  case**
- keep desktop parity explicitly out of the first support claim

**Where**:

- `local/docs/PROFILE-MATRIX.md`
- `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md`
- this document

**Exit criteria**:

- Bluetooth scope is documented without vague “future wireless” wording

---

### Phase B1 — Controller Transport Baseline

**Goal**: Establish one real Bluetooth controller path.

**Recommended first target**:

- one USB-attached Bluetooth controller family, BLE-first

**Why**:

- Red Bear already has a USB hardware path
- USB diagnostics and controller visibility already exist
- it is the narrowest realistic controller baseline before considering broader wireless scope

**What to do**:

- implement one controller transport daemon
- expose adapter presence and basic control through a Red Bear-native runtime surface
- ensure the daemon fits the userspace service/scheme model
- keep the first daemon/controller contract narrow enough that the host daemon can be built around
  one stable transport instead of a generic multi-transport abstraction from day one

**Where**:

- `local/recipes/drivers/` for the first Red Bear Bluetooth transport daemon recipe
- `config/redbear-*.toml` for profile/package wiring
- `config/redbear-device-services.toml` or a sibling shared fragment if the daemon becomes a common
  service prerequisite
- `local/scripts/` for controller bring-up validation helpers

**Initial launch path**:

- for a USB-attached first controller, the long-term attach path should be through
  `recipes/core/base/source/drivers/usb/xhcid/drivers.toml` or its eventual Red Bear equivalent
  once a Bluetooth USB class match is ready
- before that exists, the first milestone may use explicit Red Bear service startup so the transport
  daemon can be validated without pretending that USB class autospawn is already solved
- the first in-tree slice now follows exactly that bounded rule: explicit startup only, with no
  claim that `xhcid` or another USB class matcher autospawns Bluetooth yet
- if Red Bear adds that xHCI class match before it exists upstream, it should be carried as a Red
  Bear base patch rather than as an unqualified direct tree edit

**Firmware note**:

- if the first supported controller family requires firmware upload, reuse the existing
  `firmware-loader` / shared device-service pattern instead of inventing a separate firmware path
- if that complexity is too high for the first milestone, choose a controller family that can be
  initialized without introducing a second firmware-loading architecture

**Dependency**:

- if the first controller is USB-attached, this phase is blocked on the USB plan's U1-U2 baseline
  being sufficiently stable for that controller family

**Exit criteria**:

- one supported Bluetooth controller can be detected and initialized repeatedly
- controller presence can be reported honestly at runtime
- attach/detach behavior is good enough that controller disappearance does not require reboot to
  recover the service path

---

### Phase B2 — Minimal Host Daemon

**Goal**: Create the first Red Bear-native Bluetooth service layer.

**What to do**:

- add one host daemon that owns adapter state
- support scanning and connect/disconnect for one limited workload
- add persistent pairing/bond storage only once the storage path is explicitly defined
- keep the control surface small and Red Bear-native
- keep classic-Bluetooth scope and audio/profile breadth explicitly out of this phase

**Where**:

- `local/recipes/system/` for the host-daemon recipe
- `config/redbear-*.toml` and init-service wiring for runtime startup
- `/var/lib/bluetooth/` as the first Red Bear-owned bond/state directory, created by profile or
  service wiring in the same style used for other runtime-state directories

**Minimum native surface**:

- adapter presence/state
- discovery / scan state
- connect / disconnect control
- bond database lifecycle rooted at `/var/lib/bluetooth/`
- failure reporting suitable for later `redbear-info` integration

**Current in-tree bounded slice**:

- `redbear-btctl` now ships a minimal file-backed bond store rooted at `/var/lib/bluetooth/<adapter>/bonds/`
- the CLI can add/list/remove **stub** bond records and reload them across process restarts
- the btctl scheme now exposes bounded connect/disconnect control plus read surfaces for connection state and last connect/disconnect results
- the bounded connect path only targets existing stub bond records and keeps connected bond IDs in daemon memory per adapter
- `redbear-info` now reports the bond-store path/count plus bounded connection/result metadata conservatively
- this is explicitly **not** real pairing, link-key exchange, trusted-device policy, validated reconnect behavior, real device traffic, or B3 BLE workload support

**Exit criteria**:

- the daemon can rediscover and reconnect to at least one target device class across repeated runs
- the daemon's runtime state is observable enough that future `redbear-info` integration is
  straightforward rather than guesswork

---

### Phase B3 — BLE-First User Value

**Goal**: Deliver the first actually useful Bluetooth capability without overreaching.

**Recommended first workload**:

- BLE-first rather than full classic Bluetooth parity
- specifically, one experimental **battery-sensor Battery Level read** using Battery Service
  `0000180f-0000-1000-8000-00805f9b34fb` and Battery Level characteristic
  `00002a19-0000-1000-8000-00805f9b34fb`

**Examples of acceptable first workloads**:

- one BLE sensor/control workflow
- one bounded BLE peripheral interaction that needs scan/connect/read/write/notify

**Examples of bad first-workload choices**:

- generic “all BLE works”
- Bluetooth audio
- broad HID support before the input plan matures

**What to do**:

- add scan/connect support for one BLE device type
- expose only the minimal behavior the chosen workload needs
- for the current B3 slice, that means **read only** for the experimental battery-sensor workload;
  this slice does **not** claim write support or notify support
- keep support language experimental and hardware-specific

**Where**:

- host-daemon implementation under `local/recipes/system/`
- tracked profile wiring in one explicitly experimental Red Bear profile slice named
  `redbear-bluetooth-experimental`
- validation helper in `local/scripts/`

**Recommended support slice**:

- start as one explicitly experimental tracked profile named `redbear-bluetooth-experimental`
  rather than claiming Bluetooth generically across all Red Bear images

**Exit criteria**:

- one real BLE device type works reliably on the chosen controller family

---

### Phase B4 — Input Integration

**Goal**: Prepare for Bluetooth HID in a way that matches Red Bear's planned input model.

**What to do**:

- build Bluetooth HID integration on top of the named-producer / per-device / hotplug-aware input
  direction already documented for `inputd`
- avoid introducing a second incompatible input plumbing path

**Where**:

- `recipes/core/base/source/drivers/inputd/`
- `local/docs/INPUT-SCHEME-ENHANCEMENT.md`

**Exit criteria**:

- Bluetooth input devices can appear as distinct recoverable input sources rather than as an opaque
  special case

---

### Phase B5 — Desktop Control Surface

**Goal**: Add higher-level control only after the native substrate exists.

**What to do**:

- start with a small Red Bear-native control path
- add a compatibility shim only if actual desktop consumers require it
- keep desktop integration explicitly separate from core Bluetooth correctness

**Where**:

- Red Bear-native CLI/tooling under `local/recipes/system/`
- any compatibility shim under `local/recipes/` with profile-specific wiring in desktop-oriented
  Red Bear configs
- later runtime reporting hooks in `local/recipes/system/redbear-info/`

**Why**:

Red Bear already uses the pattern of adding narrow compatibility surfaces where desktop software
expects them instead of importing a whole foreign subsystem model blindly.

**Exit criteria**:

- one desktop or user-facing consumer can manage the limited supported Bluetooth path without
  changing the underlying native architecture

---

### Phase B6 — Audio and Broader Class Expansion

**Goal**: Widen Bluetooth scope only after the substrate and adjacent stacks justify it.

**What to do**:

- defer Bluetooth audio until the broader Red Bear desktop-audio compatibility path is stronger
- defer broad classic Bluetooth parity until controller and host-daemon maturity are no longer the
  main risk
- decide later whether Bluetooth networking or additional classes are worth supporting

**Where**:

- later profile/package-group expansion in `config/redbear-*.toml`
- later runtime diagnostics and support-language updates in `local/docs/` and `redbear-info`

**Exit criteria**:

- later Bluetooth classes are added only after the repo can name real prerequisites and evidence

---

### Phase B7 — Validation Slice and Support Claims

**Goal**: Turn Bluetooth from an experimental prototype into a supportable Red Bear feature slice.

**What to do**:

- create a Bluetooth-focused validation path tied to a specific profile or package-group slice
- extend runtime diagnostics conservatively once Bluetooth runtime surfaces actually exist
- add hardware-target guidance and support labels

**Recommended first support language**:

- one explicitly experimental Red Bear profile named `redbear-bluetooth-experimental` for the first
  supported controller + workload combination

**Where**:

- `local/scripts/`
- `local/recipes/system/redbear-info/`
- `local/docs/PROFILE-MATRIX.md`
- `HARDWARE.md`

**Exit criteria**:

- at least one profile can honestly claim validated experimental Bluetooth support for named
  hardware and named workload scope

**Current in-tree interpretation**:

- the repo now has the packaged checker and QEMU harness needed to satisfy the narrower
  QEMU-scoped version of this exit criterion for one stub-backed Battery Level workload on
  `redbear-bluetooth-experimental`, but that QEMU proof is still in progress
- it does **not** satisfy a broader real-hardware or generic BLE exit criterion yet

## Support-Language Guidance

Until B1 through B3 exist, Red Bear should use language such as:

- “Bluetooth is not broadly supported yet”
- “only the bounded experimental Bluetooth slice exists in-tree”
- “Bluetooth remains a future implementation workstream beyond the documented first slice”

Once B1 through B3 begin to land, prefer:

- “experimental Bluetooth bring-up exists for one controller family”
- “Bluetooth support is limited to the documented workload and profile”

Avoid language such as:

- “Bluetooth works”
- “desktop Bluetooth is supported”
- “wireless support is complete”

unless the repo has profile-scoped validation evidence to justify those claims.

## Summary

Bluetooth in Red Bear today is still not broad support.

What now exists is one bounded experimental first slice: explicit-startup, USB-attached,
BLE-first, profile-scoped to `redbear-bluetooth-experimental`, with conservative stub bond-store
persistence rooted at `/var/lib/bluetooth/<adapter>/bonds/` plus bounded connect/disconnect control
that only targets those stored stub bond IDs, plus one experimental battery-sensor Battery Level
read result for the exact Battery Service / Battery Level UUID pair above. That slice can now be
built, booted in QEMU, and exercised by the packaged `redbear-bluetooth-battery-check` helper; the
repeated end-to-end QEMU proof is still being stabilized before it should be described as validated.

What makes it feasible is not any existing Bluetooth stack, but the surrounding Red Bear
architecture: userspace daemons, runtime services, diagnostic discipline, profile-scoped support
language, firmware/runtime-service patterns, and an evolving per-device input model.

The practical feasibility judgment is:

- **yes**, Bluetooth from scratch is possible in Red Bear
- **but only** as a bounded BLE-first, transport-constrained, experimental subsystem slice
- **and no**, the current repo does not justify treating broad Bluetooth or desktop Bluetooth parity
  as a near-term from-scratch target

That means the right Bluetooth implementation plan is conservative and staged:

1. freeze scope and support language
2. bring up one controller transport path
3. add one native host daemon
4. deliver one BLE-first workload
5. integrate input and desktop control only after the substrate exists
6. widen class coverage only when adjacent subsystems are ready

That is the most credible path to Bluetooth in Red Bear without over-claiming support that the repo
does not yet have.
