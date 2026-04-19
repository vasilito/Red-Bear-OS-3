# xhcid Device-Level Improvement Plan

## Purpose

This document defines the implementation sequence for hardening `xhcid` at the device level in
Red Bear OS.

It is a focused companion to `local/docs/USB-IMPLEMENTATION-PLAN.md`. The USB plan remains the
subsystem-wide authority; this document narrows scope to the `xhcid` device lifecycle,
configuration, teardown, PM behavior, enumerator robustness, and bounded proof coverage.

## Scope

In scope:

- `recipes/core/base/source/drivers/usb/xhcid/src/xhci/device_enumerator.rs`
- `recipes/core/base/source/drivers/usb/xhcid/src/xhci/mod.rs`
- `recipes/core/base/source/drivers/usb/xhcid/src/xhci/scheme.rs`
- `recipes/core/base/source/drivers/usb/xhcid/src/xhci/irq_reactor.rs`
- bounded QEMU validation scripts under `local/scripts/`
- canonical USB documentation under `local/docs/`

Out of scope:

- generic USB redesign
- unrelated class-driver feature work
- hardware-validation claims beyond what the repo can currently prove

## Repo-Fit Note

Technical implementation targets live in upstream-owned source under
`recipes/core/base/source/...`, but durable Red Bear preservation belongs in
`local/patches/base/`. This plan names the technical work locations, not a recommendation to leave
 work stranded only in upstream-owned trees.

## Current Audited Findings

The current `xhcid` tree has already improved materially:

- lifecycle gating exists through `PortLifecycle` and `PortOperationGuard`
- `configure_endpoints_once()` is now transactional relative to earlier behavior
- detach waits before removing published state
- a bounded QEMU lifecycle proof exists

Remaining risks:

- partial attach visibility still exists around publication timing
- detach can still depend on bounded-but-incomplete purge semantics
- suspend/resume is still mainly software gating
- rollback failure is not yet a fully hardened degraded-state path
- enumerator logic still relies on timing- and assumption-heavy behavior
- proof coverage is still QEMU-bounded and misses key interleavings

## Design Invariants

The implementation should satisfy these invariants:

1. No half-attached device is publicly usable.
2. No new work is admitted after detach begins.
3. Detach always reaches a bounded terminal outcome.
4. Failed configure leaves either the old config intact or the device explicitly
   degraded/reset-required.
5. PM transitions reflect actual usable state, not only software policy.
6. Enumerator behavior is bounded and diagnosable, not panic-driven.
7. Validation claims match what scripts actually prove.

## Phase 1 — Proof-First Expansion

### Goal

Make the current blind spots reproducible before changing behavior.

### Work

- extend `test-xhci-device-lifecycle-qemu.sh`
- extend `test-usb-qemu.sh`
- extend `test-xhci-irq-qemu.sh`
- add bounded injection hooks in `xhcid` for configure-failure and attach/detach timing cases

### Required Cases

- repeated attach/detach
- detach during storage startup
- transfer-during-detach surrogate
- configure failure injection
- suspend/resume admission checks
- rapid event ordering cases

### Per-File Focus

#### `local/scripts/test-xhci-device-lifecycle-qemu.sh`

- add repeated HID/storage attach-detach loops
- add detach-during-driver-start for storage
- add storage attach long enough to exercise startup/read activity before unplug
- require explicit attach-entered, attach-finished, detach-completed evidence

#### `local/scripts/test-usb-qemu.sh`

- separate boot progress from proof failure
- keep result lines distinct for xHCI init, HID spawn, SCSI spawn, bounded readback, and crash scan
- add repeated full-stack run mode or bounded loop count if needed for ordering-sensitive regressions

#### `local/scripts/test-xhci-irq-qemu.sh`

- verify interrupt-mode evidence still holds under actual attached-device pressure, not only empty-controller boot

#### `xhci` test hooks

- add bounded test-only failure hooks in `scheme.rs` / `mod.rs` for:
  - fail after `CONFIGURE_ENDPOINT`
  - fail after `SET_CONFIGURATION`
  - optional delay before final attach commit
- current bounded implementation uses one-shot guest-side commands written to
  `/tmp/xhcid-test-hook`, consumed by `xhcid` on the next matching lifecycle point

### Exit Criteria

- scripts are syntax-clean
- new cases fail meaningfully on current gaps
- failures identify the specific missed milestone

## Phase 2 — Atomic Attach Publication

### Goal

Prevent half-built devices from becoming publicly reachable.

### Work

- refactor `Xhci::attach_device`
- split attach staging from published `PortState`
- narrow lifecycle exposure so scheme paths cannot reach a device before final commit
- make attach cleanup direct for prepublication failure

### Key Targets

- `xhci/mod.rs::Xhci::attach_device`
- `xhci/mod.rs::PortLifecycle::*`
- `xhci/device_enumerator.rs::DeviceEnumerator::run`

### Per-File Focus

#### `xhci/mod.rs`

- stop inserting into `port_states` before all attach substeps complete
- keep slot, input context, EP0 ring, quirks, and descriptors in a private staging carrier
- commit published `PortState` in one final block
- keep prepublication cleanup separate from `detach_device()` where possible

#### `xhci/device_enumerator.rs`

- ensure duplicate connect handling still treats `EAGAIN` or equivalent as "already published" rather than "half-built staging state"

### Exit Criteria

- no public state before attach commit
- attach failure leaves no published device and no child driver

## Phase 3 — Bounded Detach and Purge

### Goal

Make teardown bounded, dominant, and safe against stale completions.

### Work

- bound `PortLifecycle::begin_detaching()`
- reject all new work immediately once detach starts
- purge or tombstone pending transfer/reactor state
- separate graceful drain from forced teardown
- preserve correct slot-disable/remove ordering
- ensure child-driver shutdown cannot wedge detach

### Key Targets

- `xhci/mod.rs`
- `xhci/irq_reactor.rs`
- transfer bookkeeping in `xhci/scheme.rs`

### Per-File Focus

#### `xhci/mod.rs`

- add timeout or bounded wait to detach drain logic
- distinguish graceful drain from forced teardown
- keep `port_states.remove(...)` after terminal teardown outcome

#### `xhci/irq_reactor.rs`

- add per-port invalidation or tombstone behavior so stale completions cannot target removed state

#### `xhci/scheme.rs`

- ensure operation-entry helpers fail immediately once detach starts

### Exit Criteria

- detach cannot hang forever
- no stale completion can target removed device state
- unload-under-activity proof passes

## Phase 4 — Configure Rollback Hardening

### Goal

Make configuration changes fully transactional and recoverable.

### Work

- formalize stage/program/commit boundaries
- ensure snapshots cover all mutated controller-facing state
- promote rollback failure into explicit degraded-state handling
- define deterministic behavior for post-`SET_CONFIGURATION` failure
- keep alternate/config bookkeeping coherent after rollback
- quarantine or reset on unrecoverable ambiguity

### Key Targets

- `xhci/scheme.rs::configure_endpoints_once`
- `restore_configure_input_context`
- `configure_endpoints`
- `set_configuration`
- `set_interface`

### Per-File Focus

#### `xhci/scheme.rs`

- keep endpoint/ring state staged until commit
- verify snapshots cover every mutated slot/endpoint field
- treat rollback failure as a first-class degraded state
- ensure post-failure descriptor and alternate bookkeeping still reflect live state

### Exit Criteria

- injected configure failure preserves old state or explicitly degrades/resets device
- no staged endpoint state leaks into live software state

## Phase 5 — Real PM Sequencing

### Goal

Replace software-only PM gating with meaningful quiesce/resume semantics.

### Work

- define richer PM transition states
- quiesce before suspend
- tie resume to controller/device validity
- define PM interaction with detach
- define PM interaction with configure
- add bounded PM proof cases

### Key Targets

- `xhci/scheme.rs::suspend_device`
- `xhci/scheme.rs::resume_device`
- `xhci/scheme.rs::ensure_port_active`
- supporting helpers in `xhci/mod.rs`

### Exit Criteria

- suspend blocks new I/O only after quiesce starts
- resume only returns success from a genuinely usable state
- PM/detach/configure interleavings are deterministic

## Phase 6 — Enumerator Cleanup and Timing Hardening

### Goal

Remove panic-style and magic-delay behavior from the enumerator path.

### Work

- remove panic-class assumptions from `DeviceEnumerator::run`
- replace fixed sleeps with bounded readiness checks
- make duplicate/out-of-order event handling explicit
- align enumerator decisions with the new attach/detach state machine
- improve logging for reset/attach/detach milestones

### Key Targets

- `xhci/device_enumerator.rs`
- supporting interactions in `xhci/mod.rs`

### Exit Criteria

- no ordinary event path panics
- no unnecessary fixed sleep remains
- rapid event-order tests pass in QEMU

## Phase 7 — Final Validation, Docs, and Preservation

### Goal

Close the loop with evidence, canonical docs, and durable patch carriers.

### Work

- rerun the full bounded proof matrix on a rebuilt image
- run source-level verification (`lsp_diagnostics`, `cargo check`, `cargo test`)
- update canonical docs:
  - `local/docs/USB-IMPLEMENTATION-PLAN.md`
  - `local/docs/USB-VALIDATION-RUNBOOK.md`
- refresh durable patch carriers under `local/patches/base/`
- delete only clearly stale, superseded docs after link sweep

### Exit Criteria

- all bounded USB/xHCI proofs pass on a fresh image
- changed files are diagnostics-clean
- canonical docs match actual proof scope
- patch carrier is refreshed and reapplicable

## Validation Matrix

Required final proofs:

- `bash ./local/scripts/test-xhci-device-lifecycle-qemu.sh --check <tracked-target>`
- `bash ./local/scripts/test-usb-qemu.sh --check <tracked-target>`
- `bash ./local/scripts/test-xhci-irq-qemu.sh --check`
- `bash ./local/scripts/test-usb-maturity-qemu.sh <tracked-target>`

Required source checks:

- `lsp_diagnostics` on all changed files
- `cargo check` / `cargo test` for `xhcid`
- `cargo check` for any touched class daemon or helper crate

## Commit Strategy

1. proof/harness expansion
2. atomic attach publication
3. bounded detach and purge
4. configure rollback hardening
5. PM sequencing
6. enumerator cleanup
7. docs, patch preservation, stale-doc cleanup

## Canonical Doc Authority

Authoritative docs after cleanup:

- `local/docs/USB-IMPLEMENTATION-PLAN.md`
- `local/docs/USB-VALIDATION-RUNBOOK.md`

This xhcid plan is a focused implementation document beneath those subsystem-level authorities.

## Completion Standard

This work is complete only when:

- all seven phases are done in order
- no changed-file diagnostics remain
- `xhcid` builds/tests cleanly
- bounded QEMU proof matrix passes on a rebuilt image
- canonical docs are synchronized
- durable patch carrier is refreshed
- remaining gaps, if any, are explicitly documented as future or hardware-only work
