# 07 — Red Bear OS Implementation Plan

## Purpose

This is the canonical repository-level implementation plan for Red Bear OS.

It is not a historical phase diary and it is not a subsystem deep dive. Its job is to define:

- what Red Bear OS is trying to become,
- how Red Bear relates to upstream Redox,
- which profiles are real product surfaces,
- which workstreams are first-class,
- what the current state is,
- and what order of work best improves the project from here.

Detailed subsystem planning remains in focused documents under `local/docs/`.

## Repository Model

RedBearOS should be understood as an overlay distribution on top of Redox in the same way Ubuntu
relates to Debian.

- Redox is upstream.
- Red Bear carries integration, packaging, validation, and subsystem overlays on top.
- Upstream-owned source trees are refreshable working copies.
- Durable Red Bear state belongs in `local/patches/`, `local/recipes/`, `local/docs/`, and tracked
  Red Bear configs.

The project is in the right long-term shape only when refreshed upstream sources can be fetched,
Red Bear overlays can be reapplied, and the project still rebuilds successfully.

## Ownership Rules

### Upstream-owned layer

These are refreshable working inputs, not durable Red Bear storage:

- `recipes/*/source/`
- most of `recipes/` outside local overlay symlinks
- mainline configs such as `config/desktop.toml` and `config/minimal.toml`
- generated build outputs under `target/`, `build/`, `repo/`, and recipe-local `target/*`

### Red Bear-owned layer

These are the durable Red Bear source-of-truth paths:

- `local/patches/`
- `local/recipes/`
- `local/docs/`
- tracked Red Bear configs such as `config/redbear-*.toml`

### Upstream-first rule

For fast-moving upstream components, prefer upstream whenever upstream already solves the same
problem adequately.

Keep Red Bear patches only while they still provide unique value.

### WIP rule

If an upstream recipe or subsystem is still marked WIP, Red Bear treats it as a local project.

That means:

1. upstream WIP can be used as an input and reference,
2. but Red Bear should fix and ship from the local overlay while the work is still WIP,
3. and once upstream promotes that work to first-class supported status, Red Bear should reevaluate
   and prefer upstream where appropriate.

## Core Principles

### Preserve Redox architecture

- drivers and services remain userspace-first,
- system boundaries remain explicit,
- capability-oriented design remains intact,
- compatibility shims are acceptable when bounded and well-documented.

### Packaging is the integration layer

- functionality is delivered as packages,
- profiles are composed from packages and package groups,
- integration should prefer packaging, configuration, and overlays over invasive upstream rewrites.

### Validation over claims

- “builds” is not the same as “supported”,
- every user-visible claim should map to a profile,
- every support claim should be reproducible and evidence-backed.

## Product Surfaces

The first-class Red Bear profiles are:

- `redbear-minimal`
- `redbear-desktop`
- `redbear-wayland`
- `redbear-full`
- `redbear-kde`
- `redbear-live`

Each profile is a product surface, not just a build convenience.

### `redbear-minimal`

Primary reproducible baseline.

Scope:

- boot,
- package management,
- native wired networking,
- diagnostics,
- minimal service baseline.

### `redbear-desktop`

Main integration profile for base desktop/runtime work.

Scope:

- Orbital desktop path,
- runtime services,
- diagnostics,
- base user-facing system bring-up.

### `redbear-wayland`

Dedicated Wayland runtime validation profile.

Scope:

- narrow compositor/runtime path,
- explicit validation target for Wayland stack correctness,
- not a claim of full desktop completeness.

### `redbear-full`

Broader graphics/network/session plumbing profile.

Scope:

- desktop/runtime plumbing beyond the narrow Wayland validation slice,
- D-Bus presence,
- Qt base integration,
- broader integration surface before KDE session focus.

### `redbear-kde`

Dedicated KDE/Plasma bring-up profile.

Scope:

- KWin,
- Plasma session surfaces,
- session packaging and dependencies,
- explicit documentation of limitations while still incomplete.

### `redbear-live`

Live/demo/recovery profile.

Scope:

- diagnostics,
- recovery workflows,
- installability,
- demonstrable system identity.

## Current State Baseline

### Repository state summary

The current repo is no longer at a greenfield or “missing everything” stage.

The current evidence-backed baseline is:

- the Red Bear overlay model is documented and in active use,
- major local subsystem plans exist under `local/docs/`,
- native wired networking is present,
- Qt6 and major downstream desktop dependencies build,
- Wayland-facing relibc compatibility surfaces now rebuild from a refreshed upstream relibc source
  tree via local patch carriers,
- `libwayland` and `qtbase` build successfully from the reconstructed relibc state,
- KDE session work is in progress but not yet a stable runtime claim,
- USB, Wi-Fi, Bluetooth, and low-level controller quality remain first-class unfinished workstreams.

### What is current versus historical

Older P0–P6 wording remains useful for continuity, but it is not the canonical current execution
model anymore.

Use this document plus current `local/docs/` subsystem plans as the source of truth for current work
ordering.

## Workstream Order

The current repository-wide work order is:

1. repository discipline and overlay hygiene
2. reproducible profiles and validation surfaces
3. low-level controller and IRQ quality
4. USB maturity
5. Wi-Fi native control plane and first driver family
6. Bluetooth controller/host path
7. desktop/session compatibility on top of those runtime services
8. hardware validation and support labeling

These are all first-class targets, but they do not all have the same dependency weight.

### Blocker chain

The current blocker structure is:

```text
low-level controller / IRQ quality
  -> USB maturity
      -> realistic Bluetooth transport path

low-level controller / IRQ quality
  -> Wi-Fi driver bring-up
      -> native wireless control plane
          -> desktop-facing compatibility later
```

This means Red Bear should not present USB, Wi-Fi, Bluetooth, or low-level controller work as
optional polish. They are first-class subsystem targets, but they must be executed in dependency
order.

## Workstreams

### 1. Repository discipline and overlay hygiene

Goal:

- keep Red Bear-specific work identifiable,
- keep upstream refresh predictable,
- ensure durable overlays exist for active Red Bear-owned deltas,
- keep WIP migration logic explicit.

Current state:

- overlay model is documented,
- relibc preservation/reapply proof exists,
- WIP ownership policy is documented,
- documentation still needs cleaner indexing and some historical pruning.

Acceptance:

- refreshed upstream sources can be re-overlaid and rebuilt predictably,
- the canonical/current-vs-historical split is visible in docs,
- active Red Bear-owned deltas are preserved outside refreshable source trees.

### 2. Profiles and packaging

Goal:

- keep profiles reproducible,
- keep support surfaces obvious,
- keep package-group composition intentional.

Current state:

- tracked Red Bear profiles exist,
- profile roles are clearer than before,
- some older profile wording still overlaps with historical phase language.

Acceptance:

- each first-class profile has a documented role,
- profile behavior is reproducible,
- support labels are tied to profile-specific evidence.

### 3. Low-level controllers and IRQ quality

Goal:

- improve runtime trust in IRQ delivery, MSI/MSI-X, and IOMMU-adjacent infrastructure,
- turn compile-oriented infrastructure into runtime-proven substrate.

Current state:

- source and build evidence are good,
- runtime validation is thinner than desired,
- this remains a blocker for USB, Wi-Fi, Bluetooth, and reliable device/runtime claims.

Canonical plan:

- `local/docs/IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md`

Acceptance:

- runtime evidence exists for the claimed controller/IRQ scope,
- subsystem docs stop overstating compile-oriented proof.

### 4. USB maturity

Goal:

- mature the existing USB host/controller path into a more reliable subsystem,
- improve topology, hotplug, HID, storage, and observability confidence.

Current state:

- USB exists in-tree,
- support is still partial and variable,
- controller/runtime maturity still needs work,
- broader USB class support is not yet a safe claim.

Canonical plan:

- `local/docs/USB-IMPLEMENTATION-PLAN.md`

Acceptance:

- USB support is described honestly by validation state,
- controller/runtime quality is no longer the main blocker for first Bluetooth transport work.

### 5. Wi-Fi

Goal:

- add one bounded experimental Wi-Fi path that fits Red Bear’s native architecture.

Current state:

- one bounded experimental Intel Wi-Fi path is now in-tree,
- the corresponding tracked validation profile is `redbear-wifi-experimental`,
- `linux-kpi` now carries early wireless-subsystem compatibility scaffolding in addition to the
  earlier low-level helper layer,
- the native control-plane/profile/reporting stack now has bounded scan/connect/disconnect flows,
  including profile-manager start/stop wiring for the current Wi-Fi path,
- packaged in-target Wi-Fi validation/capture commands now exist for the current bounded Intel path
  (`redbear-phase5-wifi-check`, `redbear-phase5-wifi-link-check`, `redbear-phase5-wifi-capture`,
  `redbear-phase5-wifi-run`, `redbear-phase5-wifi-analyze`),
- the separate `redbear-phase5-network-check` / `test-phase5-network-qemu.sh` path on `redbear-full`
  now proves bounded desktop/network plumbing in QEMU and should not be confused with the Wi-Fi
  plan's later real-hardware Phase W5 completion criteria,
- real hardware scan/auth/association/data-path proof is still missing,
- `linux-kpi` is still not the Wi-Fi architecture by itself.

Canonical plan:

- `local/docs/WIFI-IMPLEMENTATION-PLAN.md`

Acceptance:

- one experimental Wi-Fi family is packaged and evidence-backed,
- post-association handoff to the existing network stack is real,
- the bounded station-mode lifecycle is visible through driver, control-daemon, profile-manager,
  and runtime-reporting surfaces,
- desktop-facing Wi-Fi claims remain honest and bounded.

### 6. Bluetooth

Goal:

- add a bounded host-side Bluetooth path after its transport/runtime dependencies are credible.

Current state:

- one bounded in-tree BLE-first experimental slice now exists,
- architecture direction is documented,
- the currently credible implementation target is one bounded BLE-first host-side slice rather than
  broad desktop Bluetooth parity,
- transport dependency on USB maturity remains explicit.

Canonical plan:

- `local/docs/BLUETOOTH-IMPLEMENTATION-PLAN.md`

Acceptance:

- one controller path, one host path, and one bounded BLE-first user-facing workflow exist with
  experimental support language.

### 7. Graphics, Wayland, and desktop/session compatibility

Goal:

- turn the current build-visible desktop stack into runtime-trusted session surfaces.

Current state:

- relibc compatibility work is materially improved,
- `libwayland` and `qtbase` build,
- Qt6 base stack builds,
- KDE recipe/session work exists,
- runtime trust is still behind build success.

Canonical references:

- `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` — canonical desktop path from console to hardware-accelerated KDE Plasma on Wayland
- `local/docs/QT6-PORT-STATUS.md`
- `local/docs/DESKTOP-STACK-CURRENT-STATUS.md`
- `docs/03-WAYLAND-ON-REDOX.md` — historical Wayland implementation rationale
- `docs/05-KDE-PLASMA-ON-REDOX.md` — historical KDE implementation rationale

Acceptance:

- `redbear-wayland` remains the narrow runtime validation slice,
- `redbear-full` remains the broader desktop/session plumbing slice,
- `redbear-kde` reaches documented session viability with honest limitations.

### 8. Hardware validation and support labeling

Goal:

- convert “builds” and “boots” into explicit support claims with evidence.

Current state:

- validation language is better than before,
- runtime support labeling still needs more consistent central presentation.

Acceptance:

- support claims are profile-scoped,
- evidence is reproducible,
- the project has a clearer matrix of current, experimental, and validated surfaces.

## Canonical Subsystem Documents

The current subsystem plans to treat as first-class are:

- `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` — canonical desktop path plan
- `local/docs/IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md`
- `local/docs/USB-IMPLEMENTATION-PLAN.md`
- `local/docs/WIFI-IMPLEMENTATION-PLAN.md`
- `local/docs/BLUETOOTH-IMPLEMENTATION-PLAN.md`
- `local/docs/RELIBC-COMPLETENESS-AND-ENHANCEMENT-PLAN.md`
- `local/docs/RELIBC-IPC-ASSESSMENT-AND-IMPROVEMENT-PLAN.md`
- `local/docs/QT6-PORT-STATUS.md`

The older architecture/roadmap docs under `docs/01`–`docs/05` remain useful, but they should be
read together with status notes and the newer local subsystem docs.

## Acceptance Model

Red Bear should use simple evidence language consistently:

- `builds`
- `boots`
- `enumerates`
- `usable`
- `validated`
- `experimental`

Do not compress these into a single “supported” claim.

## Immediate Documentation Priorities

The highest-value documentation follow-ups from the current state are:

1. add a clearer document-status matrix in `docs/README.md`,
2. add a WIP migration ledger for major upstream-WIP-to-local-overlay transitions,
3. add a concise script behavior matrix for sync/fetch/apply/build helper scripts,
4. continue pruning obsolete local overlays only after refreshed-upstream reapply proofs confirm
   upstream coverage is sufficient.

## Bottom Line

Red Bear OS is no longer at the stage where the main question is “can we start?”.

The current state is a transition from compile-oriented subsystem accumulation toward a stricter,
profile-driven, overlay-disciplined, evidence-backed system project. The implementation plan must now
optimize for:

- predictable upstream refresh,
- durable local overlays,
- honest support language,
- and execution order that respects the real blocker chain.

That is the current master plan.
