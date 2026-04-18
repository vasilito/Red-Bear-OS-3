# Red Bear OS ACPI Improvement Plan

## Truth Statement

Red Bear ACPI is **boot-baseline complete for the historical P0 bring-up goal**, but it is **not
release-grade complete**.

What is real today:

- kernel early discovery and MADT/xAPIC/x2APIC bring-up are in place,
- `acpid` owns FADT shutdown/reboot, AML execution, DMI exposure, and ACPI power exposure,
- IVRS/AMD-Vi ownership moved out of the broken `acpid` path and into `iommu`,
- `kstop` shutdown eventing exists and is integrated with `redbear-sessiond`.

What is still open:

- sleep-state support beyond `\_S5`,
- AML portability and runtime robustness on real firmware,
- clean ownership boundaries across kernel / `acpid` / IOMMU,
- bounded real-hardware validation on AMD, Intel, and at least one EC-backed platform.

This document is therefore a **ULW execution plan** for turning the current ACPI stack from
historical bring-up success into a subsystem that is honest, maintainable, and release-grade.

## Purpose

This plan does **not** replace `local/docs/ACPI-FIXES.md`.

- `local/docs/ACPI-FIXES.md` remains the historical ledger for P0 ACPI bring-up and the current
  table-by-table implementation snapshot.
- This file is the forward execution plan for closing the remaining ACPI gaps in correctness,
  ownership clarity, consumer integration, and validation trust.

The goal is not to maximize the number of parsed ACPI tables. The goal is to make the Red Bear ACPI
stack:

- correct under bad firmware,
- explicit about who owns what,
- observable when it fails,
- and validated enough that status claims are evidence-backed rather than inferred.

## Scope

This plan covers the Red Bear ACPI stack and its direct consumers:

- kernel ACPI discovery and early platform setup,
- `acpid` as the main ACPI / AML / FADT / DMI / power daemon,
- `iommu` as the IVRS / AMD-Vi runtime owner,
- `pcid` and `/config` as the PCI config-space path replacing broken MCFG-in-`acpid` stubs,
- DMI-backed quirks flowing through `acpid` and `redox-driver-sys`,
- ACPI consumers such as `redbear-sessiond`, `redbear-info`, and downstream services.

Primary focus is the current `x86_64` path. ARM64 remains in scope only where parser quality or
kernel-ownership decisions are shared.

## Canonical Related Documents

Read these alongside this plan:

- `local/docs/ACPI-FIXES.md`
- `local/docs/BAREMETAL-LOG.md`
- `local/docs/IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md`
- `local/docs/IOMMU-SPEC-REFERENCE.md`
- `local/docs/QUIRKS-SYSTEM.md`
- `docs/02-GAP-ANALYSIS.md`

## Evidence Model

This plan uses five evidence buckets and does **not** treat them as equivalent:

- **source-visible** — behavior is visible in the checked-in source tree
- **patch-carried** — behavior exists through `local/patches/*`
- **build-visible** — code compiles and stages in the current build
- **runtime-validated** — behavior has been exercised successfully in boot or runtime
- **negative-result-documented** — failures and platform gaps are explicitly recorded

This distinction matters because the current ACPI stack has already crossed the bring-up threshold,
but still has meaningful distance between **implemented**, **robust**, and **trusted**.

## Status Vocabulary

All ACPI status claims in Red Bear docs should use one of these meanings:

- **implemented** — present in code today
- **validated in QEMU** — exercised in QEMU / OVMF only
- **validated on bounded real hardware** — proven on named tested hardware only
- **transitional** — exists, but ownership or architecture is still not clean
- **known gap** — absent, incomplete, or intentionally deferred and documented

Do **not** use a bare “complete” claim without also saying whether it means boot-baseline,
bounded-hardware, or release-grade completeness.

## Current State Summary

### Strong today

- Kernel RSDP / RSDT / XSDT / MADT handling is sufficient for current boot bring-up.
- `acpid` owns FADT parsing, AML integration, DMI exposure, and ACPI-backed power-state exposure.
- `acpid` startup uses typed `StartupError` and clean exits for several boot-critical failure paths.
- AML mutex state is real-tracked in `aml_physmem.rs`, not placeholder-only.
- EC width access is implemented via byte-transaction sequences for widened reads and writes.
- IVRS ownership was removed from the broken `acpid` stub path and moved into the `iommu` daemon.
- MCFG handling was removed from `acpid` and replaced with the `pcid /config` path.
- Shutdown eventing via `/scheme/kernel.acpi/kstop` is implemented and consumed by
  `redbear-sessiond`.

### Weak today

- Sleep-state transitions beyond `\_S5` are unsupported.
- Sleep eventing is unsupported.
- `SLP_TYPb` remains incomplete for broader sleep-state handling.
- AML init order is still tied to PCI FD registration timing.
- Some physmem / opregion failure paths are still not explicit enough.
- DMAR remains orphaned in `acpid` source: present, not wired, not fully transferred.
- Repo status language can still blur “implemented” vs “validated”.
- Bare-metal validation is too thin to justify release-grade claims.

## Ownership Model

The long-term ownership split should be:

| Component | Intended owner | Current status |
|---|---|---|
| RSDP / RSDT / XSDT early discovery | Kernel | implemented |
| MADT / HPET / early unavoidable platform setup | Kernel | implemented, broader scope still transitional |
| FADT parsing, `\_S5`, PM register writes, reboot | `acpid` | implemented |
| AML execution and opregion handling | `acpid` | implemented, robustness still partial |
| DMI exposure and ACPI power surfaces | `acpid` | implemented |
| IVRS / AMD-Vi runtime handling | `iommu` | implemented |
| DMAR / Intel VT-d runtime handling | future Intel IOMMU owner | transitional / not fully assigned |
| PCI config-space access | `pcid` | implemented |
| ACPI consumers | downstream services | should consume ACPI-owned surfaces, not firmware directly |

Important ownership truth:

- **DMAR is not cleanly transferred today.**
- The `acpi/dmar/mod.rs` module still exists inside `acpid` source, but is not wired into startup.
- `iommu` is the real IVRS runtime owner today.
- Do **not** describe Intel DMAR ownership as fully complete until the orphaned `acpid` carrier is
  removed or a real Intel runtime owner is implemented and validated.

## Degraded-Mode Contract

The ACPI stack must distinguish between **fatal**, **degradable**, and **out-of-scope** failures.

| Condition | Expected behavior today | Classification |
|---|---|---|
| ACPI absent / empty root table | `acpid` exits cleanly without ACPI services | degradable |
| Bad SDT checksum | warn, continue best-effort where supported | degradable |
| Bad table length / malformed table | behavior varies too much today; must be normalized | open contract |
| AML init failure | `acpid` exits, ACPI scheme unavailable | currently fatal |
| EC timeout | AML error path should surface failure, not fabricate success | degradable |
| Missing `\_S5` | shutdown path cannot use PM registers | degradable if fallback exists |
| Sleep-state transition request | unsupported today | known gap |
| Missing `kstop` path | no kernel-orchestrated shutdown event contract | fatal for that integration path |
| Missing DMAR on Intel | no Intel VT-d runtime | degradable for non-IOMMU boot |
| Missing IVRS on AMD | no AMD-Vi runtime | degradable for non-IOMMU boot |

Wave 1 must convert the still-fuzzy cases into explicit, table-specific policy.

## ULW Execution Rules

These rules govern all work from this plan:

1. **No hidden status inflation.** Status words must match evidence.
2. **No ownership moves without a handoff contract.** “Not wired” is not the same as “cleanly moved.”
3. **No validation laundering.** QEMU success is not bare-metal success.
4. **No Wave 5 shortcuts.** Validation cannot substitute for unfinished architecture.
5. **No cross-wave dependency drift.** Later waves must not silently depend on work that was never
   formalized in earlier waves.

## Wave 0 — Contracts, truthfulness, and degraded-mode policy

### Goal

Establish one canonical answer to:

1. who owns what,
2. what counts as degraded but acceptable,
3. and what ACPI status words mean.

### Why this wave is first

Without a contract, all later hardening work turns into undocumented rewrites and docs drift.

### Primary files

- `local/docs/ACPI-FIXES.md`
- this file
- `docs/02-GAP-ANALYSIS.md`
- `README.md` and related status surfaces if needed

### Dependencies

- none

### Deliverables

- one normalized ACPI vocabulary,
- one degraded-mode contract,
- one canonical ownership statement,
- removal of doc language that implies subsystem completeness without evidence.

### Verification

- documentation review only,
- no contradictory ownership claims across ACPI docs,
- no bare “complete” wording without scope.

### Exit criteria

- one canonical ownership statement exists,
- one degraded-mode matrix exists,
- all top-level ACPI docs use the same vocabulary.

### Current status

- partially complete

## Wave 1 — Boot-path hardening and parser strictness

### Goal

Remove catastrophic or silent failure behavior from boot-critical ACPI initialization.

### Primary files

- `recipes/core/base/source/drivers/acpid/src/main.rs`
- `recipes/core/base/source/drivers/acpid/src/acpi.rs`
- `recipes/core/base/source/drivers/acpid/src/scheme.rs`
- `recipes/core/kernel/source/src/acpi/mod.rs`
- kernel ACPI submodules as needed

### Dependencies

- Wave 0 ownership and degraded-mode vocabulary in place

### Deliverables

- startup paths are typed and explicit,
- table rejection policy is documented per table class,
- parser observability is strong enough to reconstruct failures,
- degraded boot succeeds for all conditions classified as degradable.

### Specific tasks

1. Finish replacing panic-grade startup behavior in active firmware-origin paths.
2. Define table-specific reject / warn / degrade / fail rules.
3. Log accepted and rejected tables with enough evidence to debug failures.

### Verification

- malformed checksum / truncated-length tests,
- QEMU validation with intentionally damaged tables if feasible,
- one bounded AMD hardware boot recheck,
- one bounded Intel hardware boot recheck,
- evidence captured in `local/docs/BAREMETAL-LOG.md` or its successor.

### Exit criteria

- no unjustified `panic!/expect()` remains on firmware-origin startup paths,
- malformed-table decisions are deterministic and documented,
- degraded boot behavior matches Wave 0 classification.

### Current status

- partially complete

## Wave 2 — AML, opregions, EC, and power-state correctness

### Goal

Close the biggest runtime-correctness gaps in the `acpid` layer.

### Primary files

- `recipes/core/base/source/drivers/acpid/src/acpi.rs`
- `recipes/core/base/source/drivers/acpid/src/aml_physmem.rs`
- `recipes/core/base/source/drivers/acpid/src/ec.rs`

### Dependencies

- Wave 1 startup paths hardened enough that runtime work is not sitting on a fragile base

### Deliverables

- real AML synchronization semantics,
- explicit physmem / opregion failure behavior,
- deterministic AML init order,
- explicit sleep-state scope,
- honest EC behavior bounds.

### Specific tasks

1. Document and stress AML mutex timeout semantics.
2. Remove silent correctness-critical physmem failure paths.
3. Finish `AmlSymbols` initialization contract; stop tying AML readiness to fragile PCI timing.
4. Decide whether sleep support is in-scope now or explicitly deferred.
5. If in-scope now, implement and validate the missing sleep-state pieces, including `SLP_TYPb`.

### Verification

- targeted AML method execution tests,
- shutdown / reboot proof in QEMU and bounded hardware,
- EC timeout and error-path tests,
- concurrent ACPI scheme reads while AML methods run,
- at least one EC-backed platform check if available.

### Exit criteria

- AML synchronization is no longer placeholder-grade,
- physmem failures do not silently fabricate correctness-critical values,
- AML initialization order is reproducible and documented,
- sleep-state handling is either implemented or explicitly bounded as a known gap,
- EC behavior is implemented or honestly constrained.

### Current status

- partially complete

## Wave 3 — Ownership cleanup and kernel-surface reduction

### Goal

Move from transitional ownership to an architecture that can survive long-term maintenance.

### Primary files

- `recipes/core/kernel/source/src/acpi/mod.rs`
- kernel ACPI submodules as needed
- `recipes/core/base/source/drivers/acpid/src/acpi/dmar/mod.rs`
- `recipes/core/base/source/drivers/acpid/src/scheme.rs`
- `local/recipes/system/iommu/source/src/*`

### Dependencies

- Wave 1 and Wave 2 are at least partially complete

### Deliverables

- a minimum kernel ACPI contract,
- explicit handoff paths for table discovery and topology,
- DMAR no longer orphaned in `acpid`,
- ownership wording that matches the code.

### Specific tasks

1. Define the minimum kernel ACPI surface that must remain in early boot.
2. Document the userspace handoff contract for topology and table consumers.
3. Remove or relocate the orphaned DMAR carrier in `acpid`.
4. Do not claim Intel DMAR runtime ownership complete unless a real owner exists and is validated.

### Verification

- before / after boot regressions,
- Intel-specific validation for any DMAR ownership move,
- AMD regression checks showing IVRS ownership remains isolated in `iommu`.

### Exit criteria

- the minimum kernel ACPI contract is written down,
- DMAR has a concrete, non-ambiguous owner or is explicitly deferred,
- ownership reductions do not regress current bring-up.

### Current status

- partially complete

## Wave 4 — Consumer integration and eventing quality

### Goal

Make ACPI consumers correct, observable, and low-friction.

### Primary files

- `local/recipes/system/redbear-sessiond/source/src/acpi_watcher.rs`
- `recipes/core/base/source/drivers/acpid/src/scheme.rs`
- DMI / quirk consumers in `redox-driver-sys`
- reporting surfaces such as `redbear-info`

### Dependencies

- Wave 2 runtime behavior is stable enough for downstream consumers to depend on it

### Deliverables

- event-driven core power-session behavior where feasible,
- bounded DMI quirk authority,
- operator-facing observability strong enough to diagnose behavior,
- explicit treatment of unsupported sleep eventing if it remains deferred.

### Specific tasks

1. Keep shutdown eventing on `kstop` as the canonical shutdown signal.
2. Improve consumer-facing observability for ACPI state and failures.
3. Define DMI quirk precedence and limits.
4. If sleep eventing remains out-of-scope, document that explicitly and consistently.

### Verification

- repeated shutdown edge tests,
- sleep-edge tests if sleep work is in scope,
- DMI quirk application checks on known systems,
- race checks with multiple simultaneous consumers of `/scheme/acpi/*`.

### Exit criteria

- no unnecessary polling remains for core ACPI transitions where eventing is feasible,
- quirk precedence is documented,
- consumer-visible behavior is diagnosable from logs and status outputs.

### Current status

- partially complete

## Wave 5 — Validation closure and release gates

### Goal

Turn the current ACPI stack from bring-up evidence into release-grade trust.

### Dependencies

- Waves 1 through 4 have produced stable behavior worth validating

### Required validation matrix

At minimum:

- QEMU / OVMF boot with ACPI active,
- one modern AMD machine,
- one modern Intel machine,
- one platform that exercises EC-backed AML behavior,
- malformed-table or degraded-mode evidence where feasible.

### Deliverables

- a bounded platform matrix,
- negative-result capture,
- explicit release gates for both boot-baseline and full ACPI claims,
- docs that distinguish implemented from validated.

### Specific tasks

1. Publish the platform matrix in `local/docs/BAREMETAL-LOG.md` or its successor.
2. Record for each platform: firmware mode, key ACPI tables, APIC mode, shutdown / reboot,
   DMI / power exposure, AML / EC failures, and notable degraded behavior.
3. Preserve negative results such as unsupported AML opcodes or platform-specific regressions.
4. Require evidence before any stronger ACPI completeness claim is made.

### Verification

- repeated QEMU proof,
- bounded repeated bare-metal proof on AMD and Intel,
- one EC-heavy platform check,
- cross-check docs so claims match recorded evidence.

### Exit criteria

- one bounded but honest platform matrix exists,
- negative results are documented,
- ACPI status claims are tied to explicit evidence,
- release gates are defined and followed.

### Current status

- open

## Release Gates

### Gate A — Boot-Baseline ACPI Ready

This is the strongest claim the repo can make before sleep and broader ownership cleanup are done.

Require:

- clean boot on bounded QEMU + AMD + Intel validation targets,
- working MADT / APIC initialization on those targets,
- shutdown / reboot proof where supported,
- explicit degraded behavior for known firmware-bad cases,
- current docs that distinguish implemented from validated.

### Gate B — Full ACPI / Power-Management Ready

Do **not** claim this until all of the following are true:

- AML runtime behavior is stable across the bounded matrix,
- sleep-state scope is implemented and validated or explicitly excluded from the release claim,
- ownership boundaries are clean rather than merely transitional,
- consumer integration is observable and race-bounded,
- the platform matrix supports the stronger claim.

## Upstream vs Red Bear Work Split

### Prefer upstream for

- generic `acpid` startup hardening,
- AML mutex semantics,
- `SLP_TYPb` completion,
- EC error typing and generic width behavior,
- reuse of parsed tables inside `acpid`,
- DMAR leaving `acpid`,
- kernel ACPI scope reduction TODOs,
- generic parser quality in kernel ACPI modules.

### Red Bear owns

- honest status language,
- bounded validation matrices and runbooks,
- `redbear-sessiond` shutdown-consumer quality,
- DMI quirk governance and integration policy,
- patch carriers in `local/patches/*`,
- coordination across `acpid`, `iommu`, `pcid`, and downstream consumers.

## Sequencing Constraints

1. **Wave 0 first** — architecture and wording must stop drifting.
2. **Wave 1 before Wave 2** — runtime correctness should not sit on fragile startup behavior.
3. **Wave 2 before Wave 4** — consumer contracts must rely on correct AML / power behavior.
4. **Wave 3 after Waves 1 and 2 are partially stable** — ownership moves are risky on unstable
   behavior.
5. **Wave 5 last** — validation closes work; it does not replace architecture.

## Main Risks

- stricter parser behavior may expose machines currently booting only by luck,
- AML / EC changes may uncover hidden PCI-registration ordering assumptions,
- reducing kernel scope too early may regress early bring-up,
- careless DMAR cleanup may create Intel-only regressions,
- DMI quirks can become a crutch if allowed to override runtime facts indiscriminately.

## Non-Goals

- claiming sleep support that does not exist,
- calling DMAR ownership “complete” while the orphaned `acpid` module still exists,
- treating one-machine success as subsystem-level proof,
- using Wave 5 validation language to hide unfinished ownership work.

## Deliverable Order

Recommended order:

1. truth contract and doc normalization,
2. startup hardening,
3. AML / EC / power-state correctness,
4. ownership cleanup,
5. consumer / eventing quality,
6. validation closure and release gate.

## Definition of Done

This plan is substantially complete only when:

- ownership boundaries are explicit and not contradicted by the code,
- boot-critical panic / silent-fallback paths are removed or justified,
- AML and EC behavior are no longer TODO-grade,
- DMAR and IVRS ownership are no longer described ambiguously,
- consumers are event-driven or explicitly bounded where eventing is not feasible,
- sleep-state handling is implemented or explicitly excluded from the release claim,
- the repo contains bounded platform evidence that supports every status claim.

## Current Truthful Status

> Red Bear ACPI is materially complete for historical boot bring-up, but still under active
> robustness, ownership, sleep-state, and validation improvement. Shutdown eventing is implemented
> via `kstop`. Sleep-state transitions remain a known gap. EC widened access is implemented via byte
> transactions. AML mutex state is real-tracked, not placeholder. DMAR is not initialized by
> `acpid`, and Intel DMAR runtime ownership is not yet cleanly closed. Bare-metal validation for the
> full ACPI surface is still outstanding.
