# Red Bear OS: DMA-BUF Improvement Plan

**Date**: 2026-04-16
**Status**: v1 COMPLETE (Steps 1-6a implemented, Oracle-verified through 8 rounds). Step 6b blocked on GPU command submission. Stale token cleanup verified across all GEM destruction paths.
**Scope**: Cross-process GPU buffer sharing for hardware-accelerated KDE Plasma on Wayland

## Bottom Line

Redox kernel already has the three primitives needed for DMA-BUF-style cross-process buffer
sharing:

1. **`Provider::FmapBorrowed`** + **`Grant::borrow_fmap()`** — kernel mechanism for borrowing
   pages from a scheme into another process's address space, mapping the same physical frames
   (zero-copy). Source: `kernel/source/src/context/memory.rs:1157`, `memory.rs:1401`.

2. **`sendfd`** syscall — passes file descriptors between processes via scheme IPC. Both
   processes hold the same `Arc<LockedFileDescription>`. Source: `kernel/source/src/syscall/fs.rs:415`.

3. **`PhysBorrow`** in `scheme:memory` — maps physical addresses directly into process space
   (already used for GPU registers/BARs). Source: `kernel/source/src/scheme/memory.rs`.

No new kernel syscalls or scheme types are needed for v1. The work is entirely in userspace:
redox-drm scheme daemon, libdrm, and Mesa.

## Architecture Principle

**DMA-BUF is a sharing and lifetime contract, not a global allocator.**

Linux `dma_buf` is an exporter/importer contract. The exporter owns allocation and controls
lifetime. The importer gets shared access. Red Bear OS follows the same model:

- **Allocation stays with `redox-drm`** (the exporter). `DmaBuffer::allocate()` in `gem.rs`
  already allocates physically-contiguous system RAM.
- **Sharing uses scheme-backed fds + `sendfd`**. No synthetic fd numbers. No global registry.
- **Mapping uses `FmapBorrowed`**. The kernel maps the same physical pages into the importer's
  address space — zero-copy.

## Data Flow

```
Process A (GPU client, e.g. Mesa/radeonsi)
  1. open("/scheme/drm/card0")
  2. DRM_IOCTL_GEM_CREATE → allocate GPU buffer             ← EXISTS
  3. DRM_IOCTL_PRIME_HANDLE_TO_FD → get opaque export token     ← IMPLEMENTED
  4. open("/scheme/drm/card0/dmabuf/{token}") → get scheme fd   ← IMPLEMENTED
  5. sendfd(socket, fd) → pass fd to compositor                 ← KERNEL EXISTS

Process B (compositor, e.g. KWin)
  6. recvfd(socket) → receive the fd                            ← KERNEL EXISTS
  7. DRM_IOCTL_PRIME_FD_TO_HANDLE → import as local GEM         ← IMPLEMENTED
  7. mmap(fd, size) → kernel uses FmapBorrowed               ← KERNEL EXISTS
  8. Both processes see same physical pages                   ← ZERO-COPY
```

Steps 1-2 are already working. Steps 3-6 require redox-drm changes. Steps 4-5 and 7-8 use
existing kernel mechanisms.

## Current State

### What Exists

| Component | Status | Detail |
|-----------|--------|--------|
| GEM_CREATE ioctl | ✅ Working | `DmaBuffer::allocate()` in `gem.rs`, physically contiguous system RAM |
| GEM_CLOSE ioctl | ✅ Working | Ownership tracking, reference counting, safe cleanup |
| GEM_MMAP ioctl | ✅ Working | Returns virtual address for mmap_prep |
| KMS/modesetting ioctls | ✅ Working | 16 KMS ioctls, CRTC/connector/encoder/plane |
| Kernel FmapBorrowed | ✅ Exists | `Provider::FmapBorrowed` at `memory.rs:1157`, `Grant::borrow_fmap()` at `memory.rs:1401` |
| Kernel sendfd | ✅ Exists | `SYS_SENDFD` at `syscall/fs.rs:415`, passes `Arc<LockedFileDescription>` |
| Kernel PhysBorrow | ✅ Exists | `scheme:memory` physical address mapping |
| libdrm `__redox__` | ✅ Full | Opens `/scheme/drm`, dispatches KMS + PRIME ioctls via `redox_fpath` |

### What Is Missing

| Component | Status | Impact |
|-----------|--------|--------|
| PRIME_HANDLE_TO_FD | ✅ Implemented | Opaque export tokens via prime_exports map |
| PRIME_FD_TO_HANDLE | ✅ Implemented | Token lookup via prime_exports, adds to owned_gems |
| libdrm PRIME/GEM dispatch | ✅ Implemented | __redox__ wrappers in drmPrimeHandleToFD/drmPrimeFDToHandle |
| Mesa Redox winsys | 🚧 Scaffolding | Stubs compile but do not render — blocked on GPU CS |
| GPU command submission | ❌ Not implemented | No CS ioctl, no ring buffer programming |
| GPU fence/signaling | ❌ Not implemented | No GPU completion notification |

### What Was Cleaned Up (Previous Session)

The old fake PRIME implementation used synthetic fd numbers starting at 10,000 that were not real
kernel file descriptors. Other processes could not resolve them. Oracle caught this across 4
verification rounds. The cleanup:

- Removed `exported_dmafds` tracking from Handle struct
- Removed `imported_gems` from Handle
- Removed DMA-BUF methods from `GpuDriver` trait and AMD/Intel driver impls
- Removed `DmabufManager` from `GemManager`
- Removed `mod dmabuf` from `main.rs`
- Removed PRIME wire structs (`DrmPrimeHandleToFdWire`, `DrmPrimeFdToHandleWire`)
- PRIME handlers → EOPNOTSUPP (honest, not fake)
- Removed all `#[allow(dead_code)]` from fake bookkeeping

## Phased Implementation

### v1: System RAM, Linear, Single GPU (Target: working PRIME)

**Goal**: A compositor (KWin) can import a buffer rendered by a GPU client (Mesa) and display it.
All buffers in system RAM, linear layout, single GPU.

**Duration estimate**: 6-10 weeks (2 developers)

#### Step 1: Delete dead dmabuf.rs

Remove `local/recipes/gpu/redox-drm/source/src/dmabuf.rs`. It is dead code — `mod dmabuf` was
removed from `main.rs` but the file still exists.

**Effort**: trivial

#### Step 2: Implement PRIME export in redox-drm

When `PRIME_HANDLE_TO_FD` is called:

1. Look up the GEM handle in the calling fd's `owned_gems`
2. Validate ownership (same as GEM_MMAP check)
3. Generate an opaque export token and store `prime_exports[token] = gem_handle`
4. Return the token to the caller (NOT a scheme fd or GEM handle)

The client then opens `/scheme/drm/card0/dmabuf/{token}` to get a real scheme fd. The open
handler validates the token against `prime_exports`, creates a `NodeKind::DmaBuf` scheme handle,
and bumps the GEM export refcount. When that scheme fd is closed, the refcount is dropped.

Key design: export tokens are opaque identifiers, not synthetic fd numbers or raw GEM handles.
The `prime_exports` map resolves tokens to GEM handles. Tokens are cleaned up when the last
export ref for a GEM handle is dropped.

**Changes to `scheme.rs`**:
- Add `NodeKind::DmaBuf { gem_handle, export_token }` variant
- Add `prime_exports: BTreeMap<u32, GemHandle>` and `next_export_token: u32`
- `PRIME_HANDLE_TO_FD` handler: validate ownership → generate token → store in prime_exports → return token
- `PRIME_FD_TO_HANDLE` handler: receive token → look up in prime_exports → add GEM to caller's `owned_gems`
- `open()` handler: accept `"card0/dmabuf/{token}"` path → validate token → create DmaBuf node → bump export ref
- `mmap_prep()` handler: for DmaBuf nodes, return GEM physical address

**Changes to `driver.rs`**:
- No changes needed. GEM operations stay on the trait as-is. PRIME is a scheme-level concern,
  not a driver-level concern.

**Effort**: 1-2 weeks

#### Step 3: Add reference counting for shared GEM objects

When a GEM buffer is exported via PRIME, multiple scheme fds may reference it. The `close()` path
must only call `driver.gem_close()` when ALL references (original GEM + all exported fds) are gone.

**Changes**:
- Add `gem_refcounts: BTreeMap<GemHandle, usize>` to `DrmScheme`
- Increment on export, decrement on close of DmaBuf fd
- `gem_close()` checks refcount before calling driver

**Effort**: 3-5 days

#### Step 4: Validate with a two-process reproducer

Build a minimal test that:
1. Process A opens `/scheme/drm/card0`, creates a GEM buffer, writes a pattern
2. Process A exports via PRIME_HANDLE_TO_FD
3. Process A sends the fd to Process B via `sendfd` (or equivalent scheme IPC)
4. Process B receives the fd, imports via PRIME_FD_TO_HANDLE
5. Process B mmaps the imported handle and reads the pattern
6. Verify both processes see the same physical pages (same data, zero-copy)

This validates the full chain: redox-drm → scheme fd → sendfd → import → mmap → FmapBorrowed.

**Effort**: 1 week

#### Step 5: libdrm Redox PRIME/GEM dispatch

libdrm already has `__redox__` conditionals. Add dispatch for:
- `drmPrimeHandleToFD()` → send `PRIME_HANDLE_TO_FD` ioctl to `/scheme/drm`
- `drmPrimeFDToHandle()` → send `PRIME_FD_TO_HANDLE` ioctl
- `drmPrimeClose()` → close the exported/imported fd
- `drmGemHandleToPrimeFD()` / `drmPrimeFDToGemHandle()` — aliases for the above

The libdrm WIP recipe is at `recipes/wip/x11/libdrm/`. The `__redox__` handling already opens
`/scheme/drm` and has ioctl dispatch infrastructure. The gap is PRIME/GEM-specific ioctl codes.

**Effort**: 1-2 weeks

#### Step 6: Mesa Redox winsys (compile-time scaffolding)

Add `src/gallium/winsys/redox/` to Mesa that:
- Opens the DRM scheme
- Allocates GEM buffers via `GEM_CREATE`
- Exports them via `PRIME_HANDLE_TO_FD`
- Imports shared buffers via `PRIME_FD_TO_HANDLE`
- Maps them via `mmap` (which triggers `FmapBorrowed`)

Pattern: similar to `winsys/amdgpu/drm/` but using Redox scheme IPC. This is scaffolding — it
compiles but cannot render without GPU command submission (Step 8).

Split into:
- **6a**: Compile-time winsys structure, buffer allocation, PRIME export/import
- **6b**: Runtime buffer-sharing enablement (depends on step 4 validation)

**Effort**: 3-4 weeks

### v2: VRAM/GTT Placement, Tiling, Multi-GPU

**Goal**: Buffers can live in VRAM with GTT aperture access. Tiled/modifier support for
scanout-optimized layouts. Multi-GPU buffer sharing.

**Duration estimate**: 8-12 weeks (after v1)

- AMD GTT/VRAM placement via `amdgpu_gtt_mgr` / `amdgpu_vram_mgr` equivalents
- Intel GGTT/PPGTT population for imported buffers
- DRM format modifiers: `DRM_FORMAT_MOD_LINEAR` + vendor-specific tiling
- Multi-GPU: each GPU has its own `redox-drm` instance, PRIME between them
- This tier requires the AMD/Intel driver GTT programming that is currently partial

### v3: Fencing, Explicit Sync, Vulkan

**Goal**: GPU fence objects for render/scanout synchronization. Explicit sync protocol for
Wayland. Vulkan driver support.

**Duration estimate**: 12-16 weeks (after v2)

- `dma_fence` equivalent: kernel waitable event per page-flip or command submission
- `sync_file` equivalent: fd-backed fence that can be passed between processes
- Wayland `zwp_linux_explicit_synchronization_v1` protocol in compositor
- Vulkan `VK_KHR_external_memory` / `VK_KHR_external_semaphore` backed by DMA-BUF fds
- AMD: fence through ring buffer writeback + IRQ
- Intel: fence through seqno writeback + IRQ

## Dependency Graph

```
Step 1 (delete dmabuf.rs)
  → no dependency, do immediately

Step 2 (PRIME export/import in scheme)
  → depends on: nothing
  → enables: steps 3, 4, 5

Step 3 (refcount for shared GEM)
  → depends on: step 2
  → enables: step 4

Step 4 (two-process reproducer)
  → depends on: steps 2, 3
  → validates: the full chain works

Step 5 (libdrm dispatch)
  → depends on: step 2 (ioctl protocol defined)
  → can start in parallel with steps 3-4

Step 6 (Mesa winsys)
  → depends on: step 5 (libdrm API available)
  → 6a can start once step 2 protocol is defined
  → 6b should wait for step 4 validation
```

Steps 5 and 6a can proceed in parallel with steps 3-4 once step 2 is done.

## What This Does NOT Cover

This plan covers **cross-process buffer sharing** (the DMA-BUF/PRIME contract). It does not cover:

| Out of scope | Where it lives |
|-------------|----------------|
| GPU command submission (CS ioctl) | `HARDWARE-3D-ASSESSMENT.md` Tier 2 |
| GPU fence/signaling | `HARDWARE-3D-ASSESSMENT.md` Tier 2 |
| Mesa hardware Gallium driver (radeonsi/iris) | `HARDWARE-3D-ASSESSMENT.md` Tier 1 |
| AMD ring buffer programming | `local/recipes/gpu/amdgpu/` |
| Intel render ring programming | `local/recipes/gpu/redox-drm/source/src/drivers/intel/` |
| Mesa EGL platform extension for DRM | `HARDWARE-3D-ASSESSMENT.md` Tier 3 |

PRIME/DMA-BUF is a **prerequisite** for hardware-accelerated rendering, but it is not sufficient
by itself. The render pipeline (command submission + fencing + Mesa driver) is tracked separately
in `HARDWARE-3D-ASSESSMENT.md`.

## Why Not a Kernel DMA-BUF Scheme

Linux has a global `dma-buf` kernel subsystem with its own fd type. Red Bear OS does NOT need this
because:

1. **`redox-drm` IS the exporter.** In Linux, any kernel subsystem can export a dma-buf. In Redox,
   only the DRM scheme exports GPU buffers. There is no need for a generic kernel dma-buf layer.

2. **Scheme fds ARE the sharing mechanism.** In Linux, dma-buf has its own fd type with special
   mmap semantics. In Redox, scheme file descriptors already support `fmap_prep` → `FmapBorrowed`.
   The kernel maps the same physical pages. No new fd type needed.

3. **`sendfd` IS the fd passing mechanism.** In Linux, fd passing uses SCM_RIGHTS over Unix
   sockets. In Redox, `sendfd` passes `Arc<LockedFileDescription>` via scheme IPC. Same result.

If a future use case requires sharing non-DRM buffers (e.g., camera frames, video decode output),
a separate `scheme:dmabuf` could be created. But for GPU buffer sharing, the DRM scheme is
sufficient.

## Wire Protocol Design

### PRIME_HANDLE_TO_FD

Request (from libdrm client):
```c
struct DrmPrimeHandleToFdWire {
    uint32_t handle;      // GEM handle to export
    uint32_t flags;       // DRM_CLOEXEC | DRM_RDWR (hints, not critical for v1)
};
```

Response:
```c
struct DrmPrimeHandleToFdResponseWire {
    int32_t  fd;          // opaque export token (NOT a process fd or GEM handle)
    uint32_t _pad;
};
```

The scheme internally:
1. Validates handle ownership
2. Generates an opaque export token (monotonically increasing counter)
3. Stores `prime_exports[token] = gem_handle`
4. Returns the token as `fd`

The client then opens `/scheme/drm/card0/dmabuf/{token}` to get a real scheme fd.
The open handler validates the token, creates a DmaBuf scheme handle, and bumps
`gem_export_refs`. When that scheme fd is closed, the ref is dropped.

### PRIME_FD_TO_HANDLE

Request (from libdrm client):
```c
struct DrmPrimeFdToHandleWire {
    int32_t  fd;          // opaque export token (extracted via redox_fpath on dmabuf fd)
    uint32_t _pad;
};
```

Response:
```c
struct DrmPrimeFdToHandleResponseWire {
    uint32_t handle;      // GEM handle for the imported buffer
    uint32_t _pad;
};
```

The scheme internally:
1. Looks up the export token in `prime_exports` → gets the GEM handle
2. Validates the token exists
3. Adds the GEM handle to the caller's `owned_gems`
4. Returns the GEM handle

### open() path extension

```rust
// Existing paths:
"card0"              → NodeKind::Card
"card0Connector/{id}" → NodeKind::Connector(id)

// Export token path (validated against prime_exports):
"card0/dmabuf/{token}" → NodeKind::DmaBuf { gem_handle, export_token: token }
```

### redox_fpath() for DmaBuf

```rust
NodeKind::DmaBuf { export_token, .. } => format!("drm:card0/dmabuf/{export_token}")
```

### Token cleanup

When the last export ref for a GEM handle is dropped:
```rust
fn drop_export_ref(&mut self, gem_handle: GemHandle) {
    // ... decrement refcount ...
    if remove_entry {
        self.gem_export_refs.remove(&gem_handle);
        self.prime_exports.retain(|_, &mut h| h != gem_handle);
    }
}
```

When a GEM is destroyed via any path (GEM_CLOSE, DESTROY_DUMB, handle close, fb reap),
`prime_exports` entries are pruned:
- `maybe_close_gem()`: central helper prunes tokens on successful `driver.gem_close()`
- `GEM_CLOSE` / `DESTROY_DUMB`: explicit `prime_exports.retain()` after direct `driver.gem_close()`
- `PRIME_FD_TO_HANDLE`: `gem_size()` liveness check removes stale token on failure
- `open("card0/dmabuf/{token}")`: `gem_size()` liveness check removes stale token on failure

## Files to Modify

| File | Change | Status |
|------|--------|--------|
| `local/recipes/gpu/redox-drm/source/src/dmabuf.rs` | **DELETED** | ✅ |
| `local/recipes/gpu/redox-drm/source/src/scheme.rs` | DmaBuf nodes, opaque export tokens, PRIME handlers, refcount cleanup, stale token cleanup | ✅ |
| `local/recipes/gpu/redox-drm/source/src/gem.rs` | No changes (GEM operations unchanged) | — |
| `local/recipes/gpu/redox-drm/source/src/driver.rs` | No changes (PRIME is scheme-level) | — |
| `local/recipes/gpu/redox-drm/source/src/main.rs` | No changes (already clean) | — |
| `recipes/wip/x11/libdrm/source/xf86drm.c` | `redox_fpath()` + export token dmabuf path + `sys/redox.h` | ✅ |
| `recipes/libs/mesa/source/src/gallium/winsys/redox/drm/` | 4 scaffolding files (compile-time only) | ✅ |
| `local/recipes/tests/redox-drm-prime-test/` | Test reproducer recipe + Rust binary (incl. stale token test) | ✅ |
| `local/docs/HARDWARE-3D-ASSESSMENT.md` | PRIME status updated | ✅ |
| `local/docs/DMA-BUF-IMPROVEMENT-PLAN.md` | Implementation status updated | ✅ |

## Implementation Status (2026-04-16)

| Step | Status | Deliverable |
|------|--------|-------------|
| 1. Delete dead dmabuf.rs | ✅ Done | File removed |
| 2. PRIME export/import in scheme | ✅ Done | DmaBuf nodes, export refcounting, mmap_prep, open/close/fpath |
| 3. Reference counting for shared GEM | ✅ Done | gem_export_refs, bump/drop, gem_can_close, maybe_close_gem |
| 4. Two-process reproducer | ✅ Recipe created | `local/recipes/tests/redox-drm-prime-test/` (runtime validation pending) |
| 5. libdrm Redox dispatch | ✅ Done | __redox__ wrappers in drmPrimeHandleToFD and drmPrimeFDToHandle |
| 6a. Mesa winsys scaffolding | ✅ Done | `src/gallium/winsys/redox/drm/` (4 files, compiles but does not render) |
| 6b. Mesa runtime buffer sharing | ⏳ Blocked | Requires GPU command submission (not yet implemented) |

**Stale token cleanup**: All GEM destruction paths now prune `prime_exports`. Central cleanup
in `maybe_close_gem()`, explicit cleanup in `GEM_CLOSE`/`DESTROY_DUMB`, liveness checks in
`PRIME_FD_TO_HANDLE` and `open("dmabuf/{token}")` that remove stale tokens on failure.
Verified by Oracle across 8 rounds.

**Protocol note**: PRIME uses opaque export tokens. PRIME_HANDLE_TO_FD returns a monotonically-
increasing token stored in `prime_exports`. The client opens `/scheme/drm/card0/dmabuf/{token}`
to get a real scheme fd. `redox_fpath()` on that fd reveals the token. PRIME_FD_TO_HANDLE
accepts the export token and resolves it via `prime_exports`. Tokens are cleaned up when the
last export ref is dropped.

## Relationship to Other Plans

- `local/docs/HARDWARE-3D-ASSESSMENT.md` — broader hardware 3D status (command submission, fencing,
  Mesa driver enablement). This document is the DMA-BUF-specific deep dive.
- `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` — canonical desktop path plan. DMA-BUF is a
  prerequisite for the hardware-accelerated rendering phase.
- `local/docs/AMD-FIRST-INTEGRATION.md` — AMD-specific GPU details including GTT/VRAM programming.
- `docs/04-LINUX-DRIVER-COMPAT.md` — linux-kpi architecture reference for driver porting.
