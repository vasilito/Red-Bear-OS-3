# Red Bear OS Bluetooth Implementation Plan

## Purpose

This document defines the current Bluetooth state in Red Bear OS and lays out a conservative,
implementation-focused roadmap for adding Bluetooth support.

The goal is not to imply that Bluetooth already exists in-tree. The goal is to describe what the
repo currently proves, what it does not prove, and how Red Bear can grow from **no Bluetooth
support** to a realistic, host-side Bluetooth stack that fits the existing Redox/Red Bear
architecture.

## Validation States

- **builds** — code exists in-tree and is expected to compile
- **boots** — image or service path reaches a usable runtime state
- **validated** — behavior has been exercised with real evidence for the claimed scope
- **experimental** — available for bring-up, but not support-promised
- **missing** — no in-tree implementation path is currently present

This repo should not treat planned scope as equivalent to implemented support.

## Current Repo State

### Summary

Bluetooth is currently **missing** in Red Bear OS.

The repo has no first-class Bluetooth daemon, no controller transport path, no host stack, no
documented app-facing Bluetooth API, and no profile that can honestly claim Bluetooth support.

What the repo *does* have is enough adjacent infrastructure to make a Bluetooth port plausible:

- userspace drivers and schemes as the standard architectural model
- USB and PCI hardware access patterns
- runtime diagnostics discipline
- D-Bus plumbing for later desktop compatibility work
- an evolving input and hotplug model that could later absorb Bluetooth HID devices

### Current Status Matrix

| Area | State | Notes |
|---|---|---|
| Bluetooth controller support | **missing** | No HCI transport daemon in-tree |
| Bluetooth host stack | **missing** | No L2CAP / ATT / SMP / GATT / RFCOMM stack evidence |
| Pairing / bond database | **missing** | No Bluetooth-specific persistence path documented |
| Desktop Bluetooth API | **missing** | D-Bus exists generally, but no Bluetooth API/service exists |
| Bluetooth HID | **missing** | Could later build on input modernization work |
| Bluetooth audio | **missing** | Also blocked by broader desktop audio compatibility work |
| Runtime diagnostics | **partial prerequisite exists** | `redbear-info` model exists, but no Bluetooth integration exists |

## Evidence Already In Tree

### Direct negative evidence

- `HARDWARE.md` says Wi-Fi and Bluetooth are not supported yet
- `local/docs/AMD-FIRST-INTEGRATION.md` marks `Wi-Fi/BT` as missing

### Positive architectural prerequisites

- `docs/01-REDOX-ARCHITECTURE.md` describes the userspace-daemon and scheme model Red Bear must
  follow for any new hardware subsystem
- `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md` sets the repo-wide rule that support claims must be
  profile-scoped and evidence-backed
- `local/docs/PROFILE-MATRIX.md` defines the validation-language model a future Bluetooth path must
  use
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

### 1.1 Minimum Native Subsystem Shape

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

### 2.1 Repo Placement Guidance

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

### 3.1 First-Milestone Dependency

If the first supported controller is USB-attached, then Bluetooth Phase B1 depends directly on the
USB plan's controller and hotplug baseline work.

In practice that means Bluetooth should not claim a validated first controller path until the USB
stack can already support that controller family with stable enumeration, attach/detach behavior,
and honest runtime diagnostics.

### 4. Bluetooth scope depends on adjacent subsystems

Bluetooth HID depends on the modernized input path.

Bluetooth audio depends on the broader audio compatibility story that the repo already treats as
unfinished for desktop use.

That means the Bluetooth roadmap must stay sequenced and should not over-promise audio or broad
desktop integration early.

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

- declare Bluetooth support as absent today
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

- one USB-attached Bluetooth controller family

**Why**:

- Red Bear already has a USB hardware path
- USB diagnostics and controller visibility already exist
- it is the narrowest realistic controller baseline before considering broader wireless scope

**What to do**:

- implement one controller transport daemon
- expose adapter presence and basic control through a Red Bear-native runtime surface
- ensure the daemon fits the userspace service/scheme model

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

---

### Phase B2 — Minimal Host Daemon

**Goal**: Create the first Red Bear-native Bluetooth service layer.

**What to do**:

- add one host daemon that owns adapter state
- support scanning and connect/disconnect for one limited workload
- add persistent pairing/bond storage only once the storage path is explicitly defined
- keep the control surface small and Red Bear-native

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

**Exit criteria**:

- the daemon can rediscover and reconnect to at least one target device class across repeated runs

---

### Phase B3 — BLE-First User Value

**Goal**: Deliver the first actually useful Bluetooth capability without overreaching.

**Recommended first workload**:

- BLE-first rather than full classic Bluetooth parity

**What to do**:

- add scan/connect support for one BLE device type
- expose minimal read/write/notify behavior if the chosen workload needs it
- keep support language experimental and hardware-specific

**Where**:

- host-daemon implementation under `local/recipes/system/`
- package-group wiring in one explicitly experimental Red Bear package-group slice named
  `bluetooth-experimental`
- validation helper in `local/scripts/`

**Recommended support slice**:

- start as one explicitly experimental package-group addition named `bluetooth-experimental`
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

- one explicitly experimental Red Bear package-group slice named `bluetooth-experimental` for the
  first supported controller + workload combination

**Where**:

- `local/scripts/`
- `local/recipes/system/redbear-info/`
- `local/docs/PROFILE-MATRIX.md`
- `HARDWARE.md`

**Exit criteria**:

- at least one profile can honestly claim validated experimental Bluetooth support for named
  hardware and named workload scope

## Support-Language Guidance

Until B1 through B3 exist, Red Bear should use language such as:

- “Bluetooth is not supported yet”
- “Bluetooth remains missing in-tree”
- “Bluetooth is a future implementation workstream”

Once B1 through B3 begin to land, prefer:

- “experimental Bluetooth bring-up exists for one controller family”
- “Bluetooth support is limited to the documented workload and profile”

Avoid language such as:

- “Bluetooth works”
- “desktop Bluetooth is supported”
- “wireless support is complete”

unless the repo has profile-scoped validation evidence to justify those claims.

## Summary

Bluetooth in Red Bear today is not partial support — it is **missing**.

What makes it feasible is not any existing Bluetooth stack, but the surrounding Red Bear
architecture: userspace daemons, runtime services, diagnostic discipline, profile-scoped support
language, and an evolving per-device input model.

That means the right Bluetooth implementation plan is conservative and staged:

1. freeze scope and support language
2. bring up one controller transport path
3. add one native host daemon
4. deliver one BLE-first workload
5. integrate input and desktop control only after the substrate exists
6. widen class coverage only when adjacent subsystems are ready

That is the most credible path to Bluetooth in Red Bear without over-claiming support that the repo
does not yet have.
