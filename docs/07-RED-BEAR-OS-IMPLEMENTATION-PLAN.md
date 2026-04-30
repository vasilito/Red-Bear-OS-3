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

The tracked Red Bear compile targets are:

- `redbear-mini`
- `redbear-full`
- `redbear-grub`

These are the only supported compile targets. Older names such as `redbear-minimal`,
`redbear-desktop`, `redbear-wayland`, `redbear-kde`, `redbear-live`, `redbear-live-mini`,
and `redbear-live-full` may still appear in historical notes or legacy implementation details,
but they are not the current compile-target surface.

### `redbear-mini`

Primary reproducible baseline.

Scope:

- boot,
- package management,
- native wired networking,
- diagnostics,
- minimal service baseline.

### `redbear-full`

Broader desktop/network/session plumbing profile.

Scope:

- desktop/runtime plumbing,
- D-Bus presence,
- Qt base integration,
- the active desktop-capable target surface.

### `redbear-grub`

Text-only console/recovery target with GRUB boot manager for real bare metal.

Scope:

- diagnostics,
- recovery workflows,
- multi-boot bare-metal install with GRUB chainload.

### Desktop policy

- Desktop/graphics are available only on `redbear-full`.
- Validation work that does not require graphics should prefer `redbear-mini` or `redbear-grub`.
- Live `.iso` outputs are for real bare-metal boot/install workflows, not for VM/QEMU execution; virtualization should use the `harddrive.img`-based target surface.

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
- the Red Bear-native greeter/login path now has a bounded passing runtime proof, while broader KDE/KWin session stability is still not yet a general runtime claim,
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

- each tracked profile has a documented role,
- profile behavior is reproducible,
- support labels are tied to profile-specific evidence.

### 3. Low-level controllers and IRQ quality

Goal:

- improve runtime trust in IRQ delivery, MSI/MSI-X, and IOMMU-adjacent infrastructure,
- turn compile-oriented infrastructure into runtime-proven substrate.

Current state (2026-04-29):

- 5 IRQ/low-level check binaries exist: PCI IRQ, IOMMU, DMA, PS/2, timer validation
- 6 test scripts: test-msix-qemu.sh, test-iommu-qemu.sh, test-xhci-irq-qemu.sh, test-ps2-qemu.sh, test-timer-qemu.sh, test-lowlevel-controllers-qemu.sh (aggregate)
- redox-driver-sys: typed PCI/IRQ userspace substrate with host-runnable unit tests, quirk-aware interrupt-support reporting, MSI-X table helpers, affinity helpers
- redox-drm: shared interrupt abstraction with MSI-X-first and legacy-IRQ fallback
- iommu daemon: specification-rich IOMMU/interrupt-remapping direction
- Kernel: PIC, IOAPIC, LAPIC/x2APIC, IDT reservation, masking, EOI, spurious IRQ accounting
- Weakness: runtime validation thinner than desired, controller-specific characterization uneven, this remains a blocker for USB/Wi-Fi/Bluetooth reliability claims

Canonical plan:

- `local/docs/IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md`

Acceptance:

- runtime evidence exists for the claimed controller/IRQ scope,
- subsystem docs stop overstating compile-oriented proof.

### 4. USB maturity

Goal:

- mature the existing USB host/controller path into a more reliable subsystem,
- improve topology, hotplug, HID, storage, and observability confidence.

Current state (2026-04-29):

- Enhanced USB checker (redbear-usb-check): xHCI controller detection, device enumeration, HID/storage class detection, JSON output, proper cfg-gating, zero warnings
- Unified test harness (test-usb-runtime.sh): guest + QEMU modes, exit-code-based
- Legacy scripts preserved: test-usb-qemu.sh, test-usb-storage-qemu.sh, test-usb-maturity-qemu.sh
- xhcid driver exists, usbscsid for storage, USB HID support via usbhidd
- Controller/runtime maturity still needs hardware validation

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
- unified Wi-Fi runtime harness (test-wifi-runtime.sh): guest + QEMU modes, exit-code-based,
  runs wifi-check and wifi-link-check in sequence
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
- redbear-bluetooth-battery-check (666 lines, comprehensive BLE battery level checker),
- unified BT runtime harness (test-bt-runtime.sh): guest + QEMU modes, exit-code-based,
- transport dependency on USB maturity remains explicit.

Canonical plan:

- `local/docs/BLUETOOTH-IMPLEMENTATION-PLAN.md`

Acceptance:

- one controller path, one host path, and one bounded BLE-first user-facing workflow exist with
  experimental support language.

### 7. Graphics, Wayland, and desktop/session compatibility

Goal:

- turn the current build-visible desktop stack into runtime-trusted session surfaces.

Current state (2026-04-29):

- **Phase 1 (Runtime Substrate):** build-verified complete. Zero warnings, zero test failures, zero LSP errors. Four Phase 1 check binaries (evdev, udev, firmware, DRM) + `redbear-info --probe` + automated QEMU test harness exist. Runtime validation pending (requires QEMU/bare metal).
- **Phase 2 (Wayland Compositor):** bounded proof scaffold exists. `redbear-compositor` (788-line Rust compositor) builds with zero warnings and self-consistent protocol dispatch (3/3 tests pass). Known limitations: SHM fd passing uses payload bytes (not Unix SCM_RIGHTS), framebuffer compositing uses private heap memory, wire encoding uses NUL-terminated strings. Phase 2 check binary + test harness exist. Not yet a real client-compatible compositor runtime proof.
- **Phase 3 (KWin Session):** KWin recipe is a cmake config stub. Wrapper scripts delegate to `redbear-compositor`. Real KWin build requires sufficient Qt6Quick/QML build+runtime proof (qtdeclarative exists, downstream QML paths unproven). Phase 3 preflight check binary + test harness exist.
- **Phase 4 (KDE Plasma):** All Phase 4 KDE recipes (plasma-workspace, plasma-desktop, plasma-framework, kdecoration, kf6-kwayland, plasma-wayland-protocols) are cmake config stubs marked `#TODO`. Real builds gated on Qt6Quick/QML + real KWin. Legacy test scripts exist (test-phase4-wayland-qemu.sh, test-phase6-kde-qemu.sh).
- **Phase 5 (Hardware GPU):** redox-drm exists with Intel Gen8-Gen12 + AMD device support and quirk tables. Mesa builds with llvmpipe software renderer (hardware renderers not yet cross-compiled). GPU command submission (CS ioctl) missing. DRM display check binary exists. No hardware validation yet.

Canonical references:

- `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` — canonical desktop path from console to hardware-accelerated KDE Plasma on Wayland
- `local/docs/WAYLAND-IMPLEMENTATION-PLAN.md` — canonical Wayland subsystem plan beneath the desktop path
- `local/docs/GREETER-LOGIN-IMPLEMENTATION-PLAN.md` — canonical greeter/login plan beneath the desktop path
- `local/docs/QT6-PORT-STATUS.md`
- `local/docs/DESKTOP-STACK-CURRENT-STATUS.md`
- `docs/05-KDE-PLASMA-ON-REDOX.md` — historical KDE implementation rationale

Acceptance:

- `redbear-full` remains the broader desktop/session plumbing slice (the Wayland validation slice
  is handled within `redbear-full`),
- the active desktop-capable tracked targets keep honest session-viability language tied to `redbear-full`, not older historical target names.

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
- `local/docs/WAYLAND-IMPLEMENTATION-PLAN.md` — canonical Wayland subsystem plan
- `local/docs/GREETER-LOGIN-IMPLEMENTATION-PLAN.md` — canonical greeter/login plan
- `local/docs/IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md`
- `local/docs/USB-IMPLEMENTATION-PLAN.md`
- `local/docs/WIFI-IMPLEMENTATION-PLAN.md`
- `local/docs/BLUETOOTH-IMPLEMENTATION-PLAN.md`
- `local/docs/RELIBC-COMPLETENESS-AND-ENHANCEMENT-PLAN.md`
- `local/docs/RELIBC-IMPLEMENTATION-PLAN.md` — implementation roadmap for relibc POSIX gaps
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
