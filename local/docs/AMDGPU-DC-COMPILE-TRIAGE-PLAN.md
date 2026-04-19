# AMDGPU DC Compile Triage Plan

**Date:** 2026-04-18
**Scope:** Triage of the current Red Bear amdgpu AMD Display Core compile path, specifically the
decision between growing the Linux compatibility surface and narrowing the imported display/DC
source set to the bounded path actually needed for first display bring-up.

> **Planning authority note (2026-04-18):** this file is a focused amdgpu/DC compile-triage and
> execution document. It does not replace `local/docs/DRM-MODERNIZATION-EXECUTION-PLAN.md` as the
> canonical GPU/DRM plan. Use the DRM modernization plan for overall execution order, Intel/AMD
> parity criteria, and broader acceptance gates. Use this file for the specific question of how to
> triage the current amdgpu DC compile break without drifting into open-ended compatibility work.

> **Status update (2026-04-18):** Phase 1B has now been carried out in bounded form. The `amdgpu`
> recipe builds successfully on the retained Red Bear glue path (`amdgpu_redox_main.c` +
> `redox_stubs.c`), while the imported Linux AMD display, TTM, and amdgpu core trees remain
> explicitly outside the retained compile surface and still under compile triage.

## Title and intent

Red Bear currently compiles the imported AMD display tree too broadly for the evidence-backed goal
it actually has today.

The immediate goal is **not** to prove that the full imported AMD Display Core tree compiles on
Redox. The immediate goal is to unblock the bounded display path needed for first display-side
bring-up while preserving a maintainable route toward broader DC closure later.

This document exists to prevent two failure modes:

1. treating the first compile error as if it justifies unconstrained `linux-kpi` expansion, and
2. claiming progress from a narrowed compile path without documenting exactly what was excluded and
   why.

## Current grounded state

### Bottom line

The original broad-tree failure was **not** a `freesync.c`-specific logic bug. It exposed a broader
mismatch between the imported AMD DC / TTM / amdgpu trees and the current Red Bear compatibility
strategy.

After narrowing the recipe to the actual retained first-display path, the `amdgpu` recipe now
builds successfully from the Red Bear glue layer alone. That is the current truthful state: the
bounded retained path builds, while the imported Linux trees remain under compile triage rather than
being claimed as compile-complete.

### Confirmed evidence

| Area | Current evidence | Repo grounding |
|---|---|---|
| Historical broad-path rule | The old recipe compiled all `display/*.c` files and failed in optional AMD DC code before the retained path was proven | historical recipe state + `local/recipes/gpu/amdgpu/target/x86_64-unknown-redox/build/freesync.o.log` |
| Current retained build rule | The current recipe compiles only the bounded Red Bear glue path and links `libamdgpu_dc_redox.so` from that retained surface | `local/recipes/gpu/amdgpu/recipe.toml` |
| Historical first hard failure | `freesync.c -> dm_services.h -> dm_services_types.h -> os_types.h -> linux/kgdb.h` | `local/recipes/gpu/amdgpu/target/x86_64-unknown-redox/build/freesync.o.log` |
| Current shim posture | Compatibility surface is partial, not absent | `local/recipes/drivers/linux-kpi/source/src/c_headers/`, `local/recipes/gpu/amdgpu/source/redox_glue.h` |
| Small retained-path shim probes attempted | Added minimal `linux/export.h` and `linux/refcount.h` while testing whether imported TTM belonged on the retained path | `local/recipes/drivers/linux-kpi/source/src/c_headers/linux/export.h`, `.../linux/refcount.h` |
| Switch criterion outcome | Imported TTM immediately fanned into broader Linux-kernel surfaces (`__cond_acquires`, `iosys-map`, and related header fallout), so the retained path was narrowed again instead of growing shims further | retained build logs during TTM probe |
| Current Red Bear need | First display bring-up needs a bounded display path, not proof that all optional AMD DC subtrees compile | `local/docs/DRM-MODERNIZATION-EXECUTION-PLAN.md`, `local/docs/AMD-FIRST-INTEGRATION.md` |

### Why the current approach is unstable

The current amdgpu recipe uses a broad compile rule that effectively says:

> compile the imported display tree first, then see what breaks.

That is useful for discovery, but it is a poor default execution strategy for bounded bring-up.

It pulls optional and advanced display code into the same compile surface as the first modeset path,
which means a failure in a module such as FreeSync can block the entire experiment even when that
module is not yet proven necessary for the first Red Bear display target.

## Triage question

Red Bear needs an explicit answer to this question before continuing:

> Should the repo first grow the Linux compatibility layer until the full imported AMD display tree
> compiles further, or should it first narrow the imported source set to the display path Red Bear
> actually needs today?

This document answers:

- **Start with Strategy B** — narrow the DC source set.
- Use **Strategy A** — minimal shim additions — only as a controlled fallback when the retained,
  bounded display path still proves a small required compatibility gap.

## Strategy comparison

| Strategy | What it does | Best when | Success criteria | Main failure mode |
|---|---|---|---|---|
| **A. Minimal shim additions** | Add the smallest Linux compatibility surface needed to expose the next blocker | The real retained display path is already known, and the missing API surface stays small and generic | Each shim advances the build by one blocker class without broadening scope dramatically | Header whack-a-mole grows into de facto kernel-environment emulation |
| **B. Narrow the DC source set** | Replace broad full-tree compile with an explicit bounded file list aligned to the actual first display goal | Optional or advanced modules are being pulled into the build before their necessity is proven | The reduced source set compiles further or reveals the next blocker on the true bring-up path | False confidence if the narrowed claim is not documented precisely |

## Recommendation

### Recommendation summary

Start with **B: narrow the compiled DC source set to the bounded display path Red Bear actually
uses today**.

That recommendation has now been implemented in bounded form. The retained path was narrowed far
enough to prove that the current Red Bear bring-up surface does not need the imported Linux AMD
display, TTM, or amdgpu core trees in order to build the shipped `amdgpu` recipe.

The current evidence supports that recommendation because:

1. the recipe compiles the entire imported display tree,
2. the first blocker sits in a dependency cone that likely contains several more Linux/DRM header
   and semantic assumptions, and
3. Red Bear's current need is bounded display bring-up, not immediate proof that every imported
   AMD DC subsystem compiles under Redox.

### Why A is not the first move

The first hard failure (`linux/kgdb.h`) is shallow enough to tempt a quick shim fix. That is useful
only if the retained path is already known. Right now it is not. Without narrowing the source set
first, each new shim risks paying compatibility cost for files Red Bear may not need for first
bring-up.

That is the main hidden cost of Strategy A at this stage: it can create real maintenance debt before
the repo has proven that the affected code is on the first bring-up path at all.

## ULW execution plan

## Phase 0 — Freeze the baseline

### Goal

Create one canonical failure snapshot that all later triage work can refer back to.

### Actions

- Record the current broad display compile rule in the amdgpu recipe.
- Record the first failing translation unit and full include chain.
- Record the current bounded Red Bear display objective and the currently targeted ASIC/runtime
  surface.

### Exit criteria

One written baseline exists showing:

- the current full-tree compile behavior,
- the current first hard failure at `linux/kgdb.h`, and
- the current bounded display objective.

### Current status

- complete enough to proceed

## Phase 1B — Narrow-source probe

### Goal

Identify the minimum imported display/DC source set required for current Red Bear display bring-up.

### Required mindset

The question in this phase is not “what can Linux build?”

The question is:

> what does Red Bear actually need compiled now to support its present display-side target?

### Actions

- Replace broad `find .../display -name '*.c'` behavior with an explicit bounded file list.
- Treat the first retained file list as a **probe hypothesis**, not as a proven final minimum.
- Keep only the C sources required for the current Red Bear bring-up surface hypothesis:
  - device initialization,
  - connector detection and mode enumeration,
  - bounded modeset path,
  - cleanup,
  - and the currently targeted ASIC families.
- Exclude obvious scope inflators first unless the call graph proves they are required:
  - `modules/freesync/*`,
  - untargeted DCN generations,
  - `amdgpu_dm/*`,
  - optional feature modules not on the first display path.

### Verification

- The reduced file list is explicit and reviewable.
- The reduced build is re-run.
- The next failure is checked to confirm that it occurs on the retained bounded path rather than in
  an excluded optional subtree.

### Exit criteria

One of the following becomes true:

1. the narrowed set compiles meaningfully further than the current build, or
2. the next blocker appears on the real retained path and is therefore a justified compatibility
   problem.

### Failure signal

If the narrowed set cannot be described cleanly because the retained path immediately drags in broad
optional subsystems, stop and move to the decision gate rather than continuing to guess.

### Current status

- complete — the retained path is now explicit and builds

## Phase 1A — Minimal-shim probe

### Goal

Expose the next blocker with the smallest justified compatibility addition.

### Entry condition

Only do this after Phase 1B has established a retained bounded path, or after the narrowed path
proves that a small missing Linux primitive is genuinely required.

### Allowed shim order

Add one shim family at a time, in this rough priority order:

1. `linux/kgdb.h`
2. `asm/byteorder.h`
3. `linux/vmalloc.h`
4. `ktime_get_raw_ns` / timekeeping support
5. `div64_u64` / `div64_u64_rem`
6. `linux/refcount.h`

### Rules

- One shim family per change.
- No speculative shim batches.
- No ad hoc amdgpu-only workaround when the gap clearly belongs in `linux-kpi`.
- If a shim exposes a large new Linux subsystem expectation rather than a narrow primitive, stop and
  reconsider the strategy.

### Verification

- Re-run the build after each shim family.
- Confirm that the build advances by one blocker class.
- Confirm that the next failure remains on the retained bounded path.

### Exit criteria

- The build advances by exactly one blocker class, and
- the next failure still belongs to the retained bounded path.

### Failure signal

If one shim immediately reveals several unrelated Linux subsystem requirements, stop and return to
Strategy B.

## Phase 2 — Decision gate

### Stay on Strategy B if

- the blocker sits in optional or advanced code such as FreeSync,
- narrowing quickly reduces the blocker surface,
- failures outside the retained path disappear,
- or the retained path becomes understandable and controllable.

### Switch from B to A if

- **all** of the following are true:
  - an explicit retained file list has been written down,
  - the failure reproduces on that retained path after the narrowing pass,
  - the missing piece is a small generic primitive or header family rather than a broad subsystem
    expectation,
  - and the same compatibility gap is visible across multiple retained core files or one retained
    shared include chain.

### Abort A and return to B if

- more than one or two unrelated shim families are required before reaching a meaningful compile
  milestone,
- missing APIs are dominated by files outside the retained runtime path,
- or the work starts resembling unconstrained kernel-environment emulation.

## Phase 3 — Continue on the chosen path

### If B wins

- Keep the bounded file list explicit.
- Document exactly what the bounded claim covers.
- Do not quietly re-expand the tree.
- Add excluded modules back only behind explicit proof of need.
- Treat success here as **compile-triage progress only**. It does not imply full DC feature closure,
  optional-module completeness, or runtime readiness.

### If A wins

- Expand `linux-kpi` deliberately rather than scattering shims through amdgpu-local code.
- Keep each new shim family generic and reusable where possible.
- Track each new compatibility family as maintenance debt that must justify itself.

## Commit slicing

Recommended commit order:

1. narrow source set only,
2. first shim family only,
3. one blocker family per follow-up change.

Never mix broad source pruning and broad compatibility growth in the same commit.

## Red / Green / Refactor loop

### Red

The historical full-tree display build failed at `linux/kgdb.h` while compiling `freesync.c`.

### Green

Either:

- the narrowed source set compiles further, or
- one small shim advances the retained path to the next blocker.

Current green state:

- the bounded retained path now builds successfully,
- and the imported Linux AMD display / TTM / amdgpu trees remain explicitly excluded pending proven
  need.

### Refactor

Codify the smallest proven source set and execution path before adding more compatibility surface.

## Hidden failure modes

### Strategy B hidden failure mode

Strategy B can produce false confidence if the repo narrows the file list but does not write down
what functionality is now intentionally out of scope.

That is why every narrowing step must be paired with an explicit bounded claim.

### Strategy A hidden failure mode

Strategy A can feel productive because each header addition removes one hard stop. But that can hide
the fact that the repo is drifting into long-term Linux-environment emulation for code that the
current Red Bear target may not even need.

That is why A must stay subordinate to a retained, justified source set.

## Definition of done

This triage plan is complete when:

- the repo has an explicit choice between bounded source narrowing and compatibility expansion,
- the choice is backed by compile evidence,
- optional AMD DC modules are not silently treated as required for first bring-up,
- and compatibility growth, if needed, is happening in the right long-term layer.

For clarity, done here means the compile-triage path is explicit and justified. It does **not** mean
that the full AMD DC tree is complete, that excluded optional modules are unnecessary in all future
phases, or that runtime display validation is closed.

## Immediate next action

Do this next:

1. keep the retained `amdgpu` build path explicit and bounded,
2. do not quietly re-introduce imported Linux AMD display / TTM / core sources,
3. re-introduce imported subsystems only behind concrete runtime or feature evidence,
4. if a future re-introduction attempt fans into broad Linux-kernel compatibility work again,
   treat that as a new triage pass rather than as proof that the broader tree belongs in the
   default retained build.
