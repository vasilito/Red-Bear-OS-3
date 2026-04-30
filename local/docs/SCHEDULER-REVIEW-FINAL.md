# P1-P8 Scheduler & Relibc Stability Review

**Date:** 2026-04-30
**Scope:** Comprehensive review of P1-P8 kernel scheduler and relibc changes for stability, robustness, and clean code

## HIGH Severity — Fixed This Session

| # | File | Issue | Fix |
|---|------|-------|-----|
| 1 | `pthread_mutex.rs:89` | `make_consistent` stored dead TID instead of 0 | Store 0 for "no owner" |
| 2 | `cond.rs:106` | `.unwrap()` suppressed EOWNERDEAD/ENOTRECOVERABLE | Changed to `.expect()` with message |

## HIGH Severity — Documented as Known Limitations

| # | File | Issue | Status |
|---|------|-------|--------|
| 3 | `switch.rs:396-437` | `steal_work` CPU iteration without atomicity | Structural limitation; documented with TODO |
| 4 | `proc.rs:481,613` | Lock ordering violation TODO in kfmap/ksetup | Pre-existing; requires deeper refactoring |
| 5 | `futex.rs:821-844` | PI futex CAS loop with `entry().or_insert()` race | Requires atomic entry creation pattern |

## MEDIUM Severity — Documented for Follow-up

| # | File | Issue |
|---|------|-------|
| 6 | `switch.rs:171` | TODO: Better memory orderings for CONTEXT_SWITCH_LOCK |
| 7 | `futex.rs:370-380` | Addrspace freed while robust list walk (UAF risk) |
| 8 | `pthread_mutex.rs:140` | `mutex_owner_id_is_live` O(n) scan |
| 9 | `pthread_mutex.rs:37-39` | SPIN_COUNT = 0 — no adaptive spinning |
| 10 | `barrier.rs` | No pthread_barrier_destroy — memory leak |
| 11 | `sched/mod.rs` | All sched_* functions return ENOSYS (honest stubs) |
| 12 | `pthread/mod.rs:553` | pthread_setname_np allocates format! on every call |

## Build Verification

- `cargo check` relibc: ✅ passes (1 pre-existing warning)
- `make r.kernel`: ✅ passes
- P8 patches in recipe: 5 of 8 wired (3 not yet wired — initial-placement, load-balance, work-stealing)

## Honest Status Assessment

| Phase | Status | Notes |
|-------|--------|-------|
| P0 | ✅ Complete | Barrier SMP, sigmask, pthread_kill |
| P1 | ✅ Complete | Robust mutexes, sched API (honest ENOSYS) |
| P2 | ✅ Complete | RT scheduling, SchedPolicy |
| P3 | 🚧 Partial | PerCpuSched + wiring done; stealing/balancing deferred |
| P4 | ✅ Complete | Futex sharding + REQUEUE + PI + robust |
| P5 | ✅ Complete | setpriority, affinity, thread naming, schedparam |
| P6 | 🚧 Partial | Cache-affine done; NUMA deferred |
| P7-P8 | ✅ Complete | Futex REQUEUE/PI/robust deliverable |
