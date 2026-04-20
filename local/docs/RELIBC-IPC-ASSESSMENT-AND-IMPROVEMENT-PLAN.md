# Red Bear OS relibc IPC Assessment and Improvement Plan

## Purpose

This document is the IPC-focused companion to
`local/docs/RELIBC-COMPLETENESS-AND-ENHANCEMENT-PLAN.md`.

Its job is to describe the current IPC-facing relibc surface honestly, especially where the active
Red Bear build depends on recipe-applied compatibility layers rather than plain-source upstream
relibc.

## Evidence model

This document uses the same terms as the canonical relibc plan:

- **plain-source-visible**
- **recipe-applied**
- **test-present**
- **runtime-unrevalidated in this pass**

Do not collapse those into one generic "implemented" label.

## Current IPC inventory

| Surface | Plain source | Active build | Notes |
|---|---|---|---|
| `shm_open()` / `shm_unlink()` | yes | yes | provided through `sys_mman` in the live source tree |
| named POSIX semaphores | no | yes | added by `P3-semaphore-fixes.patch` on top of `shm_open()` / `mmap()` |
| `eventfd` | no | yes | added by `P3-eventfd.patch` through `/scheme/event/eventfd/...` |
| `signalfd` | no | yes | added by `P3-signalfd.patch` through `/scheme/event` plus signal-mask handling |
| `timerfd` | no | yes | added by `P3-timerfd.patch` through `/scheme/time/{clockid}` |
| `waitid()` | no | yes | added by `P3-waitid.patch` |
| `ifaddrs` / `net_if` support used by IPC-adjacent consumers | no | yes | added by `P3-ifaddrs-net_if.patch`; currently synthetic |
| SysV shm (`sys/shm.h`) | no | no | bounded carriers exist locally, but they are not part of the active concrete-wave recipe surface |
| SysV sem (`sys/sem.h`) | no | no | bounded carriers exist locally, but they are not part of the active concrete-wave recipe surface |
| POSIX message queues (`mqueue.h`) | no | no | still TODO in the live source tree |
| SysV message queues (`sys/msg.h`) | no | no | still TODO in the live source tree |

## Observed limitations

### Named POSIX semaphores

The active patch chain implements named semaphores by storing a `Semaphore` inside shared memory
opened through `shm_open()` and mapped with `mmap()`. That is a useful bounded compatibility path,
but it should still be described as a Red Bear recipe-applied layer, not a plain-source upstream
relibc completion.

### fd-event APIs

`eventfd`, `signalfd`, and `timerfd` are present in the active build, but they are all scheme-backed
compatibility layers:

- `eventfd` depends on `/scheme/event/eventfd/...`
- `signalfd` depends on `/scheme/event` and blocks the supplied mask with `sigprocmask()`
- `timerfd` depends on `/scheme/time/{clockid}` and currently rejects unsupported flag combinations

These are real compatibility layers, but they should still be described as bounded until broader
consumer/runtime proof is recorded.

### Deferred SysV shm/sem work

Bounded SysV shm/sem carriers still exist under `local/patches/relibc/`, but they were not wired
into the active concrete-wave recipe surface implemented in this pass. They should therefore be
treated as deferred follow-up work, not as active build behavior.

### Interface enumeration used by networking-adjacent consumers

The current `P3-ifaddrs-net_if.patch` replaces `ENOSYS`, but it does so with a synthetic two-entry
model:

- `loopback`
- `eth0`

That is enough for some bounded consumers, but it should not be described as live full interface
enumeration.

## Downstream pressure

### Qt / KDE

Qt and KDE remain the strongest pressure on relibc IPC semantics.

They do not only need headers to exist. They need the active compatibility layers to behave well
enough for:

- shared-memory consumers,
- named semaphore consumers,
- direct `eventfd` / `timerfd` users,
- and process-control paths such as `waitid()`.

### Wayland-facing consumers

Wayland-facing pressure is strongest on the fd-event side of the IPC story:

- `eventfd`
- `signalfd`
- `timerfd`

That is a different pressure profile from the SysV and named-semaphore side.

## Fresh verification in this pass

This pass revalidated the active concrete-wave IPC-facing surface through the relibc test recipe:

- `sys_eventfd/eventfd`
- `sys_signalfd/signalfd`
- `sys_timerfd/timerfd`
- `waitid`
- `semaphore/named`
- `semaphore/unnamed`

These are bounded relibc-target proofs. They improve confidence in the active fd-event and named
semaphore surface, but they do not change the deferred status of broader SysV shm/sem or message
queues.

## Improvement plan

### Phase I1 — Keep IPC claims aligned with the active build surface

- document patch-applied IPC layers as patch-applied
- stop describing them as plain-source-visible unless they move into the live source tree
- keep this doc aligned with `recipes/core/relibc/recipe.toml`

### Phase I2 — Decide the support contract for bounded IPC layers

For each major IPC area, choose one of these paths explicitly:

- bounded compatibility layer with honest documentation,
- or broader semantics work with explicit proof targets.

This is especially important for:

- SysV shm,
- SysV sem,
- named semaphores,
- and `ifaddrs`-driven interface discovery.

### Phase I3 — Add proof where current docs only imply confidence

Highest-value areas:

- the fd-event slice used by Wayland-facing consumers,
- shared-memory and named-semaphore behavior used by Qt/KDE,
- and the currently synthetic interface-discovery path.

### Phase I4 — Triage message queues directly

Message queues are still genuine absences, not just bounded implementations.

This doc should keep them visible until Red Bear either:

- implements them,
- proves they are unnecessary for the intended consumer set,
- or explicitly documents them as deferred/non-goals.

### Phase I5 — Converge with upstream deliberately

When upstream relibc absorbs equivalent IPC functionality, prefer the upstream path and shrink the
Red Bear patch chain. Until then, keep the active IPC carrier set explicit and documented.

## Bottom line

The current Red Bear relibc IPC story is **material patch-applied compatibility, not plain-source
completion**.

That is still valuable progress, but the repo should describe it honestly: several important IPC
surfaces exist in the active build, several of them are still bounded, and message queues remain a
real missing area.
