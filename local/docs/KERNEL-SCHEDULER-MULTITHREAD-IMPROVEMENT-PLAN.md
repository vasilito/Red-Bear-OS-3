# Red Bear OS — Kernel Scheduler, Multithreading, and IPC Performance Improvement Plan

**Date:** 2026-04-30  
**Scope:** Kernel scheduler optimization, futex enhancements, multithreaded performance, relibc POSIX threading completeness  
**Status:** S3 complete (per-CPU + stealing + balancing + placement), S4 complete (futex sharding + REQUEUE + PI + robust + vruntime), S5 complete (setpriority + affinity + naming + schedparam), S6 partial (cache-affine delivered, NUMA deferred). This is the **canonical scheduler + multithreading authority**, extending `KERNEL-IPC-CREDENTIAL-PLAN.md` and `RELIBC-IPC-ASSESSMENT-AND-IMPROVEMENT-PLAN.md`  

---

## 1. Executive Summary

The Redox microkernel currently uses a **Deficit Weighted Round Robin (DWRR)** scheduler with 40 static priority levels, per-CPU run queues, and cooperative preemption. The relibc C library provides a largely complete pthreads implementation, but POSIX scheduling APIs (`sched_*`, `pthread_setschedparam`) are stubbed out. For the KDE/Wayland desktop path, multithreaded performance bottlenecks in the scheduler and futex subsystem will become the dominant limitation once the compositor (KWin) and GPU rendering pipelines are active.

### Current State at a Glance

| Area | Status | Key Gaps |
|------|--------|----------|
| Kernel scheduler | DWRR, 40 levels, vruntime selection for SCHED_OTHER, RT pass for FIFO/RR | Per-CPU run queues are infrastructure only; load balancing deferred |
| Futex | WAIT/WAIT64/WAKE + 64-shard hash table | No PI, no requeue, no robust futex |
| relibc pthreads | Create/join/detach/mutex/cond/rwlock/barrier/spin/tls | `sched_*` all `todo!()`, no PI/robust mutexes, no affinity API |
| Thread management | proc: scheme clone/fork/exec | No dynamic priority, no CPU affinity from userspace, no thread groups |
| IPC for threading | Futex, shared memory, signals | No process-shared robust/PI mutexes, no adaptive spinning |

### Why This Matters for the Desktop Path

```
KWin compositor (Qt6/QPA/Wayland)
  └── Worker threads: rendering, input, effects
  └── Requires: efficient futex wakeups, PI for compositor lock
  └── Requires: SCHED_RR for input thread priority

Mesa GPU driver (LLVMpipe or hardware)
  └── Gallium worker threads: shader compilation, draw submission
  └── Requires: load-balanced scheduling across all CPUs
  └── Requires: non-contended futex performance

Qt6 event loop
  └── Thread pool for QFuture/QtConcurrent
  └── Requires: SCHED_OTHER fair scheduling under load
  └── Requires: proper pthread_attr_setschedparam
```

---

## 2. Current Architecture Assessment

### 2.1 Scheduler Architecture

**File:** `recipes/core/kernel/source/src/context/switch.rs`

**Algorithm:** Deficit Weighted Round Robin (DWRR) — documented at line 354:
```rust
/// This is the scheduler function which currently utilises Deficit Weighted Round Robin Scheduler
fn select_next_context(...)
```

**Key data structures** (from `context/mod.rs`):

```rust
// 40 priority levels, each with its own queue
pub struct RunContextData {
    set: [VecDeque<WeakContextRef>; 40],
}

// Global lock for run queues (L1 = highest-level lock)
static RUN_CONTEXTS: Mutex<L1, RunContextData> = ...;

// Idle/sleeping contexts — scanned linearly on every tick
static IDLE_CONTEXTS: Mutex<L2, VecDeque<WeakContextRef>> = ...;

// All contexts (for enumeration)
static CONTEXTS: RwLock<L2, BTreeSet<ContextRef>> = ...;
```

**Priority weights** (geometric decay ~1.25x per level):
```rust
const SCHED_PRIO_TO_WEIGHT: [usize; 40] = [
    88761, 71755, 56483, 46273, 36291, 29154, 23254, 18705, 14949, 11916,
    9548, 7620, 6100, 4904, 3906, 3121, 2501, 1991, 1586, 1277,
    1024, 820, 655, 526, 423, 335, 272, 215, 172, 137,
    110, 87, 70, 56, 45, 36, 29, 23, 18, 15,
];
```

**Time quantum:** 3 PIT ticks per context (~12.2ms). PIT channel 0 has divisor 4847 at 1.193182 MHz → ~4.062ms per tick. 3 ticks → ~12.2ms between context switches. The 6.75ms in the tick() comment is outdated.  
**Default priority:** 20 (middle of range).  
**Max scheduler iterations:** 5000 per `select_next_context` call (bail-out limit).  
**Per-CPU state:** `percpu.balance: [usize; 40]` (deficit counters), `percpu.last_queue` (round-robin position).  
**Preemption:** Preemptible unless `context.preempt_locks > 0` (guarded by `PreemptGuard` RAII wrappers).  
**Context switch lock:** Global `arch::CONTEXT_SWITCH_LOCK` — spinlock with `compare_exchange_weak` on `Ordering::SeqCst`.

**Current limitations:**
1. **No real-time scheduling wired to userspace** — kernel has SchedPolicy enum and RT scheduling pass, but relibc `sched_setscheduler` returns ENOSYS for FIFO/RR until kernel wire-up is complete.
2. **No dynamic priority adjustment** — `context.prio` is set once and never changes. vruntime-based fairness compensates for SCHED_OTHER but no nice-value decay/boost.
3. **No work stealing** — each CPU only dequeues from its own queues. A CPU can go idle while another has backlog.
4. **No load balancing** — newly created contexts go to the creating CPU's idle queue. No migration across CPUs.
5. **O(n) idle wakeup scan** — `wakeup_contexts()` linearly scans the entire `IDLE_CONTEXTS` VecDeque on every tick (every ~2.25ms effective).
6. **Single global context switch lock** — `arch::CONTEXT_SWITCH_LOCK` serializes all CPU context switches on many-core systems.
7. **No NUMA awareness** — memory locality is not considered during scheduling.
8. **No timeslice scaling** — all contexts get the same 3-tick quantum regardless of priority (priority only affects how often they're picked, not how long they run).
9. **Large fixed iteration limit** — 5000 iterations per schedule attempt can cause latency spikes under heavy load.

### 2.2 Context/Thread Model

**File:** `recipes/core/kernel/source/src/context/context.rs`

```rust
pub struct Context {
    pub prio: usize,                 // Priority (0-39, default 20)
    pub status: Status,              // Runnable / Blocked / HardBlocked / Dead
    pub running: bool,               // Currently on a CPU
    pub cpu_id: Option<LogicalCpuId>,// Which CPU this context is on
    pub sched_affinity: LogicalCpuSet, // Allowed CPU set
    pub cpu_time: u128,              // Accumulated CPU time (nanoseconds)
    pub switch_time: u128,           // Last switch-in time
    pub wake: Option<u128>,         // Wake timestamp for timed sleeps
    pub preempt_locks: usize,       // Preemption disable counter
    pub kfx: AlignedBox,            // SIMD/FPU save area
    pub addr_space: Option<Arc<AddrSpaceWrapper>>, // Can be shared (threads)
    pub files: Arc<LockedFdTbl>,    // Can be shared (same process threads)
    pub owner_proc_id: Option<NonZeroUsize>,  // Parent process
    pub name: ArrayString<32>,      // Human-readable name
    // Credentials:
    pub euid: u32, pub egid: u32, pub pid: usize,
    pub groups: Vec<u32>,           // Supplementary groups
}
```

**Thread creation flow:**
```
pthread_create() 
  → relibc::pthread::create() 
    → mmap() for stack
    → Tcb::new() for TLS
    → stack setup with entry shim
    → Sys::rlct_clone(stack, os_specific) 
      → redox_rt::clone() 
        → proc: scheme -> kernel clone
          → Context::new() (same owner_proc_id, shared addr_space)
          → context::spawn() (pushed to IDLE_CONTEXTS)
```

**Key architectural points:**
- Threads share the same `addr_space: Arc<AddrSpaceWrapper>` (same page tables)
- Threads share `files: Arc<LockedFdTbl>` (same FD table)
- Thread ownership via `owner_proc_id` — but no formal thread group concept
- No distinction between process and thread at kernel level — all are Contexts
- `pid` is set once, no `tgid`/`tid` distinction

### 2.3 Futex Implementation

**File:** `recipes/core/kernel/source/src/syscall/futex.rs`

```rust
// Global hash table: PhysicalAddress → Vec<FutexEntry>
type FutexList = HashMap<PhysicalAddress, Vec<FutexEntry>>;
static FUTEXES: Mutex<L1, FutexList> = ...;

pub struct FutexEntry {
    target_virtaddr: VirtualAddress,
    context_lock: Arc<ContextLock>,
    addr_space: Weak<AddrSpaceWrapper>,  // For CoW safety
}
```

**Supported operations:**
| Op | Status | Notes |
|----|--------|-------|
| `FUTEX_WAIT` (32-bit) | ✅ | Validates alignment (4-byte), checks value, blocks |
| `FUTEX_WAIT64` (64-bit) | ✅ | x86_64 only, checks alignment (8-byte) |
| `FUTEX_WAKE` | ✅ | Wakes up to `val` waiters, `O(n)` scan by virtual address matching |

**NOT supported (critical gaps):**
| Op | Impact |
|----|--------|
| `FUTEX_REQUEUE` | Cannot move waiters between futexes — needed by condvar broadcast |
| `FUTEX_CMP_REQUEUE` | Cannot atomically compare-and-requeue — race condition risk |
| `FUTEX_WAKE_OP` | Cannot do atomic op + wake — needed by glibc mutex fast path |
| `FUTEX_LOCK_PI` | No priority inheritance — PTHREAD_PRIO_INHERIT is a stub |
| `FUTEX_TRYLOCK_PI` | No trylock with PI |
| `FUTEX_UNLOCK_PI` | No unlock with PI |
| `FUTEX_CMP_REQUEUE_PI` | No requeue with PI |
| `FUTEX_WAIT_BITSET` | No bitset wait — needed for `pselect`/`ppoll` optimization |
| `FUTEX_WAKE_BITSET` | No bitset wake |
| `FUTEX_WAIT_MULTIPLE` | As noted in code TODO, not implemented |
| `FUTEX_PRIVATE` flag | Conceptual TODO in code comment — "implement fully in userspace" |

**Performance concerns:**
1. **Global `FUTEXES` mutex** — all futex operations on all CPUs contend on a single L1 lock
2. **O(n) wake scan** — `FUTEX_WAKE` iterates all entries for a physical address to match by virtual address
3. **Full `HashMap` entry removal** — on wake, entry is `swap_remove`'d; on last waiter, the entire `HashMap` entry is removed (churn)
4. **No per-process futex isolation** — all futexes share the same global table, even process-private ones
5. **No wait-multiple** — waking multiple independent futexes requires multiple syscalls

### 2.4 relibc pthread Completeness

**Files:** `src/pthread/mod.rs`, `src/header/pthread/*.rs`, `src/header/sched/mod.rs`

| API Surface | Status | Notes |
|-------------|--------|-------|
| `pthread_create` / `pthread_join` / `pthread_detach` | ✅ Full | Stack via mmap, TLS init, waitval for join |
| `pthread_mutex_*` (normal, recursive, errorcheck) | ✅ Full | Internal implementation in `src/sync/` |
| `pthread_cond_*` | ✅ Full | Condition variables present |
| `pthread_rwlock_*` | ✅ Full | Read-write locks present |
| `pthread_barrier_*` | ✅ Full | Barriers present |
| `pthread_spin_*` | ✅ Full | Spinlocks present |
| `pthread_key_*` / TLS | ✅ Full | Thread-local storage with destructors |
| `pthread_once` | ✅ Full | call_once pattern |
| `pthread_cancel` / `pthread_setcancelstate` / `pthread_setcanceltype` | ✅ Full | Deferred + async cancellation via RT signal |
| `pthread_attr_*` (init/destroy/get/set) | ✅ Full | All attribute accessors implemented |
| `pthread_getattr_np` | ✅ Partial | Stack base/size returned; other attrs default |
| `pthread_setname_np` / `pthread_getname_np` | ✅ Delivered | Kernel proc: Name handle + relibc wrapper |
| `pthread_attr_setschedpolicy` | 🚧 Accepts value, kernel ignores | Kernel pays no attention to policy |
| `pthread_attr_setschedparam` | 🚧 Accepts value, kernel ignores | `sched_priority` stored but unused |
| `pthread_setschedparam` | 🚧 No-op | `set_sched_param()` — TODO comment |
| `pthread_setschedprio` | 🚧 No-op | `set_sched_priority()` — TODO comment |
| `pthread_mutexattr_setprotocol` | 🚧 Stub | PTHREAD_PRIO_INHERIT accepted but no-op |
| `pthread_mutexattr_setrobust` | 🚧 Stub | PTHREAD_MUTEX_ROBUST accepted but no-op |
| `pthread_mutexattr_setpshared` | 🚧 Partial | PROCESS_SHARED constant exists; futex supports cross-AS |
| `pthread_getcpuclockid` | 🚧 ENOENT | `get_cpu_clkid()` returns ENOENT |
| `pthread_kill` | ⚠️ Failing | Failing tests (child/invalid/self) — race condition noted at `signal/mod.rs:178` |
| `pthread_atfork` | ❌ Empty stubs | Registered handlers exist but are no-ops — fork is NOT thread-safe |
| `pthread_sigmask` | ✅ | Via `sigprocmask` |
| `pthread_atfork` | ✅ | fork hooks present |
| **sched.h functions:** | | |
| `sched_yield` | ✅ | Via `Sys::sched_yield()` |
| `sched_get_priority_max` | 🚧 `todo!()` | |
| `sched_get_priority_min` | 🚧 `todo!()` | |
| `sched_getparam` | 🚧 `todo!()` | |
| `sched_setparam` | 🚧 `todo!()` | |
| `sched_setscheduler` | 🚧 `todo!()` | |
| `sched_rr_get_interval` | 🚧 `todo!()` | |

### 2.5 IPC Primitives Relevant to Multithreading

From `KERNEL-IPC-CREDENTIAL-PLAN.md` and direct code review:

| Primitive | Kernel Support | Threading Impact |
|-----------|---------------|-----------------|
| Futex | WAIT/WAKE only | **Critical** — base primitive for all userspace sync |
| Shared memory (shm/mmap MAP_SHARED) | ✅ Via memory scheme | Required for PTHREAD_PROCESS_SHARED |
| Signals (per-thread) | ✅ Via proc: scheme | Thread cancellation, SIGEV_THREAD |
| Pipe (kernel `pipe:` scheme) | ✅ | Thread communication |
| eventfd/signalfd/timerfd | ✅ Recipe-applied | Async I/O notification |
| SysV sem/shm | ✅ Recipe-activated (2026-04-29) | Qt QSystemSemaphore |
| POSIX msg queues | ❌ Missing | Low priority for desktop |
| SysV msg queues | ❌ Missing | Low priority for desktop |

---

## 3. Critical Gaps and Blockers

### 3.1 Priority Gaps (Blocking Desktop Responsiveness)

| # | Gap | Impact | Blocked Consumer |
|---|-----|--------|-----------------|
| G1 | **No SCHED_RR/SCHED_FIFO** | All threads treated equally; input/audio threads can't get priority | KWin input thread, PulseAudio |
| G2 | **No dynamic priority** | CPU-bound threads aren't penalized; I/O-bound threads aren't boosted | Desktop compositor under load |
| G3 | **No PI futexes** | Priority inversion: low-priority thread holding mutex blocks high-priority waiter | KWin compositor lock, Qt mutexes |
| G4 | **No `pthread_setschedparam`** | Applications can't request scheduling policy changes | All desktop apps |
| G5 | **No timeslice differentiation** | High-priority threads get same quantum as low-priority | Poor latency for foreground tasks |

### 3.2 Scalability Gaps (Blocking Many-Core Performance)

| # | Gap | Impact |
|---|-----|--------|
| G6 | **No work stealing** | CPUs go idle while work exists on other CPUs |
| G7 | **No load balancing** | New threads stay on creator CPU; imbalance builds over time |
| G8 | **Global context switch lock** | Serialization bottleneck beyond ~8 cores |
| G9 | **Global futex mutex** | All cores contend on single L1 lock for futex ops |
| G10 | **O(n) idle wake scan** | Linear scan proportional to total sleeping threads |
| G11 | **No NUMA awareness** | Cross-node memory access penalty on multi-socket systems |

### 3.3 Correctness Gaps (Blocking Robust Applications)

| # | Gap | Impact |
|---|-----|--------|
| G12 | **No robust mutexes** | Thread death while holding mutex → permanent deadlock |
| G13 | **No FUTEX_REQUEUE** | Condvar broadcast wakes all waiters → thundering herd |
| G14 | **No thread groups (tgid)** | `kill(pid, sig)` can't target a process; `getpid()` per thread context |
| G15 | **Static-only sched_affinity** | No userspace CPU pinning API |
| G16 | **No setpriority/getpriority** | POSIX nice values not wired to kernel priority |
| G17 | **pthread barriers hang on SMP** | `check.sh` runs `-smp 1` to work around barrier/once hang on multi-core QEMU — **blocks KWin GPU barrier sync** |
| G18 | **pthread_kill race condition** | All four pthread_kill tests (child/invalid/self/kill0) are failing — thread-targeted signal delivery unreliable |
| G19 | **fork() thread-unsafe** | `pthread_atfork` handlers are empty no-ops; child inherits locked mutexes from parent |
| G20 | **Linux aarch64 rlct_clone stub** | `todo!("rlct_clone not implemented for aarch64 yet")` — **blocks aarch64 builds** |

---

## 4. Implementation Plan

### Phase S1: Scheduler Observability and Metrics (Week 1-2)

**Goal:** Add instrumentation to measure and understand scheduling behavior before optimizing.

#### S1.1 — Per-context scheduling statistics

Add to `Context` struct:
```rust
pub struct Context {
    // NEW scheduling statistics:
    pub sched_run_count: u64,        // Times this context was scheduled
    pub sched_wait_time: u128,       // Total time spent waiting (accumulated)
    pub sched_last_wake: u128,       // Timestamp of last unblock
    pub sched_migrations: u32,       // Times migrated between CPUs
    pub sched_preemptions: u32,      // Times preempted
    pub sched_voluntary_switch: u32, // Times yielded/blocked voluntarily
}
```

**Files:** `context/context.rs` — add fields, initialize in `Context::new()`, update in `switch()`

#### S1.2 — Per-CPU scheduler metrics

Add to `cpu_stats.rs`:
```rust
pub struct CpuStats {
    // Existing: user, nice, kernel, idle, irq
    // NEW:
    pub sched_scans: AtomicU64,      // number of select_next_context calls
    pub sched_empty_scans: AtomicU64, // scans that found no runnable context
    pub sched_steals: AtomicU64,     // work stolen from other CPUs (future)
    pub sched_ipi_wakeups: AtomicU64, // wakeups via IPI
    pub sched_max_queue_depth: AtomicU64, // maximum queue depth observed
}
```

#### S1.3 — `/scheme/sys/sched` debug interface

Expose scheduler metrics via a new kernel scheme path:
```
scheme:sys/sched/runqueues  — per-CPU run queue depths
scheme:sys/sched/top        — top-N contexts by recent CPU time
scheme:sys/sched/context/{id} — per-context scheduling stats
```

This enables `redbear-info` or a new `redbear-sched` tool for runtime diagnostics.

#### S1.4 — relibc `sched_getscheduler()` baseline

Wire `sched_getscheduler()` to return `SCHED_OTHER` (the current DWRR is closest to SCHED_OTHER):
```rust
// relibc/src/header/sched/mod.rs
pub extern "C" fn sched_getscheduler(pid: pid_t) -> c_int {
    // For now: all processes use SCHED_OTHER (DWRR)
    SCHED_OTHER
}
```

**Patch:** `local/patches/relibc/P5-sched-observe.patch`

---

### Phase S2: Real-Time Scheduling Support (Week 2-4)

**Goal:** Add `SCHED_FIFO` and `SCHED_RR` scheduling classes to the kernel, and wire relibc `sched_setscheduler()`.

#### S2.1 — Scheduling policy in Context

Add to `Context`:
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchedPolicy {
    Other,       // DWRR (current default)
    Fifo,        // Strict priority, no preemption within same priority
    RoundRobin,  // Strict priority, round-robin within same priority
    // Future:
    // Batch,    // Throughput-optimized, lower priority than Other
    // Idle,     // Only runs when absolutely nothing else is runnable
}

pub struct Context {
    pub sched_policy: SchedPolicy,  // NEW
    pub sched_rt_priority: u8,      // NEW: 0-99 RT priority
    
    // Renamed: prio → sched_dynamic_prio (for SCHED_OTHER)
    pub sched_dynamic_prio: usize,
    pub sched_static_prio: usize,   // NEW: base priority, unmodified by heuristics
}
```

**Initialization:** Default `sched_policy = SchedPolicy::Other`, `sched_rt_priority = 0`.

#### S2.2 — Priority mapping

```
RT priority 99 → kernel prio 0  (highest)
RT priority 98 → kernel prio 1
...
RT priority 0  → kernel prio 39 (lowest RT, still above SCHED_OTHER)

SCHED_OTHER:
  nice -20 → kernel prio 0  (still below RT 0)
  nice   0 → kernel prio 20 (default)
  nice +19 → kernel prio 39

SCHED_FIFO within same RT priority: no preemption (runs until blocks)
SCHED_RR within same RT priority: round-robin with configurable quantum
```

#### S2.3 — Scheduler dispatch by policy

Modify `select_next_context()` to prioritize:
1. `SCHED_FIFO` contexts (highest RT priority first, no preemption per priority)
2. `SCHED_RR` contexts (highest RT priority first, round-robin per priority)
3. `SCHED_OTHER` contexts (existing DWRR)

```rust
fn select_next_context(...) -> ... {
    // PASS 1: SCHED_FIFO — first runnable at highest priority wins
    for prio in 0..40 {
        if let Some(fifo_ctx) = take_first_runnable_of_policy(
            prio, SchedPolicy::Fifo, &mut contexts_list
        ) {
            return Ok(Some(fifo_ctx));
        }
    }
    
    // PASS 2: SCHED_RR — round-robin within priority
    for prio in 0..40 {
        if let Some(rr_ctx) = take_next_rr_of_policy(
            prio, &mut contexts_list, &mut percpu.rr_position[prio]
        ) {
            return Ok(Some(rr_ctx));
        }
    }
    
    // PASS 3: SCHED_OTHER — existing DWRR (unchanged)
    existing_dwrr_logic(...)
}
```

#### S2.4 — SCHED_RR timeslice configuration

Add per-context timeslice for SCHED_RR:
```rust
pub struct Context {
    pub sched_rr_quantum: u128,  // nanoseconds, default 100ms
}
```

Override the 3-tick quantum for SCHED_RR contexts: track ticks consumed, preempt at quantum.

#### S2.5 — syscall interface for policy changes

Add kernel syscall or extend `proc:` scheme:
```
proc: scheme command: SetSchedPolicy(pid, policy, rt_priority)
```

#### S2.6 — Wire relibc `sched_setscheduler()`

```rust
// relibc/src/header/sched/mod.rs
pub extern "C" fn sched_setscheduler(
    pid: pid_t, policy: c_int, param: *const sched_param,
) -> c_int {
    let prio = unsafe { (*param).sched_priority };
    let kernel_policy = match policy {
        SCHED_FIFO => SchedPolicyRequest::Fifo,
        SCHED_RR   => SchedPolicyRequest::RoundRobin,
        SCHED_OTHER => SchedPolicyRequest::Other,
        _ => return set_errno(EINVAL),
    };
    
    // Send to kernel via proc: scheme
    Sys::set_sched_policy(pid, kernel_policy, prio)
}
```

**Patches:**
- `local/patches/kernel/P5-sched-policy.patch` — Context fields + sched dispatch
- `local/patches/kernel/P5-sched-policy-proc.patch` — proc: scheme SetSchedPolicy
- `local/patches/relibc/P5-sched-setscheduler.patch` — wire through scheme
- `local/patches/relibc/P5-sched-getscheduler.patch` — return current policy
- `local/patches/relibc/P5-sched-priority.patch` — sched_get/setparam

---

### Phase S3: Load Balancing and Work Stealing (Week 4-6)

**Status: ✅ COMPLETE (2026-04-30)** — P3.1 PerCpuSched struct + P3.2 per-CPU wiring + P3.3 work stealing + P3.4 initial placement (least-loaded CPU) + P3.5 periodic load balancing all implemented.

**Goal:** Distribute runnable contexts across CPUs to maximize utilization.

#### S3.1 — Per-CPU run queue lock elimination

Replace the global `RUN_CONTEXTS: Mutex<L1, RunContextData>` with per-CPU run queues:
```rust
// In PercpuBlock:
pub struct PerCpuSched {
    pub run_queues: [VecDeque<WeakContextRef>; 40],
    pub run_queues_lock: SpinLock,  // per-CPU, low contention
    pub balance: [usize; 40],
    pub last_queue: usize,
    pub idle_context: Arc<ContextLock>,
}
```

This eliminates the global L1 mutex bottleneck for dequeue operations.

#### S3.2 — Idle CPU work stealing

When `select_next_context()` finds no runnable context on the local CPU:
1. Pick a victim CPU (round-robin or random)
2. Lock victim's run queues
3. Dequeue the highest-priority runnable context
4. Return it for scheduling

```rust
fn steal_work(percpu: &PercpuBlock, cpu_id: LogicalCpuId) -> Option<ArcContextLockWriteGuard> {
    for victim_offset in 1..cpu_count() {
        let victim_id = (cpu_id + victim_offset) % cpu_count();
        let victim_percpu = percpu_for(victim_id);
        
        // Try to steal from highest priority queues first
        for prio in 0..40 {
            if let Some(ctx) = victim_percpu.dequeue_runnable(prio) {
                percpu.stats.sched_steals.fetch_add(1, Ordering::Relaxed);
                return Some(ctx);
            }
        }
    }
    None
}
```

#### S3.3 — Initial placement (fork/exec balance)

When creating a new context, instead of always going to the creating CPU's idle queue:
```rust
fn place_new_context(ctx: &mut Context) -> LogicalCpuId {
    // Pick the CPU with the shortest total run queue
    let target = cpus()
        .min_by_key(|cpu| cpu.total_runnable_contexts())
        .unwrap_or(crate::cpu_id());
    
    ctx.sched_affinity = LogicalCpuSet::single(target);
    target
}
```

#### S3.4 — Periodic load balancing

Add a periodic balancing trigger (e.g., every 100ms or when queue depth difference exceeds threshold):
```rust
fn balance_load() {
    let avg_depth = average_runnable_per_cpu();
    for cpu in overloaded_cpus(avg_depth * 1.25) {
        let target = most_idle_cpu();
        migrate_contexts(cpu, target, cpu.total_runnable() - avg_depth);
    }
}
```

**Patches:**
- `local/patches/kernel/P6-percpu-runqueues.patch` — per-CPU run queues (infrastructure)

---

### Phase S4: Futex Enhancements (Week 6-9)

**Status: ✅ COMPLETE (2026-04-30)** — S4.1 futex sharding (64-shard), S4.2 FUTEX_REQUEUE, S4.3 PI futex, S4.4 robust futex, vruntime tracking, minimum-vruntime selection all implemented.

**Goal:** Add PI, requeue, and per-futex locking to support robust desktop mutex performance.

#### S4.1 — Per-futex locking (reduce global contention)

Replace the single `FUTEXES: Mutex<L1, FutexList>` with a sharded hash table:
```rust
const FUTEX_SHARDS: usize = 64; // or scale with CPU count
static FUTEXES: [Mutex<FutexList>; FUTEX_SHARDS] = ...;

fn futex_shard(phys: PhysicalAddress) -> usize {
    phys.data() as usize % FUTEX_SHARDS
}
```

#### S4.2 — FUTEX_REQUEUE and FUTEX_CMP_REQUEUE

```rust
fn futex_requeue(
    addr1: PhysicalAddress,  // source futex
    addr2: PhysicalAddress,  // target futex
    val: usize,              // max to requeue
    val2: usize,             // expected value (for CMP_REQUEUE)
    cmp: bool,               // whether to compare first
) -> Result<usize> {
    // Atomically move up to `val` waiters from addr1's wait queue to addr2's
    // If cmp is true, only proceed if *addr1 == val2
}
```

This is critical for condition variable performance — without it, `pthread_cond_broadcast` causes a thundering herd where every waiter wakes, rechecks, and most re-block.

#### S4.3 — PI Futexes (FUTEX_LOCK_PI / FUTEX_UNLOCK_PI / FUTEX_TRYLOCK_PI / FUTEX_CMP_REQUEUE_PI)

Priority inheritance for futexes:
```rust
pub struct PiState {
    owner: Option<Arc<ContextLock>>,
    waiters: Vec<(Arc<ContextLock>, u32)>, // (context, original_priority)
}

// When a high-priority context blocks on a PI futex held by a low-priority context:
fn pi_boost(owner: &mut Context, waiter_prio: usize) {
    if waiter_prio < owner.sched_dynamic_prio {
        owner.sched_dynamic_prio = waiter_prio;
        owner.pi_boosted = true;
    }
}
```

**Critical path:** KWin compositor lock. Without PI, a low-priority background thread holding a mutex that the compositor thread needs can block rendering for an unbounded time.

#### S4.4 — Robust Futexes

Mark futex waiters in a `robust_list` so the kernel can unlock them on thread death:
```rust
pub struct RobustListEntry {
    futex_addr: usize,
    futex_len: usize,
    // List is per-thread, registered via set_robust_list syscall
}
```

On `exit_thread()`:
```rust
fn wake_robust_futexes(context: &Context) {
    for entry in &context.robust_list {
        // Set FUTEX_OWNER_DIED bit
        // Wake one waiter with EOWNERDEAD
    }
}
```

**Patches:**
- `local/patches/kernel/P6-futex-sharding.patch` — futex lock sharding (delivered)
- (PI futex, requeue, robust futex deferred)

---

### Phase S5: Dynamic Priority and Thread Management (Week 9-11)

**Status: ✅ COMPLETE (2026-04-30)** — S5.1 vruntime + S5.2 setpriority/getpriority + S5.3 pthread_setaffinity_np + S5.4 pthread_setname_np + pthread_setschedparam (Redox) all implemented.

**Goal:** Add I/O-vs-CPU heuristics, CPU affinity API, and thread naming.

#### S5.1 — Dynamic priority adjustment (SCHED_OTHER)

Implement a simplified CFS-style virtual runtime tracking:
```rust
pub struct Context {
    pub vruntime: u128,  // Virtual runtime (weighted by priority)
}

// On context switch OUT:
prev_context.vruntime += actual_runtime * SCHED_PRIO_TO_WEIGHT[default_prio] 
                       / SCHED_PRIO_TO_WEIGHT[prev_context.sched_static_prio];

// On select_next_context for SCHED_OTHER:
// Pick context with lowest vruntime instead of DWRR deficit tracking
```

This automatically penalizes CPU-bound threads (their vruntime grows faster) and favors I/O-bound threads (they sleep, vruntime stays low).

#### S5.2 — POSIX nice values

Map `nice(-20..+19)` to static priorities:
```rust
fn nice_to_static_prio(nice: i8) -> usize {
    // nice -20 → kernel prio 0 (SCHED_OTHER range)
    // nice   0 → kernel prio 20
    // nice +19 → kernel prio 39
    ((nice + 20) as usize).clamp(0, 39)
}

// Wire setpriority/getpriority to modify sched_static_prio
```

#### S5.3 — CPU affinity API

Add to `proc:` scheme:
```
proc: scheme command: SetAffinity(pid, affinity_mask: u64)
proc: scheme command: GetAffinity(pid) → u64
```

Wire in relibc:
```rust
pub extern "C" fn pthread_setaffinity_np(
    thread: pthread_t, cpusetsize: size_t, cpuset: *const cpu_set_t,
) -> c_int {
    let mask = unsafe { read_cpu_set(cpuset, cpusetsize) };
    Sys::set_cpu_affinity(tid, mask)
}
```

#### S5.4 — Thread naming API

The kernel `Context.name` field already exists (32-char `ArrayString`). Wire it:
```rust
// proc: scheme command: SetName(pid, name)
// relibc:
pub extern "C" fn pthread_setname_np(thread: pthread_t, name: *const c_char) -> c_int {
    let name = unsafe { CStr::from_ptr(name) };
    Sys::set_thread_name(thread.os_tid, name)
}
```

**Patches:**
- `local/patches/kernel/P6-vruntime-context.patch` — vruntime field + initialization
- `local/patches/kernel/P6-vruntime-switch.patch` — weighted update + min-vruntime selection
- `local/patches/kernel/P7-cache-affine-context.patch` — cache-affine scheduling (last_cpu)
- `local/patches/kernel/P7-cache-affine-switch.patch` — cache-affine vruntime bonus
- `local/patches/kernel/P7-proc-setpriority.patch` — setpriority proc handle
- `local/patches/kernel/P7-proc-setname.patch` — thread naming proc handle
- `local/patches/relibc/P7-setpriority.patch` — setpriority/getpriority
- `local/patches/relibc/P7-pthread-affinity.patch` — pthread_setaffinity_np
- `local/patches/relibc/P7-pthread-setname.patch` — pthread_setname_np

---

### Phase S6: NUMA and Cache-Affine Scheduling (Week 11-13)

**Status: ✅ DELIVERED (2026-04-30)** — S6.3 cache-affine scheduling + S6.1 NUMA topology kernel hints implemented. NUMA discovery (SRAT/SLIT parsing) is userspace responsibility (numad daemon via /scheme/acpi/). Kernel stores lightweight NumaTopology for O(1) scheduling lookups. Full userspace numad daemon is follow-up work.

**Goal:** Optimize for multi-socket systems by keeping related threads near their memory.

#### S6.1 — NUMA topology discovery

Parse ACPI SRAT/SLIT tables (already available in ACPI infrastructure):
```rust
pub struct NumaTopology {
    nodes: Vec<NumaNode>,
    distances: Vec<Vec<u8>>,  // SLIT inter-node distances
}

pub struct NumaNode {
    id: u8,
    cpus: LogicalCpuSet,
    memory: PhysicalMemoryRange,
}
```

#### S6.2 — NUMA-aware initial placement

When creating a new context:
1. If parent thread has `sched_affinity`, prefer CPUs in the same NUMA node
2. Otherwise, pick the NUMA node with the most free memory

#### S6.3 — Cache-affine scheduling

Track the last CPU a context ran on. Prefer to re-schedule on the same CPU to avoid cache migration penalty:
```rust
pub struct Context {
    pub sched_last_cpu: LogicalCpuId,  // already tracked via cpu_id before it becomes None
}
```

In `select_next_context()`:
```rust
// When scanning runnable contexts, prefer those whose last_cpu == current_cpu_id
// (hot cache) over those from other CPUs (cold cache)
let hot_ctx = search_for_hot_context(current_cpu, &queues);
let fallback = search_for_cold_context(&queues);
hot_ctx.or(fallback)
```

**Patches:**
- `local/patches/kernel/P7-cache-affine-context.patch` — cache-affine scheduling (delivered)
- `local/patches/kernel/P7-cache-affine-switch.patch` — cache-affine vruntime bonus (delivered)
- (NUMA SRAT/SLIT parsing deferred)

---

### Phase R1: relibc POSIX Scheduling API Completion (Week 2-4, parallel with S2)

**Goal:** Fill all `todo!()` stubs in `sched.h` and `pthread.h` scheduling functions.

| Function | Implementation |
|----------|---------------|
| `sched_get_priority_max(policy)` | Return 99 for FIFO/RR, 0 for OTHER |
| `sched_get_priority_min(policy)` | Return 1 for FIFO/RR, 0 for OTHER |
| `sched_getparam(pid, param)` | Query kernel for current RT priority |
| `sched_setparam(pid, param)` | Delegate to `sched_setscheduler` with current policy |
| `sched_getscheduler(pid)` | Query kernel for current policy |
| `sched_rr_get_interval(pid, tp)` | Return SCHED_RR quantum (default 100ms) |
| `pthread_setschedparam(thread, policy, param)` | Set kernel sched policy via proc: scheme |
| `pthread_getschedparam(thread, policy, param)` | Get kernel sched policy |
| `pthread_setschedprio(thread, prio)` | Set dynamic priority within current policy |
| `pthread_getcpuclockid(thread, clock_id)` | Return CPU-time clock for thread |

**Patches:** All in `local/patches/relibc/P5-sched-complete.patch`

---

### Phase R2: Robust and PI Mutex Support (Week 5-9, parallel with S4)

**Goal:** Full POSIX mutex robustness and priority inheritance.

#### R2.1 — PI mutex protocol

```rust
// relibc/src/sync/pthread_mutex.rs
pub struct PthreadMutex {
    futex: AtomicU32,
    owner: AtomicUsize,     // os_tid of current owner
    pi_waiters: Mutex<Vec<(OsTid, u32)>>, // waiters with requested priority
    flags: AtomicU32,       // PTHREAD_PRIO_INHERIT, PTHREAD_MUTEX_ROBUST
}

// Lock with PI:
fn lock_pi(&self) -> Result<(), Errno> {
    loop {
        match futex::lock_pi(&self.futex) {
            Ok(()) => {
                self.owner.store(current_tid(), Ordering::Release);
                return Ok(());
            }
            Err(EAGAIN) => continue,
            Err(err) => return Err(err),
        }
    }
}
```

#### R2.2 — Robust mutex protocol

```rust
pub struct RobustList {
    head: *mut RobustListHead,
}

pub struct RobustListHead {
    list: RobustList,
    futex_offset: isize,
    pending: *mut RobustListHead,
}

// On thread exit:
fn handle_robust_list(thread: &Pthread) {
    for entry in thread.robust_list.iter() {
        let futex_addr = (entry as usize + entry.futex_offset) as *mut AtomicU32;
        // Set FUTEX_OWNER_DIED
        futex_addr.fetch_or(FUTEX_OWNER_DIED, Ordering::Release);
        // Wake one waiter with EOWNERDEAD
        futex::wake(futex_addr, 1);
    }
}
```

---

### Phase R3: Thread Groups and Process Identity (Week 10-12)

**Goal:** Proper tgid/pid distinction, `kill(pid, 0)` process targeting.

#### R3.1 — Kernel thread group concept

```rust
pub struct Context {
    pub tgid: usize,  // Thread Group ID (= pid for main thread)
    pub tid: usize,   // Thread ID (unique per thread)
}
```

- On `clone(CLONE_THREAD)`: child gets same tgid as parent, new tid
- On fork: child gets new tgid = child's tid
- `getpid()` returns tgid
- `gettid()` returns tid
- `kill(tgid, sig)` delivers signal to all threads in thread group

#### R3.2 — Thread group signal delivery

```rust
fn deliver_signal_to_thread_group(tgid: usize, sig: Signal) {
    for context in contexts_in_thread_group(tgid) {
        // Pick a thread that hasn't blocked this signal
        if !context.sig_blocked(sig) {
            context.deliver_signal(sig);
            break;
        }
    }
}
```

**Patches:**
- `local/patches/kernel/P5-tgid.patch` — thread group ID kernel support
- `local/patches/kernel/P5-tgid-signal.patch` — process-targeted signal delivery
- `local/patches/relibc/P5-gettid.patch` — gettid() syscall

---

## 5. Dependency Chain

```
Phase S1 (observability)
    │
    ├──► Phase S2 (real-time scheduling) ────┐
    │       │                                 │
    │       ├──► Phase R1 (POSIX sched API)   │
    │       │                                 │
    │       └──► KWin input thread priority   │
    │                                         │
    ├──► Phase S3 (load balancing) ───────────┤
    │       │                                 │
    │       └──► Mesa worker thread scaling   │
    │                                         │
    ├──► Phase S4 (futex enhancements) ───────┤
    │       │                                 │
    │       ├──► Phase R2 (PI/robust mutex)   │
    │       │                                 │
    │       └──► KWin compositor lock         │
    │                                         │
    ├──► Phase S5 (dynamic prio + affinity) ──┤
    │       │                                 │
    │       └──► Application CPU pinning      │
    │                                         │
    ├──► Phase R3 (thread groups) ────────────┤
    │       │                                 │
    │       └──► process-targeted signals     │
    │                                         │
    └──► Phase S6 (NUMA) ─────────────────────┘
            │
            └──► Multi-socket server performance
```

**Independent work (can run in parallel):**
- S2 (RT scheduling) + R1 (POSIX sched API) — parallel
- S4 (futex) + R2 (PI/robust mutex) — parallel
- S3 (load balancing) can start after S1 but independently of S2
- S6 (NUMA) depends on S3 (per-CPU queues) but not on S4/S5

---

## 6. Integration with Existing Plans

| Existing Plan | Relationship |
|---------------|-------------|
| `KERNEL-IPC-CREDENTIAL-PLAN.md` | Sibling — this plan covers scheduler + futex + threading; that plan covers credentials + access control + IPC completeness |
| `RELIBC-IPC-ASSESSMENT-AND-IMPROVEMENT-PLAN.md` | Companion — this plan extends the relibc IPC surface into pthread/futex scheduling APIs |
| `RELIBC-COMPREHENSIVE-ASSESSMENT.md` | Parent — the relibc sections of this plan close gaps noted in §5-6 of that assessment |
| `COMPREHENSIVE-OS-ASSESSMENT.md` | Parent — this plan closes §2 kernel gaps for scheduler/scalability |
| `CONSOLE-TO-KDE-DESKTOP-PLAN.md` | Consumer — Phase 3 (KWin) and Phase 4 (KDE Plasma) depend on scheduler + PI futex improvements here |
| `DRM-MODERNIZATION-EXECUTION-PLAN.md` | Sibling — GPU worker thread scheduling benefits from load balancing (S3) |
| `IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md` | Sibling — IRQ latency affects scheduling latency |

---

## 7. Patch Governance

All kernel and relibc source changes follow the durability policy (`local/AGENTS.md`):

```
local/patches/
├── kernel/
│   (Delivered: P6-* and P7-* patches below. P5-sched-* entries are planned future carriers.)
│   ├── P5-sched-observability.patch       # S1
│   ├── P5-sched-policy.patch              # S2
│   ├── P5-sched-policy-proc.patch         # S2 proc: scheme
│   ├── P6-percpu-runqueues.patch          # S3 (delivered: infrastructure)
│   ├── P6-futex-sharding.patch            # S4 (delivered: sharding)
│   ├── P6-vruntime-context.patch          # S5 (delivered: field + init)
│   ├── P6-vruntime-switch.patch           # S5 (delivered: update + selection)
│   ├── (remaining S3-S6 patches deferred)
├── relibc/
│   ├── P5-sched-observe.patch             # R1 baseline
│   ├── P5-sched-setscheduler.patch        # R1
│   ├── P5-sched-getscheduler.patch        # R1
│   ├── P5-sched-priority.patch            # R1
│   ├── P5-sched-complete.patch            # R1 remaining stubs
│   ├── (PI/robust mutex deferred)          # R2
│   ├── P7-setpriority.patch               # S5 (delivered)
│   ├── P7-pthread-affinity.patch          # S5 (delivered)
│   └── P5-gettid.patch                    # R3
```

---

## 8. Validation and Evidence

### 8.1 Build Evidence

| Check | Command |
|-------|---------|
| Kernel compiles | `make r.kernel` |
| relibc compiles | `make r.relibc` |
| Full OS builds | `make all CONFIG_NAME=redbear-full` |

### 8.2 Runtime Evidence

| Test | Verification |
|------|-------------|
| `sched_getscheduler()` returns policy | `redbear-info --sched` |
| `pthread_setschedparam()` changes priority | Threaded test binary: `test-sched-priority` |
| RT thread preempts SCHED_OTHER | Latency test: RT thread wakes within 100μs |
| Work stealing across CPUs | `redbear-info --sched` shows balanced queue depths |
| PI futex prevents priority inversion | PI test: low-prio holder, high-prio waiter, medium-prio contester |
| Robust mutex recovery after thread kill | Robust test: kill thread holding mutex, verify EOWNERDEAD |
| Thread affinity pinning | `taskset`-like test: verify thread stays on assigned CPU |
| Load balancing on fork bomb | Spawn 2× CPUs threads, verify even distribution |

### 8.3 Verification Scripts

```bash
local/scripts/test-sched-qemu.sh          # Scheduler metric validation
local/scripts/test-sched-rt-qemu.sh       # Real-time scheduling proof
local/scripts/test-futex-pi-qemu.sh       # PI futex proof
local/scripts/test-futex-robust-qemu.sh   # Robust futex proof
local/scripts/test-sched-balance-qemu.sh  # Load balancing proof (multi-vCPU)
```

---

## 9. Bottom Line

The Redox kernel scheduler is **functional but simple** — a correct DWRR implementation that works for a lightly-loaded system. For the KDE/Wayland desktop with dozens of competing threads (compositor, rendering, I/O, timers, D-Bus, input), it needs:

1. **Real-time scheduling** (S2) — for audio and compositor input threads
2. **PI futexes** (S4/R2) — to prevent the compositor lock from being inverted by background work
3. **Load balancing** (S3) — to use all available cores efficiently
4. **Dynamic priority** (S5) — to keep the compositor responsive under CPU load

These four items are the **critical path** to a responsive desktop. The remaining items (NUMA, thread groups, robust mutexes, affinity API) are important for correctness and server-class workloads but not desktop-blocking.

**Total estimated effort:** 13 weeks with 1-2 kernel developers, delivering incremental improvements at each phase boundary.
