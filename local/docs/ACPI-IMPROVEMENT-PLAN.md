# Red Bear OS ACPI Improvement Plan

## Purpose

This document turns the current ACPI assessment into a concrete execution plan.

It does **not** replace `local/docs/ACPI-FIXES.md`. That file remains the historical record for the
P0 bring-up work and the current table-by-table status snapshot. This document is the forward-looking
plan for improving **completeness**, **robustness**, **ownership clarity**, **consumer integration**,
and **validation quality**.

The goal is not to treat ACPI as a generic checklist of table parsers. The goal is to make the Red
Bear ACPI stack:

- correct enough to survive bad firmware,
- clear enough that ownership boundaries stay maintainable,
- observable enough that failures are diagnosable,
- and validated enough that "complete" means more than "boots on one machine".

## Scope

This plan covers the Red Bear ACPI stack and its direct dependency chain:

- kernel ACPI discovery and early table handling,
- `acpid` as the main ACPI/AML/FADT/DMI/power daemon,
- `iommu` as the IVRS/AMD-Vi runtime owner,
- `pcid` / `/config` as the MCFG replacement path,
- DMI-backed quirks flowing through `acpid` and `redox-driver-sys`,
- ACPI-consuming services such as `redbear-sessiond` and `redbear-info`.

Primary focus is the current x86_64 path, because that is the active Red Bear hardware target and the
area where the current implementation and validation debt is concentrated. ARM64 ACPI support remains
in scope only where kernel ownership decisions or generic parser quality would affect it.

## Evidence Model

This plan uses five evidence buckets and does **not** treat them as equivalent:

- **source-visible** — behavior is visible in the current checked-in source tree
- **patch-carried** — behavior exists through `local/patches/*` rather than plain upstream source
- **build-visible** — code compiles and stages successfully in the current build
- **runtime-validated** — behavior has been exercised successfully in real boot/runtime paths
- **negative-result-documented** — failure modes and platform gaps are recorded explicitly

This matters because the current ACPI stack has already crossed the bring-up threshold, but still has
meaningful gaps between **implemented**, **robust**, and **trusted**.

## Ownership Model

The long-term ownership split should be:

- **Kernel ACPI** — minimum early discovery and unavoidable early platform setup
- **`acpid`** — ACPI table serving, AML execution, FADT power/reboot logic, DMI exposure,
  power-state exposure, ACPI table quirk filtering
- **`iommu` daemon** — IVRS runtime parsing and AMD-Vi controller ownership
- **future Intel IOMMU owner** — DMAR runtime handling, not `acpid`
- **`pcid`** — PCI config space access replacing broken MCFG-in-acpid stubs
- **consumers** — query ACPI-exposed services; do not parse ACPI firmware directly unless they are
  the designated owner

This ownership split is **not fully enforced today**. The plan below is designed to move the current
tree from transitional ownership to explicit ownership without destabilizing the working bring-up
path.

## Current State Summary

### What is strong today

- Kernel RSDP/RSDT/XSDT/MADT handling exists and is sufficient for current boot bring-up.
- `acpid` owns FADT parsing, AML integration, DMI exposure, and ACPI-backed power state exposure.
- IVRS was correctly removed from the broken `acpid` stub path and moved to the `iommu` daemon.
- MCFG ownership was correctly removed from `acpid` and replaced with the `pcid /config` path.
- DMI-backed quirks are integrated through `/scheme/acpi/dmi` and `redox-driver-sys`.
- `acpid` startup uses typed `StartupError` with explicit error messages and clean exit paths (Wave 1
  boot-path hardening partially complete).
- AML mutex state has real tracked implementation with handle-based acquire/release semantics in
  `aml_physmem.rs` (Wave 2 AML mutex work partially complete).
- EC access width is handled via `read_bytes`/`write_bytes` byte-transaction sequences for u16/u32/u64
  accesses (Wave 2 EC width work partially complete).
- DMAR table parsing module exists in `acpid` but is not wired into the startup path; DMAR ownership
  is effectively deferred to the `iommu` daemon (Wave 3 DMAR separation partially complete).
- Shutdown eventing uses `/scheme/kernel.acpi/kstop` as the kernel-to-userspace shutdown signal;
  `redbear-sessiond` listens on this path for `PrepareForShutdown` D-Bus signals.
- Kernel registers the `kstop` scheme at boot and ACPI subsystem shutdown uses PM1a/PM1b CNT writes
  with `\_S5` sleep types.

### What is still weak today

- Sleep state transitions (`\_Sx` methods beyond `\_S5`) and sleep eventing remain unsupported; there is
  no `/scheme/acpi/sleep` or event-driven sleep contract.
- AML opregion error propagation still has some silent failure paths; not all correctness-critical
  reads return error to caller.
- `AmlSymbols` initialization order is still tied to PCI FD registration timing; AML initialization
  is not fully deterministic.
- `SLP_TYPb` handling remains unimplemented for sleep states beyond `\_S5`.
- DMAR table parsing module is present but unused; the module itself has not been removed, creating
  latent confusion about ownership.
- Docs still risk equating "implemented" with "validated" without explicit evidence qualification.

### Honest status statement

Red Bear ACPI is **materially complete for the historical P0 boot goal**, but it is **not yet complete
for robustness, ownership cleanliness, sleep state support, or broad platform confidence**. Sleep
state eventing is a known gap. The shutdown eventing contract via `kstop` is implemented but only
validated in QEMU; bare-metal validation is still outstanding.

## Canonical Related Documents

Read these alongside this plan:

- `local/docs/ACPI-FIXES.md` — current status ledger and historical P0 fixes
- `local/docs/IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md` — controller-level validation and
  quality context
- `local/docs/IOMMU-SPEC-REFERENCE.md` — IVRS/DMAR technical reference
- `local/docs/QUIRKS-SYSTEM.md` — DMI-backed quirks and ACPI table blacklist behavior
- `local/docs/AMD-FIRST-INTEGRATION.md` — historical AMD-first framing and hardware context
- `local/docs/BAREMETAL-LOG.md` — real-machine failure notes and negative results

## Work Classification

Every task in this plan is tagged by its main purpose:

- **Completeness** — functionality exists but is still missing or partial
- **Robustness** — behavior exists but is too fragile under bad firmware or runtime stress
- **Quality** — ownership, observability, maintainability, or docs are below target

## Wave 0 — Contracts, truthfulness, and degraded-mode policy

### Goal

Stop treating ACPI as a loose cluster of working code and instead define:

1. who owns what,
2. which failures are fatal versus degradable,
3. and what status words in the docs actually mean.

### Why this wave is first

Without an explicit contract, later hardening work turns into hidden rewrites and docs drift.

### Scope

- `local/docs/ACPI-FIXES.md`
- `local/docs/IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md`
- this file
- related references in `README.md`, `docs/02-GAP-ANALYSIS.md`, and `AGENTS.md` if needed

### Status: Wave 0 execution partially complete

Tasks 0.1, 0.2, and 0.3 are partially executed in this documentation pass. The degraded-mode matrix
and normalized vocabulary are new in this pass. Ownership boundaries are partially documented below;
the canonical statement still lives in `local/docs/ACPI-FIXES.md`.

### Vocabulary normalization (Task 0.2 — partially executed)

Replace ambiguous wording such as "complete" with one of:

- **implemented** — behavior exists in the current source tree
- **validated in QEMU** — behavior has been exercised in QEMU/OVMF but not on real hardware
- **validated on bounded real hardware** — behavior verified on specific hardware that was tested
- **still transitional** — behavior exists but ownership or robustness is not yet clean
- **known gap** — functionality is absent or broken; the gap is documented

### ACPI degraded-mode matrix (Task 0.1 — new)

This matrix documents the expected system behavior for ACPI failure cases. All entries reflect
implemented behavior visible in the current source tree.

| Condition | Kernel behavior | Userspace (`acpid`) behavior | Session impact |
|-----------|----------------|----------------------------|----------------|
| Bad RSDP checksum | Warns, continues with best-effort RSDP parse | No ACPI init if RSDT/XSDT unreadable; exits cleanly | No ACPI services |
| Bad SDT checksum | Logs warning per table, continues | Table skipped; other tables still served | Reduced ACPI surface |
| Truncated FADT | FADT fields fall back to zero defaults | Uses zero defaults for PM registers; `acpi_shutdown` may not fire | Shutdown may fall back to keyboard controller |
| Truncated DMAR | N/A (DMAR not used by kernel) | Logs error, continues without DMAR; `iommu` daemon not started | No Intel IOMMU via DMAR |
| Truncated IVRS | N/A (IVRS not used by kernel) | No effect on `acpid` (IVRS owned by `iommu` daemon) | No AMD-Vi via IVRS |
| AML interpreter init failure | N/A | `acpid` exits with typed error; no ACPI scheme | No AML, no power methods |
| EC timeout | N/A | Returns `AmlError::MutexAcquireTimeout` to AML interpreter | AML opregion access fails gracefully |
| EC unsupported width access | N/A | Wider accesses split into byte transactions via `read_bytes`/`write_bytes` | Works on byte-access ECs only |
| Missing DMAR on Intel | N/A | `acpid` logs DMAR absent; `iommu` daemon not started | No Intel VT-d |
| Missing IVRS on AMD | N/A | No effect (IVRS owned by `iommu` daemon) | No AMD-Vi |
| Missing `\_S5` sleep types | N/A | `acpi_shutdown` logs error and returns without writing PM registers | Shutdown may fall back to keyboard controller |
| Missing `/scheme/kernel.acpi/kstop` | Kernel does not register kstop scheme | `acpid` exits with error on startup | No kernel-orchestrated shutdown |
| Sleep state transition (`\_Sx`) | N/A | Not implemented; no event-driven sleep contract | Sleep states not available |
| `redbear-sessiond` shutdown watcher | Kernel signals `kstop` on shutdown | `acpi_watcher.rs` reads kstop and emits D-Bus `PrepareForShutdown` | Login1 session manager informed |

### Ownership boundaries (Task 0.3 — partially documented)

This section documents the current ownership split as visible in the source tree. Items marked
**transitional** indicate the ownership boundary is not yet fully enforced by code.

| Component | Owner | Status |
|-----------|-------|--------|
| Early table discovery (RSDP, RSDT, XSDT) | Kernel | implemented |
| MADT, HPET, SPCR, GTDT parsing | Kernel | implemented |
| FADT parsing, `\_S5` sleep types, PM registers | `acpid` | implemented |
| AML interpreter initialization and execution | `acpid` | implemented |
| EC access (byte-wide and widened via byte transactions) | `acpid` | implemented |
| AML mutex state tracking | `acpid` (`aml_physmem.rs`) | implemented (real tracked state, not placeholder) |
| FADT shutdown/reboot via PM1a/PM1b CNT | `acpid` | implemented |
| Keyboard controller fallback reboot | `acpid` | implemented |
| DMAR table parsing | `acpid` (module present) | **transitional** — module not wired; effectively owned by `iommu` daemon |
| IVRS ownership | `iommu` daemon | implemented |
| MCFG/PCI config space | `pcid` `/config` endpoint | implemented |
| DMI exposure and quirks | `acpid` via `/scheme/acpi/dmi` | implemented |
| Power methods (`\_PS0`/`\_PS3`/`\_PPC`) | `acpid` | implemented |
| Sleep state transitions (`\_Sx` beyond `\_S5`) | none | **known gap** |
| Sleep eventing | none | **known gap** |
| Shutdown event via `kstop` | Kernel + `acpid` + `redbear-sessiond` | implemented (QEMU-validated; bare-metal validation outstanding) |
| DMAR runtime ownership (Intel VT-d) | `iommu` daemon | **transitional** — not yet fully separated from `acpid` DMAR module |

### Acceptance criteria

- one canonical ownership statement exists — **partially met** (this table, plus `ACPI-FIXES.md`)
- one degraded-mode matrix exists — **met** (this pass)
- all high-level ACPI status claims use the same vocabulary — **partially met** (normalized in this pass)

### Validation

- doc review only,
- no code changes required for vocabulary and matrix,
- Wave 0 should be treated as ongoing; new evidence may require matrix updates

## Wave 1 — Boot-path hardening and parser strictness

### Goal

Remove catastrophic or silent failure behavior from the boot-critical ACPI path.

### Main files

- `recipes/core/base/source/drivers/acpid/src/main.rs`
- `recipes/core/base/source/drivers/acpid/src/acpi.rs`
- `recipes/core/base/source/drivers/acpid/src/scheme.rs`
- `recipes/core/kernel/source/src/acpi/mod.rs`
- kernel ACPI submodules for RSDP/RSDT/XSDT/MADT/HPET/SPCR/GTDT

### Status: Task 1.1 partially executed

`acpid` main.rs now uses a typed `StartupError` enum covering:

- `ReadRootTable` — failed to read `/scheme/kernel.acpi/rxsdt`
- `ParseRootTable` — failed to parse `[R|X]SDT`
- `UnexpectedRootTableSignature` — wrong root table signature
- `MalformedRootTableEntries` — malformed entry area
- `InitializeAcpi` — failed ACPI context init
- port I/O rights acquisition failure
- shutdown pipe open failure (`/scheme/kernel.acpi/kstop`)
- event queue creation failure
- scheme socket creation failure
- event queue subscription failure
- scheme registration failure

Each failure path logs a human-readable error and calls `std::process::exit(1)`. No `panic!`
remains on these paths. Empty RSDT (no ACPI) causes a clean `exit(0)` after `daemon.ready()`.

Tasks 1.2 and 1.3 remain open.

### Tasks

#### Task 1.1 — Replace panic-grade startup failures in `acpid` — **partially done**

Typed `StartupError` enum is implemented in `main.rs`. The following failure classes are now handled:

- hard fail with typed error message and exit code 1,
- soft fail with degraded behavior (ACPI absent → clean exit 0),
- or early clean exit when `/scheme/kernel.acpi/rxsdt` is empty.

#### Task 1.2 — Make table rejection policy explicit — **open**

For kernel and `acpid` table use, define when a bad length/checksum/revision:

- is logged and ignored,
- is logged and downgraded,
- or is fatal.

This policy must be table-specific, not one global "warn and continue" convention.

#### Task 1.3 — Improve parser observability — **open**

Every accepted or rejected table should leave enough evidence to reconstruct why:

- table signature,
- physical address if known,
- length/revision/checksum status,
- consumer that requested it,
- fallback path chosen.

### Acceptance criteria

- no `panic!/expect()` remains on firmware-origin or optional-service startup paths in `acpid` — **partially met** (Tasks 1.2 and 1.3 still open),
- malformed-table decisions are deterministic and documented — **open**,
- degraded boot still succeeds in all cases classified as degradable by Wave 0 — **open**.

### Validation

- negative tests for malformed checksums and table lengths,
- QEMU validation with intentionally damaged tables if feasible,
- one AMD and one Intel bounded hardware boot recheck,
- evidence captured in `local/docs/BAREMETAL-LOG.md` or a successor log.

## Wave 2 — AML, opregions, EC, and power-state correctness

### Goal

Close the biggest runtime-correctness gaps in the ACPI stack.

### Main files

- `recipes/core/base/source/drivers/acpid/src/acpi.rs`
- `recipes/core/base/source/drivers/acpid/src/aml_physmem.rs`
- `recipes/core/base/source/drivers/acpid/src/ec.rs`

### Status: Tasks 2.1, 2.2, and 2.5 partially executed

#### Task 2.1 — Remove placeholder AML mutex behavior — **partially done**

`AmlMutexState` in `aml_physmem.rs` implements real tracked state:

- `AmlMutexState::create_handle()` generates unique handles via incrementing `next_handle`
- `AmlMutexState::states` is a `FxHashMap<Handle, bool>` tracking locked/unlocked state
- `lock_aml_mutexes()` wraps the state map with proper `Mutex` guard and poisoned-state recovery
- The `acquire()` method looks up the handle in the map, sets it to `true` on success, and returns
  `AmlError::MutexAcquireTimeout` on timeout or unknown handle

This is no longer a placeholder implementation. Remaining work: timeout semantics documentation
and concurrent acquire/release stress testing.

#### Task 2.2 — Eliminate silent zero-on-failure physical reads — **partially done**

EC reads via `read_bytes` now propagate `AmlError::MutexAcquireTimeout` on EC timeout rather than
returning zero. Kernel-physmem reads still have some silent failure paths; this task is not fully
closed.

#### Task 2.5 — Decide and validate EC width behavior — **partially done**

`ec.rs` now implements `read_u16`, `read_u32`, `read_u64`, `write_u16`, `write_u32`, and `write_u64`
on `Ec` via byte-transaction sequences in `read_bytes`/`write_bytes`:

- `ensure_access()` validates the access fits in a u8 addressable range
- `read_bytes<const N: usize>` loops over individual byte reads with timeout per byte
- `write_bytes()` loops over individual byte writes with timeout per byte

Wider accesses are emulated through byte transactions rather than being rejected. This is implemented
behavior, not a placeholder. Validation on real EC hardware remains outstanding.

### Tasks still open

#### Task 2.3 — Finish `AmlSymbols` initialization contract — **open**

`AmlSymbols` initialization order is still tied to PCI FD registration timing. AML initialization
is not fully deterministic. The upstream TODO to "use these parsed tables for the rest of acpid"
remains.

#### Task 2.4 — Fix power-state completeness gaps — **open**

`SLP_TYPb` handling remains unimplemented. Sleep state transitions beyond `\_S5` are not supported.
Sleep eventing is not implemented. These are documented as known gaps.

### Acceptance criteria

- AML synchronization is no longer placeholder-driven — **partially met** (2.1 done; 2.3 open),
- physmem failures do not silently fabricate correctness-critical values — **partially met** (EC done; kernel-physmem still open),
- AML initialization order is reproducible and documented — **open** (Task 2.3),
- sleep-state handling is explicit for both implemented and out-of-scope states — **open** (Task 2.4),
- EC behavior is either implemented or honestly bounded — **met** (byte transactions for wider widths)

### Validation

- targeted AML method execution tests,
- shutdown/reboot proof on QEMU and bounded real hardware,
- EC timeout/error-path tests where possible,
- concurrent ACPI scheme reads while AML methods run.

## Wave 3 — Ownership cleanup: reduce kernel ACPI scope and remove DMAR from `acpid`

### Goal

Move from transitional ownership to architecture that is easier to maintain.

### Main files

- `recipes/core/kernel/source/src/acpi/mod.rs`
- kernel ACPI submodules as needed
- `recipes/core/base/source/drivers/acpid/src/acpi/dmar/mod.rs`
- `recipes/core/base/source/drivers/acpid/src/scheme.rs`
- `local/recipes/system/iommu/source/src/*`

### Status: Tasks 3.1 and 3.2 partially executed

#### Task 3.1 — Define the minimum kernel ACPI surface — **open**

The kernel still carries TODOs for kernel ACPI scope reduction. No staged migration contract has
been written yet.

#### Task 3.2 — Move DMAR to the correct owner — **partially done**

The `acpi/dmar/mod.rs` module remains present in `acpid` source but is not imported or called from
`main.rs` startup. The DMAR parsing code itself is not executed at daemon startup. However, the
module has not been removed from the source tree, creating latent confusion about ownership.

The `iommu` daemon is responsible for IVRS/DMAR runtime handling. DMAR is not initialized by
`acpid`. The exit path from `acpid` for DMAR is therefore effectively achieved, but the cleanup
(task: remove the unused module or move it to the `iommu` crate) is not complete.

#### Task 3.3 — Ensure handoff paths are explicit — **open**

Handoff paths for table discovery and CPU/topology are not yet documented as a staged migration
contract.

### Acceptance criteria

- the minimal kernel ACPI contract is written down — **open**,
- DMAR has a concrete exit path from `acpid` — **partially met** (not wired; module still present),
- ownership reductions are staged and do not break current bring-up — **open**

### Validation

- before/after boot regressions,
- Intel-specific validation for DMAR path changes,
- AMD regression checks proving IVRS ownership remains isolated in `iommu`.

## Wave 4 — Consumer integration and eventing quality

### Goal

Make ACPI consumers correct and low-friction, not just functional.

### Main files

- `local/recipes/system/redbear-sessiond/source/src/acpi_watcher.rs`
- `recipes/core/base/source/drivers/acpid/src/scheme.rs`
- DMI/quirk consumers under `redox-driver-sys` and reporting surfaces

### Status: Task 4.1 partially executed; sleep eventing still a gap

#### Task 4.1 — Replace polling-based ACPI state consumption — **partially done**

Shutdown eventing is now event-driven via `/scheme/kernel.acpi/kstop`:

- `acpid` opens `kstop` at startup and subscribes to it via `RawEventQueue`
- When the kernel triggers shutdown, `acpid` receives an event on the `kstop` file descriptor
- `redbear-sessiond`'s `acpi_watcher.rs` opens `kstop` and reads one byte in a blocking
  `spawn_blocking` call, then emits D-Bus `PrepareForShutdown(true)` signal

Sleep eventing (`\_Sx` transitions) remains unsupported. There is no `/scheme/acpi/sleep` surface
and no event-driven sleep contract. This is a known gap.

#### Task 4.2 — Bound DMI quirk authority — **open**

#### Task 4.3 — Improve operator-facing observability — **open**

### Acceptance criteria

- no periodic polling remains for core ACPI power/session transitions if eventing is feasible — **partially met** (shutdown done; sleep still polling/absent),
- quirk precedence is documented — **open**,
- consumer-visible behavior is diagnosable from logs and status outputs — **open**

### Validation

- repeated shutdown/sleep edge tests,
- DMI quirk application checks on known systems,
- race checks with multiple simultaneous consumers of `/scheme/acpi/*`.

## Wave 5 — Validation closure and release gate

### Goal

Convert the current implementation from bring-up evidence into release-grade trust.

### Validation matrix

At minimum, require:

- QEMU/OVMF boot with ACPI active,
- modern AMD hardware,
- modern Intel hardware,
- one platform that exercises EC-backed AML behavior,
- malformed-table or degraded-mode evidence where feasible.

### Tasks

#### Task 5.1 — Publish a platform matrix

For each validated platform, record:

- firmware mode,
- key ACPI tables detected,
- APIC mode,
- whether shutdown/reboot worked,
- whether DMI and power exposure worked,
- whether any AML/EC failure was observed.

#### Task 5.2 — Capture negative results

Do not hide unsupported AML opcodes, partial EC behavior, or platform-specific regressions behind a
generic "works on tested hardware" label.

#### Task 5.3 — Define the ACPI release gate

Before calling ACPI complete for current Red Bear goals, require:

- clean boot on the bounded matrix,
- explicit degraded-mode behavior for known bad firmware cases,
- documented ownership state,
- and current docs that distinguish implemented vs validated.

### Acceptance criteria

- one bounded but honest validation matrix exists,
- negative results are documented,
- ACPI status claims are tied to explicit evidence rather than inference.

## Upstream vs Red Bear Work Split

### Upstream-first work

These are generic ACPI correctness or architecture issues and should be solved upstream whenever
possible, with temporary Red Bear patch carriers only if necessary:

- `acpid` startup hardening
- AML mutex semantics in `aml_physmem.rs`
- `SLP_TYPb` completion
- EC error typing and possibly EC access-width handling
- using parsed tables for the rest of `acpid`
- DMAR leaving `acpid`
- kernel ACPI scope reduction TODOs
- generic parser quality for kernel ACPI modules

### Red Bear-owned work

These remain Red Bear responsibilities even if upstream code improves:

- honest status/phase documentation
- bounded validation matrix and operator runbooks
- `redbear-sessiond` event consumption quality
- DMI quirk governance and integration policy
- temporary durable patch carriers in `local/patches/*`
- coordination between `acpid`, `iommu`, `pcid`, and downstream consumers

## Sequencing Constraints

1. **Wave 0 must come first** so later changes do not drift architecturally.
2. **Wave 1 must come before Wave 2** so runtime correctness sits on a hardened startup path.
3. **Wave 2 should come before Wave 4** because consumer contracts should depend on correct AML and
   power behavior.
4. **Wave 3 should not start until Waves 1 and 2 are at least partially complete**; ownership moves
   are dangerous if the runtime behavior is still fragile.
5. **Wave 5 closes the work**; it must not be used as a substitute for architecture.

## Main Risks

- stricter parser/error handling may expose machines that currently boot only by luck,
- AML/EC changes may uncover hidden ordering assumptions with PCI registration,
- reducing kernel ownership too early may regress early platform bring-up,
- moving DMAR out of `acpid` may create Intel-only regressions if the replacement contract is vague,
- DMI quirks can become a crutch if they are allowed to override runtime facts indiscriminately.

## Deliverable Order

If work from this plan is executed, the recommended order is:

1. documentation and degraded-mode contract,
2. startup hardening,
3. AML/EC correctness,
4. ownership cleanup,
5. consumer/eventing quality,
6. validation closure.

## Definition of Done for the Current ACPI Plan

This plan can be considered substantially complete only when:

- ownership boundaries are explicit — **partially met** (this doc; module-level cleanup still needed)
- boot-critical panic/silent-fallback paths are removed or justified — **partially met** (Task 1.1 done; Tasks 1.2 and 1.3 open)
- AML and EC behavior are no longer TODO-grade — **partially met** (mutex state and EC width done; AML init order and SLP_TYPb open)
- DMAR and IVRS ownership are cleanly separated — **partially met** (DMAR not wired; module still present in acpid)
- ACPI consumers are event-driven or explicitly bounded — **partially met** (shutdown done via kstop; sleep not implemented)
- sleep state transitions and eventing are implemented or explicitly documented as known gaps — **open**
- the repo contains platform evidence that supports its status claims — **open** (QEMU validated; bare-metal evidence still needed)

Current truthful status for Red Bear ACPI:

> materially complete for historical bring-up, but still under active robustness, ownership,
> sleep-state, and validation improvement. Shutdown eventing is implemented via kstop. Sleep state
> transitions are a known gap. EC width support is implemented via byte transactions. AML mutex
> state is real-tracked, not placeholder. DMAR is not initialized by acpid. Bare-metal validation
> for the full ACPI surface is still outstanding.
