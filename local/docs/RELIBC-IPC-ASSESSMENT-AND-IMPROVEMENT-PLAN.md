# Red Bear OS relibc IPC Assessment and Improvement Plan

## Purpose

This document assesses the current **IPC-related relibc surface** in Red Bear OS and turns that
assessment into a concrete improvement plan.

The focus here is narrower than the general relibc plan:

- POSIX shared memory and semaphores
- System V shared memory and semaphores
- missing System V / POSIX IPC areas such as message queues
- IPC-adjacent descriptor/event primitives that downstream software treats as part of the same
  coordination substrate: `eventfd`, `signalfd`, and `timerfd`
- the downstream subsystem pressure created by Qt, KDE, Wayland, and related userland

This is not a generic libc-compliance document. It is grounded in the current repository state.

## Evidence Model

This assessment distinguishes four evidence levels:

- **source-visible** — behavior exists in relibc source now
- **test-visible** — behavior is exercised by focused relibc tests
- **build-visible downstream** — real consumers compile/link against it
- **runtime-validated** — behavior has been exercised in real Redox or consumer runtime paths

The key IPC problem in the current tree is not simple absence. It is the gap between
**source-visible**, **bounded**, **build-proven**, and **runtime-trusted**.

## Upstream vs Red Bear separation

For this IPC work, keep the storage model explicit:

- the live implementation under `recipes/core/relibc/source/src/header/` is the working upstream
  tree used for builds and tests
- the durable Red Bear ownership boundary is `local/patches/relibc/` plus `local/docs/`

So the IPC implementation is only truly safe when:

1. the upstream-owned relibc source tree builds with the change now, and
2. the same delta is preserved in `local/patches/relibc/` so a fresh upstream refetch can recover it

This repo should be able to pull renewed upstream sources every day and still rebuild after
reapplying the local relibc patch carriers. That requirement is part of the IPC improvement plan,
not an afterthought.

The same section also implies an upstream-preference policy:

- when upstream relibc already provides the same IPC fix, prefer upstream
- keep Red Bear IPC patches only for gaps that upstream still does not solve adequately
- review patch carriers regularly and delete or shrink ones made obsolete by upstream evolution

## Current Implementation Note

This repo pass did not just assess the IPC surface; it also restored the missing relibc IPC modules
that the drafted Red Bear docs were already assuming existed in-tree.

The current tree now contains source-visible implementations for:

- `sys/eventfd.h` / `eventfd()` / `eventfd_read()` / `eventfd_write()`
- `sys/timerfd.h` / `timerfd_create()` / `timerfd_settime()` / `timerfd_gettime()`
- `sys/signalfd.h` / `signalfd()` / `signalfd4()`
- `open_memstream()`
- bounded `sys/ipc.h`, `sys/shm.h`, and `sys/sem.h` compatibility layers
- a bounded `waitid()` path sufficient to satisfy current Qt process-side linking

This pass also added focused relibc tests for:

- `stdio/open_memstream`
- `sys_sem/semget`
- `sys_timerfd/timerfd`
- `sys_signalfd/signalfd`

Current manual verification in this repo pass:

- `cargo check --target x86_64-unknown-linux-gnu` passes for relibc
- host-side focused IPC tests execute successfully for `open_memstream` and `semget`
- targeted Redox runtime execution now validates the `timerfd` and `signalfd` tests directly through the repaired `write-exec` path instead of relying on bounded host-side fallback behavior
- `CI=1 ./target/release/repo cook relibc` completes successfully after clearing a stale stage-dir collision
- `CI=1 ./target/release/repo cook qtbase` now succeeds after exporting `eventfd_t` and restoring a bounded `waitid()` path
- a fresh `repo unfetch relibc` → `repo fetch relibc` cycle plus reapplication of
  `local/patches/relibc/` again supports successful downstream `libwayland` and `qtbase` builds,
  which is the current proof that the relibc IPC overlay is recoverable from refreshed upstream
  source, not only from the previously edited working tree

In other words, the current relibc IPC work is no longer just “working in the checked-out source
tree”. It is now proven as an overlay workflow:

1. refresh upstream relibc source
2. reapply the local relibc compatibility overlays
3. rebuild relibc
4. rebuild real downstream consumers (`libwayland`, `qtbase`)

For the current tree, that overlay story now includes the tracked
`local/patches/relibc/redox.patch` carrier owning the bounded interface-enumeration and
resolver-header compatibility deltas (`ifaddrs` / `net_if`, `arpa/nameser.h`, `resolv.h`) rather
than leaving those as standalone transient patch files.

## Scope Map

### In scope in relibc today

| Area | State | Primary evidence |
|---|---|---|
| `shm_open()` / `shm_unlink()` | implemented | `recipes/core/relibc/source/src/header/sys_mman/mod.rs` |
| POSIX unnamed semaphores | implemented | `recipes/core/relibc/source/src/header/semaphore/mod.rs` |
| POSIX named semaphores | implemented but bounded | `recipes/core/relibc/source/src/header/semaphore/mod.rs` |
| SysV shared memory | implemented but bounded | `recipes/core/relibc/source/src/header/sys_shm/mod.rs` |
| SysV semaphores | implemented but bounded | `recipes/core/relibc/source/src/header/sys_sem/mod.rs` |
| `eventfd` | implemented; stronger than the other descriptor-event APIs | `recipes/core/relibc/source/src/header/sys_eventfd/mod.rs` |
| `signalfd` | implemented, but runtime-thin and not broadly Redox-runtime-trusted yet | `recipes/core/relibc/source/src/header/signal/signalfd.rs` |
| `timerfd` | implemented, but semantically narrow and not broadly Redox-runtime-trusted yet | `recipes/core/relibc/source/src/header/sys_timerfd/mod.rs` |

### Explicitly incomplete or absent

| Area | Current state | Evidence |
|---|---|---|
| POSIX message queues | absent | `recipes/core/relibc/source/src/header/mod.rs` still has `TODO: mqueue.h` |
| SysV message queues | absent | `recipes/core/relibc/source/src/header/mod.rs` still has `TODO: sys/msg.h` |
| `threads.h` / other broader libc completeness | outside this IPC focus, still incomplete | `recipes/core/relibc/source/src/header/mod.rs` |

## Current Implementation Assessment

### 1. Strong spots

The strongest IPC-related point is that relibc is no longer missing its core coordination substrate.
The current tree has real, source-visible implementations for POSIX shm, POSIX semaphores, SysV
shared memory, SysV semaphores, `eventfd`, `signalfd`, and `timerfd`. This is already enough to
move several downstreams from patch-side workarounds to actual libc usage.

`shm_open()` and `shm_unlink()` are cleanly tied to the Redox-native `/scheme/shm/` path in
`sys_mman/mod.rs`. That is a good architectural fit: Red Bear is not pretending to have a Linux
kernel IPC model under the hood, but it still exposes familiar libc entry points on top of Redox
schemes.

The second strong point is that the IPC work is not just source-visible anymore. The focused relibc
tests already cover `sem_open`, `shmget`, `open_memstream`, `semget`, `eventfd`, and the targeted
Redox-runtime `timerfd` / `signalfd` cases. The broader relibc plan also records successful
downstream builds for `libwayland`, `qtbase`, and `openssh`, which means real consumers are already
benefiting from this work, but those consumers do **not** all prove IPC depth equally.

### 2. Weak spots

The biggest weakness is **boundedness masquerading as compatibility**. The SysV layers exist, but
they are deliberately thin wrappers over `/scheme/shm/` and relibc-local bookkeeping, not a broad
Unix-complete implementation.

In `sys_shm/mod.rs`, `shmat()` rejects non-null attach addresses with `ENOSYS`, `SHM_RND` is
defined but not meaningfully implemented, and `shmctl()` only meaningfully supports `IPC_RMID` and
`IPC_STAT`. This is good enough for simple `IPC_PRIVATE` workflows and current compile-time
consumers, but it is not strong enough to claim general SysV shared-memory completeness.

In `sys_sem/mod.rs`, `semget()` rejects any `nsems != 1`, so the implementation is effectively a
single-semaphore set model rather than a full semaphore-set model. `semop()` supports multiple
operations in one call, but only for semaphore number 0, and there is no `semtimedop()` support.
`SEM_UNDO` is defined but not actually implemented. Compared with the standard `semop(2)` model,
this means the current layer matches only the narrowest downstream cases.

Named POSIX semaphores are also present but still bounded. `sem_open()` is implemented on top of
`shm_open()`, which is a practical Redox-native strategy, but the current code comments already mark
it as a bounded Redox path rather than a full Linux/glibc-equivalent semantic model.

The descriptor-event primitives are in a materially better state than before. `eventfd` now has a
real counter-style runtime path instead of only a source-visible wrapper, and the targeted Redox
runtime test harness now executes strict `eventfd`, `signalfd`, and `timerfd` test binaries
successfully through the repaired `write-exec` runner path. The older "unavailable is success"
fallbacks were removed from those focused tests, so these are now actual runtime checks rather than
mere launch proofs.

The preserved overlay story for those paths is now simpler than it was during the original bounded
bring-up. The current relibc tree already contains the fd-event implementations and focused tests
upstream, so the active Red Bear recipe replay no longer needs the old standalone
`P3-eventfd.patch`, `P3-signalfd.patch`, `P3-signalfd-header.patch`, `P3-timerfd.patch`, and
`P3-fd-event-tests.patch` carriers. In the current repo, `redox.patch` remains the active shared
Red Bear relibc delta, while the historical P3 files are legacy references rather than recipe inputs.

The remaining caution is semantic breadth, not whether the paths execute at all. `timerfd` is now
runtime-validated for the bounded relibc test harness, but downstream consumers such as KWin still
pressure Linux-oriented details like `TFD_TIMER_CANCEL_ON_SET`, so broad desktop/runtime trust
should still be described as narrower than full Linux equivalence.

### 3. Missing areas

The obvious missing IPC area is message queues. Both `mqueue.h` and `sys/msg.h` remain TODOs in the
header tree, which means relibc currently has no story at all for POSIX message queues or SysV
message queues. That is not necessarily today’s highest-value blocker, but it is still a real IPC
gap and should be named directly instead of being buried under generic TODO volume.

## Downstream Subsystem Assessment

### Qt / KDE

Qt and KDE are the clearest subsystem forcing IPC depth rather than just IPC surface area.

`local/docs/QT6-PORT-STATUS.md` already treats `QSharedMemory`, `QSystemSemaphore`, and `QProcess`
as moved from “missing libc surface” to “present, but still needs runtime validation”. That is the
right framing. The libc surface is no longer the primary blocker; confidence and semantics are.

The strongest concrete consumers in-tree are:

- `local/recipes/kde/kf6-kservice/source/src/sycoca/kmemfile.cpp` — heavy `QSharedMemory` usage
- `local/recipes/kde/kf6-solid/source/src/solid/devices/backends/udisks2/udisksopticaldisc.cpp` —
  `QSharedMemory` plus `QSystemSemaphore`
- `local/recipes/kde/kf6-kio/source/src/gui/previewjob.cpp` — direct SysV `shmget` / `shmat`
- `local/recipes/kde/kwin/source/src/utils/xcbutils.cpp` — direct `shmget`
- `local/recipes/kde/kwin/source/src/core/syncobjtimeline.cpp` and kio scoped-process code —
  `eventfd`
- `local/recipes/kde/kwin/source/src/plugins/nightlight/clockskewnotifierengine_linux.cpp` —
  `timerfd` with `TFD_TIMER_CANCEL_ON_SET`

This matters because it shows two different downstream classes:

1. **Qt abstractions** (`QSharedMemory`, `QSystemSemaphore`) that can tolerate bounded underlying
   libc behavior if their common paths work.
2. **Direct Unix/Linux-style callers** (KIO/KWin) that expose the places where the current relibc
   SysV and timerfd layers are still semantically narrower than software expects.

### Wayland stack

Wayland is less about classic shared-memory IPC completeness now and more about the descriptor-event
side of the same subsystem family. The repo’s existing docs correctly show that `signalfd`,
`timerfd`, `eventfd`, and `open_memstream` were the historical blockers and are now source-visible.
`libwayland` cooking successfully is strong build-side proof, but the remaining work is runtime
behavior under a compositor/session stack.

### Secondary consumers: OpenSSH / GLib / tmux

These are weaker IPC drivers and stronger networking/resolver drivers. They still matter because they
show a pattern: once relibc exports the needed surface, downstream recipes can drop fake fallbacks,
but runtime validation still trails source visibility. For an IPC-focused roadmap, they are useful
secondary evidence, not primary IPC blockers.

The downstream proof should therefore be read this way:

- `qtbase` is the strongest IPC-facing downstream because it directly pressures shared memory,
  semaphores, and process behavior.
- KDE consumers on top of Qt are the strongest subsystem evidence for where IPC semantics still need
  runtime trust.
- `libwayland` is strongest as descriptor-event proof (`signalfd`, `timerfd`, `eventfd`,
  `open_memstream`) rather than SysV IPC proof.
- `openssh`, `glib`, and `tmux` are useful proof that relibc header/export cleanup is helping real
  ports, but they should not be over-counted as core IPC validation.

## Main Blockers

### Blocker 1 — SysV layers are intentionally narrower than their API surface suggests

This is the highest-value blocker because it affects both direct consumers and Qt/KDE confidence.

Current examples:

- `semget()` only supports one semaphore per set
- `semop()` only supports semaphore number 0
- `SEM_UNDO` is not implemented
- `semtimedop()` is absent
- `shmat()` does not support non-null attach addresses
- `shmctl()` does not cover the broader control matrix
- SysV message queues are absent entirely

None of these invalidate the current build work. But together they mean “API present” is still not
the same as “subsystem-complete”.

### Blocker 2 — Runtime validation is still shallower than subsystem importance

The IPC surface is better-tested than before, but runtime validation still trails the subsystem’s
importance.

Current test story:

- host-side focused execution exists for `sem_open`, `shmget`, `open_memstream`, `semget`, and
  `eventfd`
- targeted Redox runtime execution now exists for `signalfd`, `timerfd`, and `eventfd` via
  `relibc-tests-bins` and the repaired `cookbook_redoxer write-exec` path, with strict pass/fail
  semantics rather than availability fallbacks
- downstream build evidence exists for `libwayland`, `qtbase`, and `openssh`

What is still missing is stronger Redox-target or consumer-runtime proof for Qt/KDE and Wayland
paths that actually exercise shared memory, semaphores, and timer/signal descriptor behavior in a
live session.

The strongest safe claim today is therefore:

- **source-visible** across the major IPC surfaces,
- **test-visible** for focused host-side and Redox-target fd-event cases,
- **build-visible downstream** for meaningful consumers,
- with **bounded runtime trust on Redox for the relibc fd-event harness**,
- but **not yet broad proof of full Linux-equivalent semantics for every desktop consumer path**.

### Blocker 3 — Descriptor-event semantics are still narrower than Linux-oriented callers expect

KWin’s timer code wants `TFD_TIMER_CANCEL_ON_SET`. The current bounded relibc timerfd layer does
not claim that full Linux cancel-on-clock-change semantic. The preserved test/runtime slice proves
one-shot behavior and successful `TFD_TIMER_ABSTIME` / bounded flag-surface handling, while broader
Linux-equivalent cancel-on-clock-change semantics remain an explicit downstream expectation gap.

Likewise, `signalfd` support is no longer merely visible/exported; it now passes the targeted
Redox-runtime relibc test path. The remaining question is broader consumer semantics and long-tail
desktop/runtime confidence, not basic availability.

### Blocker 4 — Message queues remain a completely open IPC front

`mqueue.h` and `sys/msg.h` are still absent. This is not the first blocker to fix for today’s
desktop stack, but it is the clearest “IPC truly not implemented yet” gap left in relibc.

## Current Non-Goals / Not Yet Claimed

The current tree should **not** be described as claiming any of the following:

- full SysV semaphore-set semantics
- full SysV shared-memory semantics
- full Linux-equivalent `timerfd` semantics
- broad Redox-runtime trust for `signalfd` or `timerfd`
- any POSIX message queue support
- any SysV message queue support

## Recommended Improvement Plan

### Phase I1 — Reclassify the IPC support language

**Goal:** Make subsystem docs accurately describe the current state.

**Do:**

- describe POSIX shm and semaphores as implemented
- describe SysV shm and semaphores as **bounded compatibility layers**, not comprehensive support
- describe `eventfd` as stronger than `signalfd` / `timerfd`
- describe message queues as still absent

**Exit criteria:** repo docs stop using broad phrases that imply complete IPC compatibility.

### Phase I2 — Harden the bounded SysV compatibility layers

**Goal:** Make the existing SysV support less misleading and more useful.

**Do:**

- decide whether Red Bear wants full semaphore-set support or an intentionally limited single-set model
- if limited, document that choice explicitly in relibc and subsystem docs
- otherwise extend `semget` / `semop` / `semctl` beyond the current semaphore-0-only model
- implement or explicitly reject `SEM_UNDO`
- add `semtimedop()` if downstreams need it
- expand `shmctl()` and `shmat()` support where real consumers need more than the current `IPC_PRIVATE`
  attach workflow

**Exit criteria:** the SysV shm/sem layers either become materially broader or are clearly documented
as intentionally bounded Redox compatibility shims.

### Phase I3 — Close the Qt/KDE runtime-proof gap

**Goal:** Move the IPC story from build-visible to desktop-visible.

**Do:**

- validate `QSharedMemory` under real Qt/KDE usage paths
- validate `QSystemSemaphore` in KDE consumers such as Solid
- validate KIO / KWin direct SysV shm paths
- record exactly which Qt/KDE IPC paths are now runtime-trusted versus merely build-capable

**Exit criteria:** Qt/KDE docs stop listing shared memory and semaphore support as unresolved relibc
confidence gaps.

### Phase I4 — Improve descriptor-event completeness for compositor/session code

**Goal:** Turn the current `eventfd` / `signalfd` / `timerfd` set into a more trustworthy runtime layer.

**Do:**

- keep `eventfd` on the current stable path
- validate `signalfd` in real event-loop style consumers
- extend `timerfd` semantics where current downstream code expects more than `TFD_TIMER_ABSTIME`
  (notably `TFD_TIMER_CANCEL_ON_SET`)
- build targeted Redox-target tests where host behavior is inherently not representative

**Exit criteria:** at least one meaningful compositor/session consumer is runtime-validated against
the current descriptor-event path.

### Phase I5 — Triage message queues explicitly

**Goal:** Stop leaving message queues as unprioritized TODOs.

**Do:**

- determine whether any current Red Bear subsystem actually needs POSIX or SysV message queues
- if not, mark them as lower-priority completeness debt
- if yes, create a dedicated implementation plan rather than burying them in generic header backlog

**Exit criteria:** `mqueue.h` and `sys/msg.h` are either on a concrete roadmap or explicitly treated
as non-blocking backlog.

## Recommended Order

The current best order is:

1. documentation cleanup and accurate IPC classification
2. SysV shm/sem hardening or explicit non-goal documentation
3. Qt/KDE runtime validation
4. descriptor-event runtime validation and timerfd semantic expansion
5. message queue triage

That order matches the current subsystem pressure better than a generic “finish all missing IPC
headers” strategy.

## Bottom Line

relibc IPC in Red Bear OS is no longer a story of missing primitives. It is now a story of **real
surface area with bounded compatibility depth**.

The strongest parts are POSIX shm, POSIX semaphores, `eventfd`, and the fact that major downstreams
already build. The weakest parts are the narrow SysV semantics, the lack of message queues, and the
runtime-proof gap for the desktop/session stack. The right next step is not random header work; it
is to harden and validate the IPC layers that current Qt/KDE and Wayland-adjacent consumers are
already trying to use.
