# Red Bear OS relibc Completeness and Enhancement Plan

## Purpose

This document assesses relibc in Red Bear OS for **strengths**, **deficiencies**, **subsystem-facing
gaps**, and **overall quality**, then defines a practical plan for improving it.

The goal is not to treat relibc as a generic libc project. The goal is to describe:

- what is already strong,
- what still depends on active local overlay state rather than upstream relibc itself,
- what is still incomplete or weak,
- what downstream subsystems still depend on relibc improvement,
- and what order of work best improves real system capability.

This is a Red Bear-specific document. It is grounded in the current repo state rather than older,
pre-correction roadmap assumptions.

## Evidence Model

This plan uses four evidence buckets and does **not** treat them as equivalent:

- **source-visible** — behavior visible directly in the current relibc source tree
- **patch-carried** — behavior carried in the active `local/patches/relibc/*.patch` recipe inputs rather than upstream relibc itself
- **build-visible downstream** — downstream packages now compile because the libc surface exists
- **runtime-validated** — behavior has been exercised successfully in real downstream/runtime paths

This distinction matters because relibc’s current problem is often **not** “API absent,” but the gap
between **implemented**, **patch-carried**, **build-proven**, and **runtime-trusted**.

## Upstream vs Red Bear ownership

For relibc, the ownership boundary must stay explicit:

- `recipes/core/relibc/source/` is the live upstream-owned working tree used for actual build and
  validation
- the active Red Bear-owned durable relibc compatibility carrier is the recipe-replayed
  `local/patches/relibc/*.patch` set; in the current tree that active replay has narrowed to
  `local/patches/relibc/redox.patch`
- older `local/patches/relibc/P3-*.patch` files are historical bring-up references unless a current
  recipe still replays them
- `local/docs/...` is the durable explanation of what those changes mean and how to reapply them

That means a relibc change is not truly preserved until its ownership is explicit in the right
place:

1. if upstream now owns the behavior, the live relibc source tree is the canonical implementation
2. if Red Bear still owns a unique delta, it must also exist in the active
   `local/patches/relibc/` recipe input set so the same result can be recreated after an upstream
   refresh

The repo standard for success is not merely “the current source tree builds.” The standard is:

> we can fetch fresh upstream relibc sources, reapply the active Red Bear relibc patch carriers, and still
> rebuild the same working result.

Any relibc work that exists only under `recipes/core/relibc/source/` should therefore be treated as
validated-but-not-yet-preserved.

Because relibc is also one of the fastest-moving upstream areas, Red Bear should apply one more
rule here:

> if a Red Bear relibc patch solves a problem that upstream has already solved, prefer the upstream
> solution and retire or reduce the local patch.

The goal is durable compatibility, not a permanent relibc fork.

## Current Repo State

> **Implementation note (current Red Bear tree):** this repo pass moved several relibc items from
> patch-carried-only or downstream-workaround status into source-visible libc behavior. The current
> tree now contains source-visible and strict Redox-target runtime-tested `signalfd`, `timerfd`, `eventfd`, `open_memstream`,
> `F_DUPFD_CLOEXEC`, `MSG_NOSIGNAL`, a bounded `waitid()` path, bounded `RLIMIT_NOFILE` /
> `RLIMIT_MEMLOCK` behavior, a bounded `eth0`-backed `net/if.h` / `ifaddrs.h` view, a source-visible
> `resolv.h` plus bounded `res_query()` / `res_search()` compatibility paths with receive/send
> timeout hardening, a first named-semaphore implementation on top of the existing shm path, and
> bounded `sys/ipc.h` / `sys/shm.h` surfaces for the `IPC_PRIVATE` / `shmget` / `shmat` /
> `shmdt` / `shmctl(IPC_RMID)` workflow.

> **Downstream validation note (current Red Bear tree):** `libwayland` now cooks successfully
> against the updated relibc, and qtbase now configures, builds, and stages with
> `FEATURE_process=ON`, `FEATURE_sharedmemory=ON`, and `FEATURE_systemsemaphore=ON` in the current
> tree. The relibc `tests/` harness also now builds focused Redox-target binaries for `eventfd`,
> `waitid`, `res_init`, `res_query`, `sem_open`, and `shmget`, and the host-target variants of those same
> focused tests now execute successfully under the relibc-built host sysroot. That does not mean
> relibc is complete, but it does mean the implementation has crossed real downstream build/stage
> gates and direct execution-level proof rather than remaining an isolated libc-only pass. The
> current host-side `res_query` proof is still bounded: it compiles, runs, and fails fast under the
> relibc sysroot instead of hanging, but it is not yet a runtime-trusted downstream DNS proof.
>
> **Additional downstream proof (current Red Bear tree):** the in-tree `openssh` recipe now cooks
> successfully against the relibc resolver surface after switching the recipe to the rebuilt relibc
> headers/libraries and removing stale Redox-specific resolver fallbacks from the OpenSSH patch.
> That is still build/stage proof rather than runtime SSH validation, but it demonstrates that real
> consumers can now compile and link `res_init`, `res_query`, and `dn_expand` from relibc.
>
> **Fresh revalidation pass (current Red Bear tree):** the focused host-side relibc proofs were
> rerun for `eventfd`, `waitid`, `res_init`, `res_query`, `sem_open`, and `shmget`; the binaries all
> built, and the executions succeeded for `eventfd`, `waitid`, `res_init`, `sem_open`, and `shmget`
> with the bounded `res_query` test still failing fast rather than hanging. The main downstream
> consumers previously used as evidence were also rerun successfully: `CI=1 ./target/release/repo cook libwayland`,
> `CI=1 ./target/release/repo cook qtbase`, and `CI=1 ./target/release/repo cook openssh` now all
> succeed in the current tree.
>
> **Additional focused coverage (current Red Bear tree):** integrated relibc tests were also added
> for `open_memstream`, SysV semaphores via `semget`/`semop`/`semctl`, `timerfd`, `signalfd`, and
> `eventfd`. On the host-side relibc sysroot, `open_memstream`, `semget`, and the bounded SysV shm
> path execute successfully. On the Redox-target runtime path, the repaired `cookbook_redoxer`
> `write-exec` flow now executes the targeted `eventfd`, `signalfd`, and `timerfd` binaries
> successfully against the staged relibc test tree, and those tests now fail hard if the APIs are
> unavailable. That moves the fd-event APIs from source-visible/build-visible status into explicit
> runtime-tested status for the bounded relibc harness.
>
> **Fresh-upstream reapply proof (current Red Bear tree):** a fresh `repo unfetch relibc` →
> `repo fetch relibc` cycle was used to reconstruct the relibc source tree from upstream-owned
> sources, the durable `local/patches/relibc/` carrier set was reapplied to that fresh tree, and the
> resulting rebuild again supported successful downstream `libwayland` and `qtbase` cooks. That is
> the current proof that Red Bear’s relibc work is not only buildable in-place, but also recoverable
> after a fresh upstream source refresh.

> **Current reconstructed-state proof set:** with the refreshed source tree rebuilt from the local
> relibc overlay set, the repo now has successful cookbook evidence for all three layers in order:
> `CI=1 ./target/release/repo cook relibc`, then `CI=1 ./target/release/repo cook libwayland`, then
> `CI=1 ./target/release/repo cook qtbase`. This is the strongest current proof that the relibc
> compatibility work is preserved in the right place for long-term maintenance.
>
> **Current patch-carrier note:** the bounded `ifaddrs` / `net_if` work and the bounded
> `arpa/nameser.h` / `resolv.h` compatibility work are now preserved in the tracked
> `local/patches/relibc/redox.patch` carrier instead of separate transient patch files. The durable
> relibc recipe patch chain therefore consists only of tracked local patch files plus
> `recipes/core/relibc/recipe.toml` wiring.

### Summary

relibc is one of Red Bear’s strongest foundational subsystems, but it is not complete.

The current repo shows a relibc that is already strong in:

- broad header/libc surface coverage
- real Redox-native platform integration
- source-visible implementations of the historical Wayland-facing P3 APIs, with patch carriers still retained as sync/upstream artifacts
- enough maturity to unlock major build-side progress in Wayland, Qt, and KDE
- a substantial generic upstream-style test tree

The current repo also shows relibc is still weak in:

- shared memory / SysV IPC completeness
- named semaphores
- process/runtime quality for some downstreams
- networking/resolver/interface completeness
- Redox-target and downstream-runtime validation depth

### Status Matrix

| Area | State | Notes |
|---|---|---|
| Core POSIX/header breadth | **strong / partial** | Large header surface exists, but many TODO headers and feature gaps remain |
| Wayland-facing P3 APIs | **implemented / runtime-tested / bounded** | `signalfd`, `timerfd`, `eventfd`, `open_memstream`, socket flags, and `F_DUPFD_CLOEXEC` now exist in the relibc source tree; strict targeted relibc runtime tests now execute on Redox, but broader consumer semantics still need careful documentation |
| Networking/libc socket surface | **usable / partial** | AF_INET/AF_UNIX paths exist, but interface/reporting/resolver behavior remains narrow |
| Qt/KDE downstream unblockers | **build-side improved / multiple gates crossed** | `QProcess`, `QSharedMemory`, and `QSystemSemaphore` now configure, build, and stage on in-tree qtbase; broader runtime validation is still needed |
| Shared memory / semaphore completeness | **partial** | `shm_open` exists through the Redox shm path, but SysV IPC/shared-memory and named semaphore completeness remain open |
| Process/runtime completeness | **partial** | Some process-facing functionality still uses stubs or downstream workarounds |
| Dedicated test surface | **present / Redox-specific coverage still thin** | relibc has a substantial `source/tests/` tree, but the Red Bear-visible Redox/P3/runtime validation story is still weaker than the generic libc test surface |
| Runtime validation against real consumers | **improved / still bounded** | relibc fd-event runtime tests now execute on Redox; broader desktop consumer semantics still need continued confirmation |

## Strong Points

### 1. relibc already exposes a broad libc/header surface

`recipes/core/relibc/source/src/header/mod.rs` shows a broad libc/header tree with networking,
threading, polling, stdio, locale, signal, socket, time, and many Unix-facing modules already
present.

That means Red Bear should treat relibc work as **quality and completeness hardening**, not as a
greenfield libc effort.

### 2. The historical P3 Wayland-facing API bridge is now source-visible

The local relibc patch carriers documented the APIs that historically blocked Wayland and downstream
consumers. In the current preserved tree, those fd-event and adjacent IPC surfaces are now present
in the active upstream relibc source itself, and the relibc-facing recipes no longer replay the old
standalone P3 carrier set for `eventfd`, `signalfd`, `timerfd`, `waitid`, SysV IPC, or their focused
test files. The active Red Bear relibc recipe replay has narrowed back to the shared
`local/patches/relibc/redox.patch` compatibility delta, while the historical P3 patch files remain
useful as prior bring-up evidence rather than current recipe inputs.

### 3. Focused fd-event proof record

The bounded fd-event runtime proof now has a small tracked record here so it does not depend only on
session history.

Preserved command shape:

- rebuild relibc from tracked carriers: `repo unfetch relibc && repo fetch relibc && repo cook relibc`
- rebuild targeted test package: `TESTBIN=sys_eventfd/eventfd CI=1 ./target/release/repo cook relibc-tests-bins`
- execute inside staged Redox target via `cookbook_redbear_redoxer write-exec`

Recorded bounded runtime markers from the current pass:

- `eventfd_runtime_finalfinal_ok`
- `signalfd_runtime_finalfinal_ok`
- `timerfd_runtime_finalfinal_ok`
- `eventfd_runtime_kernelreplay_ok`

These markers should be read as proof of the bounded relibc fd-event harness only. They do not by
themselves claim full Linux-equivalent semantics for every downstream desktop consumer.

The upstream-first policy still applies here, but the durable patch-carrier set should be trimmed
only when a fresh upstream refetch plus reapply plus downstream rebuild actually proves the upstream
coverage is sufficient. In the current Red Bear tree, `open_memstream`, `F_DUPFD_CLOEXEC`, and the
socket flag work still need to remain in the relibc overlay set because the clean reconstructed
consumer path still depends on them.

This is one of relibc’s strongest current points: Red Bear already has the exact P3 compatibility
surface that older docs used to describe as absent.

The local patches still matter as provenance and sync-upstream carriers for the gaps upstream does
not yet solve, but they should be retired as soon as upstream makes them redundant.

### 3. Downstream build progress proves relibc is materially useful

The current docs consistently show that relibc has already enabled substantial downstream progress:

- `docs/02-GAP-ANALYSIS.md` now marks the P3 bridge as implemented in-tree with strict Redox-target runtime proof for the fd-event slice
- `local/docs/WAYLAND-IMPLEMENTATION-PLAN.md` says the build-side relibc/libwayland bridge is restored and that the remaining blocker is runtime validation, not basic POSIX availability
- `local/docs/QT6-PORT-STATUS.md` treats many earlier relibc blockers as moved from “missing” to “present but still needs downstream validation”

This is a major quality signal: relibc is already strong enough to unlock real build-side subsystem work.

### 4. relibc already has a substantial generic test surface

`recipes/core/relibc/source/tests/` is real and large. It already covers many libc-facing areas such
as:

- `fcntl/`
- `net/` and `netdb/`
- `pthread/`
- `stdio/`
- `sys_mman/`
- `sys_socket/`
- `sys_resource/`
- `time/`
- `unistd/`

That is a genuine strength and should be documented as one.

The remaining weakness is narrower: Red Bear still lacks a strong **Redox-target / P3 API /
downstream-runtime** validation story that is as visible and deliberate as this generic relibc test
tree.

### 5. The current relibc problem is no longer one single blocker

The downstream evidence shows that relibc now has **multiple completeness fronts**:

- Wayland-facing POSIX/event APIs
- Qt/KDE shared memory and semaphore support
- process-facing behavior such as `waitid()`
- networking/resolver completeness
- legacy but still-consumed items such as `sigjmp_buf` and locale/runtime edges

That means the right enhancement plan is no longer “finish one missing API and unblock everything.”
The work has to be triaged by downstream impact.

### 6. The Redox networking model is reflected in relibc

`recipes/core/relibc/source/src/platform/redox/socket.rs` shows a real Redox-native socket/path
model instead of a pure stub implementation. That is another strong point: relibc already knows
about Redox-native runtime behavior.

## Deficiencies and Gaps

### 1. Header coverage is still incomplete in visible source

`recipes/core/relibc/source/src/header/mod.rs` still contains a meaningful backlog of TODO or absent
header surfaces, including examples such as:

- `iconv.h`
- `mqueue.h`
- `spawn.h`
- `sys/msg.h`
- `threads.h`
- `wordexp.h`

Some of these are lower-value than others, but they still show that relibc has real completeness work left.

### 2. Named semaphores are now source-visible, but still incomplete

`recipes/core/relibc/source/src/header/semaphore/mod.rs` is still a clear example of partial completeness.

Basic unnamed semaphore paths exist (`sem_init`, `sem_post`, `sem_wait`, `sem_timedwait`, etc.),
and the named semaphore path is now source-visible too:

- `sem_open`
- `sem_close`
- `sem_unlink`

These are now implemented on top of the existing shm path instead of left as raw `todo!()` stubs.

The remaining weakness is semantic and validation depth, not pure absence:

- broader POSIX semaphore semantics are still not strongly runtime-validated
- downstream configure/runtime behavior still needs continued confirmation
- the SysV semaphore surface remains thinner than a full Unix implementation

This directly affects downstream consumers such as `QSystemSemaphore`.

### 3. Shared memory is present, but not complete enough for downstream GUI/runtime work

The current relibc source already exposes one meaningful shared-memory path:

- `recipes/core/relibc/source/src/header/sys_mman/mod.rs` provides `shm_open()` and `shm_unlink()`
- on Redox, that path resolves to `/scheme/shm/`
- `recipes/core/base/source/ipcd/src/shm.rs` implements the backing shared-memory scheme

That is a real strength and should not be described as “shared memory absent.”

The real gap is that shared-memory completeness is still insufficient for broader downstream use:

- the source tree now has visible `sys/shm.h` / `sys/ipc.h` / `sys/sem.h` modules, but they remain bounded rather than comprehensive
- Qt/KDE-facing docs still treat `shm_open()` / `shmget()`-class behavior as unresolved enough to block full `QSharedMemory` confidence
- the current repo still lacks a strong end-to-end validation story for these paths in desktop consumers

### 4. Resolver and interface-networking completeness are still uneven

The downstream scan shows that networking-facing userland still hits relibc gaps beyond raw socket
basics.

Examples from downstream recipes and docs:

- `recipes/wip/qt/qtbase/recipe.toml` still leaves QtNetwork disabled because of broader networking/runtime concerns such as `in6_pktinfo` and richer interface semantics, even though minimal `resolv.h` and `arpa/nameser.h` surfaces now exist
- `recipes/net/openssh/recipe.toml` and its patch history still call out `resolv.h`
- `recipes/wip/terminal/tmux/redox.patch` comments out `resolv.h`
- `recipes/libs/glib/redox.patch` still touches resolver-facing includes

### 5. The networking surface is narrower than generic Unix software expects

The current source still shows important limits that should be named directly:

- `recipes/core/relibc/source/src/platform/redox/socket.rs` has AF_INET / AF_UNIX socket handling
- `recipes/core/relibc/source/src/header/net_if/mod.rs` now exposes a bounded `eth0`-backed interface view instead of a permanent `stub`
- `recipes/core/relibc/source/src/header/ifaddrs/mod.rs` now provides a bounded `eth0`-backed `getifaddrs()` path instead of pure `ENOSYS`
- source-visible `resolv.h` / `arpa/nameser.h` plus bounded `res_query()` / `res_search()` compatibility are now present, and at least one real downstream (`openssh`) now builds against them, but broader resolver compatibility is still incomplete

That is enough to support the current Red Bear native network path in a bounded sense, but it is not
yet strong enough to claim broad interface-aware compatibility for higher-level consumers. Resolver/
header gaps and interface-model assumptions still show up in ports such as QtNetwork, OpenSSH,
tmux, glib, curl, and libuv.

### 6. Process/runtime completeness is still uneven

The repo still has process/runtime unevenness, but one meaningful consumer-facing gap has now moved:

- relibc now provides a bounded `waitid()` implementation over the existing `waitpid` path
- the old Qt-side injected `waitid()` stub has been retired from the Qt recipe layer

The source state needs to be classified carefully:

- `sigjmp_buf` exists in `recipes/core/relibc/source/include/setjmp.h`, so older downstream comments treating it as absent are better read as compatibility/staleness signals rather than primary source truth
- `getgroups()` has a Redox implementation path in `platform/redox/mod.rs`
- `getrlimit()` is no longer a pure placeholder for all consumers: Red Bear now has bounded `RLIMIT_NOFILE` and `RLIMIT_MEMLOCK` behavior, but broader resource-limit completeness is still weak

So process/runtime completeness should be treated as a real subsystem-quality track, but the plan
must distinguish **missing**, **implemented but weak**, and **stale downstream complaint**.

### 7. Source quality still contains many TODO / unimplemented branches

The current source has a large amount of unfinished or explicitly deferred behavior across:

- `pthread`
- `time`
- `unistd`
- `platform/redox`
- `epoll`
- `ptrace`
- locale and stdio internals

This does not mean relibc is unusable. It means completeness and quality work now needs a stronger
triage model instead of treating all missing items as equally important.

### 8. Redox-target and downstream validation remain thin relative to subsystem importance

The current repo already contains a substantial generic relibc test tree, but the Red Bear-visible
validation story is still thin in the areas that matter most for current subsystem unblockers.

Right now much of relibc’s confidence in the Red Bear docs still comes from:

- source inspection
- patch carriers
- build-side downstream success
- limited runtime validation via downstream stacks

That is not enough for a component as central as libc, especially for the Redox-target and
downstream-consumer paths Red Bear depends on.

## Downstream-Blocking Gaps by Subsystem

### Wayland

The old “basic POSIX APIs are missing” story is no longer the main one.

Current state:

- `signalfd`, `timerfd`, `eventfd`, `open_memstream`, bounded `waitid()`, key socket flags, and the adjacent SysV IPC surfaces are now source-visible in the active relibc tree without needing the old standalone P3 replay set
- the active Red Bear relibc replay has narrowed to the shared `redox.patch` compatibility delta while those older P3 files remain historical references
- `libwayland` now rebuilds with a much smaller Redox patch

Remaining blocker:

- runtime validation of the full relibc -> libwayland -> compositor path

So the current relibc task for Wayland is primarily **runtime proof and patch reduction**, not just
adding obvious libc symbols.

Current Red Bear evidence is stronger than before: `libwayland` now cooks successfully against the
rebuilt relibc image produced from the current upstream-backed relibc tree plus the active shared
Red Bear compatibility delta, which means the `signalfd`, `timerfd`, `eventfd`, `stdio.h`, and
`sys/socket.h` surfaces are sufficient for at least one major downstream consumer in the current
rebuild model.

### Qt / KDE

The Qt/KDE-facing relibc backlog is still substantial.

The biggest libc-facing gaps are:

- shared memory (`shm_open` / `shmget`) for `QSharedMemory`
- named/system semaphores (`sem_open` / `semget`) for `QSystemSemaphore`
- stronger process/runtime behavior for `QProcess`
- runtime validation of QtNetwork against the current relibc networking surface
- resolver/header completeness (`resolv.h`) and network-interface semantics for QtNetwork
- broader process/runtime validation after the new bounded `waitid()` path

This makes Qt/KDE the clearest downstream consumer pushing relibc from “build-capable” toward
“desktop-capable”.

Current Red Bear evidence is stronger than before here too: qtbase now configures, builds, and
stages with
`FEATURE_process=ON`, `FEATURE_sharedmemory=ON`, and `FEATURE_systemsemaphore=ON` in the current
tree. The remaining work is therefore less about “make the feature visible at all” and more about
runtime semantics, broader compatibility, and downstream cleanup.

### Networking and interface-aware software

The current relibc networking model is usable, but still narrow enough that higher-level consumers
keep carrying workarounds or disabled features.

The newer bounded `eth0`-backed `net_if` / `ifaddrs` work improves the source-visible story, but it
is still only a first Red Bear-shaped interface view, not a full generic Unix interface model.

This is why the plan should treat networking as **usable but still validation-heavy**, not “done”.

### General userland / server software

The downstream scan also shows relibc gaps outside graphics:

- PostgreSQL and some libraries still carry `sigjmp_buf`-related downstream notes that need revalidation against current headers
- SQLite still notes `getrlimit()` / `getgroups()` gaps, even though the current source state now splits those two differently
- Apache and other ports still touch semaphore or IPC assumptions

That is important because it means relibc completeness is not only about desktop bring-up. It also
affects core application/server breadth.

### Desktop/session path

Session and desktop work depends less on one dramatic relibc gap than on overall libc quality:

- process semantics
- IPC completeness
- synchronization primitives
- runtime interaction with D-Bus/Qt/Wayland consumers

This is why relibc should be treated as a cross-cutting runtime-quality subsystem, not just a POSIX checklist.

## Quality Assessment

### What relibc is good at now

- broad visible libc/header coverage
- practical Redox-native integration rather than fake stubs everywhere
- concrete P3 compatibility work for real downstreams
- enough maturity to unlock major subsystem builds
- a substantial generic test tree

### What relibc is bad at now

- uneven implementation depth
- too many TODO/unimplemented branches for a component this central
- patch-carried functionality that is still not strongly reflected in visible source snapshots
- too little Redox-target and downstream-runtime validation relative to the generic test tree
- too much downstream confidence still derived from “compiles” instead of “runtime-proven”

## Enhancement Plan

### Phase R0 — Evidence and Ownership Cleanup

**Goal**: Make relibc status honest before widening scope.

**What to do**:

- explicitly track relibc claims as `source-visible`, `patch-carried`, `build-proven`, or `runtime-validated`
- keep the P3 patch carriers discoverable and documented as canonical until upstreamed
- stop describing relibc gaps with outdated “missing basics” language where the code already exists

**Exit criteria**:

- subsystem docs consistently distinguish between missing, patch-carried, and runtime-proven relibc behavior

---

### Phase R1 — Stabilize the newly source-visible P3 APIs

**Goal**: Keep the newly source-visible P3 APIs aligned with their patch-carrier and downstream expectations.

**What to do**:

- keep `signalfd`, `timerfd`, `eventfd`, `open_memstream`, socket flags, and `F_DUPFD_CLOEXEC` visible and maintained as canonical relibc behavior
- reduce downstream assumptions that these APIs are still absent
- ensure generated/exported headers stay aligned with the source-visible implementation set

**Exit criteria**:

- the repo consistently treats these P3 APIs as source-visible functionality that now needs validation and downstream cleanup rather than invention

---

### Phase R2 — Close the shared-memory and semaphore completeness gap

**Goal**: Unlock the next meaningful Qt/KDE-facing libc surface.

**What to do**:

- keep the existing `shm_open` / `/scheme/shm/` path explicit and documented
- implement the missing SysV IPC/shared-memory side or document a deliberate non-goal if Red Bear does not want full SysV compatibility
- harden and validate the now source-visible named semaphore support (`sem_open`, `sem_close`, `sem_unlink`)
- close the specific `QSharedMemory` and `QSystemSemaphore` blockers identified in the Qt docs

**Exit criteria**:

- the Qt/KDE docs no longer list shared memory and named semaphores as unresolved relibc blockers

---

### Phase R3 — Process/runtime correctness for desktop consumers

**Goal**: Reduce downstream process workarounds.

**What to do**:

- strengthen process-facing libc/runtime behavior enough to remove targeted workarounds such as the Qt `waitid()` shim path
- close or intentionally document the remaining `sigjmp_buf` / `getrlimit()` / `getgroups()` quality gaps that still force downstream patches
- validate process semantics against real downstream consumers, not only isolated libc expectations

**Current implementation note:** the bounded `waitid()` path is now source-visible, the old Qt-side
`waitid()` shim is gone, and qtbase now configures/builds/stages with process support enabled. The
remaining work is broader process/runtime validation and cleanup, not the old total absence of `waitid()`.

**Exit criteria**:

- downstream process workarounds are reduced or eliminated for the current desktop stack

---

### Phase R4 — Networking/runtime validation

**Goal**: Turn the current networking surface from “present” into “trusted”.

**What to do**:

- validate QtNetwork and similar consumers against the current relibc socket/ioctl/interface model
- close the highest-value resolver/header gaps such as `resolv.h` where they are still forcing downstream stubs or disabled modules
- evolve the new bounded `eth0`-backed interface-reporting path into a better general Redox interface model where needed
- document which current networking semantics are intentionally Redox-specific and which are intended to mimic broader Unix behavior

**Exit criteria**:

- at least one meaningful higher-level network consumer is validated against the current relibc networking surface

---

### Phase R5 — Dedicated relibc validation expansion

**Goal**: Improve libc confidence without waiting for whole desktop stacks.

**What to do**:

- build a stronger dedicated Redox-target and P3/downstream validation layer on top of the existing generic relibc test tree
- ensure new APIs and bugfixes come with focused libc-level tests where practical
- keep downstream consumer tests, but stop relying on them as the only quality signal

**Exit criteria**:

- relibc has explicit Redox-target and downstream-runtime validation beyond the generic upstream-style test tree

---

### Phase R6 — General completeness triage

**Goal**: Attack the remaining TODO/unimplemented backlog by priority rather than by random header count.

**What to do**:

- rank remaining TODO/unimplemented items by downstream subsystem impact
- prioritize IPC, synchronization, process, time, and networking correctness over obscure or deprecated headers
- keep deprecated/low-value gaps documented, but do not let them drive the roadmap ahead of higher-value runtime work

**Exit criteria**:

- relibc backlog is organized by real system impact instead of undifferentiated TODO volume

## Recommended Order of Work

The current best order is:

1. evidence cleanup and canonicalization of what already exists
2. shared memory and named semaphores
3. process/runtime correctness
4. networking/runtime validation
5. Redox-target and downstream validation expansion
6. broader backlog triage and cleanup

That order matches the current downstream blocker chain better than a generic “finish all missing headers” strategy.

## Support-Language Guidance

Until the runtime-validation phases are materially complete, Red Bear should avoid saying:

- “relibc POSIX gaps are solved”
- “Qt/Wayland blockers are fully gone”
- “network/process/shared-memory support is complete”

Prefer language such as:

- “consumer-visible P3 APIs are now present, with runtime validation still needed”
- “relibc is materially stronger, but desktop-facing completeness work remains”
- “the remaining relibc problem is now quality and downstream proof, not just symbol absence”

## Summary

relibc is one of Red Bear’s strongest foundational subsystems, but it is not complete.

Its strongest current qualities are:

- broad libc/header coverage
- real Redox-native platform integration
- concrete source-visible and patch-backed solutions to the historical P3 Wayland-facing blockers
- clear downstream build progress because of those fixes
- a substantial generic test surface

Its largest remaining weaknesses are:

- incomplete shared memory and named semaphore support
- process/runtime unevenness
- networking/resolver/interface completeness gaps
- too many TODO/unimplemented branches in central paths
- too little Redox-target and downstream-runtime validation relative to the generic test tree

The correct relibc roadmap is therefore **not** “hunt random missing symbols.” It is to turn the
current build-capable libc into a runtime-trusted subsystem by closing the high-value desktop/runtime
gaps, strengthening validation, and reducing patch-carried ambiguity.
