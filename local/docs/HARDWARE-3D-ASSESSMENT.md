# Red Bear OS: Hardware-Accelerated 3D Assessment

**Date**: 2026-04-18
**Scope**: AMD + Intel GPU hardware OpenGL/Vulkan for KDE Plasma desktop

> **Planning authority note (2026-04-18):** this file is the current render-gap assessment and
> dependency reference. It is no longer the canonical GPU/DRM execution plan; use
> `local/docs/DRM-MODERNIZATION-EXECUTION-PLAN.md` for sequencing and acceptance criteria.

## Bottom Line

PRIME/DMA-BUF cross-process buffer sharing is **now implemented** at the scheme level. GEM
allocation, PRIME export/import, and zero-copy mmap via FmapBorrowed all work through the
redox-drm scheme daemon and libdrm. The remaining gaps for hardware 3D are GPU command
submission (CS ioctl), GPU fence/signaling, and Mesa hardware Gallium driver enablement.
These are tracked separately in `local/docs/DMA-BUF-IMPROVEMENT-PLAN.md`.

## Capability Stack

```
Application (KDE Plasma / Qt6 / Wayland compositor)
        ↓
EGL / GBM / Wayland protocol
        ↓
Mesa (Gallium state tracker → hardware driver)        ← ONLY swrast (CPU), Redox winsys scaffolding exists
        ↓
libdrm (userspace DRM wrapper)                         ← __redox__ PRIME dispatch ✅, opens /scheme/drm
        ↓
DRM scheme ioctls (GEM, PRIME, render)                 ← GEM ✅, PRIME ✅ (DmaBuf nodes), bounded private CS surface ✅, real render path ❌
        ↓
redox-drm (userspace DRM/KMS daemon)                   ← display ✅, buffer sharing ✅, render ❌
        ↓
Kernel (FmapBorrowed, sendfd, GPU interrupts)          ← buffer sharing ✅, GPU fences ❌
        ↓
GPU hardware (AMD RDNA / Intel Gen)
```

## Layer-by-Layer Status

### 1. GPU Hardware Drivers (redox-drm + amdgpu + linux-kpi)

| Component | Status | Lines | What's Implemented |
|-----------|--------|-------|-------------------|
| DRM/KMS modesetting | ✅ Code complete | ~500 | 16 KMS ioctls, CRTC/connector/encoder/plane |
| AMD display backend (bounded retained path) | ✅ Builds | ~2 C glue files + Rust FFI surface | Red Bear display glue (`amdgpu_redox_main.c`, `redox_stubs.c`) plus the Rust FFI consumer build; imported Linux AMD DC/TTM/core remain under compile triage |
| Intel Display Driver | ✅ Compiles | ~800 | Display pipe, GGTT, forcewake |
| GEM buffer management | ✅ Full | ~350 | create/close/mmap with DmaBuffer |
| GEM scheme ioctls | ✅ Wired | ~100 | GEM_CREATE, GEM_CLOSE, GEM_MMAP |
| PRIME scheme ioctls | ✅ Implemented | ~120 | PRIME_HANDLE_TO_FD + PRIME_FD_TO_HANDLE via DmaBuf nodes + export refcounting |
| libdrm PRIME dispatch | ✅ Implemented | ~30 | __redox__ wrappers: open dmabuf path + fpath-based GEM handle extraction |
| Mesa Redox winsys | 🚧 Scaffolding | ~4 files | Directory structure + stubs in src/gallium/winsys/redox/drm/ |
| Render command submission | ⚠️ Bounded shared surface only | small shared slice | private CS contract exists, but no vendor-usable render ioctl or ring programming |
| GPU context management | ❌ Missing | 0 | No context create/destroy |
| Fence/sync objects | ❌ Missing | 0 | No shared backend-complete GPU fence signaling |
| AMD ring buffer | ⚠️ Partial | ~100 | Page flip only, no general command submission |

### 2. Mesa Build Configuration

| Setting | Current Value | Needed for HW 3D |
|---------|--------------|-------------------|
| `gallium-drivers` | `swrast` | `swrast,radeonsi` (AMD) or `swrast,iris` (Intel) |
| `vulkan-drivers` | `swrast` | `swrast,amd` (RADV) or `swrast,intel` (ANV) |
| `platforms` | `redox` | `redox` (same) |
| EGL | enabled | enabled (same) |
| GBM | enabled | enabled (same) |
| `gallium-winsys` | none (swrast doesn't need one) | New Redox winsys for radeonsi/iris |
| `egl/platform_redox.c` | 540 lines, legacy display-backed | Needs DRM backend for HW buffers |

### 3. Kernel Infrastructure

| Feature | Status | Impact |
|---------|--------|--------|
| PCI enumeration | ✅ | GPU devices discovered |
| Memory scheme (phys mmap) | ✅ | GPU register access works |
| IRQ scheme (MSI-X) | ✅ | GPU interrupts can be delivered |
| DMA-BUF fd passing | ✅ Scheme-level | FmapBorrowed + sendfd + DmaBuf nodes enable zero-copy cross-process sharing |
| GPU fence/wait | ❌ | No shared backend-complete GPU completion signaling |
| IOMMU/GPU page tables for imports | ❌ | Imported buffers can't be mapped into GPU GTT |

## The Render Path Gap

For hardware OpenGL, the data path is:

```
Mesa Gallium (radeonsi)
  → libdrm open("drm:card0")
  → DRM_IOCTL_GEM_CREATE (allocate GPU buffer)          ← EXISTS
  → DRM_IOCTL_PRIME_HANDLE_TO_FD (export for sharing)   ← ✅ IMPLEMENTED (DmaBuf node + scheme fd)
  → bounded private CS submit surface                    ← EXISTS, but not a real vendor render path
  → DRM_IOCTL_AMDGPU_CS (submit commands to GPU)         ← DOES NOT EXIST
  → fence wait (GPU completion)                          ← DOES NOT EXIST
  → present via KMS (PAGE_FLIP)                          ← EXISTS
```

Steps 1-2 now have full scheme ioctl support with cross-process buffer sharing via DmaBuf scheme
nodes, sendfd, and FmapBorrowed. There is now also a bounded private CS contract used to harden
shared DRM semantics, but steps 3-4 (real vendor command submission, fencing) remain the critical
gaps. The shared-core path now also applies explicit allocation caps for GEM and dumb-buffer
creation. The buffer sharing foundation is in place — compositors and clients can share GPU buffers
zero-copy. PRIME export now uses opaque non-guessable tokens rather than synthetic fd numbers.
The missing piece is still GPU command submission for actual rendering.

## What Was Implemented

| Change | Before | After |
|--------|--------|-------|
| `DRM_IOCTL_GEM_CREATE` | Not in scheme | Full ioctl handler: allocate GEM buffer, track ownership |
| `DRM_IOCTL_GEM_CLOSE` | Not in scheme | Full ioctl handler with ownership check |
| `DRM_IOCTL_GEM_MMAP` | Not in scheme | Full ioctl handler: return virtual address |
| `DRM_IOCTL_PRIME_HANDLE_TO_FD` | EOPNOTSUPP | Full implementation: opaque export tokens, prime_exports map, dmabuf fd creation |
| `DRM_IOCTL_PRIME_FD_TO_HANDLE` | EOPNOTSUPP | Full implementation: accepts export token (from redox_fpath), resolves via prime_exports |
| `libdrm __redox__ PRIME` | Not present | drmPrimeHandleToFD opens dmabuf path via export token; drmPrimeFDToHandle extracts token via redox_fpath |
| `NodeKind::DmaBuf` | Not present | DmaBuf node with mmap_prep returning GEM virtual address (enables FmapBorrowed) |
| `gem_export_refs` tracking | Not present | BTreeMap refcount for shared GEM objects, prevents premature gem_close |
| Mesa winsys scaffolding | Not present | src/gallium/winsys/redox/drm/ stub directory structure |

## What Remains (Ordered by Dependency)

### Tier 1: Can be done without kernel changes

1. **Mesa Gallium hardware driver enablement** — Change recipe from `-Dgallium-drivers=swrast` to
   include `radeonsi` or `iris`. This will fail to build without a winsys, but the attempt reveals
   the exact Mesa-side gaps.

2. **Redox Mesa winsys** — Scaffolding exists at `src/gallium/winsys/redox/drm/` (compile-time
   stubs). Needs real implementation of buffer allocation, PRIME export/import, and mmap.
   PRIME ioctls are now implemented in redox-drm and libdrm has `__redox__` dispatch.

3. **libdrm Redox backend** — libdrm already has `__redox__` conditional handling, opens
   `/scheme/drm`, and dispatches PRIME ioctls via `redox_fpath()` and dmabuf path opening.
   The remaining gap is GPU-family-specific command submission ioctls.

### Tier 2: Requires kernel work

4. **GPU command submission** — The amdgpu and Intel drivers need ring buffer programming for
   3D command submission, not just page flip. This is GPU-family-specific:
   - AMD: GFX ring, compute ring, SDMA ring
   - Intel: render ring, blitter ring

6. **GPU fence/signaling** — After submitting commands, the kernel needs to signal completion
   back to userspace. This requires IRQ handling that maps GPU interrupts to fence objects.

### Tier 3: Requires significant new code

7. **GTT/PPGTT population for imported buffers** — When Mesa imports a DMA-BUF into the GPU,
   the buffer's physical pages must be mapped into the GPU's address space. Currently only
   internally-allocated GEM objects get GTT mappings.

8. **Mesa EGL platform extension** — `platform_redox.c` currently uses the legacy display backend for buffer
   management. It needs an alternative path that uses DRM GEM for hardware-accelerated
   surfaces.

## Estimated Effort (2 developers)

| Tier | Duration | Deliverable |
|------|----------|-------------|
| Tier 1 (userspace) | 8-16 weeks | Mesa builds with radeonsi, winsys talks to DRM scheme |
| Tier 2 (kernel/driver) | 12-20 weeks | GPU command submission, fences, VRAM placement |
| Tier 3 (integration) | 6-12 weeks | Hardware-accelerated OpenGL applications |
| **Total** | **26-48 weeks** | **Hardware 3D on AMD** |

Intel (iris) is expected to be faster than AMD (radeonsi is ~6M lines vs iris ~400k) but both are
equal-priority Red Bear OS targets. The order of enablement is driven by driver complexity, not
platform priority.

## Relationship to Other Plans

- `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` — Phase 5 covers hardware GPU enablement
- `local/docs/AMD-FIRST-INTEGRATION.md` — AMD-specific GPU driver details
- `local/docs/AMDGPU-DC-COMPILE-TRIAGE-PLAN.md` — AMD DC compile-triage and bounded source-set strategy
- `docs/04-LINUX-DRIVER-COMPAT.md` — linux-kpi architecture reference
