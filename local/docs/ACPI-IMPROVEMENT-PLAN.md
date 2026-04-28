# Red Bear OS ACPI Improvement Plan

## Truth Statement

Red Bear ACPI is **boot-baseline complete for the historical P0 bring-up goal**, but it is **not
release-grade complete**.

What is real today:

- kernel early ACPI discovery exists and is used,
- MADT / APIC / HPET boot-baseline handling is real,
- `acpid` owns most runtime ACPI policy,
- `/scheme/kernel.acpi/kstop` shutdown eventing exists,
- `redbear-sessiond` consumes that shutdown-prep signal,
- IVRS / AMD-Vi ownership moved out of the broken `acpid` path and into `iommu`,
- MCFG-in-`acpid` was removed in favor of the `pcid /config` path,
- `hwd` now forwards `RSDP_ADDR` / `RSDP_SIZE` to `acpid` explicitly when those values are present,
- x86 userspace AML bootstrap now has a bounded BIOS RSDP search fallback when explicit handoff is absent,
- `/scheme/acpi/power` is backed by real AML-driven adapter / battery probing rather than a pure placeholder surface, even though it is still not trustworthy enough for stronger support claims.

What is still open:

- `acpid` startup is not yet fully hardened,
- userspace AML bootstrap no longer depends solely on `RSDP_ADDR` on x86, but the explicit boot-path handoff contract is still underdocumented and non-BIOS paths remain unresolved,
- normal service ownership is still transitional: `hwd` and `acpid` live on the initfs boot path rather than under a stable long-lived rootfs service contract,
- AML readiness is still coupled to PCI registration timing,
- initfs boot order now starts `pcid` and `acpid` explicitly before `hwd`, and `hwd` no longer spawns `acpid` ad hoc,
- the non-ACPI `LegacyBackend` fallback is still effectively a TODO no-op,
- failed `/scheme/acpi/register_pci` handoff now uses a bounded retry path before degrading, but the degraded contract is still not strong enough to call Wave 1 closed,
- the `\_S5` / shutdown path is not yet trustworthy enough to call robust,
- `/scheme/acpi/power` is still not a trustworthy runtime power surface,
- sleep-state support beyond `S5` is incomplete,
- Intel DMAR runtime ownership is still unresolved,
- bounded bare-metal validation remains too thin for release-grade claims.

This document is the execution plan for turning the current ACPI stack from historical bring-up
success into a subsystem that is correct under failure, explicit about ownership, honest in its
status claims, and backed by bounded runtime evidence.

## Purpose

This plan does **not** replace `local/docs/BOOT-PROCESS-ASSESSMENT.md` (historical boot record).

- `local/docs/BOOT-PROCESS-ASSESSMENT.md` (historical boot record) remains the historical P0 bring-up ledger and implementation snapshot.
- This file is the forward plan for correctness hardening, ownership cleanup, consumer integration,
  and validation closure.

The goal is not to maximize the number of parsed ACPI tables. The goal is to make the ACPI stack:

- correct under bad firmware,
- explicit about who owns what,
- observable when it fails,
- honest about what is implemented versus what is validated.

## Scope

This plan covers the Red Bear ACPI stack and its direct consumers:

- kernel ACPI discovery and early platform setup,
- `acpid` as the main ACPI / AML / FADT / DMI / power daemon,
- `iommu` as the IVRS / AMD-Vi runtime owner,
- `pcid` and `/config` as the PCI config-space path,
- DMI-backed quirks flowing through `acpid` and `redox-driver-sys`,
- ACPI consumers such as `redbear-sessiond`, `redbear-info`, and downstream services.

Primary focus is the current `x86_64` path. ARM64 remains in scope only where parser quality or
kernel-ownership decisions are shared.

## Canonical Related Documents

Read these alongside this plan:

- `local/docs/BOOT-PROCESS-ASSESSMENT.md` (historical boot record)
- `local/docs/BOOT-PROCESS-ASSESSMENT.md`
- `local/docs/IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md`
- `local/docs/IOMMU-SPEC-REFERENCE.md`
- `local/docs/QUIRKS-SYSTEM.md`
- `local/docs/LINUX-BORROWING-RUST-IMPLEMENTATION-PLAN.md`
- `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md`

## Evidence Model

This plan uses five evidence buckets and does **not** treat them as equivalent:

- **source-visible** — behavior is visible in the checked-in source tree
- **patch-carried** — behavior exists through `local/patches/*`
- **build-visible** — code compiles and stages in the current build
- **runtime-validated** — behavior has been exercised successfully in boot or runtime
- **negative-result-documented** — failures and platform gaps are explicitly recorded

The current ACPI stack has already crossed the bring-up threshold, but there is still meaningful
distance between **implemented**, **robust**, and **trusted**.

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
- Kernel ACPI export is intentionally narrow: `rxsdt` and `kstop` are real and used.
- `acpid` owns FADT parsing, AML integration, DMI exposure, and ACPI scheme surfaces.
- IVRS ownership was removed from the broken `acpid` stub path and moved into the `iommu` daemon.
- MCFG handling was removed from `acpid` and replaced with the `pcid /config` path.
- Shutdown eventing via `/scheme/kernel.acpi/kstop` is implemented and consumed by
  `redbear-sessiond`.
- AML mutex state is real-tracked in `aml_physmem.rs`, not placeholder-only.
- EC width access is implemented via byte-transaction sequences for widened reads and writes.
- `power_snapshot()` performs real AML-backed adapter / battery discovery and the ACPI scheme only exposes `/scheme/acpi/power` when that snapshot path succeeds.

### Weak today

- `acpid` startup still contains active panic-grade `expect` paths.
- userspace AML bootstrap now has an explicit handoff path plus x86 BIOS fallback, but the producer side of that contract is still underdocumented and non-BIOS fallback remains unresolved.
- service lifecycle is still transitional: `hwd` and `acpid` are primarily initfs-owned rather than by an explicit long-lived rootfs unit.
- `\_S5` derivation currently depends on AML readiness that is still gated on PCI registration.
- `hwd` no longer owns an ad hoc `acpid` spawn path; `LegacyBackend` fallback is still a TODO no-op rather than a meaningful degraded probe path.
- `pcid` can continue without ACPI integration after a bounded retry window, so AML readiness still transitions from transient-not-ready to durable degraded mode without a stronger recovery contract.
- post-PCI AML bootstrap failure is now surfaced as an explicit error instead of a quietly empty symbol surface, but that path still needs broader boot-path proof.
- `set_global_s_state()` is effectively `S5`-only.
- Sleep eventing is unsupported.
- `SLP_TYPb` remains incomplete for broader sleep-state handling.
- `power_snapshot()` exists, but its bootstrap preconditions and runtime evidence are still too weak to justify stronger `/scheme/acpi/power` trust claims.
- Some physmem / opregion failure paths are still not explicit enough.
- DMAR remains orphaned in `acpid` source: present, not wired, not fully transferred.
- Repo status language can still blur “implemented” versus “validated”.
- Bare-metal validation is too thin to justify release-grade claims.

## Ownership Model

The long-term ownership split should be:

| Component | Intended owner | Current status |
|---|---|---|
| RSDP / RSDT / XSDT early discovery | Kernel | implemented |
| MADT / HPET / early unavoidable platform setup | Kernel | implemented, broader scope still transitional |
| FADT parsing, `\_S5`, PM register writes, reboot | `acpid` | implemented, robustness still partial |
| AML execution and opregion handling | `acpid` | implemented, robustness still partial |
| DMI exposure | `acpid` | implemented |
| ACPI runtime power surface | `acpid` | transitional / incomplete |
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

## Current Runtime Contract

The ACPI stack must distinguish between **fatal**, **degradable**, and **out-of-scope** failures.

| Condition | Expected behavior target | Classification |
|---|---|---|
| ACPI absent / empty root table | `acpid` exits cleanly without ACPI services | degradable |
| Bad SDT checksum | warn, continue best-effort where supported | degradable |
| Bad table length / malformed table | deterministic reject or degrade policy | open contract |
| Missing or unproven explicit `RSDP_ADDR` producer for userspace AML | kernel ACPI may still boot and x86 AML now has a bounded BIOS fallback, but the explicit producer contract remains incomplete from the repo-visible boot path | open contract |
| AML init failure | explicit failure, not panic | currently too fragile |
| Failed `/scheme/acpi/register_pci` handoff | boot degrades without full ACPI integration after a bounded retry window, but the degraded contract still lacks stronger recovery semantics | degradable |
| ACPI backend fallback to legacy probing | degraded hardware discovery should still be useful, but current legacy fallback is effectively a no-op | known gap |
| EC timeout | AML error path should surface failure, not fabricate success | degradable |
| Missing `\_S5` | shutdown path cannot use PM registers | degradable only if failure is explicit |
| Sleep-state transition request beyond `S5` | unsupported today | known gap |
| Missing `kstop` path | no kernel-orchestrated shutdown event contract | fatal for that integration path |
| Missing DMAR on Intel | no Intel VT-d runtime | degradable for non-IOMMU boot |
| Missing IVRS on AMD | no AMD-Vi runtime | degradable for non-IOMMU boot |

Wave 0 and Wave 1 must turn the still-fuzzy cases into explicit policy.

## Execution Rules

These rules govern all work from this plan:

1. **No hidden status inflation.** Status words must match evidence.
2. **No ownership moves without a handoff contract.** “Not wired” is not the same as “cleanly moved.”
3. **No validation laundering.** QEMU success is not bare-metal success.
4. **No runtime fake-success paths.** Empty defaults and fabricated values must not masquerade as real support.
5. **No cross-wave dependency drift.** Later waves must not silently depend on work that was never formalized earlier.

## Phase Overview Matrix

| Wave | Theme | Current status | Main blocker | Primary closure signal |
|---|---|---|---|---|
| Wave 0 | Contracts / truthfulness | partially complete | doc drift across adjacent ACPI-facing docs | one canonical vocabulary and ownership story across the repo |
| Wave 1 | Startup hardening / parser policy | partially complete | boot-path contract gaps (explicit `RSDP_ADDR` producer ownership and still-transitional initfs lifecycle) plus remaining panic-grade startup and fault paths | firmware-origin startup failures are bounded and typed and AML bootstrap preconditions are explicit |
| Wave 2 | AML ordering / shutdown / sleep scope | partially complete | shutdown/reboot result semantics and broader runtime proof still remain incomplete | deterministic `\_S5` derivation and bounded shutdown behavior |
| Wave 3 | Honest ACPI power surface | open | current power reporting is real but still provisional and under-validated | `/scheme/acpi/power` exposes only behavior that the runtime evidence can honestly support |
| Wave 4 | AML physmem / EC / runtime fault handling | partially complete | placeholder-like runtime error behavior remains in places | no correctness-critical fabricated runtime values |
| Wave 5 | Ownership cleanup / kernel contract | open | DMAR still orphaned and kernel/userspace contract still implicit | explicit long-term ownership map with no orphan carriers |
| Wave 6 | Consumer integration / observability | partially complete | consumers still rely on uneven status surfaces | shutdown/event/power consumers describe and observe reality honestly |
| Wave 7 | Validation closure / release gates | open | bounded evidence set still too thin | release claims backed by a bounded matrix and negative-result capture |

The waves are intentionally ordered. Wave 0 defines truth. Wave 1 makes boot behavior survivable.
Wave 2 fixes the most dangerous runtime correctness problems. Wave 3 stops downstream services from
depending on misleading power semantics. Waves 4–6 harden the remaining runtime edges and ownership
boundaries. Wave 7 is where the stronger claims are either earned or denied.

## Wave 0 — Contracts, truthfulness, and degraded-mode policy

### Goal

Establish one canonical answer to:

1. who owns what,
2. what counts as degraded but acceptable,
3. what ACPI status words mean,
4. and what current ACPI eventing actually covers.

### Why this wave is first

Without a contract, later hardening work turns into undocumented rewrites and docs drift.

### Primary files

- `local/docs/BOOT-PROCESS-ASSESSMENT.md` (historical boot record)
- this file
- `HARDWARE.md`
- `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md`
- related status surfaces as needed

### Dependencies

- none

### Deliverables

- one normalized ACPI vocabulary,
- one degraded-mode contract,
- one canonical ownership statement,
- one explicit statement that current eventing is shutdown-focused,
- removal of doc language that implies subsystem completeness without evidence.

### Execution slices

| ID | Work slice | Concrete output | QA evidence |
|---|---|---|---|
| W0.1 | Vocabulary normalization | All ACPI-facing docs use the same status words for implemented / transitional / known gap | grep review across ACPI docs shows no conflicting support language |
| W0.2 | Ownership statement | One canonical statement for kernel / `acpid` / `iommu` / future DMAR ownership | `ACPI-IMPROVEMENT-PLAN.md`, `BOOT-PROCESS-ASSESSMENT.md`, and `IOMMU-SPEC-REFERENCE.md` agree |
| W0.3 | Eventing scope truthfulness | `kstop` and shutdown-only semantics become explicit everywhere they are summarized | `DBUS-INTEGRATION-PLAN.md`, `DESKTOP-STACK-CURRENT-STATUS.md`, and `AGENTS.md` stay aligned |
| W0.4 | Evidence-carrier cleanup | validation logs are treated as evidence carriers, not support-policy sources | `BOOT-PROCESS-ASSESSMENT.md` and `HARDWARE.md` no longer overclaim support |

### Specific tasks

1. Normalize ACPI status language across the canonical plan, historical ledger, hardware summary, and
   public status summaries.
2. Keep `kstop` and shutdown-only eventing explicit anywhere login1, D-Bus, or desktop consumers
   summarize ACPI behavior.
3. Keep DMAR ownership language transitional until a concrete Intel runtime owner exists.
4. Keep validation logs framed as evidence carriers, not as the source of support policy.
5. Reject any doc wording that implies startup hardening, honest power reporting, or full sleep
   lifecycle support before those waves actually close.

### Verification

- documentation review only,
- no contradictory ownership claims across ACPI docs,
- no bare “complete” wording without scope,
- no doc claim of startup hardening that the active code does not support.

### Exit criteria

- one canonical ownership statement exists,
- one degraded-mode matrix exists,
- all top-level ACPI docs use the same vocabulary,
- current shutdown-only eventing scope is explicit.

### Current status

- overall: partially complete
- W0.1 Vocabulary normalization — substantially complete
- W0.2 Ownership statement — substantially complete
- W0.3 Eventing scope truthfulness — substantially complete
- W0.4 Evidence-carrier cleanup — partially complete; core carriers are aligned, but future ACPI-facing summaries must keep using this vocabulary

## Wave 1 — Boot-path hardening and parser strictness

### Goal

Remove catastrophic or silent failure behavior from boot-critical ACPI initialization.

### Primary files

- `recipes/core/base/source/drivers/acpid/src/main.rs`
- `recipes/core/base/source/drivers/acpid/src/acpi.rs`
- `recipes/core/base/source/drivers/acpid/src/scheme.rs`
- `recipes/core/base/source/drivers/hwd/src/main.rs`
- `recipes/core/base/source/drivers/hwd/src/backend/acpi.rs`
- `recipes/core/base/source/drivers/hwd/src/backend/legacy.rs`
- `recipes/core/base/source/init.initfs.d/40_hwd.service`
- `recipes/core/base/source/init/src/service.rs`
- `recipes/core/base/source/bootstrap/src/exec.rs`
- `recipes/core/kernel/source/src/scheme/sys/mod.rs`
- `recipes/core/kernel/source/src/acpi/mod.rs`
- kernel ACPI submodules as needed

### Dependencies

- Wave 0 ownership and degraded-mode vocabulary in place

### Deliverables

- startup paths are typed and explicit,
- AML bootstrap preconditions are explicit and satisfied by an in-tree handoff path or are clearly documented as unresolved,
- boot-path ownership between init, `hwd`, `acpid`, and `pcid` is explicit enough that degraded behavior is diagnosable,
- table rejection policy is documented per table class,
- parser observability is strong enough to reconstruct failures,
- degraded boot succeeds for all conditions classified as degradable,
- no active firmware-origin startup path still depends on panic-grade behavior.

### Execution slices

| ID | Work slice | Concrete output | QA evidence |
|---|---|---|---|
| W1.1 | Startup failure typing | `acpid` startup paths classify clean exit vs fatal vs degraded continue | startup logs and code review show no firmware-path `expect()` dependence |
| W1.2 | Table policy definition | SDT/FADT/root-table reject/warn/degrade rules are written down and implemented | malformed-table tests match the documented policy |
| W1.3 | Parser observability | accepted/rejected tables are logged with enough detail to diagnose boot failures | bounded bad-table boots produce reconstructable logs |
| W1.4 | Degraded boot proof | ACPI-bad but degradable boots continue without panicking | one bounded AMD and one bounded Intel degraded-path proof |
| W1.5 | AML bootstrap contract | the source of `RSDP_ADDR` / `RSDP_SIZE` is made explicit or the contract is replaced with a documented in-tree alternative; x86 fallback remains bounded and honest | boot-path docs, init wiring, and `acpid` startup code agree on how AML bootstrap happens |

### Specific tasks

1. Finish replacing panic-grade startup behavior in active firmware-origin paths.
2. Define and validate the userspace AML bootstrap contract, including whether `RSDP_ADDR` / `RSDP_SIZE` remains the intended path.
3. Define table-specific reject / warn / degrade / fail rules.
4. Log accepted and rejected tables with enough evidence to debug failures.
5. Normalize `acpid` startup into clean exit, fatal error, and degraded-continue classes.
6. Make the boot-path ownership between init, `hwd`, `acpid`, and `pcid` explicit enough that degraded behavior is diagnosable.

### Verification

- malformed checksum / truncated-length tests,
- QEMU validation with intentionally damaged tables using a documented bounded harness or a retained negative-result record,
- boot-path evidence showing where AML bootstrap parameters come from or an explicit retained blocker stating that the producer remains unresolved,
- one bounded AMD hardware boot recheck,
- one bounded Intel hardware boot recheck,
- evidence captured in `local/docs/BOOT-PROCESS-ASSESSMENT.md`.

### Exit criteria

- no unjustified `panic!/expect()` remains on firmware-origin startup paths,
- AML bootstrap preconditions are explicit and consistent with the in-tree boot path,
- malformed-table decisions are deterministic and documented,
- degraded boot behavior matches Wave 0 classification.

### Current status

- overall: partially complete
- W1.1 Startup failure typing — partially complete
- W1.5 AML bootstrap contract — partially complete

## Wave 2 — AML ordering, shutdown correctness, and sleep-state scope

### Goal

Close the highest-risk runtime-correctness gaps in the `acpid` layer.

### Primary files

- `recipes/core/base/source/drivers/acpid/src/acpi.rs`
- `recipes/core/base/source/drivers/acpid/src/sleep.rs`
- `recipes/core/base/source/drivers/acpid/src/scheme.rs`

### Dependencies

- Wave 1 startup paths hardened enough that runtime work is not sitting on a fragile base

### Deliverables

- deterministic AML init order,
- deterministic `\_S5` derivation,
- explicit shutdown success/failure behavior,
- explicit reboot correctness and fallback behavior,
- explicit sleep-state scope,
- honest `SLP_TYPb` status.

### Execution slices

| ID | Work slice | Concrete output | QA evidence |
|---|---|---|---|
| W2.1 | `\_S5` derivation timing | `\_S5` is derived at a deterministic valid point instead of accidental fallback timing | logs show when `\_S5` was computed and from what readiness state |
| W2.2 | AML readiness contract | documented split or sequencing between early AML and PCI-dependent AML | code path and docs agree on when AML is considered ready |
| W2.3 | Shutdown and reboot result semantics | shutdown and reboot paths return bounded results, log failures explicitly, and keep fallback behavior honest | QEMU + bounded real-hardware shutdown/reboot proof with failure-path logs |
| W2.4 | Sleep-scope truthfulness | non-`S5` support is either implemented in bounded form or kept explicitly deferred | no docs or APIs imply broader sleep lifecycle support prematurely |

### Specific tasks

1. Fix the `\_S5` ordering bug by **primarily** recomputing `\_S5` after PCI registration, using an early-AML split only if the recompute path proves insufficient on bounded hardware.
2. Document and enforce that AML readiness contract explicitly.
3. Make `set_global_s_state()` return explicit outcomes instead of relying on write-then-spin behavior.
4. Bound shutdown failure semantics when PM1 writes do not power off the machine.
5. Document and validate reboot ownership, including reset-register and keyboard-controller fallback behavior.
6. Decide whether non-`S5` sleep support is in scope now or explicitly deferred.
7. If deferred, keep the scope truthful in code and docs.

### Verification

- targeted AML method execution checks,
- shutdown / reboot proof in QEMU and bounded hardware,
- induced AML-not-ready path tests,
- log proof of when `\_S5` was derived,
- one bounded Intel and one bounded AMD shutdown/reboot recheck.

### Exit criteria

- AML initialization order is reproducible and documented,
- `\_S5` is no longer derived through fragile fallback timing,
- shutdown and reboot failures do not degrade into panic or silent hang only,
- sleep-state handling is either implemented or explicitly bounded as a known gap.

### Current status

- overall: partially complete
- W2.1 `\_S5` derivation timing — partially complete
- W2.2 AML readiness contract — partially complete
- W2.3 Shutdown and reboot result semantics — partially complete
- current-tree behavior now defers `\_S5` cleanly until PCI-backed AML readiness, surfaces pre-PCI shutdown as AML-not-ready, preserves shutdown dispatch details on non-completion, and treats reboot dispatch failure/returned reboot attempts as explicit non-success instead of silent success

## Wave 3 — Honest runtime power surface

### Goal

Stop exposing incomplete runtime power state as if it were implemented.

### Primary files

- `recipes/core/base/source/drivers/acpid/src/acpi.rs`
- `recipes/core/base/source/drivers/acpid/src/scheme.rs`
- downstream consumers such as `local/recipes/system/redbear-upower/source/src/main.rs`

### Dependencies

- Wave 2 runtime ordering and shutdown behavior stable enough that consumers can rely on ACPI state

### Deliverables

- an explicitly reduced and honest `/scheme/acpi/power` surface first,
- current `power_snapshot()` behavior is documented as real but provisional,
- consumer-visible distinction between unsupported, unavailable, and populated power state.

### Execution slices

| ID | Work slice | Concrete output | QA evidence |
|---|---|---|---|
| W3.1 | Power-surface decision | explicit primary path to reduce `/scheme/acpi/power` to an honest bounded surface before any expansion | docs and service code describe the same support level |
| W3.2 | Snapshot semantics | adapter/battery state becomes real or explicitly unavailable/unsupported | direct scheme reads show distinct responses for each state |
| W3.3 | Consumer honesty | `redbear-upower` and downstream docs stop overclaiming support | D-Bus/current-state docs match actual scheme behavior |
| W3.4 | Reporting consistency | all public summaries use the same bounded wording for ACPI-backed power | grep review shows no stale “bounded real” UPower claims |

### Specific tasks

1. Reduce or constrain the current `/scheme/acpi/power` surface so empty defaults do not masquerade as support.
2. Ensure downstream consumers can tell unsupported from currently unavailable.
3. Treat the current AML-backed adapter / battery enumeration as provisional until its bootstrap preconditions and bounded hardware evidence are strong enough to trust.
4. Keep all downstream status language pinned to the reduced surface until bounded runtime proof supports stronger claims.

### Verification

- scheme reads on supported and unsupported systems,
- downstream consumer checks,
- log review for unavailable and unsupported cases.

### Exit criteria

- `/scheme/acpi/power` no longer returns misleading empty-success behavior,
- consumers can distinguish unsupported from unavailable,
- power reporting claims in docs match the actual runtime surface.

### Current status

- open

## Wave 4 — AML physmem, EC, and runtime fault handling

### Goal

Remove correctness-critical fake values and placeholder runtime behavior.

### Primary files

- `recipes/core/base/source/drivers/acpid/src/aml_physmem.rs`
- `recipes/core/base/source/drivers/acpid/src/ec.rs`
- `recipes/core/base/source/drivers/acpid/src/acpi.rs`

### Dependencies

- Wave 1 startup hardening complete

### Deliverables

- explicit physmem / opregion failure behavior,
- EC error paths that are typed and diagnosable,
- documented AML mutex and timeout semantics,
- runtime failures that propagate clearly to callers.

### Execution slices

| ID | Work slice | Concrete output | QA evidence |
|---|---|---|---|
| W4.1 | Physmem failure propagation | correctness-critical reads stop silently returning fabricated values | forced read-failure tests produce explicit errors |
| W4.2 | EC error typing | widened-access and timeout failures are surfaced consistently | EC timeout path tests and log review |
| W4.3 | AML mutex semantics | acquire/release/timeout behavior is documented and reflected in runtime behavior | concurrent AML scheme-read/eval checks stay understandable |
| W4.4 | Runtime fault observability | callers receive clear failure categories instead of placeholder success | operator-visible logs distinguish source and impact |

### Specific tasks

1. Audit `aml_physmem.rs` for all correctness-critical “log then fabricate 0” paths.
2. Convert correctness-critical failures into explicit propagated errors.
3. Finish EC error typing and document widened-access behavior.
4. Document AML mutex timeout behavior and actual guarantees.

### Verification

- induced physmem mapping/read failure tests,
- EC timeout path tests,
- concurrent AML scheme-read and AML-eval checks,
- one EC-backed machine sanity check or one retained documented blocker explaining why that proof is still absent.

### Exit criteria

- correctness-critical runtime paths do not silently fabricate values,
- EC behavior is implemented or explicitly bounded,
- AML synchronization behavior is documented and tested.

### Current status

- overall: partially complete
- W4.1 Physmem failure propagation — partially complete
- W4.2 EC error typing — partially complete
- W4.3 AML mutex semantics — substantially complete in tracked state, still needs clearer runtime-proof coverage
- W4.4 Runtime fault observability — open

## Wave 5 — Ownership cleanup and kernel-surface reduction

### Goal

Move from transitional ownership to a durable architecture that can survive long-term maintenance.

### Primary files

- `recipes/core/kernel/source/src/acpi/mod.rs`
- kernel ACPI submodules as needed
- `recipes/core/kernel/source/src/scheme/acpi.rs`
- `recipes/core/base/source/drivers/acpid/src/acpi/dmar/mod.rs`
- `local/recipes/system/iommu/source/src/*`

### Dependencies

- Waves 1 and 2 are at least partially stable

### Deliverables

- a minimum kernel ACPI contract,
- explicit handoff paths for topology and table consumers,
- DMAR no longer orphaned in `acpid`,
- ownership wording that matches the code.

### Execution slices

| ID | Work slice | Concrete output | QA evidence |
|---|---|---|---|
| W5.1 | Kernel contract write-down | explicit minimal kernel ACPI contract in docs/comments | kernel/export surfaces match the written contract |
| W5.2 | DMAR carrier cleanup | orphaned `acpid` DMAR carrier is explicitly deferred unless a real Intel runtime owner is ready in the same implementation slice | no doc claims a hidden owner that code does not implement |
| W5.3 | IOMMU ownership alignment | IVRS/DMAR ownership text across `iommu` and ACPI docs becomes stable | `ACPI-IMPROVEMENT-PLAN.md`, `IOMMU-SPEC-REFERENCE.md`, and Linux-borrowing plan agree |
| W5.4 | Regression containment | ownership cleanup does not break existing bring-up paths | before/after boot checks on AMD and Intel remain stable |

### Specific tasks

1. Define the minimum kernel ACPI surface that must remain in early boot.
2. Keep `rxsdt` and `kstop` as explicit exported contract until a real replacement exists.
3. Treat explicit deferral of the orphaned DMAR carrier as the primary path until a real Intel runtime owner exists.
4. Remove or relocate the orphaned `acpid` DMAR carrier only in the same change set that introduces and validates the replacement owner.
5. Do not claim Intel DMAR runtime ownership complete unless a real owner exists and is validated.
6. Preserve IVRS ownership in `iommu`.

### Verification

- before / after boot regressions,
- Intel-specific validation for any DMAR ownership move,
- AMD regression checks showing IVRS ownership remains isolated in `iommu`.

### Exit criteria

- the minimum kernel ACPI contract is written down,
- DMAR has a concrete, non-ambiguous owner or is explicitly deferred,
- ownership reductions do not regress current bring-up.

### Current status

- open

## Wave 6 — Consumer integration and eventing quality

### Goal

Make ACPI consumers correct, observable, and low-friction.

### Primary files

- `local/recipes/system/redbear-sessiond/source/src/acpi_watcher.rs`
- `recipes/core/base/source/drivers/acpid/src/main.rs`
- `recipes/core/base/source/drivers/acpid/src/scheme.rs`
- DMI / quirk consumers in `redox-driver-sys`
- reporting surfaces such as `redbear-info`

### Dependencies

- Waves 2 through 4 stable enough that consumers can depend on ACPI behavior

### Deliverables

- shutdown-focused eventing quality as a required consumer contract,
- bounded DMI quirk authority,
- operator-facing observability strong enough to diagnose behavior,
- explicit treatment of unsupported sleep eventing if it remains deferred.

### Execution slices

| ID | Work slice | Concrete output | QA evidence |
|---|---|---|---|
| W6.1 | Shutdown consumer contract | `redbear-sessiond` and D-Bus docs describe shutdown-only behavior correctly | `PrepareForShutdown` stays current; `PrepareForSleep` stays future-only |
| W6.2 | DMI quirk authority | quirk precedence and bounds are documented for ACPI/DMI consumers | `QUIRKS-SYSTEM.md` and ACPI plan do not disagree |
| W6.3 | Operator observability | AML readiness, shutdown attempts, and power availability are diagnosable | log review and status outputs distinguish unsupported vs unavailable |
| W6.4 | Consumer wording discipline | adjacent docs stop translating provisional ACPI surfaces into “real” support claims | desktop/D-Bus/Qt status docs remain aligned with the canonical plan |

### Specific tasks

1. Keep shutdown eventing on `kstop` as the canonical shutdown signal.
2. Improve consumer-facing observability for AML readiness, PCI registration state, shutdown attempts, and power availability.
3. Define DMI quirk precedence and limits.
4. If sleep eventing remains out-of-scope, document that explicitly and consistently.

### Verification

- repeated shutdown-edge tests,
- race checks with multiple simultaneous consumers of `/scheme/acpi/*`,
- DMI quirk application checks on known systems,
- log review that diagnoses unsupported versus unavailable behavior.

### Exit criteria

- no misleading consumer contract remains for core ACPI transitions,
- quirk precedence is documented,
- consumer-visible behavior is diagnosable from logs and status outputs.

### Current status

- overall: partially complete
- W6.1 Shutdown consumer contract — substantially complete
- W6.2 DMI quirk authority — partially complete
- W6.3 Operator observability — open
- W6.4 Consumer wording discipline — substantially complete

## Wave 7 — Validation closure and release gates

### Goal

Turn the current ACPI stack from bring-up evidence into release-grade trust.

### Primary files

- `local/docs/BOOT-PROCESS-ASSESSMENT.md`
- `HARDWARE.md`
- this file
- `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md`
- validation scripts such as `local/scripts/test-baremetal.sh` and bounded ACPI-related QEMU / runtime harnesses as they exist

### Dependencies

- Waves 1 through 6 have produced stable behavior worth validating

### Required validation matrix

At minimum:

- QEMU / OVMF boot with ACPI active,
- one modern AMD machine,
- one modern Intel machine,
- one platform that exercises EC-backed AML behavior,
- malformed-table or degraded-mode evidence, or a retained blocker entry explaining why that proof could not yet be produced.

### Required matrix fields

Each matrix entry should record, at minimum:

- date,
- platform name,
- firmware mode,
- profile / config used,
- kernel / patch baseline,
- key ACPI tables present,
- APIC mode,
- shutdown result,
- reboot result,
- DMI exposure,
- power-surface state,
- AML / EC failures,
- degraded behavior observed,
- evidence location (log, script output, photo, or captured artifact),
- final classification: implemented only / QEMU-validated / bounded real-hardware validated / failed.

### Repetition standard

This plan should treat one successful run as **initial evidence**, not closure.

- QEMU proof should be repeatable at least twice on the same bounded harness.
- Each bounded real-hardware class should have at least one named passing run and one retained
  negative-or-regression note if failures were seen during bring-up.
- Gate B claims should rely on repeated evidence across more than one hardware class, not a single
  lucky machine.

### Deliverables

- a bounded platform matrix,
- negative-result capture,
- explicit release gates for both boot-baseline and full ACPI claims,
- docs that distinguish implemented from validated.

### Execution slices

| ID | Work slice | Concrete output | QA evidence |
|---|---|---|---|
| W7.1 | Matrix carrier | one canonical bounded validation matrix exists | `BOOT-PROCESS-ASSESSMENT.md` holds named platform entries |
| W7.2 | Positive proof set | QEMU + AMD + Intel + EC-backed paths each have bounded proof entries | repeated runs recorded with dates and configs |
| W7.3 | Negative-result discipline | unresolved AML/EC/platform failures stay visible | negative results persist in logs/docs instead of disappearing |
| W7.4 | Release-gate enforcement | stronger ACPI claims are tied to explicit gate passage | summary docs do not exceed the evidence in the matrix |

### Specific tasks

1. Publish the platform matrix in `local/docs/BOOT-PROCESS-ASSESSMENT.md`.
2. Record for each platform: firmware mode, key ACPI tables, APIC mode, shutdown / reboot, DMI / power exposure, AML / EC failures, and notable degraded behavior.
3. Preserve negative results such as unsupported AML opcodes or platform-specific regressions.
4. Require evidence before any stronger ACPI completeness claim is made.
5. Keep a canonical evidence link or artifact pointer in each matrix row so support language can be traced back to an actual run.
6. Refuse Gate B wording unless the repeated-proof standard above is met.

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

## Recommended PR Sequence

Recommended order:

1. docs/status correction,
2. `acpid` startup hardening,
3. `\_S5` / AML ordering, shutdown, and reboot correctness,
4. honest `/scheme/acpi/power`,
5. AML physmem / EC hardening,
6. DMAR ownership cleanup,
7. kernel/userspace ACPI contract write-down,
8. eventing / consumer contract cleanup,
9. validation matrix and release gates.

This order intentionally follows the wave order: Wave 0 → Wave 1 → Wave 2 → Wave 3 → Wave 4 → Wave 5 → Wave 6 → Wave 7. If a single wave is split across multiple PRs, keep the wave ordering authoritative and treat sub-PR sequencing as an implementation detail rather than a competing plan order.

## Release Gates

### Gate A — Boot-Baseline ACPI Ready

This is the strongest claim the repo can make before sleep and broader ownership cleanup are done.

Require:

- clean boot on bounded QEMU + AMD + Intel validation targets,
- working MADT / APIC initialization on those targets,
- working and bounded shutdown / reboot proof where supported,
- explicit degraded behavior for known firmware-bad cases,
- current docs that distinguish implemented from validated.

### Gate B — Full ACPI / Power-Management Ready

Do **not** claim this until all of the following are true:

- AML runtime behavior is stable across the bounded matrix,
- shutdown correctness is validated on bounded real hardware,
- sleep-state scope is implemented and validated or explicitly excluded from the release claim,
- ownership boundaries are clean rather than transitional,
- consumer integration is observable and race-bounded,
- the platform matrix supports the stronger claim.

## Main Risks

- stricter parser behavior may expose machines currently booting only by luck,
- AML ordering fixes may reveal hidden PCI-registration assumptions,
- power-surface honesty may break consumers assuming empty means supported,
- reducing kernel scope too early may regress early bring-up,
- careless DMAR cleanup may create Intel-only regressions,
- QEMU success may continue to hide bare-metal correctness gaps if validation stays too shallow.

## Definition of Done

This plan is substantially complete only when:

- startup failure behavior is bounded and non-panic-grade,
- `\_S5` shutdown behavior is deterministic and validated,
- exported power and event surfaces are honest,
- kernel/userspace ownership boundaries are explicit and not contradicted by the code,
- DMAR and IVRS ownership are not described ambiguously,
- sleep-state handling is implemented or explicitly excluded from the release claim,
- the repo contains bounded platform evidence that supports every status claim.

## Current Truthful Status

> Red Bear ACPI is materially complete for historical boot bring-up, but still under active
> correctness, ownership, power-surface, sleep-state, and validation improvement. Shutdown eventing
> is implemented via `kstop`. Current eventing is shutdown-focused, not full sleep lifecycle
> management. The `acpid` runtime surface still needs startup hardening, deterministic AML ordering,
> honest power reporting, and explicit Intel DMAR ownership before stronger ACPI claims are justified.
