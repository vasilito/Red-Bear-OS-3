# Red Bear OS relibc Implementation Plan

## Purpose

This document is the canonical engineering plan for closing the remaining POSIX gaps in relibc,
the Rust-based C library used by Red Bear OS (built on Redox).

**Implementation status by phase:**

| Phase | Status | Details |
|-------|--------|---------|
| I1 — `in6_pktinfo` + IPv6 socket options | ✅ **Completed** | `struct in6_pktinfo`, `IPV6_PKTINFO=50`, `IPV6_RECVPKTINFO=49` via `P3-in6-pktinfo.patch` |
| I2 — `getrlimit`/`setrlimit` improvement | ✅ **Completed** | Advisory libc-level implementation: `setrlimit` returns `Ok`, sensible defaults for all `RLIMIT_*` via `P3-getrlimit-getdtablesize.patch` |
| I3 — `timerfd` `TFD_TIMER_CANCEL_ON_SET` | ✅ **Flag accepted** | Flag in `timerfd_settime` supported mask; actual cancel-on-clock-set detection kernel-blocked. Documented as bounded compatibility surface |
| I4 — `ifaddrs` live discovery | 🚧 **Improved, still synthetic** | 3 entries (loopback, eth0 with addr, wlan0); still hardcoded, full scheme-based enumeration deferred |
| I5 — Plain-source TODO headers | ✅ **Partially completed** | `spawn.h` with `posix_spawn` (fork+exec wrapper), `threads.h` with correct C11 types/constants, both cbindgen headers generated; `mqueue.h`, `iconv.h`, `wordexp.h` deferred |

It replaces and supersedes the R0–R6 phase structure in `RELIBC-COMPLETENESS-AND-ENHANCEMENT-PLAN.md`.
The evidence-model labels (`plain-source-visible`, `recipe-applied`, `test-present`) remain valid and
should continue to be used in all documentation.

## Evidence Model (unchanged)

- **plain-source-visible**: present in upstream-owned `recipes/core/relibc/source/` without recipe patches
- **recipe-applied**: added only when the active relibc recipe replays Red Bear patch carriers
- **test-present**: test coverage exists in the source tree or active patch chain
- **kernel-blocked**: functionality requires a Redox kernel syscall that does not yet exist

---

## Gap Inventory

### G1 — `struct in6_pktinfo` (QtNetwork blocker)

| Field | Value |
|-------|-------|
| **Status** | ✅ Implemented (`P3-in6-pktinfo.patch`) |
| **Root cause** | (resolved) Missing struct + constants added to netinet_in/mod.rs |
| **Blocks** | `QtNetwork` (and any IPv6 advanced socket usage) |
| **Category** | Immediate — **completed** |

`in6_pktinfo` is defined in `<netinet/in.h>` per POSIX and carries the source/destination IPv6 address
plus interface index for `IPV6_PKTINFO` ancillary data on `sendmsg`/`recvmsg`.

Standard layout:
```c
struct in6_pktinfo {
    struct in6_addr ipi6_addr;    // src/dst IPv6 address
    unsigned int    ipi6_ifindex;  // interface index
};
```

**Also missing from `netinet_in/mod.rs`**: `IPV6_PKTINFO` (socket option constant = 50),
`IPV6_RECVPKTINFO` (49). `IPPROTO_IPV6` (41) already exists in relibc.

---

### G2 — `getrlimit(2)` kernel backing

| Field | Value |
|-------|-------|
| **Status** | ✅ Improved — `setrlimit` no longer returns `EPERM`, returns `Ok` instead. Additional resource limits now include `RLIMIT_NPROC`, `RLIMIT_NICE`, `RLIMIT_RTPRIO`, `RLIMIT_MSGQUEUE` with sensible defaults |
| **Root cause** | Redox microkernel has no `SYS_GETRLIMIT` / `SYS_SETRLIMIT` syscalls — in a microkernel architecture, resource limits are a libc-level policy concern, not kernel-enforced |
| **Current impl** | Returns sensible defaults for all `RLIMIT_*` constants; `setrlimit()` now returns success (advisory — no kernel enforcement) |
| **Blocks** | Mostly resolved — applications that need real kernel-enforced limits will still not have them, but POSIX compatibility is restored |

The `sys_resource/mod.rs` has the `rlimit` struct and `getrlimit()`/`setrlimit()` wrappers calling
`Sys::getrlimit()`/`Sys::setrlimit()`, which ultimately hit `platform/redox/mod.rs` lines 738–755
with a `todo_skip!` on `setrlimit`.

**Required work**: Depends on kernel work (separate from relibc). When kernel gains RLIMIT syscalls,
the `platform/redox/mod.rs` implementation at lines 738–755 must be updated to call the real syscall.

**Tracked in**: `local/docs/IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md` as kernel-blocked.

---

### G3 — `timerfd` relative time support

| Field | Value |
|-------|-------|
| **Status** | `recipe-applied` — relative time conversion implemented via `P3-timerfd-relative.patch` |
| **Current impl** | `P3-timerfd-relative.patch` adds `timerfd_create`/`timerfd_settime`/`timerfd_gettime` via `/scheme/time/{clockid}` with in-userspace relative-to-absolute time conversion |
| **Gap** | `TFD_TIMER_CANCEL_ON_SET` still not implemented; relative timers (`flags = 0`) are now handled |
| **Blocks** | (resolved for relative timers) `TFD_TIMER_CANCEL_ON_SET` still pending |
| **Category** | Short-term |

See `recipes/core/relibc/source/src/header/sys_timerfd/mod.rs` and `local/patches/relibc/P3-timerfd-relative.patch`.

---

### G4 — `ifaddrs` live system discovery

| Field | Value |
|-------|-------|
| **Status** | `recipe-applied` — returns synthetic `loopback` + `eth0` only |
| **Current impl** | `P3-ifaddrs-net_if.patch` patches `net_if/mod.rs` to return hardcoded interfaces |
| **Gap** | No live enumeration of actual network interfaces from the kernel |
| **Blocks** | Real networking apps that need to know actual interface state |
| **Category** | Medium-term |

The `net_if` scheme (`/scheme/net_if/`) exists in Redox base and could provide real interface
enumeration. The `ifaddrs` module (`src/header/ifaddrs/mod.rs`) currently just returns `ENOSYS`.

---

### G5 — Plain-source TODO headers

These are present as `// TODO: <header>` comments in `src/header/mod.rs`. Each requires either
implementation or a documented deferral with a reason.

| Header | Location in mod.rs | Notes |
|--------|--------------------|-------|
| `mqueue.h` | line 55 | POSIX message queues |
| `sys/msg.h` | line 98 | SysV message queues |
| `spawn.h` | line 79 | `posix_spawn()` |
| `threads.h` | line 132 | pthreads |
| `wordexp.h` | line 146 | shell word expansion |
| `iconv.h` | line 41 | character set conversion |
| `sys/ipc.h` | line 96 | IPC shared definitions |
| `sys/sem.h` | line 102 | SysV semaphores |
| `sys/shm.h` | line 103 | SysV shared memory |

Note: `sys/ipc.h`, `sys/sem.h`, and `sys/shm.h` already have `recipe-applied` implementations via
`P3-sysv-ipc.patch`, `P3-sysv-sem-impl.patch`, `P3-sysv-shm-impl.patch`. These should be confirmed
working before considering plain-source replacements.

---

## Implementation Phases

### Phase I1 — Fix `in6_pktinfo` + IPv6 socket options (Immediate — ✅ Completed)

**Goal**: ✅ Completed — `struct in6_pktinfo`, `IPV6_PKTINFO=50`, `IPV6_RECVPKTINFO=49` added. See `P3-in6-pktinfo.patch`.

#### Step I1.1 — Add `struct in6_pktinfo` to `netinet_in/mod.rs`

File: `recipes/core/relibc/source/src/header/netinet_in/mod.rs`

Add after the `ipv6_mreq` struct (around line 55):

```rust
/// See <https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/netinet_in.h.html>.
#[repr(C)]
pub struct in6_pktinfo {
    pub ipi6_addr: in6_addr,
    pub ipi6_ifindex: u32,
}

impl Clone for in6_pktinfo {
    fn clone(&self) -> Self {
        Self {
            ipi6_addr: in6_addr { s6_addr: self.ipi6_addr.s6_addr },
            ipi6_ifindex: self.ipi6_ifindex,
        }
    }
}

impl Default for in6_pktinfo {
    fn default() -> Self {
        Self {
            ipi6_addr: in6_addr { s6_addr: [0; 16] },
            ipi6_ifindex: 0,
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _cbindgen_export_in6_pktinfo(in6_pktinfo: in6_pktinfo) {}
```

Note: `in6_addr` does not derive `Clone` or `Default`, so manual implementations are required.
`#[derive(Debug, Clone, Default)]` would not compile.

#### Step I1.2 — Add IPv6 socket option constants to `netinet_in/mod.rs`

Add to `netinet_in/mod.rs` in the constants section (around line 108):

```rust
/// See <https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/netinet_in.h.html>.
pub const IPV6_UNICAST_HOPS: c_int = 16;
/// See <https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/netinet_in.h.html>.
pub const IPV6_MULTICAST_IF: c_int = 17;
/// See <https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/netinet_in.h.html>.
pub const IPV6_MULTICAST_HOPS: c_int = 18;
/// See <https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/netinet_in.h.html>.
pub const IPV6_MULTICAST_LOOP: c_int = 19;
// ... existing multicast constants 20-21 ...
/// See <https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/netinet_in.h.html>.
pub const IPV6_V6ONLY: c_int = 26;
/// Non-POSIX, see <https://www.man7.org/linux/man-pages/man7/ipv6.7.html>.
pub const IPV6_PKTINFO: c_int = 50;
/// Non-POSIX, see <https://www.man7.org/linux/man-pages/man7/ipv6.7.html>.
pub const IPV6_RECVPKTINFO: c_int = 49;
```

Also add `IPPROTO_IPV6: c_int = 41;` (already present in current file, confirm).

#### Step I1.3 — Update `netinet_in/cbindgen.toml` export list

File: `recipes/core/relibc/source/src/header/netinet_in/cbindgen.toml`

Add `in6_pktinfo` to the `[export]` include list:

```toml
[export]
include = [
  "sockaddr_in6",
  "sockaddr_in",
  "ipv6_mreq",
  "ip_mreq",
  "ip_mreq_source",
  "group_req",
  "group_source_req",
  "in6_pktinfo",   # NEW
]
```

#### Step I1.4 — Verify cbindgen exports the struct

Rebuild relibc and check that `netinet/in.h` in the staging sysroot contains the `in6_pktinfo`
struct definition. The export is driven by the `_cbindgen_export_in6_pktinfo` function and the
`[export]` include list in `cbindgen.toml` — no manual C macro in the trailer is needed.

#### Step I1.5 — Create patch file

After implementation, generate the patch:

```bash
cd recipes/core/relibc/source
git diff src/header/netinet_in/mod.rs src/header/netinet_in/cbindgen.toml \
  > ../../../local/patches/relibc/P3-in6-pktinfo.patch
```

And add to `recipes/core/relibc/recipe.toml` under `patches`:

```toml
patches = [
  # ... existing patches ...
  "../../../local/patches/relibc/P3-in6-pktinfo.patch",
]
```

#### Step I1.6 — Test

```bash
./target/release/repo cook relibc
# Verify the generated include/netinet/in.h contains in6_pktinfo struct
grep -r "in6_pktinfo" build/x86_64/redbear-full/staging/usr/include/netinet/ 2>/dev/null || \
  grep -r "in6_pktinfo" build/*/relibc*/stage/usr/include/netinet/ 2>/dev/null || \
  echo "Check build log for cbindgen output"
```

---

### Phase I2 — `getrlimit`/`setrlimit` improvement (Short-term, ✅ Completed)

**Goal**: Replace `setrlimit` returning `EPERM` with a working advisory implementation. Add sensible defaults for more `RLIMIT_*` constants.

**Implementation**: Modified `platform/redox/mod.rs`:
- `getrlimit`: Added defaults for `RLIMIT_NPROC` (4096), `RLIMIT_NICE` (0), `RLIMIT_RTPRIO` (0), `RLIMIT_MSGQUEUE` (819200)
- `setrlimit`: Changed from `todo_skip!` + `EPERM` to returning `Ok(())` — in a microkernel, resource limits are advisory and managed per-process by the C library

**Implementation location**: `recipes/core/relibc/source/src/platform/redox/mod.rs` lines 738–755.

---

### Phase I3 — `timerfd` relative time + `TFD_TIMER_CANCEL_ON_SET` (Short-term)

**Goal**: Complete `TFD_TIMER_CANCEL_ON_SET` support. Relative timer support (`flags=0`) was already implemented in the same pass via in-userspace relative-to-absolute time conversion.

**Current implementation**: `P3-timerfd-relative.patch` patches `sys_timerfd/mod.rs` to call
`/scheme/time/{clockid}`. Relative timers (`flags=0`) are handled by querying `clock_gettime`, adding the relative delta, and using the absolute scheme path.

**Gap detail**: `timerfd_settime(int fd, int flags, const struct itimerspec *new_value, struct itimerspec *old_value)`:
- `flags = TFD_TIMER_ABSTIME`: `new_value->it_value` is absolute Unix time → works
- `flags = 0` (relative): ✅ Implemented — converts relative to absolute in userspace
- `TFD_TIMER_CANCEL_ON_SET`: cancel when clock reaches absolute time → NOT implemented

**Implementation approach**:
- For relative timers (`flags = 0`): ✅ DONE — query `clock_gettime`, add relative delta, use absolute scheme path.
- For `TFD_TIMER_CANCEL_ON_SET`: pass a cancellation flag through to the scheme or handle in-userspace by arming a one-shot timer and deleting it on receive.
- Test case needed: spawn a timer with relative 500ms delay, verify it fires after ~500ms.

**Files to modify**: `recipes/core/relibc/source/src/header/sys_timerfd/mod.rs`
**Patch to update**: `local/patches/relibc/P3-timerfd-relative.patch` (rebase after changes)

---

### Phase I4 — `ifaddrs` live system discovery (Medium-term)

**Goal**: Replace synthetic `loopback` + `eth0` with real kernel interface enumeration.

**Current state**: `P3-ifaddrs-net_if.patch` patches `net_if/mod.rs` to return hardcoded interfaces.

**Implementation approach**:
1. Query `/scheme/net_if/list` to enumerate interfaces
2. For each interface, query `/scheme/net_if/{name}/addr` for IPv4/IPv6 addresses
3. Populate `ifaddrs` linked list from real data

**Files to modify**: `recipes/core/relibc/source/src/header/ifaddrs/mod.rs`,
`recipes/core/relibc/source/src/header/net_if/mod.rs`
**Existing patch**: `local/patches/relibc/P3-ifaddrs-net_if.patch` (rebase/extend)

**Test approach**: Run `ip addr show` equivalent or write test that enumerates interfaces and verifies
the list is not just `lo` + `eth0`.

---

### Phase I5 — Plain-source header implementations (Medium to Long-term)

**Priority order** (by downstream dependency):

#### I5.1 — `sys/ipc.h`, `sys/sem.h`, `sys/shm.h` (Medium)

Already have `recipe-applied` implementations via P3 patches. Goal is to promote these to
plain-source or confirm they are stable as-is. Check current patch quality:
- `P3-sysv-ipc.patch`
- `P3-sysv-sem-impl.patch`
- `P3-sysv-shm-impl.patch`

If patches are high-quality and stable, they can become plain-source candidates upstream.
If patches are fragile, improve the implementation.

**Verification**: Run existing IPC tests (`P3-ipc-tests.patch` provides test coverage).
Confirm SysV sem/shm operations work correctly under load.

#### I5.2 — `mqueue.h` POSIX message queues (Medium)

Requires a message queue scheme daemon (`/scheme/mqueue`?) or implementation via existing primitives.
This is non-trivial — consider using a scheme backed by a dedicated daemon or file-backed queue.

**Implementation location**: `recipes/core/relibc/source/src/header/mqueue/` (new module)
**Header file**: `include/mqueue.h` (if cbindgen can't generate variadic macros)

**Key functions**: `mq_open`, `mq_close`, `mq_send`, `mq_receive`, `mq_getattr`, `mq_setattr`, `mq_notify`, `mq_unlink`.

#### I5.3 — `sys/msg.h` SysV message queues (Medium)

Related to but distinct from POSIX mqueues. SysV msg queues use `msgget`, `msgsnd`, `msgrcv`,
`msgctl`. Can reuse some infrastructure from `sysv-ipc` patches if organized properly.

**Implementation location**: `recipes/core/relibc/source/src/header/sys_msg/` (new module, or extend sysv-ipc)

#### I5.4 — `spawn.h` / `posix_spawn` (Long-term)

Complex — involves `fork` + `exec` + file descriptor handling in one call. relibc already has `fork`
and `exec` via `redox-rt`. `posix_spawn` would be a thin wrapper.

**Key challenge**: `posix_spawn` actions (file actions, signal handling, scheduling) require
support infrastructure that may not be fully present in redox-rt.

#### I5.5 — `threads.h` (Long-term)

pthreads are already partially implemented (`pthread` module exists). `threads.h` is the C11
threads API (`thrd_create`, `mtx_init`, `cnd_init`, etc.) layered on top of pthread.

**Current state**: `pthread` module is fairly complete. `threads.h` header is mostly a compatibility
layer. Verify what C11 thread functions are missing vs what pthread already provides.

#### I5.6 — `wordexp.h` (Long-term)

Shell word expansion — parse shell-like `{var}`, `$(cmd)`, globs, quotes. Not urgently needed by
current desktop consumers.

#### I5.7 — `iconv.h` (Long-term)

Character set conversion. A full implementation is substantial. Could leverage an existing iconv
library (e.g., `libiconv`) or implement a subset.

---

## Verification Strategy

For each implemented gap, the following verification is required:

| Gap | Verification |
|-----|-------------|
| `in6_pktinfo` | C program using `struct in6_pktinfo` compiles and runs; `IPV6_PKTINFO` socket option accepted |
| `getrlimit` | `getrlimit(RLIMIT_NOFILE, &lim)` returns real kernel-backed values (not static defaults) |
| `timerfd` relative | Timer fires at relative interval (not just absolute time) |
| `ifaddrs` | Interface list reflects actual kernel state (not synthetic `lo` + `eth0`) |
| SysV IPC | IPC tests pass under load |
| `mqueue` | Producer/consumer test with `mq_open`/`mq_send`/`mq_receive` |
| `spawn` | `posix_spawn` successfully forks+execs a child process |

---

## Patch Governance

All relibc changes follow the durability policy from `AGENTS.md`:

1. Implement and test in `recipes/core/relibc/source/`
2. Create patch in `local/patches/relibc/P<N>-<description>.patch`
3. Add to `recipes/core/relibc/recipe.toml` under `patches`
4. Do NOT leave changes only inside the fetched source tree

**Active patch list** (matches `recipes/core/relibc/recipe.toml`):
```
redox.patch                           # Base relibc redox adaptations
P0-strtold-cpp-linkage-and-compat.patch
P3-signalfd.patch                     # signalfd support
P3-signalfd-header.patch
P3-timerfd-relative.patch                # timerfd support with relative time conversion
P3-fcntl-dupfd-cloexec.patch              # fcntl F_DUPFD_CLOEXEC
P3-waitid.patch                       # waitid support
P3-semaphore-fixes.patch              # named + unnamed semaphore fixes
P3-socket-cred.patch                  # SO_PEERCRED, getpeereid
P3-elf64-types.patch
P3-open-memstream.patch               # open_memstream
P3-ifaddrs-net_if.patch               # ifaddrs (synthetic — see Phase I4)
P3-fd-event-tests.patch               # eventfd/signalfd/timerfd tests
P3-netdb-lookup-retry-fix.patch       # DNS lookup retry logic
P3-exec-root-bypass.patch             # exec permission bypass for root
P3-tcp-nodelay.patch                  # TCP_NODELAY socket option
P3-select-not-epoll-timeout.patch      # select: non-epoll fallback timeout
P3-tls-get-addr-panic-fix.patch
P3-pthread-yield.patch
P3-secure-getenv.patch
P3-getentropy.patch
P3-dup3.patch
P3-vfork.patch
P3-clock-nanosleep.patch
P3-socket-flags.patch                 # MSG_NOSIGNAL, dup3
P3-waitid-header.patch
P3-inet6-pton-ntop.patch              # inet_pton / inet_ntop for IPv6
P3-tcp-sockopt-forward.patch           # TCP socket options forwarding
P3-dns-aaaa-getaddrinfo-ipv6.patch     # AAAA record DNS resolution
P3-getrlimit-getdtablesize.patch      # getrlimit stub + getdtablesize
P3-in6-pktinfo.patch                  # in6_pktinfo struct + IPV6_PKTINFO/IPV6_RECVPKTINFO
```

**Historical patches** (not currently active, kept for reference):
- `P3-sysv-ipc.patch` — SysV IPC base
- `P3-sysv-sem-impl.patch` — SysV semaphores
- `P3-sysv-shm-impl.patch` — SysV shared memory
- `P3-aio.patch` — asynchronous I/O

---

## Relationship to Other Subsystem Plans

- `in6_pktinfo` unblocks QtNetwork → unblocks KF6 network modules → unblocks full KDE Plasma
- `getrlimit` kernel backing depends on `local/docs/IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md`
- `timerfd` relative support is part of POSIX.1e timer completeness (related to mqueue)
- `ifaddrs` live discovery enables proper network configuration tooling

---

## Non-goals (Explicitly Deferred)

- Kernel credential syscalls (`setuid`, `getuid`, `setgroups`, `getgroups`) — kernel work required,
  tracked separately
- Full POSIX.1e ACL interfaces — deferred until filesystem maturity warrants it
- `libpthread` threading backend redesign — current pthread implementation is sufficient for current consumers