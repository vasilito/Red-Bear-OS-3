# Red Bear OS — Kernel, IPC, and Credential Syscalls Plan

**Date:** 2026-04-30
**Scope:** Kernel architecture, IPC infrastructure, credential syscalls, process isolation
**Implementation status:** Phases K1-K2, K4 ✅ complete. Phases K3, K5 deferred.
**Status:** This document is the canonical kernel + IPC plan, extending `local/docs/COMPREHENSIVE-OS-ASSESSMENT.md`

## 1. Purpose

This plan defines the implementation roadmap for kernel hardening, IPC improvements, and credential
syscall implementation in Red Bear OS. It is the **canonical kernel authority** superseding scattered
kernel guidance in other docs.

**Relationship to existing plans:**

| Document | Relationship |
|----------|-------------|
| `COMPREHENSIVE-OS-ASSESSMENT.md` | Parent: this plan extends §2 (Kernel & Core Infrastructure) |
| `IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md` | Sibling: IRQ/PCI/MSI-X — not duplicated here |
| `RELIBC-IPC-ASSESSMENT-AND-IMPROVEMENT-PLAN.md` | Companion: relibc IPC surface — this plan covers kernel side |
| `ACPI-IMPROVEMENT-PLAN.md` | Sibling: ACPI power/shutdown — relevant for §4 (shutdown robustness) |
| `CONSOLE-TO-KDE-DESKTOP-PLAN.md` | Consumer: desktop stack depends on kernel work here |

## 2. Current Architecture Assessment

### 2.1 Kernel Overview

The Redox microkernel (`recipes/core/kernel/source/`) is a ~20-40k LoC Rust microkernel. It runs in
ring 0 and provides:

- **12 kernel schemes**: `debug`, `event`, `memory`, `pipe`, `irq`, `time`, `sys`, `proc`, `serio`,
  `acpi`, `dtb`, `user` (userspace scheme wrapper)
- **~35 handled syscalls**: file I/O, memory mapping, process control, futex, time
- **Catch-all ENOSYS**: all unhandled syscall numbers return `ENOSYS`

```
recipes/core/kernel/source/src/
├── syscall/           # Syscall dispatch: mod.rs (handlers), fs.rs, process.rs, futex.rs, time.rs
│   └── mod.rs         # Main syscall() dispatch: 35 explicit match arms, _ => ENOSYS
├── scheme/            # Kernel schemes: debug, event, memory, pipe, irq, time, sys, proc, serio
│   ├── mod.rs         # Scheme trait definition, SchemeId, FileHandle types
│   ├── proc.rs        # Process manager scheme (fork, exec, signal, credential setting)
│   └── sys/           # System info scheme: context list, syscall debug, uname
├── context/           # Process/thread context management
│   ├── context.rs     # Context struct: euid, egid, pid, files, signals, addr_space
│   └── memory.rs      # Address space, grants, mmap implementation
├── memory/            # Physical/virtual memory management, page tables
└── sync/              # Locking primitives (RwLock, Mutex, CleanLockToken)
```

### 2.2 Syscall Dispatch Architecture

The kernel's `syscall()` function in `syscall/mod.rs` dispatches based on `a` (syscall number):

```rust
// From recipes/core/kernel/source/src/syscall/mod.rs (line 75)
match a {
    SYS_WRITE2 => file_op_generic_ext(..),
    SYS_WRITE  => sys_write(..),
    SYS_FMAP   => { .. },           // Anonymous or file-backed mmap
    SYS_READ2  => file_op_generic_ext(..),
    SYS_READ   => sys_read(..),
    SYS_FPATH  => file_op_generic(..),
    SYS_FSTAT  => fstat(..),
    SYS_DUP    => dup(..),
    SYS_DUP2   => dup2(..),
    SYS_SENDFD => sendfd(..),
    SYS_OPENAT => openat(..),
    SYS_UNLINKAT => unlinkat(..),
    SYS_CLOSE  => close(..),
    SYS_CALL   => call(..),         // Scheme IPC: send message to scheme
    SYS_FEVENT => fevent(..),       // Register event on fd
    SYS_YIELD  => sched_yield(..),
    SYS_NANOSLEEP => nanosleep(..),
    SYS_CLOCK_GETTIME => clock_gettime(..),
    SYS_FUTEX  => futex(..),
    SYS_MPROTECT => mprotect(..),
    SYS_MREMAP => mremap(..),
    // ... ~15 more file operations (fchmod, fchown, fcntl, flink, frename, ftruncate, fsync, etc.)
    _ => Err(Error::new(ENOSYS)),   // ← CATCH-ALL: all credential syscalls fall here
}
```

Syscall numbers come from the external `redox_syscall` crate (crates.io), not from the kernel tree.
The kernel consumes them via `use syscall::number::*`.

### 2.3 Credential Architecture (Current)

**Kernel Context struct** (`context/context.rs`):

```rust
pub struct Context {
    // Credential fields (initialized to 0):
    pub euid: u32,      // Effective user ID — used for scheme access control
    pub egid: u32,      // Effective group ID
    pub pid: usize,     // Process ID (set via proc scheme)

    // NOT present in kernel:
    //   ruid, suid     — real/saved UID (maintained in userspace redox-rt)
    //   rgid, sgid     — real/saved GID (maintained in userspace redox-rt)
    //   supplementary groups — not implemented anywhere

    // Access control interface:
    pub fn caller_ctx(&self) -> CallerCtx {
        CallerCtx { uid: self.euid, gid: self.egid, pid: self.pid }
    }
}
```

**Credential read path** (userspace, no kernel involvement):
```
getuid() → relibc::platform::redox::getuid()
  → redox_rt::sys::posix_getresugid()
    → reads local DYNAMIC_PROC_INFO { ruid, euid, suid, rgid, egid, sgid }
    → returns cached userspace values (NO kernel syscall)
```

**Credential write path** (through `proc:` scheme):
```
setresuid(ruid, euid, suid) → relibc::platform::redox::setresuid()
  → redox_rt::sys::posix_setresugid(&Resugid { ruid, euid, suid, .. })
    → packs 6×u32 into buffer
    → this_proc_call(&buf, CallFlags::empty(), &[ProcCall::SetResugid as u64])
      → SYS_CALL to proc: scheme
        → kernel proc scheme handler (scheme/proc.rs:1269):
            guard.euid = info.euid;
            guard.egid = info.egid;
```

**Key finding**: The kernel DOES support credential setting through the `proc:` scheme, using
`ProcSchemeAttrs` with `euid`/`egid`/`pid`/`prio`/`debug_name` fields. The `getuid()`/`getgid()`
functions work through userspace-cached values in `redox-rt`. `setresuid()`/`setresgid()` work
through the proc scheme.

**What's genuinely broken:**

| Function | Status | Root Cause |
|----------|--------|------------|
| `setgroups()` | **ENOSYS stub** | relibc/redox/mod.rs:1205 — `todo_skip!(0, "setgroups({}, {:p}): not implemented")` |
| `getgroups()` | /etc/group-based | Works via `getpwuid()` + `getgrent()` iteration — doesn't use kernel groups |
| `initgroups()` | No-op | No supplementary group infrastructure |

### 2.4 IPC Architecture

**Scheme-based IPC** is the primary IPC mechanism:

```
┌─────────────┐     SYS_CALL(syscall)      ┌──────────────┐
│  Userspace   │ ──────────────────────────→│   Kernel      │
│  Process A   │   open/read/write/fevent   │   Scheme      │
│              │ ←──────────────────────────│   Dispatch    │
└─────────────┘     result (usize/-errno)   └──────┬───────┘
                                                    │
                              ┌─────────────────────┤
                              │                     │
                         ┌────▼──────┐       ┌──────▼──────┐
                         │  Kernel    │       │  Userspace   │
                         │  Schemes   │       │  Scheme      │
                         │  (12)      │       │  Daemons     │
                         │            │       │  (via user:) │
                         │ debug:     │       │              │
                         │ event:     │       │ ptyd         │
                         │ memory:    │       │ pcid         │
                         │ pipe:      │       │ ext4d        │
                         │ irq:       │       │ fatd         │
                         │ time:      │       │ redox-drm    │
                         │ sys:       │       │ ...          │
                         │ proc:      │       │              │
                         │ serio:     │       │              │
                         └───────────┘       └──────────────┘
```

**IPC primitives available:**

| Primitive | Mechanism | Kernel/Userspace |
|-----------|-----------|-----------------|
| `pipe:` scheme | Kernel pipe scheme — bidirectional byte streams | Kernel |
| `shm_open()` / `mmap(MAP_SHARED)` | Shared memory via memory scheme grants | Kernel |
| `SYS_CALL` + scheme messages | Send/receive typed messages to scheme daemons | Kernel dispatch, userspace handler |
| `fevent()` | Register kernel-level events on file descriptors | Kernel |
| `sendfd()` | Pass file descriptors between processes | Kernel |
| `event:` scheme | Kernel event notification (used by eventfd/signalfd/timerfd) | Kernel |
| Signals | `sigprocmask` + `sigaction` via proc: scheme | Kernel delivery, userspace handling |
| Futex | Fast userspace mutex via `SYS_FUTEX` | Kernel |

**Current IPC limitations:**

| Limitation | Impact |
|-----------|--------|
| No `SYS_PTRACE` | ptrace not available (handled via proc: scheme paths) |
| No `SYS_KILL` | Signal sending via proc: scheme only |
| eventfd/signalfd/timerfd recipe-applied | Bounded compatibility layers, not plain-source |
| `ifaddrs` synthetic | Only `loopback` + `eth0`, not live enumeration |
| POSIX message queues not implemented | `mqueue.h` missing entirely |
| SysV message queues not implemented | `sys/msg.h` missing entirely |
| No UNIX domain sockets (`AF_UNIX`) path | Socket-based IPC limited |

### 2.5 Process Model

Redox uses a **userspace process manager** (`procmgr` via `proc:` scheme):

- **fork**: Implemented through proc: scheme → kernel creates new Context with cloned address space
- **exec**: Replaces address space with new executable image
- **spawn**: Combined fork+exec via proc: scheme
- **wait/waitpid/waitid**: Recipe-applied patch via proc: scheme (signals child exit)
- **Credentials on fork**: Address space cloned (userspace `DYNAMIC_PROC_INFO` inherited)
- **Credentials on exec**: `setresuid()` behavior (suid-bit not implemented in kernel)

The kernel's Context struct tracks:
- `owner_proc_id: Option<NonZeroUsize>` — parent process for exit notification
- `files: Arc<LockedFdTbl>` — file descriptor table (can be shared)
- `addr_space: Option<Arc<AddrSpaceWrapper>>` — address space (can be shared = threads)
- `sig: Option<SignalState>` — signal handler configuration

## 3. Critical Gaps and Blockers

### 3.1 Credential Syscall Blocker (Priority: P0-CRITICAL)

The `setgroups()` function is **ENOSYS**. This blocks:
- `polkit` — uses `setgroups()` for privilege management
- `dbus-daemon` — uses credentials for service activation
- `logind` / `redbear-sessiond` — needs credential awareness
- `sudo` / `su` — uses `initgroups()` → `setgroups()`
- Any program that changes user identity

**Root cause chain:**
1. `redox_syscall` crate (crates.io, upstream) has no `SYS_SETGROUPS`/`SYS_GETGROUPS` numbers
2. Kernel has no supplementary group table in Context struct
3. No group inheritance on fork/exec
4. relibc `setgroups()` is a `todo_skip!()` stub
5. `getgroups()` bypasses kernel entirely (reads /etc/group)

### 3.2 Kernel-Level Access Control Gap (Priority: P1)

The kernel's `caller_ctx()` provides `{euid, egid, pid}` to scheme handlers, but:

1. **No consistent enforcement**: Kernel schemes may or may not check caller credentials
2. **No ruid/suid tracking**: Cannot distinguish real vs effective identity in kernel
3. **All processes start as root** (euid=0, egid=0): No privilege separation at boot
4. **No supplementary groups in kernel**: Only egid checked

### 3.3 IPC Completeness Gaps (Priority: P2)

| Gap | Priority | Blocked By |
|-----|----------|------------|
| POSIX message queues (`mqueue.h`) | P2 | Scheme design needed |
| SysV message queues (`sys/msg.h`) | P2 | Scheme design needed |
| UNIX domain sockets (`AF_UNIX`) | P2 | Kernel or scheme implementation |
| Non-synthetic `ifaddrs` | P3 | Network stack enumeration |
| eventfd/signalfd/timerfd → plain-source | P3 | Upstream relibc convergence |

### 3.4 Resource Limits (Priority: P2)

`SYS_GETRLIMIT` / `SYS_SETRLIMIT` return ENOSYS. This is a microkernel design choice:
- Resource limits are typically library-level policy in capability systems
- Current approach: limits enforced in userspace daemons
- Desktop impact: systemd/logind expect rlimit support for service management

### 3.5 Shutdown Robustness (Priority: P2)

ACPI shutdown via `kstop` eventing exists but has gaps:
- `acpid` startup has panic-grade `expect` paths
- `_S5` derivation gated on PCI timing
- DMAR orphaned in `acpid` source
- See `local/docs/ACPI-IMPROVEMENT-PLAN.md` for full detail

## 4. Implementation Plan

### Phase K1: Kernel Credential Foundation (Week 1-2)

**Goal**: Add supplementary group support to the kernel and wire `setgroups()`/`getgroups()`.

#### K1.1 — Add supplementary groups to kernel Context

```rust
// Context struct additions (context/context.rs):
pub struct Context {
    // Existing:
    pub euid: u32,
    pub egid: u32,
    pub pid: usize,

    // NEW: Real/saved IDs (moved from userspace redox-rt to kernel):
    pub ruid: u32,
    pub rgid: u32,
    pub suid: u32,
    pub sgid: u32,

    // NEW: Supplementary groups
    pub groups: Vec<u32>,  // Or Arc<[u32]> for sharing
}
```

**Files modified:**
- `recipes/core/kernel/source/src/context/context.rs` — add fields, initialize, clone on fork
- `recipes/core/kernel/source/src/scheme/proc.rs` — extend `ProcSchemeAttrs` to include ruid/suid/rgid/sgid/groups
- `local/patches/kernel/` — new patch: `P4-credential-fields.patch`

#### K1.2 — Add `SYS_SETGROUPS` and `SYS_GETGROUPS` to redox_syscall

The `redox_syscall` crate is upstream (crates.io). Red Bear must either:
- **Option A (preferred)**: Contribute upstream PR to add syscall numbers
- **Option B**: Vendor fork of `redox_syscall` in `local/` overlay
- **Option C**: Define Red Bear-local syscall numbers in kernel directly

**Recommended: Option A + B fallback**:
1. Submit upstream PR to `redox_syscall` adding:
   - `SYS_SETGROUPS`, `SYS_GETGROUPS`
   - `SYS_SETUID`, `SYS_SETGID`, `SYS_GETUID`, `SYS_GETGID`
   - `SYS_GETEUID`, `SYS_GETEGID`
   - `SYS_SETREUID`, `SYS_SETREGID`
   - `SYS_GETRESUID`, `SYS_GETRESGID`

2. While upstream PR is pending, use a local `redox_syscall` patch:
   - Copy `redox_syscall` crate into `local/vendor/redox_syscall/`
   - Add syscall number constants
   - Point kernel Cargo.toml to local path
   - Patch tracked in `local/patches/kernel/P4-redox-syscall-numbers.patch`

#### K1.3 — Add kernel syscall handlers

**New file:** `recipes/core/kernel/source/src/syscall/cred.rs`

```rust
// Credential syscall handlers
pub fn setresuid(ruid: u32, euid: u32, suid: u32, token: &mut CleanLockToken) -> Result<usize> {
    let context_lock = context::current();
    let mut context = context_lock.write(token.token());

    // Permission check: must be root or match current values
    if context.euid != 0 {
        if let Some(ruid) = ruid_opt { /* check ruid == current ruid/euid/suid */ }
        // ... POSIX permission model
    }

    // Set values
    if ruid != u32::MAX { context.ruid = ruid; }
    if euid != u32::MAX { context.euid = euid; }
    if suid != u32::MAX { context.suid = suid; }
    Ok(0)
}

pub fn setgroups(groups: &[u32], token: &mut CleanLockToken) -> Result<usize> {
    // Requires: euid == 0
    let context_lock = context::current();
    let mut context = context_lock.write(token.token());
    if context.euid != 0 { return Err(Error::new(EPERM)); }
    context.groups = groups.to_vec();
    Ok(0)
}

pub fn getgroups(token: &mut CleanLockToken) -> Result<Vec<u32>> {
    let context_lock = context::current();
    let context = context_lock.read(token.token());
    Ok(context.groups.clone())
}
```

**Modified file:** `recipes/core/kernel/source/src/syscall/mod.rs`
```rust
match a {
    // ... existing arms ...
    SYS_SETRESUID => setresuid(b as u32, c as u32, d as u32, token),
    SYS_SETRESGID => setresgid(b as u32, c as u32, d as u32, token),
    SYS_GETRESUID => getresuid(UserSlice::wo(b, c)?, token),
    SYS_GETRESGID => getresgid(UserSlice::wo(b, c)?, token),
    SYS_SETUID   => setuid(b as u32, token),
    SYS_SETGID   => setgid(b as u32, token),
    SYS_GETUID   => Ok(getuid(token)),
    SYS_GETGID   => Ok(getgid(token)),
    SYS_GETEUID  => Ok(geteuid(token)),
    SYS_GETEGID  => Ok(getegid(token)),
    SYS_SETGROUPS => setgroups(UserSlice::ro(b, c)?, token).map(|()| 0),
    SYS_GETGROUPS => getgroups(UserSlice::wo(b, c)?, token),
    // ... existing arms ...
}
```

#### K1.4 — Wire relibc setgroups()/getgroups() through real syscalls

**Modified:** `recipes/core/relibc/source/src/platform/redox/mod.rs`
```rust
// Replace todo_skip!() stub:
unsafe fn setgroups(size: size_t, list: *const gid_t) -> Result<()> {
    if size < 0 || size > NGROUPS_MAX { return Err(Errno(EINVAL)); }
    let groups = core::slice::from_raw_parts(list, size as usize);
    syscall::setgroups(groups)?;
    Ok(())
}

// Replace /etc/group-based getgroups:
fn getgroups(mut list: Out<[gid_t]>) -> Result<c_int> {
    let mut buf = [0u32; NGROUPS_MAX as usize];
    let count = syscall::getgroups(&mut buf)?;
    for (i, gid) in buf[..count].iter().enumerate() {
        list[i] = *gid as gid_t;
    }
    Ok(count as c_int)
}
```

#### K1.5 — Add credential syscall stubs in redox-rt

**Modified:** `recipes/core/relibc/source/redox-rt/src/sys.rs`
```rust
pub fn setgroups(groups: &[u32]) -> Result<()> {
    unsafe {
        redox_syscall::syscall5(
            redox_syscall::SYS_SETGROUPS,
            groups.as_ptr() as usize,
            groups.len(),
            0, 0, 0,
        )
        .map(|_| ())
        .map_err(|e| Error::new(e.errno as i32))
    }
}

pub fn getgroups(buf: &mut [u32]) -> Result<usize> {
    unsafe {
        redox_syscall::syscall3(
            redox_syscall::SYS_GETGROUPS,
            buf.as_mut_ptr() as usize,
            buf.len(),
            0,
        )
        .map_err(|e| Error::new(e.errno as i32))
    }
}
```

#### K1.6 — Patch management

All kernel and relibc source changes must be mirrored into `local/patches/`:

```bash
local/patches/
├── kernel/
│   ├── redox.patch                    # Updated symlink target
│   ├── P4-credential-fields.patch     # Context struct additions
│   ├── P4-credential-syscalls.patch   # Syscall handlers + dispatch
│   └── P4-redox-syscall-numbers.patch # Local redox_syscall additions
├── relibc/
│   ├── P4-setgroups-kernel.patch      # Setgroups through real syscall
│   ├── P4-getgroups-kernel.patch      # Getgroups through real syscall
│   └── P4-redox-rt-cred-syscalls.patch # redox-rt syscall wrappers
```

### Phase K2: Kernel Access Control Hardening (Week 2-3)

**Goal**: Enforce credential checks in kernel schemes, add proper privilege separation.

#### K2.1 — Enforce scheme-level credential checks

Each kernel scheme handler currently receives `CallerCtx { uid, gid, pid }`. Ensure consistent
credential enforcement:

| Scheme | Current Check | Required Check |
|--------|--------------|----------------|
| `memory:` | Physical memory access → root only | ✅ Already enforced (euid==0 for phys) |
| `irq:` | IRQ registration → root only | ✅ Already enforced |
| `proc:` | Process inspection → caller == target OR root | 🔄 Review: ensure consistent |
| `sys:` | System info → read-only for all | ✅ Appropriate |
| `debug:` | Debug output → should be root-only | 🔄 Review: add check |
| `serio:` | PS/2 device → root only | 🔄 Review: add check |
| `event:` | Event registration → process-own only | 🔄 Review: ensure isolation |

#### K2.2 — Bootstrap with non-root init process

Currently all processes start as euid=0/egid=0. The boot sequence should:
1. Kernel bootstrap context starts as root (euid=0, egid=0) — required for init
2. Init (`/sbin/init`) runs as root
3. Init drops privileges before spawning user services:
   ```rust
   // In init or service manager:
   setresuid(1000, 1000, 1000);  // Drop to regular user
   setgroups(&[1000, 27, 100]);  // Set supplementary groups
   // Then spawn child services with restricted permissions
   ```

#### K2.3 — Add `initgroups()` support

```rust
// In relibc/src/platform/redox/mod.rs:
fn initgroups(user: CStr, group: gid_t) -> Result<()> {
    // 1. Set primary group
    setgid(group)?;
    // 2. Parse /etc/group for supplementary groups containing this user
    let mut groups = vec![group];
    // ... iterate getgrent() to find user memberships ...
    // 3. Set supplementary groups via kernel syscall
    setgroups(&groups)?;
    Ok(())
}
```

### Phase K3: IPC Infrastructure Improvements (Week 3-5)

**Goal**: Complete IPC primitives needed for desktop infrastructure.

#### K3.1 — POSIX Message Queues (`mqueue.h`)

**Design decision**: Implement as a userspace scheme daemon (not kernel syscalls).

```
mqd:
├── Registers as scheme:mqueue
├── Stores queues in memory backed by shm_open() + mmap()
├── mq_open() → open scheme:mqueue/{name}
├── mq_send() → write to fd
├── mq_receive() → read from fd
├── mq_notify() → fevent() on fd for async notification
├── mq_close() → close fd
└── mq_unlink() → unlink scheme:mqueue/{name}
```

**Implementation:**
- New Red Bear package: `local/recipes/system/mqueued/`
- Relibc header: `recipes/core/relibc/source/src/header/mqueue/`
- Recipe in `local/recipes/system/mqueued/recipe.toml`
- Init service: `/usr/lib/init.d/50_mqueued.service`

#### K3.2 — SysV Message Queues (`sys/msg.h`)

**Design decision**: Implement as scheme daemon or on top of POSIX message queues.
- Recommended: implement directly alongside `mqueued` using shared infrastructure.
- Low priority — Qt/KDE do not depend on SysV msg queues.

#### K3.3 — UNIX Domain Sockets (`AF_UNIX` / `SOCK_STREAM`)

**Current state**: D-Bus uses abstract sockets on Linux. Redox uses scheme-based communication.
- For D-Bus compatibility: `redbear-sessiond` already uses `zbus` with custom transport
- For general `AF_UNIX`: implement as `scheme:unix` daemon backed by kernel pipe scheme
- Priority: P3 — D-Bus is already working through scheme transport

#### K3.4 — Non-synthetic Interface Enumeration

Replace the hardcoded `loopback` + `eth0` model with live network interface enumeration:
- Query `smolnetd` or equivalent for active interfaces
- Expose through `getifaddrs()` properly
- Priority: P3 — needed for NetworkManager-like functionality

#### K3.5 — eventfd/signalfd/timerfd → plain-source convergence

Current state: all three are recipe-applied patches. Goal: upstream into relibc mainline.
- Monitor upstream relibc for equivalent implementations
- When upstream absorbs: shrink/drop Red Bear patch chain
- When upstream does NOT absorb after 3+ months: promote to durable Red Bear-maintained
- See `local/docs/RELIBC-IPC-ASSESSMENT-AND-IMPROVEMENT-PLAN.md` Phase I5

### Phase K4: Resource Limits and Process Management (Week 4-6)

#### K4.1 — RLIMIT Support

**Decision**: Enforce resource limits in userspace, not kernel.
- The kernel is a microkernel — resource limits are policy
- `getrlimit()` / `setrlimit()` → libc stubs with reasonable defaults
- Process enforcement → `procmgr` (userspace process manager) via proc: scheme
- File descriptor limits → already enforced via `CONTEXT_MAX_FILES` in kernel
- Memory limits → userspace `procmgr` can kill processes exceeding limits

```rust
// relibc implementation (userspace, no kernel changes needed):
fn getrlimit(resource: c_int, rlim: *mut rlimit) -> Result<()> {
    match resource {
        RLIMIT_NOFILE => { rlim.rlim_cur = 1024; rlim.rlim_max = 4096; }
        RLIMIT_NPROC =>  { rlim.rlim_cur = 256;  rlim.rlim_max = 1024; }
        RLIMIT_AS =>     { rlim.rlim_cur = RLIM_INFINITY; rlim.rlim_max = RLIM_INFINITY; }
        RLIMIT_CORE =>   { rlim.rlim_cur = 0;    rlim.rlim_max = RLIM_INFINITY; }
        // ... other resource types with reasonable defaults
        _ => return Err(Errno(EINVAL)),
    }
    Ok(())
}
```

#### K4.2 — PTRACE via proc: scheme

`SYS_PTRACE` is not implemented as a direct syscall. The Redox model uses the `proc:` scheme
for process inspection and manipulation:
- Already partially implemented in `scheme/proc.rs`
- Memory read/write through proc: scheme file operations
- Register read/write through proc: scheme
- Signal injection through proc: scheme

Improvements needed:
- Document the proc: scheme ptrace API surface
- Ensure all ptrace operations have proc: scheme equivalents
- Add `PTRACE_*` constants to redox_syscall for compatibility

#### K4.3 — clock_settime

`SYS_CLOCK_SETTIME` returns ENOSYS. Implementation:
- Add scheme write path to `/scheme/sys/update_time_offset`
- Or implement as direct syscall for precision
- Priority: P3 — needed for NTP synchronization

### Phase K5: Shutdown and Power Management (Week 5-7)

See `local/docs/ACPI-IMPROVEMENT-PLAN.md` for full ACPI plan. This section covers kernel-specific
work only.

#### K5.1 — Hardened acpid Startup

- Remove panic-grade `expect` paths in kernel ACPI/AML handling
- Add graceful fallback when ACPI tables are missing or malformed
- See ACPI-IMPROVEMENT-PLAN.md Wave 1

#### K5.2 — kstop Shutdown Robustness

- Current: `_S5` shutdown via `kstop` event exists but gated on PCI timing
- Required: deterministic shutdown ordering:
  1. Notify userspace services of impending shutdown
  2. Sync filesystems
  3. Power off via ACPI/FADT
- See ACPI-IMPROVEMENT-PLAN.md Wave 2

#### K5.3 — Sleep State Support

- S3 (suspend-to-RAM) and S4 (hibernate) are not yet supported
- Requires: kernel state serialization, device reinitialization
- Priority: P4 — long-term, not blocking desktop

## 5. Dependency Chain

```
Phase K1 (credential syscalls) ─────────────────────┐
    │                                                 │
    ├──► polkit compatibility                        │
    ├──► dbus-daemon credential checks                │
    ├──► sudo/su user switching                       │
    ├──► redbear-sessiond login1 handoff              │
    └──► greeter/session-launch credential drop        │
                                                      │
Phase K2 (access control) ────────────────────────────┤
    │                                                 │
    ├──► Privilege-separated boot sequence            │
    ├──► Scheme-level credential enforcement          │
    └──► initgroups() for service launching            │
                                                      │
Phase K3 (IPC) ───────────────────────────────────────┤
    │                                                 │
    ├──► POSIX message queues → needed by some apps   │
    ├──► AF_UNIX → broader D-Bus transport options    │
    └──► eventfd/signalfd/timerfd → KDE/Qt runtime     │
                                                      │
Phase K4 (limits/ptrace) ─────────────────────────────┤
    │                                                 │
    ├──► RLIMIT → systemd/logind compatibility        │
    ├──► PTRACE → debugging support                   │
    └──► clock_settime → NTP synchronization           │
                                                      ▼
                                          Desktop infrastructure
                                          ready for KDE Plasma
```

## 6. Integration with Existing Work

### 6.1 Already in Progress (do not duplicate)

| Area | Canonical Plan | Status |
|------|---------------|--------|
| IRQ / MSI-X / IOMMU | `IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md` | Waves 1-6 complete, hardware validation open |
| ACPI shutdown / power | `ACPI-IMPROVEMENT-PLAN.md` | Waves 1-2 complete, sleep states deferred |
| relibc IPC surface | `RELIBC-IPC-ASSESSMENT-AND-IMPROVEMENT-PLAN.md` | Phases I1-I5, message queues deferred |
| D-Bus / sessiond | `DBUS-INTEGRATION-PLAN.md` | Phase 1 complete, Phase 2 in progress |
| Greeter / login | `GREETER-LOGIN-IMPLEMENTATION-PLAN.md` | Active, bounded proof passing |
| Desktop path | `CONSOLE-TO-KDE-DESKTOP-PLAN.md` | Phase 1-5 model, KWin building |

### 6.2 This Plan Covers (uniquely)

| Area | This Plan | Not Covered By |
|------|-----------|---------------|
| Kernel credential architecture | §3, Phase K1 | Any existing plan |
| Kernel access control hardening | §3.2, Phase K2 | Any existing plan |
| `setgroups()` / `getgroups()` kernel implementation | Phase K1.2-K1.4 | Only stub noted elsewhere |
| Supplementary group infrastructure | Phase K1.1 | Not covered anywhere |
| POSIX/SysV message queues | Phase K3.1-K3.2 | Deferred in relibc-IPC plan |
| UNIX domain sockets | Phase K3.3 | Not covered |
| RLIMIT design decision | Phase K4.1 | Noted as gap only |
| PTRACE via proc: scheme | Phase K4.2 | Not covered |
| clock_settime implementation | Phase K4.3 | Noted as gap only |

## 7. Patch Governance

All kernel and relibc source changes must follow the durability policy (see `local/AGENTS.md`):

1. **Make changes** in `recipes/core/kernel/source/` or `recipes/core/relibc/source/`
2. **Generate patches**: `git diff` in the source tree → `local/patches/<component>/P4-*.patch`
3. **Wire patches** into `recipes/core/<component>/recipe.toml` patches list
4. **Commit** patches + recipe changes before session end
5. **Assume** source trees may be thrown away by `make distclean` or upstream refresh

### Patch naming convention:
```
local/patches/kernel/P4-credential-fields.patch
local/patches/kernel/P4-credential-syscalls.patch
local/patches/kernel/P4-redox-syscall-numbers.patch
local/patches/relibc/P4-setgroups-kernel.patch
local/patches/relibc/P4-getgroups-kernel.patch
local/patches/relibc/P4-redox-rt-cred-syscalls.patch
local/patches/relibc/P4-initgroups.patch
```

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
| `getuid()` returns non-zero after login | `id` command in guest |
| `setgroups()` succeeds for root | `sudo -u user id` in guest |
| `setresuid()` properly changes euid | `su user -c 'id'` |
| `initgroups()` populates groups | `groups` command in guest |
| Credentials survive fork | `bash -c 'id'` |
| Credentials dropped on exec (if SUID implemented) | TBD |
| polkit can query credentials | `pkexec echo ok` |
| dbus-daemon starts without errors | `dbus-monitor` |

### 8.3 Verification Scripts

Create bounded proof scripts:
```bash
local/scripts/test-credential-syscalls-qemu.sh    # QEMU launcher
local/scripts/test-credential-syscalls-guest.sh   # In-guest checker
```

## 9. References

- `local/docs/COMPREHENSIVE-OS-ASSESSMENT.md` — Parent assessment, §2 kernel gaps
- `docs/01-REDOX-ARCHITECTURE.md` — Architecture reference
- `local/docs/IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md` — IRQ/PCI plan (sibling)
- `local/docs/RELIBC-IPC-ASSESSMENT-AND-IMPROVEMENT-PLAN.md` — IPC surface plan (companion)
- `local/docs/ACPI-IMPROVEMENT-PLAN.md` — ACPI/shutdown plan (sibling)
- `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` — Desktop path plan (consumer)
- `recipes/core/kernel/source/src/syscall/mod.rs` — Syscall dispatch (primary implementation target)
- `recipes/core/kernel/source/src/context/context.rs` — Context struct (credential fields)
- `recipes/core/kernel/source/src/scheme/proc.rs` — Proc scheme (credential setting)
- `recipes/core/relibc/source/src/platform/redox/mod.rs` — relibc Redox platform (credential stubs)
- `recipes/core/relibc/source/redox-rt/src/sys.rs` — redox-rt credential primitives
