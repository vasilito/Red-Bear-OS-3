# Red Bear OS relibc Assessment and Improvement Plan

## Purpose

This document is the canonical Red Bear assessment of relibc quality, completeness, and robustness.

It is intentionally stricter than older relibc notes. This pass is grounded in what is visible in:

- the upstream-owned working tree under `recipes/core/relibc/source/`,
- the active relibc recipe patch list in `recipes/core/relibc/recipe.toml`,
- the durable Red Bear patch carriers under `local/patches/relibc/`,
- and the tests added by the active patch chain.

It does **not** flatten those evidence types into one generic claim of "implemented".

## Evidence model

Use these labels consistently when describing relibc in this repository:

- **plain-source-visible**: present in the current upstream-owned `recipes/core/relibc/source/` tree without relying on recipe patch replay
- **recipe-applied**: added only when the active relibc recipe replays Red Bear patch carriers
- **test-present**: test coverage exists in the source tree or the active patch chain
- **documented downstream build evidence**: another in-repo document records downstream build success, but that success was not re-executed as part of this documentation pass
- **runtime-unrevalidated in this pass**: do not describe as runtime-trusted here unless this review actually reran it

This distinction matters because the largest relibc documentation problem in the repo was overclaiming
plain-source convergence when the active build still depends on a substantial recipe-applied patch
chain.

## Ownership boundary

- `recipes/core/relibc/source/` is an upstream-owned working tree and may be replaced on refresh.
- `recipes/core/relibc/recipe.toml` defines the currently active relibc build surface.
- `local/patches/relibc/` is the durable Red Bear compatibility carrier set.
- `local/docs/` is the durable explanation of what Red Bear currently depends on and why.

For relibc, the honest maintenance target is:

> fresh upstream relibc sources can be refetched, the active Red Bear relibc patch chain can be
> replayed, and the same intended build surface can be reconstructed.

## Current implementation assessment

### 1. Plain source and active build are materially different

The current upstream-owned header tree still contains clear incompleteness markers in
`recipes/core/relibc/source/src/header/mod.rs`, including:

- `iconv.h`
- `mqueue.h`
- `spawn.h`
- `sys/msg.h`
- `threads.h`
- `wordexp.h`

The live source tree also still shows relibc areas that are **not** yet plain-source-complete:

- `recipes/core/relibc/source/src/header/semaphore/mod.rs` still contains `todo!("named semaphores")`
- `recipes/core/relibc/source/src/header/ifaddrs/mod.rs` still returns `ENOSYS`
- `recipes/core/relibc/source/src/header/mod.rs` still keeps `sys/ipc.h`, `sys/sem.h`, and `sys/shm.h` behind TODO comments

That means older wording such as "now source-visible in the current tree" was too strong for much of
the current relibc surface.

### 2. The active relibc build relies on a broad patch chain

The active recipe in `recipes/core/relibc/recipe.toml` currently replays more than `redox.patch`.
The tracked patch list still includes, among others:

- `redox.patch`
- `P0-strtold-cpp-linkage-and-compat.patch`
- `P3-eventfd.patch`
- `P3-signalfd.patch`
- `P3-signalfd-header.patch`
- `P3-timerfd.patch`
- `P3-waitid.patch`
- `P3-semaphore-fixes.patch`
- `P3-socket-cred.patch`
- `P3-elf64-types.patch`
- `P3-open-memstream.patch`
- `P3-ifaddrs-net_if.patch`
- `P3-fd-event-tests.patch`

So the active Red Bear relibc story is still **recipe-applied compatibility plus partial upstream
source**, not a nearly converged plain-source state.

### 3. What the active patch chain actually provides

Observed directly from the current patch set:

- `P3-eventfd.patch`: adds `sys/eventfd.h` support through `/scheme/event/eventfd/...`
- `P3-signalfd.patch`: adds `signalfd` / `signalfd4` support through `/scheme/event` plus signal-mask handling
- `P3-timerfd.patch`: adds `sys/timerfd.h` support through `/scheme/time/{clockid}`
- `P3-waitid.patch`: adds a bounded `waitid()` implementation plus a focused test
- `P3-semaphore-fixes.patch`: adds named semaphore support on top of `shm_open()` / `mmap()` and fixes unnamed semaphore error behavior
- `P3-open-memstream.patch`: adds `open_memstream()` plus a focused stdio test
- `P3-ifaddrs-net_if.patch`: adds a bounded `ifaddrs` / `net_if` surface that currently synthesizes only `loopback` and `eth0`
- `P3-fd-event-tests.patch`: adds focused `eventfd`, `signalfd`, and `timerfd` tests

This is meaningful progress, but it is still a patch-carried compatibility layer, not a finished libc
surface.

### 4. Fresh bounded-wave verification in this pass

This documentation pass also executed a fresh bounded relibc verification cycle against the active
recipe surface:

- `./target/release/repo unfetch relibc`
- `./target/release/repo fetch relibc`
- `./target/release/repo cook relibc`
- targeted `relibc-tests-bins` executions for:
  - `sys_eventfd/eventfd`
  - `sys_signalfd/signalfd`
  - `sys_timerfd/timerfd`
  - `waitid`
  - `semaphore/named`
  - `semaphore/unnamed`
  - `stdio/open_memstream`
  - `ifaddrs/getifaddrs`

These are bounded relibc-target proofs, not broad desktop-session runtime proof. They do, however,
move the active concrete-wave surface from documented intent to directly revalidated recipe behavior.

## Quality assessment

### Strong points

1. **The patch carriers are explicit and reviewable.**
   The relibc recipe points at named patch files instead of hiding Red Bear behavior in an
   untracked working tree.

2. **Several high-value desktop-facing APIs exist in the active build.**
   `eventfd`, `signalfd`, `timerfd`, `waitid`, and named semaphore support are all represented in the
   active patch chain instead of remaining vague TODO items.

3. **Focused tests now exist for the active concrete-wave surface and were rerun in this pass.**
   The current patch chain now covers `eventfd`, `signalfd`, `timerfd`, `waitid`, named and unnamed
   semaphores, `open_memstream`, and the bounded `ifaddrs` view.

4. **The build integration point is simple and durable.**
   The active surface is controlled centrally from `recipes/core/relibc/recipe.toml` and durable
   carriers under `local/patches/relibc/`.

### Weak points

1. **The repo has drifted between plain-source truth and documentation truth.**
   Several canonical docs previously described patch-carried functionality as if it already existed
   in the plain upstream-owned source tree.

2. **The active API surface is broader than its semantic maturity.**
   The patch chain exposes interfaces, but several of them are bounded compatibility layers rather
   than broad Unix-complete implementations.

3. **Patch-chain size is still a maintainability risk.**
   The active recipe still depends on a substantial set of P3 carriers. That is workable, but it is
   not yet the convergence story older docs implied.

## Completeness assessment

### Plain-source-visible gaps

Still absent or TODO in the live source tree:

- `mqueue.h`
- `sys/msg.h`
- named semaphores in `semaphore.h`
- `ifaddrs` plain-source implementation
- plain-source `sys/ipc.h`, `sys/sem.h`, and `sys/shm.h`

### Recipe-applied but bounded surfaces

The active build surface includes several features that should be described as **bounded**, not
fully complete:

- `timerfd`: the patch exposes `TFD_TIMER_CANCEL_ON_SET`, but `timerfd_settime()` only accepts
  `TFD_TIMER_ABSTIME`
- `ifaddrs` / `net_if`: current patch-provided interface enumeration is a fixed `loopback` + `eth0`
  model, not live system discovery
- `open_memstream`: now active in the recipe-applied surface, but still validated here only through
  focused relibc tests rather than broad downstream usage proof
- named semaphores: implemented through `shm_open()` / `mmap()` as a practical compatibility path,
  but not yet a broad semantics-proofed story

### Still-missing areas

The clearest remaining gaps are still real gaps, not just "needs more runtime proof":

- POSIX message queues
- SysV message queues
- broader thread / spawn / iconv / wordexp completeness

The broader SysV shm/sem carriers still exist under `local/patches/relibc/`, but they are not part
of the active bounded concrete wave implemented in this pass.

## Robustness assessment

Robustness is the weakest part of the current relibc story.

The repo now has a meaningful active patch-applied compatibility surface, but several pieces are
still narrow enough that the safest language is:

- useful for bounded downstream compatibility,
- not yet broad semantics-proof,
- and not yet safely describable as a plain-source upstream relibc completion story.

Concretely:

- fd-event APIs depend on scheme paths such as `/scheme/event` and `/scheme/time`
- `ifaddrs` currently reports a synthetic interface view rather than live network state
- named semaphores remain a bounded shm-backed path rather than a broader semantics-proofed story

## Recommended support language

Use this language in project docs unless stronger evidence is gathered:

- **Good:** "The active relibc recipe patch chain provides bounded `eventfd` / `signalfd` /
  `timerfd` compatibility for current Red Bear consumers."
- **Good:** "Named semaphores and `ifaddrs` currently exist through recipe-applied Red Bear
  compatibility patches, not as plain-source upstream relibc convergence."
- **Avoid:** "These surfaces are now source-visible in the current relibc tree."
- **Avoid:** "relibc is complete for desktop consumers."

## Improvement plan

### Phase R0 — Keep the evidence model honest

Goals:

- keep plain-source, recipe-applied, and runtime-proof language distinct
- keep canonical relibc docs aligned with `recipes/core/relibc/recipe.toml`
- stop describing patch-carried functionality as already upstream-visible unless it really is

Exit criteria:

- relibc docs match the active recipe patch list
- repo-level summaries use bounded/evidence-qualified language

### Phase R1 — Make the active patch chain the explicit build contract

Goals:

- treat the current relibc recipe patch list as the build contract for Red Bear relibc behavior
- review that list regularly against upstream relibc changes
- retire carriers only when the recipe no longer needs them

Exit criteria:

- every relibc carrier still replayed by the recipe is documented as active
- every historical-but-not-active carrier is clearly marked historical

### Phase R2 — Strengthen proof for the patch-applied surface

Goals:

- keep focused tests for `waitid`, semaphores, and other patch-applied APIs
- expand consumer-facing checks for the APIs Red Bear actually depends on
- avoid treating build success alone as semantics proof

Exit criteria:

- each active compatibility surface names its current proof level and missing proof

### Phase R3 — Harden bounded compatibility layers

Highest-value targets:

- fd-event semantics that current desktop consumers rely on
- named semaphore behavior beyond the current narrow shm-backed path
- `ifaddrs` / `net_if` behavior beyond the synthetic `loopback` + `eth0` model

Exit criteria:

- docs no longer need to caveat these areas as merely synthetic or narrowly bounded unless that
  boundedness is intentional and accepted

### Phase R4 — Decide the real SysV IPC contract

The current bounded SysV shm/sem layer is better than raw absence, but it is not a broad final
design.

Decision needed:

- either keep a clearly documented bounded compatibility contract,
- or implement a broader system-backed contract and test it accordingly.

Exit criteria:

- the repo stops implying broad SysV completeness where only a narrow compatibility slice exists

### Phase R5 — Triage the still-missing surfaces

Priority candidates:

- message queues,
- thread/spawn completeness,
- other TODO headers that block real consumers rather than theoretical completeness checklists.

Exit criteria:

- each remaining TODO surface is either implemented, explicitly deferred, or removed from misleading
  summary language

### Phase R6 — Converge with upstream where possible

Goals:

- shrink the relibc patch chain whenever upstream absorbs equivalent behavior
- avoid carrying Red Bear-local relibc deltas longer than necessary

Exit criteria:

- the active recipe patch chain is smaller for evidence-based reasons, not for documentation optics

## Bottom line

relibc in the current Red Bear repo is neither a greenfield libc nor a nearly converged upstream
story.

It is a **partially upstream, materially patch-applied compatibility surface** that already covers
important desktop-facing APIs, but still has real completeness gaps, bounded semantics, and a larger
patch-chain dependency than older docs admitted.
