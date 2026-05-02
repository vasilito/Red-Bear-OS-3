# Build System Invariants

## Purpose

This document defines the non-negotiable build-system invariants for Red Bear OS.

Its job is to answer four questions unambiguously:

1. **What build surface is authoritative?**
2. **How must build surfaces propagate to consumers?**
3. **What state is durable vs disposable?**
4. **What must preflight verify before expensive builds begin?**

This file is the execution authority for:

- build preflight design
- sysroot synchronization logic
- release/archive orchestration
- patch/source durability checks

---

## Core rule

The Red Bear build must prefer **explicit, authoritative, reproducible state** over implicit, stale, or opportunistic state.

If a successful build depends on hidden state, stale copied headers, or untracked nested-source edits, the build system is considered incorrect even if the image happens to complete once.

When a build blocker is caused by a missing dependency surface, missing producer artifact, or invalid integration boundary, the default policy is:

> **Always do your best to fix before disabling.**

That means:

- prefer restoring the missing producer surface over disabling the consumer
- prefer repairing dependency visibility over commenting out downstream features
- treat disabling as a last resort, not a convenience path

If disabling is temporarily unavoidable, it must be:

- explicit
- narrowly scoped
- documented with the real upstream/producer-side blocker
- treated as temporary debt to remove, not as the desired final state

---

## Surface ownership model

### 1. relibc staged surface

**Authoritative location**

- `recipes/core/relibc/target/<target>/stage/usr/include`
- `recipes/core/relibc/target/<target>/stage/usr/lib`

**Authority level**

This is the authoritative build output for libc headers and libc-adjacent runtime libraries.

**Why**

- relibc is the canonical producer of the libc header/runtime surface used by downstream packages
- downstream recipes must not invent or preserve stale copies of relibc-provided interfaces when the staged surface has changed

**Invariant R1**

If relibc headers or exports change, the staged relibc surface is the only source of truth for those changes.

**Invariant R2**

No downstream package may rely on older copies of relibc-provided headers or libs once relibc has been refreshed.

---

### 2. Recipe consumer sysroot

**Authoritative location**

- per-recipe `target/<target>/sysroot`

**Authority level**

Derived surface, not root authority.

**Why**

- recipe sysroots are dependency assemblies for consumers
- they may contain additional recipe-local compatibility shims, but they do not supersede the canonical producer of those surfaces

**Invariant S1**

Recipe sysroots are derived from authoritative producers and may only diverge intentionally, minimally, and visibly.

**Invariant S2**

Recipe-local shims or injected compatibility headers must be treated as temporary exceptions, not silent permanent state.

**Invariant S3**

If a recipe sysroot needs manual copying or compatibility injection, that work must be explicit in the recipe and considered a candidate for future centralization.

---

### 3. Prefix toolchain sysroot

**Authoritative location**

- `prefix/x86_64-unknown-redox/sysroot`

**Authority level**

Derived toolchain-consumer surface.

**Why**

- GCC/Clang and their bundled target include resolution may consult the prefix sysroot directly
- some downstream packages, especially C++ consumers, may see this surface before or instead of a recipe-local sysroot

**Invariant T1**

If the compiler toolchain resolves target headers or libs from the prefix sysroot, critical relibc-provided interfaces must be synchronized there after relibc refresh.

**Invariant T2**

The prefix sysroot is not allowed to remain silently stale relative to the relibc staged surface for critical libc headers/libs.

**Invariant T3**

Sync into the prefix sysroot must be explicit, repeatable, and owned by shared build logic rather than scattered one-off package repairs where possible.

---

### 4. Release/archive source surface

**Authoritative location**

- `sources/redbear-<release>/...`

**Authority level**

Authoritative source origin in release mode.

**Why**

- release mode must be offline and reproducible
- release builds must not depend on network fetch or accidental live working-tree state

**Invariant A1**

When `REDBEAR_RELEASE` is set, archived release sources are the authoritative source surface.

**Invariant A2**

Release-mode builds must not require opportunistic upstream fetches.

**Invariant A3**

Before a release-mode build begins, required source trees must either already exist or be ensured from archived release sources.

---

### 5. Durable patch/source surface

**Authoritative location for persistence**

- `local/patches/...`
- `local/recipes/...`
- tracked Red Bear configs and docs

**Not authoritative for persistence**

- `recipes/*/source/`

**Why**

- upstream-owned source trees are disposable working trees
- successful compilation alone does not make a live source edit durable

**Invariant P1**

Any intended Red Bear change to an upstream-owned source tree must be mirrored into a durable patch carrier or Red Bear-owned location.

**Invariant P2**

An upstream-owned working tree with uncarried edits is an invalid end state, even if the current build succeeds.

**Invariant P3**

Patch wiring in recipe metadata is part of the durable source of truth.

---

## Propagation order

The expected propagation order for critical libc/runtime surfaces is:

1. upstream-owned source tree is patched or refreshed
2. authoritative recipe stage is rebuilt
3. recipe consumer sysroots are refreshed from authoritative producers
4. toolchain prefix sysroot is refreshed if compiler resolution depends on it
5. downstream package feature visibility is revalidated

If step 4 or 5 is skipped, the build may observe stale success/failure behavior and is considered unstable.

---

## Config-to-feature honesty rules

### Rule F1

If a config enables a package, the build system must verify that all required metadata exists before heavy compilation:

- source metadata
- hash metadata where required
- patch files
- dependency wiring

### Rule F2

If a recipe claims a feature is enabled, downstream discovery must also succeed.

Examples:

- CMake package visibility
- pkg-config metadata visibility
- required headers visible to consumers
- required shared/static libraries visible to linkers

### Rule F3

A feature is not considered present merely because a build flag says it is on.

It is only considered present if downstream consumers can discover and use it successfully.

---

## Patch-state classification rules

Any patch application outcome should be classified as one of the following:

1. **Applies cleanly**
   - source is missing the change and patch applies normally

2. **Already applied**
   - source already contains the intended delta

3. **Drifted/conflicting**
   - source changed enough that the patch no longer applies mechanically, and equivalence is not yet proven

4. **Obsolete**
   - the patch’s intent is now satisfied by upstream, relibc, toolchain, or another authoritative producer

**Invariant C1**

Build logic must not treat all non-applying patches as the same condition.

**Invariant C2**

Obsolete patches should be shrinkable or removable once equivalence is verified.

---

## Release-mode behavior rules

### Rule A4

Release-mode checks must happen before expensive build execution:

- archive existence
- archive mapping completeness
- required source trees present or ensure-able
- critical manifests consistent with build expectations

### Rule A5

If release-mode recovery is possible, the build system must say exactly what recovery path to use.

### Rule A6

Release-mode flow must be explainable through one coherent orchestration path, not many partial helpers with overlapping responsibility.

---

## Preflight-required checks

The preflight stage must catch cheap, known failures before deep compilation.

### Metadata checks

- tar-based recipes missing required `blake3`
- declared patch files missing from disk
- broken local overlay symlinks
- release archive/source mapping gaps in release mode

### Critical libc/runtime surface checks

- relibc staged headers exist for required known surfaces:
  - `sys/signalfd.h`
  - `sys/timerfd.h`
  - `sys/eventfd.h`
  - `threads.h`
- relibc export/header visibility exists for known problematic interfaces:
  - `strtold`
  - `thrd_exit`

### Toolchain coherence checks

- prefix toolchain sysroot reflects current relibc critical surface
- known critical headers are not stale relative to authoritative relibc stage

### Feature-surface checks

- QtNetwork is discoverable when enabled
- known Wayland prerequisite headers/macros are visible to configure probes
- package-family feature claims are not contradicted by missing downstream CMake/pkg-config surfaces

---

## Known anti-patterns

The following are explicitly disallowed or should be treated as defects:

1. **Late discovery of cheap metadata errors**
   - e.g. missing `blake3` found after long build progress

2. **Silent stale sysroot dependence**
   - build result depends on old copied relibc headers/libs remaining in the toolchain or recipe sysroot

3. **Uncarried upstream-owned source edits**
   - live source changes not mirrored into `local/patches/`

4. **Feature dishonesty**
   - a feature is marked enabled but downstream consumers cannot actually find or use it

5. **Indistinguishable patch skip behavior**
   - already-applied, drifted, and obsolete patches all looking like the same “skip” outcome

6. **Top-level orchestration script absorbing permanent package-specific hacks**
   - `build-redbear.sh` should coordinate, not become the only place build correctness lives

---

## Acceptance checklist

This invariants document is usable when all of the following are true:

- [ ] It names the authoritative owner of relibc stage, recipe sysroot, prefix toolchain sysroot, release archives, and durable patch state
- [ ] It defines the required propagation order for critical runtime surfaces
- [ ] It defines what counts as a valid durable upstream-owned source edit
- [ ] It defines the patch-state classes needed by the build system
- [ ] It gives preflight a concrete required check set
- [ ] It gives downstream tasks enough specificity to implement without guessing intent

---

## Downstream task consumers

This file is intended to directly unblock:

- **Build preflight command**
  - consumes the preflight-required check list

- **Sysroot sync helper**
  - consumes the ownership and propagation rules for relibc/recipe/toolchain surfaces

- **Patch-state classifier**
  - consumes the patch-state and durability rules

- **Release-mode orchestration**
  - consumes the archive/source authority and release-mode rules

---

## First implementation implications

Based on these invariants, the first practical implementation slice should do all of the following:

1. add preflight checks for hashes, patch presence, and critical relibc surfaces
2. formalize relibc-to-toolchain sysroot refresh as shared logic
3. classify patch outcomes instead of generic skip behavior
4. ensure release-mode source trees before deep build execution

If those four changes land cleanly, the build system will already move from reactive deep-build debugging toward proactive build-state validation.
