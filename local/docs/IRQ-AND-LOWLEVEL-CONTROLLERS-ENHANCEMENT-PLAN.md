# Red Bear OS IRQ and Low-Level Controllers Enhancement Plan

## Purpose

This document assesses the current IRQ and low-level controller implementation in Red Bear OS for
completeness and quality, then defines the next enhancement plan in execution order.

It is grounded in the current repository state, especially:

- `local/recipes/drivers/redox-driver-sys/`
- `local/recipes/drivers/linux-kpi/`
- `local/recipes/gpu/redox-drm/`
- `local/recipes/system/iommu/`
- `recipes/core/kernel/source/src/acpi/`
- `recipes/core/base/source/drivers/acpid/`
- `local/docs/IOMMU-SPEC-REFERENCE.md`
- `local/docs/ACPI-FIXES.md`
- `docs/04-LINUX-DRIVER-COMPAT.md`

The goal is not to restate that these pieces compile, but to separate:

- what exists architecturally,
- what is only build-validated,
- what is runtime-validated,
- and what still needs focused enhancement work.

## Evidence Model

This plan uses four different evidence buckets and does **not** treat them as equivalent:

- **Checked-in source** — what is visible directly in the current source tree.
- **Local patch state** — behavior carried by `local/patches/*` that may not be visible in the
  unpacked upstream source snapshot until patches are applied.
- **Build-validated** — code or recipes compile successfully.
- **Runtime-validated** — behavior has been exercised in a real boot/runtime path.

Where a statement depends on local patches instead of the visible source snapshot, that is called
out explicitly below.

## Controller Inventory and Ownership

| Area | Primary owner | Main entry points | Current evidence class |
|---|---|---|---|
| LAPIC / xAPIC / x2APIC | kernel | `recipes/core/kernel/source/src/acpi/madt/`, `arch/x86_shared/device/local_apic.rs` | source + local patch + boot/runtime evidence |
| IOAPIC / IRQ overrides | kernel | `recipes/core/kernel/source/src/arch/x86_shared/device/ioapic.rs`, MADT ISO parsing | source |
| Legacy PIC | kernel | `arch/x86_shared/device/pic.rs` | source |
| ACPI power/reset methods | userspace `acpid` | `recipes/core/base/source/drivers/acpid/src/acpi.rs` plus local base patch | source + local patch + runtime evidence |
| HPET / timer tables | kernel | `recipes/core/kernel/source/src/acpi/hpet.rs` | source |
| PIT fallback timer | kernel | `recipes/core/kernel/source/src/arch/x86_shared/device/mod.rs`, `pit.rs` | source |
| PCI interrupt plumbing | userspace `pcid` / driver layer | `recipes/core/base/source/drivers/pci/`, `scheme:irq`, `scheme:pci` | source + runtime evidence |
| Driver IRQ abstraction | `redox-driver-sys` | `local/recipes/drivers/redox-driver-sys/source/src/irq.rs` | source |
| Linux IRQ compatibility | `linux-kpi` | `local/recipes/drivers/linux-kpi/source/` headers | source |
| GPU MSI/MSI-X usage | `redox-drm` | `local/recipes/gpu/redox-drm/source/` | source + build evidence |
| IOMMU / interrupt remapping | `iommu` daemon | `local/recipes/system/iommu/source/src/main.rs`, `local/docs/IOMMU-SPEC-REFERENCE.md` | source + build evidence |
| Kernel serio / PS2 path | kernel `serio` + userspace `ps2d` | `recipes/core/kernel/source/src/scheme/serio.rs`, `recipes/core/base/source/drivers/input/ps2d/src/main.rs` | source |
| Input controller path | `inputd` / `evdevd` / `udev-shim` | base driver + local system recipes | source + runtime evidence |
| USB xHCI host controller | userspace `xhcid` | `recipes/core/base/source/drivers/usb/xhcid/src/main.rs` | source + build evidence |
| Port I/O / legacy controller access | kernel + `redox-driver-sys` | `iopl`, `io.rs`, legacy driver code | source |
| Legacy IRQ dispatch / ownership map | kernel | `recipes/core/kernel/source/src/arch/x86_shared/interrupt/irq.rs` | source |

## Current State Summary

### What is already in place

Red Bear OS already has a meaningful low-level controller and interrupt foundation:

- ACPI boot, FADT power control, visible MADT parsing for LAPIC/IOAPIC/interrupt overrides, and
  HPET initialization are in place in the checked-in source.
- Additional MADT x2APIC / NMI / power-method handling exists in the local patch set and in prior
  runtime validation notes, but that behavior should not be conflated with the unpatched source
  snapshot.
- `redox-driver-sys` provides userspace driver primitives for MMIO, DMA, PCI access, IRQ handles,
  MSI-X table mapping, and IRQ affinity control.
- `linux-kpi` exposes Linux-style IRQ, PCI, memory, and synchronization APIs on top of
  `redox-driver-sys`.
- `redox-drm` already contains a shared interrupt abstraction with MSI-X-first and legacy-IRQ
  fallback paths for GPU drivers.
- The AMD-Vi / Intel VT-d reference material and the in-tree `iommu` daemon establish a serious
  implementation direction for IOMMU and interrupt-remapping work.

### What is still weak

The dominant weakness is not missing abstractions. It is missing runtime proof and uneven
controller-specific validation.

- MSI-X support exists architecturally but is still weak on hardware validation.
- IOMMU support is specification-rich and code-rich, but still unvalidated on real hardware.
- IRQ routing quality-of-service remains primitive: raw wait handles exist, but balancing,
  coalescing, and validation of affinity behavior remain thin.
- Input stacks (`inputd`, `evdevd`, `udev-shim`) now exist as a runtime substrate, but the exact
  end-to-end interrupt-to-consumer path still needs sustained validation discipline.
- Low-level controller quality is uneven: ACPI/APIC are much further along than IOMMU, MSI-X, and
  controller-specific runtime characterization.

## Architectural Assessment

### 1. IRQ delivery architecture

The project’s IRQ delivery model is fundamentally sound.

- Kernel/platform side routes interrupts through APIC/x2APIC infrastructure.
- Userspace consumes interrupts through `scheme:irq` handles.
- MSI-X vector allocation is already modeled per CPU via the IRQ scheme.

This is the right design for Red Bear OS. The main enhancement need is validation and quality, not
an architectural rewrite.

### 2. PCI and MSI/MSI-X

The PCI and MSI-X model is one of the strongest parts of the current stack.

- Config-space access exists.
- Capability parsing exists.
- MSI-X table mapping exists.
- GPU drivers already use the abstraction.

The gap is that the repository still talks too often in “compiles” language instead of “validated on
hardware with real interrupts firing” language.

Current runtime-proof entrypoint now present in-tree:

- `local/scripts/test-msix-qemu.sh` — QEMU/UEFI boot path that verifies live `virtio-net`
  initialization reporting `virtio: using MSI-X`

### 3. IOMMU and interrupt remapping

IOMMU is the most important low-level controller area that is still incomplete in practice.

- The implementation direction is correct.
- The data structures and register model are already documented deeply.
- But the hardware-validation story is still effectively open, and current daemon discovery is still
  only partially integrated: the daemon now searches common IVRS table locations automatically, but
  full platform-native discovery and hardware validation are still open.
- The current QEMU path now reaches AMD-Vi unit detection and `scheme:iommu` registration without
  crashing at daemon startup, but unit initialization is still deferred and real hardware validation
  remains open.
- The current guest-driven first-use proof now reaches AMD-Vi MMIO reads in QEMU (`control=0x0`,
  `status=0x0`), but still dies during the completion path with a CPU-side page fault while touching
  the completion-store region. That narrows the remaining blocker to DMA mapping/page-coverage
  behavior rather than to missing discovery, missing scheme wiring, or unreadable MMIO registers.

This makes IOMMU the highest-value long-term controller enhancement area after basic MSI-X runtime
validation.

### 4. Input/controller path

The input/controller path is no longer missing. It is now a quality and observability problem.

- `inputd` exists.
- `evdevd` exists.
- `udev-shim` exists.
- Phase 3 validation helpers exist.

The enhancement task is to keep turning these from “service present” into “interrupt path proven,”
especially under real runtime scenarios.

## Completeness Assessment by Area

### ACPI / APIC / x2APIC

**State**: materially complete for current platform bring-up goals.

**Important source note**: the checked-in MADT parser in
`recipes/core/kernel/source/src/acpi/madt/mod.rs` visibly handles `LocalApic`, `IoApic`,
 `IntSrcOverride`, `Gicc`, and `Gicd`. Additional x2APIC/NMI support referenced elsewhere in the
 repo is currently evidenced through the local patch set and prior validation notes rather than the
 plain source snapshot alone.

Strengths:

- MADT entries for xAPIC/x2APIC/NMI are handled.
- ACPI reboot/shutdown/power methods exist.
- x2APIC and SMP platform bring-up have already crossed the foundational threshold.

Open enhancement items:

- Better controller/runtime characterization on diverse hardware.
- Clearer documentation for what is kernel-complete versus only tested on limited platforms.

### IOAPIC / interrupt source override routing

**State**: present in ACPI parsing, but less explicitly validated than LAPIC/x2APIC paths.

Concrete checked-in owner:

- `recipes/core/kernel/source/src/arch/x86_shared/device/ioapic.rs`
- `recipes/core/kernel/source/src/acpi/madt/mod.rs`

Open enhancement items:

- explicit validation of interrupt source overrides on more real machines
- repo-visible test notes for IOAPIC routing behavior

### HPET / timer controller surface

**State**: present, but still thinly characterized.

Concrete checked-in owner:

- `recipes/core/kernel/source/src/acpi/hpet.rs`

Open enhancement items:

- runtime verification beyond “initialized from ACPI”
- clearer single-HPET limitation documentation

### PIT fallback timer path

**State**: explicit checked-in fallback controller path.

Concrete checked-in owner:

- `recipes/core/kernel/source/src/arch/x86_shared/device/mod.rs`
- `recipes/core/kernel/source/src/arch/x86_shared/device/pit.rs`

Current behavior:

- the kernel prefers HPET when available
- if HPET initialization fails or is unavailable, it falls back to PIT
- PIT interrupt ticks currently drive timeout and scheduler timing paths

Open enhancement items:

- document runtime characterization of PIT-only boots
- clarify timer-source selection evidence in validation notes

### PCI interrupt plumbing / MSI / MSI-X

**State**: architecturally strong, validation-incomplete.

Open enhancement items:

- real hardware MSI-X proof for AMD and Intel GPU paths
- controller-level observability for vector allocation and affinity behavior
- testable records of fallback behavior between MSI-X and legacy IRQs

Current runtime-validation surface now present in-tree:

- `local/scripts/test-msix-qemu.sh` — boots a Red Bear image and confirms a live MSI-X path via
  `virtio-net` log evidence in QEMU

### IOMMU / interrupt remapping

**State**: the biggest completeness gap.

Concrete checked-in owner:

- `local/recipes/system/iommu/source/src/main.rs`
- `local/docs/IOMMU-SPEC-REFERENCE.md`

Open enhancement items:

- real AMD-Vi initialization validation
- event log and fault-path validation
- interrupt remapping validation under device load
- explicit distinction between “daemon builds” and “controller works”
- replacement of `IOMMU_IVRS_PATH`-only discovery with real system discovery/integration
- diagnosis/fix for the remaining QEMU first-use blocker where completion-store CPU access faults
  even after MMIO reads and multiple completion-store placement strategies succeed structurally

Current implementation improvement:

- the daemon no longer depends only on `IOMMU_IVRS_PATH`; it now searches common IVRS table paths
  automatically before falling back to the environment variable override
- daemon startup now defers AMD-Vi unit initialization until first scheme use, which keeps the
  QEMU validation path alive long enough to prove detection plus `scheme:iommu` registration
- a guest-driven self-test path now exists (`/usr/bin/iommu --self-test-init` via
  `redbear-phase-iommu-check` / `test-iommu-qemu.sh`) and proves that the remaining failure is in
  runtime completion/DMA-page handling, not in daemon startup or bare MMIO readability

### Legacy IRQ ownership and dispatch map

**State**: explicit checked-in kernel ownership exists, but it is under-documented in higher-level
controller discussions.

Concrete checked-in owner:

- `recipes/core/kernel/source/src/arch/x86_shared/interrupt/irq.rs`

Current covered paths include:

- PIT timer interrupt handling
- keyboard and mouse interrupt delivery
- serial COM1/COM2 delivery
- PIC/APIC mask, acknowledge, and EOI behavior
- spurious IRQ accounting for IRQ7 and IRQ15

Open enhancement items:

- document legacy IRQ ownership and routing expectations explicitly in validation notes
- record PIC-vs-APIC runtime behavior on more hardware classes

### Kernel `serio` / PS2 controller path

**State**: present and important, but easy to miss if input work is described only in terms of the
later `evdevd`/`udev-shim` stack.

Concrete checked-in owner:

- `recipes/core/kernel/source/src/scheme/serio.rs`
- `recipes/core/base/source/drivers/input/ps2d/src/main.rs`

Current behavior:

- the kernel owns the serio byte queues to avoid PS/2 controller races
- `ps2d` consumes `/scheme/serio/0` and `/scheme/serio/1`
- that path then feeds the broader input producer chain

Open enhancement items:

- keep validation language explicit about the PS/2 path versus the later generic input stack
- add platform notes for systems that still rely on PS/2 keyboard/mouse delivery

### USB xHCI controller interrupt path

**State**: present, but not honestly interrupt-complete in the checked-in source.

Concrete checked-in owner:

- `recipes/core/base/source/drivers/usb/xhcid/src/main.rs`

Current behavior:

- xHCI has MSI/MSI-X and legacy INTx detection logic in source
- the hardwired polling override in `xhcid` has been removed, and the driver now uses the existing
  MSI-X / MSI / INTx selection logic again
- `local/scripts/test-xhci-irq-qemu.sh --check` now provides a repo-visible runtime proof path by
  booting a Red Bear image in QEMU and checking the xHCI interrupt-mode log output
- `redox-driver-sys` now logs allocated MSI-X vectors so interrupt selection is more observable in
  runtime logs

Open enhancement items:

- validate the restored interrupt path beyond early boot/logging, especially event-ring behavior
- validate the checked-in event-ring growth path under sustained runtime/device activity

### Port I/O / legacy controller support

**State**: exists, but under-characterized.

Concrete current consumers/owners include:

- legacy PIC handling in `recipes/core/kernel/source/src/arch/x86_shared/device/pic.rs`
- port-I/O wrappers in `local/recipes/drivers/redox-driver-sys/source/src/io.rs`
- ACPI reset fallback via keyboard-controller port writes in the base/acpid patch path documented in
  `local/docs/ACPI-FIXES.md`

Open enhancement items:

- determine which real devices still need the port-I/O path
- validate that the current wrappers are sufficient for those devices

## Quality Assessment

### Strong points

- The layering is correct: kernel/platform routing below, userspace schemes and driver wrappers
  above.
- The repository already has serious implementation artifacts, not just speculative plans.
- The low-level controller work is documented more deeply than many higher-level desktop areas.
- ACPI and early-platform work is significantly more mature than the rest of the low-level stack.

### Weak points

- Validation language is still inconsistent across docs. “builds” and “validated” are too often
  treated as adjacent states when they are not.
- IOMMU progress is easy to overread because the spec reference is detailed, but the runtime proof
  and discovery story are not there yet.
- Some controller areas are rich in abstractions but poor in operator-facing validation procedures.
- Hardware-controller quality is still under-documented in terms of negative results and known
  failure modes.
- Earlier summaries in the repo can blur checked-in source, local patches, and validated runtime
  behavior; this document should be used to keep those categories separate.
- Broad category labels can hide concrete controller owners unless PIT, `serio`/PS2, legacy IRQ
  dispatch, and xHCI are named explicitly.

## Enhancement Priorities

## Priority 1 — MSI-X runtime validation on real devices

Goal: move MSI-X from “implemented abstraction” to “repeatedly proven behavior.”

Deliverables:

- explicit AMD GPU MSI-X validation notes
- explicit Intel GPU MSI-X validation notes
- verified fallback behavior to legacy IRQs when MSI-X is unavailable
- logged CPU/vector affinity behavior in real runs

Why first:

This is the lowest-level controller feature that already exists in the main runtime driver path and
blocks confidence in GPU/display work above it.

## Priority 2 — IOMMU hardware bring-up and fault-path validation

Goal: move IOMMU from spec-driven implementation to actual controller bring-up.

Deliverables:

- validated AMD-Vi daemon initialization on real hardware
- device table / command buffer / event log validation
- explicit interrupt-remapping validation notes
- negative-result documentation if hardware still fails

Why second:

It is the largest remaining low-level completeness gap, and it affects the safety and correctness of
userspace driver DMA.

## Priority 3 — IRQ quality-of-service and observability

Goal: make IRQ behavior easier to reason about in production.

Deliverables:

- better logging/telemetry around allocated IRQs and vectors
- explicit affinity-validation procedures
- measured notes on whether current userspace IRQ wait behavior is good enough for display/input
  latency needs

Why third:

This improves reliability without changing the underlying architecture.

## Priority 4 — input/controller runtime proof

Goal: continue turning the existing input substrate into a well-proven low-level controller path.

Deliverables:

- sustained validation of `inputd` → `evdevd` → consumer path
- documentation of real interrupt-backed input evidence, not only service existence
- explicit known limitations for consumer nodes and path expectations

Why fourth:

The architecture is there. What remains is proof quality.

## Priority 5 — timer/controller characterization

Goal: reduce uncertainty around HPET/APIC-timer behavior and controller assumptions.

Deliverables:

- a compact validation note for HPET behavior on real hardware
- notes on timer-controller assumptions and known limits

Why fifth:

Important, but less immediately blocking than MSI-X and IOMMU.

## Priority 6 — xHCI interrupt restoration

This is Priority 6 **within the low-level controller plan itself**, not within the repository-wide
subsystem order. At the repo-wide level, low-level controller quality remains ahead of USB/Wi-Fi/
Bluetooth because these later subsystems depend on the controller/runtime proof work documented
here.

Goal: move USB host-controller operation from polling back to real interrupt-driven behavior.

Deliverables:

- restore the actual `get_int_method` path in `xhcid`
- validate MSI/MSI-X or INTx behavior for xHCI on real hardware and/or QEMU
- update docs so USB controller quality is not overstated while polling remains active

Why sixth:

This is a real completeness gap in an important low-level controller, but it is narrower in scope
than the cross-cutting MSI-X and IOMMU priorities above.

## Execution Plan

### Step A — Establish validation vocabulary in all related docs

For every low-level controller area, use the same four states consistently:

- builds
- boots
- validated
- experimental

Do not mark controller infrastructure “complete” unless the claimed runtime behavior is actually
proven.

### Step B — Add dedicated validation notes for MSI-X and IOMMU

The project already has enough code to justify dedicated runtime-validation docs for:

- GPU MSI-X behavior
- IOMMU bring-up and fault handling

There is now also an in-tree generic MSI-X runtime proof helper:

- `local/scripts/test-msix-qemu.sh`

These should record both successful and failed hardware runs.

### Step C — Expand runtime-proof tooling where signal is weak

The project already has a good pattern for this in the Phase 3/4/5 validation helpers.

Use the same pattern for low-level controllers:

- one host-side launcher/check path
- one guest-side runtime check path
- one doc entry that records what “passing” actually means

### Step D — Keep the controller plan separate from higher-level desktop work

Do not let IRQ/IOMMU/controller planning get absorbed into generic Wayland/KDE roadmaps.

Controller quality must remain measurable at its own layer.

## Recommended New Documentation Work

The current project docs should eventually include dedicated runtime-validation companion documents
for:

- MSI-X validation
- IOMMU bring-up and fault validation
- timer/controller characterization
- input/controller runtime evidence

This document is the umbrella enhancement plan; those would be the execution/validation companions.

## Current Validation Entry Points

The following in-tree validation paths now exist and should be treated as the current controller
runtime-evidence surface:

- `local/scripts/test-xhci-irq-qemu.sh --check` — xHCI interrupt-mode proof from QEMU boot logs
- `local/scripts/test-msix-qemu.sh` — live MSI-X proof via `virtio-net`
- `local/scripts/test-iommu-qemu.sh --check` — AMD IOMMU device visibility plus guest boot reachability
- `local/scripts/test-usb-storage-qemu.sh` — USB mass-storage autospawn probe (currently still an
  active blocker path)

## Bottom Line

Red Bear OS does **not** need a new IRQ/controller architecture.

It already has the correct architectural direction:

- scheme-based userspace IRQ delivery
- safe Rust driver wrappers
- PCI/MSI-X support
- IOMMU direction
- ACPI/APIC groundwork

What it needs now is disciplined completion work in this order:

1. MSI-X runtime proof
2. IOMMU hardware validation
3. IRQ observability and affinity proof
4. input/controller runtime evidence
5. timer/controller characterization

The main quality risk is no longer missing design. It is over-claiming readiness before low-level
controller runtime evidence exists.
