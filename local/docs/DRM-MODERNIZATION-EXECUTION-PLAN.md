# Red Bear OS DRM Modernization Execution Plan

**2026-04-29 build verification update:** All individual DRM/Mesa recipes compile successfully (redox-driver-sys, linux-kpi, redox-drm, mesa/swrast, amdgpu, firmware-loader, iommu). amdgpu is now included in redbear-full (ignore removed from config). Hardware GPU rendering (command submission, fences, Mesa hardware winsys) remains blocked — these are large engineering tasks requiring GPU-architecture-specific work. See hard blockers below.
**Position in the doc set:** This is the single comprehensive GPU/DRM execution plan beneath `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md`. It does not replace the canonical desktop path. It is the canonical GPU/DRM plan and should be preferred over older GPU-specific planning docs when execution order, acceptance criteria, or claim language conflict.

**Supersedes as planning authority:**

- `local/docs/AMD-FIRST-INTEGRATION.md` for forward execution order
- `local/docs/HARDWARE-3D-ASSESSMENT.md` for roadmap ordering
- `local/docs/DMA-BUF-IMPROVEMENT-PLAN.md` for PRIME/render dependency ordering

Those documents remain useful as implementation detail, status, and historical/reference material, but this file is the single planning source of truth for GPU/DRM work.

## Title and intent

Red Bear OS already has meaningful DRM build-side progress. The next step is not to overclaim hardware support. The next step is to turn the current stack into an evidence-driven execution plan that treats modern Intel and AMD support at the same acceptance bar.

Equal priority here does **not** mean equal code volume, equal driver complexity, or identical sequencing inside each backend. It means Red Bear should require the same evidence quality, the same runtime gates, and the same acceptance standards before claiming modern Intel or AMD support.

## Scope boundaries

This plan covers:

- shared GPU substrate from `redox-driver-sys` through `linux-kpi`
- firmware delivery and GPU-facing runtime service readiness
- `redox-drm` shared DRM/KMS, GEM, PRIME, IRQ, and bounded command-submission surfaces
- Intel and AMD backend maturation inside `local/recipes/gpu/redox-drm/source/src/drivers/`
- userland handoff to `libdrm`, Mesa, GBM, EGL, and compositor/session layers
- runtime validation and claim discipline

This plan does **not** claim:

- completed hardware rendering on either vendor
- completed hardware validation on either vendor
- that display/KMS maturity implies render/3D maturity
- that Track C in the canonical desktop plan can bypass Track A runtime trust work

## Current-state summary

### Bottom line

The repo has real progress in shared DRM/KMS, GEM, PRIME, firmware plumbing, interrupt plumbing, and vendor backend structure. That is enough to justify a modernization plan. It is not enough to claim modern Intel or AMD GPU support yet.

### Current strengths

| Area | Current evidence | Repo grounding |
|---|---|---|
| GPU substrate | Present and build-visible | `local/recipes/drivers/redox-driver-sys/source/src/`, `local/recipes/drivers/linux-kpi/source/src/lib.rs` |
| Quirk-aware device policy | Present, data-driven, shared across drivers | `local/recipes/drivers/redox-driver-sys/source/src/quirks/mod.rs` |
| Firmware service | Present as real Redox daemon | `local/recipes/system/firmware-loader/source/src/main.rs` |
| DRM scheme daemon | Present and scheme-backed | `local/recipes/gpu/redox-drm/source/src/main.rs` |
| KMS ioctl surface | Implemented in shared scheme layer | `local/recipes/gpu/redox-drm/source/src/scheme.rs` |
| GEM allocation and mapping | Implemented in shared scheme and GEM manager | `local/recipes/gpu/redox-drm/source/src/gem.rs`, `local/recipes/gpu/redox-drm/source/src/scheme.rs` |
| PRIME and DMA-BUF style sharing | Implemented at scheme level | `local/docs/HARDWARE-3D-ASSESSMENT.md`, `local/docs/DMA-BUF-IMPROVEMENT-PLAN.md`, `local/recipes/gpu/redox-drm/source/src/scheme.rs` |
| AMD display backend | Build-visible on the bounded retained path, firmware-aware, interrupt-aware; amdgpu C port compiles | `local/recipes/gpu/redox-drm/source/src/drivers/amd/mod.rs`, `local/recipes/gpu/amdgpu/source/amdgpu_redox_main.c` |
| Intel display backend | Build-visible, GGTT and ring scaffolding present | `local/recipes/gpu/redox-drm/source/src/drivers/intel/mod.rs`, `.../intel/ring.rs` |
| Mesa userland base | Builds with EGL, GBM, OSMesa, software Gallium path (swrast) | `recipes/libs/mesa/recipe.toml` |
| AMD GPU C port (amdgpu) | ✅ Builds + included in redbear-full (2026-04-29) — C-language port using linux-kpi compatibility; `amdgpu = "ignore"` removed from config | `local/recipes/gpu/amdgpu/`, `config/redbear-full.toml` |
| redbear-full image | ✅ Rebuilt with amdgpu included (2026-04-29) — harddrive.img generated successfully | `build/x86_64/redbear-full/harddrive.img` |

### Hard blockers

| Blocker | Why it matters | Current evidence |
|---|---|---|
| General GPU command submission | Modern rendering cannot ship without it | `local/docs/HARDWARE-3D-ASSESSMENT.md` says render CS is still missing |
| GPU fence and completion signaling | Rendering correctness and sync depend on it | Same assessment calls out missing fences and sync |
| Runtime validation on real Intel and AMD hardware | Build-only status is not enough for support claims | Canonical desktop plan and desktop current-status doc both say hardware runtime validation is still missing |
| Mesa hardware winsys and renderer enablement | Hardware 3D path is blocked without it | `recipes/libs/mesa/recipe.toml` still builds `-Dgallium-drivers=swrast` |
| Imported-buffer GPU mapping and real render path maturity | PRIME sharing alone is not hardware rendering | `local/docs/HARDWARE-3D-ASSESSMENT.md` separates buffer sharing from actual rendering |

## Assessment findings

### 1. Shared substrate is real enough to build on

Red Bear already has the correct architectural layers for modern DRM work:

`redox-driver-sys -> linux-kpi -> firmware-loader -> redox-drm -> vendor backends -> libdrm/Mesa -> compositor/session`

That matters because the repo is not starting from a blank page. The modernization task is mainly about closing runtime and render-path gaps, not replacing the architecture.

Relevant files:

- `local/recipes/drivers/redox-driver-sys/source/src/quirks/mod.rs`
- `local/recipes/drivers/linux-kpi/source/src/lib.rs`
- `local/recipes/system/firmware-loader/source/src/main.rs`
- `local/recipes/gpu/redox-drm/source/src/main.rs`

### 2. Display/KMS maturity is ahead of render/3D maturity

This distinction must stay explicit in all future status claims.

Current evidence shows:

- shared KMS ioctls exist in `scheme.rs`
- shared GEM create, close, and mmap exist in `gem.rs` and `scheme.rs`
- PRIME export and import are implemented in `scheme.rs`
- AMD and Intel display backends both have connector, CRTC, and IRQ-facing structure

Current evidence does **not** show:

- general vendor-usable GPU CS ioctls for modern rendering
- fence objects or reliable completion waits at production quality
- Mesa hardware winsys closure and real hardware renderer proof

So the honest state is:

- **Display/KMS:** meaningful build-side maturity, bounded runtime validation still needed
- **Render/3D:** not mature, blocked on CS, fences, Mesa hardware path, and runtime proof

### 3. Shared DRM core is now a major leverage point

`local/recipes/gpu/redox-drm/source/src/scheme.rs` already centralizes the most important common control plane:

- mode resource queries
- connector and mode queries
- CRTC set and page flip
- dumb buffer and framebuffer lifecycle
- GEM lifecycle
- PRIME handle export and import
- bounded private CS submit and wait entry points

That means shared DRM core work can unblock both vendors, even when vendor-specific render work diverges later.

### 4. Vendor parity must be measured by evidence, not by line count

AMD and Intel are both first-class targets, but they are not symmetric engineering tasks. AMD has heavier firmware and backend complexity. Intel has a smaller stack but still needs the same support bar. The parity rule for this plan is therefore:

> No vendor is considered modern and supported until it clears the same evidence classes for display, render, userland integration, and runtime validation.

## Dependency graph

```text
Shared substrate
  redox-driver-sys
  linux-kpi
  firmware-loader
  PCI, IRQ, memory, quirks, firmware runtime
        |
        v
Shared DRM core
  redox-drm main/scheme/driver/gem
  KMS, GEM, PRIME, IRQ dispatch, bounded CS surface
        |
   +----+-------------------+
   |                        |
   v                        v
Intel track              AMD track
  display/gtt/ring         display/gtt/ring + amdgpu port
  connector runtime        firmware-backed display runtime
  GGTT mapping             GTT and VM programming
  render path closure      render path closure
   |                        |
   +-----------+------------+
               |
               v
Userland integration
  libdrm
  Mesa winsys
  GBM/EGL
  compositor/session
               |
               v
Validation and acceptance
  QEMU bounded checks
  real Intel hardware checks
  real AMD hardware checks
  renderer proof
  regression coverage
```

## Workstreams

### Workstream A, shared substrate hardening

**Goal:** Make the shared GPU-facing runtime substrate trustworthy enough that later failures are clearly DRM or backend bugs, not basic device-service failures.

**Primary dependencies:** none beyond current repo state.

**Tasks:**

| ID | Task | Why it matters | Repo references |
|---|---|---|---|
| A1 | Lock down quirk-source ownership and usage in GPU paths | AMD and Intel need one shared policy source for IRQ, IOMMU, firmware, and accel-disable decisions | `local/recipes/drivers/redox-driver-sys/source/src/quirks/mod.rs` |
| A2 | Validate runtime firmware service with real GPU-facing requests | AMD display path depends on honest firmware loading behavior | `local/recipes/system/firmware-loader/source/src/main.rs`, `local/recipes/gpu/redox-drm/source/src/main.rs`, `local/recipes/gpu/amdgpu/source/amdgpu_redox_main.c` |
| A3 | Validate interrupt delivery quality for both vendor paths | Display events, vblank flow, and later fence work depend on this | `local/recipes/gpu/redox-drm/source/src/drivers/interrupt.rs` |
| A4 | Keep shared substrate acceptance vendor-neutral | Prevent AMD-only or Intel-only claim drift | this plan + `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` |

**Exit gate:** both Intel and AMD can rely on the same substrate contracts for device discovery, quirks, IRQ policy, and firmware service behavior.

**Current implementation status for A1:**

- `redox-drm` shared-core and Intel init now consume canonical GPU quirk policy at the Rust driver boundary.
- imported DMA-BUF handles are explicitly kept outside the bounded private CS path in `scheme.rs`.
- `fsync` no longer pretends to be a successful render-fence contract when no shared sync contract exists.
- the AMD C backend still logs linux-kpi quirk-informed IRQ expectations, but firmware gating is no longer duplicated there.
- the PCI quirk extractor foundation has been upgraded so future reviewed GPU quirk imports can rely on explicit handler-body evidence instead of handler-name guessing.

**What A1 does not mean yet:** reviewed Linux 7.0 PCI extraction has not produced enough high-confidence modern Intel/AMD DRM GPU entries to replace the existing hand-authored GPU quirk set. Additional DRM-focused mining and review are still required before quirk-table expansion claims, and Intel-side quirk expansion remains deferred until the Intel runtime policy surface can consume those flags honestly.

**Current PCI ID naming policy:** human-readable PCI vendor/device naming now comes from the shipped
canonical `pciids` database, while DRM quirk policy remains on the reviewed Red Bear/Linux-backed
quirk path. Do not use the Linux quirk extractor as a substitute for PCI naming coverage.

**Current implementation status for A2:**

- `redox-drm` now makes Rust-side firmware preload expectations explicit before backend construction.
- preload policy is now explicit at the Rust DRM startup boundary: AMD still uses the canonical `NEED_FIRMWARE` signal, while the bounded Intel startup path uses a device-manifest-driven DMC requirement for the first covered Intel families.
- vendors with no Rust-side preload manifest are logged honestly rather than being treated as if firmware had been validated.
- AMD firmware preload errors now report the checked candidate set and summarize missing blobs, which makes the firmware service evidence surface more useful for runtime validation.
- both the Rust preload path and the AMD C firmware bridge now reject oversized firmware blobs before allocation, keeping firmware honesty from turning into unbounded memory requests.
- this is still preload honesty, not final real-hardware firmware-service proof; the runtime validation work in Stage 1 remains required.

**Required Intel follow-up under A2:**

- Red Bear must not treat Intel firmware as an afterthought. When an Intel platform actually needs firmware, the import/preload policy must run from startup at the same Rust-side boundary used for AMD.
- The Intel firmware classes that matter are distinct and should stay distinct in policy and docs:
- **DMC** — display-path firmware; required for modern display power management on Gen9+ style platforms
- **GuC** — scheduler / power-management firmware; important for render/runtime maturity
- **HuC** — media-offload firmware; optional for some features
- **GSC** — newer security/authentication controller needed for some modern Intel firmware flows
- Red Bear now has a bounded Intel-side startup manifest for display-critical **DMC** blobs at the Rust preload boundary.
- The first bounded implementation currently covers TGL, ADLP, DG2, and MTL DMC startup candidates and treats them as required from startup for the covered device families.
- Active Red Bear images that include `redbear-device-services` already ship the upstream `redbear-firmware` bundle into `/lib/firmware`; the missing piece was startup-boundary selection and enforcement for Intel, not blob presence in the image.
- `local/scripts/fetch-firmware.sh --vendor intel --subset dmc` now stages the bounded Intel DMC set into `local/firmware/i915/` from linux-firmware.
- Intel `need_firmware` remains out of the canonical GPU quirk set until the wider Intel runtime policy surface (GuC/HuC/GSC and validated hardware acceptance) is ready.
- Future Intel firmware import still expands in this order:
  1. keep the DMC startup manifest honest and validated,
  2. add GuC/HuC/GSC only when their runtime consumers exist,
  3. only then reintroduce any broader Intel `NEED_FIRMWARE` quirk policy.

### Workstream B, shared DRM core completion

**Goal:** Finish the common DRM control plane before pushing more vendor-specific divergence.

**Tasks:**

| ID | Task | Why it matters | Repo references |
|---|---|---|---|
| B1 | Audit and stabilize KMS, GEM, and PRIME interfaces as the shared baseline | Both vendors consume the same scheme surface | `local/recipes/gpu/redox-drm/source/src/scheme.rs`, `driver.rs`, `gem.rs` |
| B2 | Keep command-submission entry points honest and bounded until real backend support exists | Avoid fake hardware-rendering claims | `local/recipes/gpu/redox-drm/source/src/driver.rs`, `scheme.rs` |
| B3 | Define fence and wait semantics in the shared layer before backend claims expand | Prevent each backend from inventing incompatible completion models | `driver.rs`, IRQ handling in `main.rs` and vendor modules |
| B4 | Separate display acceptance from render acceptance in all docs and tests | Prevent status inflation | this plan, `local/docs/HARDWARE-3D-ASSESSMENT.md` |

**Exit gate:** Red Bear has one clear shared DRM contract for display and one explicit, evidence-backed roadmap for render completion.

**Current implementation status for B2:**

- `driver.rs` now exposes explicit bounded private CS submit/wait contract types with unsupported backends rejecting them honestly by default.
- `scheme.rs` validates handle ownership for private CS paths, rejects imported DMA-BUF handles in the bounded path, bounds source/destination ranges against GEM sizes, and returns `EOPNOTSUPP` for fake or unsupported synchronization paths instead of silently succeeding.
- `scheme.rs` also caps `GEM_CREATE` and `CREATE_DUMB` at a shared-core trusted size limit, and `GemManager` enforces the same cap as a second line of defense.
- unit tests now cover the shared contract for unsupported waits, imported-buffer rejection, out-of-bounds rejection, local-buffer submission reachability, and `fsync` honesty.

**Current implementation status for B3 groundwork:**

- the raw `(crtc_id, vblank_count)` IRQ tuple path has been replaced with a small shared driver-event model for internal driver → main loop → scheme transport.
- `scheme.rs` now owns event ingestion through a shared helper, so page-flip retirement remains tied to explicit vblank events while non-vblank events do not pretend to be render completion.
- both Intel and AMD now forward shared hotplug events through the same internal event path instead of backend-specific side handling.
- `scheme.rs` now turns shared hotplug and vblank events into a queued scheme-visible `EVENT_READ` surface for `card0`, and hotplug also targets the matching connector handle.
- unit tests now cover card-level hotplug readiness, connector-targeted hotplug readiness, queued vblank delivery, and event draining, while preserving the rule that non-vblank events do not retire pending page flips.
- this is structural groundwork only; real fence objects, sync waits, and backend-proven render completion semantics are still not implemented.

### Workstream C, Intel backend maturation

**Goal:** Turn the Intel path from build-visible DRM code into an evidence-backed modern Intel track.

**Tasks:**

| ID | Task | Why it matters | Repo references |
|---|---|---|---|
| C1 | Validate connector discovery, modes, and bounded modeset on real Intel hardware | First honest Intel display bar | `local/recipes/gpu/redox-drm/source/src/drivers/intel/mod.rs` |
| C2 | Add real Intel firmware manifest + startup preload policy at the Rust driver boundary | Intel firmware must be imported from the start when the platform needs it | `local/recipes/gpu/redox-drm/source/src/main.rs`, `.../drivers/intel/mod.rs`, `local/docs/QUIRKS-IMPROVEMENT-PLAN.md` |
| C3 | Validate GGTT-backed GEM mapping at runtime | Render-path groundwork depends on this | `.../intel/mod.rs`, `.../intel/gtt.rs` |
| C4 | Close Intel render-ring submission path from bounded proof to usable DRM backend work | Modern rendering needs real command submission | `.../intel/ring.rs`, `.../intel/mod.rs` |
| C5 | Connect Intel backend completion signaling to shared fence semantics | Render correctness depends on it | `.../intel/mod.rs`, `driver.rs` |
| C6 | Prove Intel path in userland with Mesa and compositor evidence | Support claims must reach user-visible surfaces | `recipes/libs/mesa/recipe.toml`, compositor/session docs |

**Exit gate:** Intel clears both display acceptance and render acceptance criteria, not just code compilation.

### Workstream D, AMD backend maturation

**Goal:** Turn the AMD path from a bounded retained display build plus broader imported amdgpu/DC triage into an evidence-backed modern AMD track.

**Tasks:**

| ID | Task | Why it matters | Repo references |
|---|---|---|---|
| D1 | Validate firmware-backed connector discovery, modes, and bounded modeset on real AMD hardware | AMD display path is firmware-sensitive | `local/recipes/gpu/redox-drm/source/src/drivers/amd/mod.rs`, `local/recipes/gpu/amdgpu/source/amdgpu_redox_main.c` |
| D2 | Validate GTT and VM programming against real runtime behavior | Imported and local buffer mapping depend on it | `.../amd/gtt.rs`, `.../amd/mod.rs` |
| D3 | Expand AMD ring work from bounded copy and page-flip support toward real render submission | PRIME and page flip alone do not produce hardware rendering | `.../amd/ring.rs`, `scheme.rs`, `driver.rs` |
| D4 | Connect AMD interrupt and completion behavior to shared fence semantics | Stable render completion needs it | `.../amd/mod.rs`, `main.rs` |
| D5 | Prove AMD path in userland with Mesa and compositor evidence | Support claims must reach user-visible surfaces | `recipes/libs/mesa/recipe.toml`, compositor/session docs |

**Exit gate:** AMD clears both display acceptance and render acceptance criteria, not just backend compilation.

**Current bounded validation tooling:**

- `redbear-drm-display-check` is now the in-guest bounded DRM display checker for Stage 3 entry evidence.
- `local/scripts/test-drm-display-runtime.sh` provides the shared shell wrapper around that checker.
- `local/scripts/test-amd-gpu.sh` and `local/scripts/test-intel-gpu.sh` are thin vendor wrappers over that shared harness.
- The checker now proves connector/mode enumeration directly against the Red Bear DRM ioctl surface and can perform a bounded direct modeset proof. This remains display-only evidence, not render proof.

### Workstream E, userland DRM integration

**Goal:** Turn the working DRM scheme and vendor backends into a userland path that real graphics stacks can use honestly.

**Tasks:**

| ID | Task | Why it matters | Repo references |
|---|---|---|---|
| E1 | Keep libdrm aligned with Redox DRM node and PRIME behavior | It is the first userland contract above the scheme | referenced by `local/docs/HARDWARE-3D-ASSESSMENT.md` |
| E2 | Add real Mesa Redox winsys work for hardware drivers | Hardware rendering is blocked without it | `local/docs/HARDWARE-3D-ASSESSMENT.md`, `local/docs/DMA-BUF-IMPROVEMENT-PLAN.md` |
| E3 | Move Mesa recipe from software-only evidence to dual software plus hardware candidate builds | Current recipe still proves software only | `recipes/libs/mesa/recipe.toml` |
| E4 | Keep compositor and session integration downstream from honest DRM evidence | Avoid blaming KWin or Plasma for missing GPU core work | `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` |

**Exit gate:** userland can distinguish software fallback from true hardware-backed renderers, and the repo has evidence for both cases.

### Workstream F, validation and claims discipline

**Goal:** Make support claims depend on repeatable evidence instead of code presence.

**Tasks:**

| ID | Task | Why it matters |
|---|---|---|
| F1 | Maintain separate validation tracks for shared core, Intel, AMD, and userland integration | One passing path must not mask another failing path |
| F2 | Keep QEMU checks bounded and honest | QEMU is useful for shared control-plane checks, not final hardware claims |
| F3 | Require real Intel and real AMD evidence before modern support claims | Equal acceptance bar is the heart of this plan |
| F4 | Treat display and render as separate acceptance surfaces | Avoid overclaiming based on modeset-only proof |

## Milestones and phases

This plan does not reuse the old historical P0-P6 numbering.

### Stage 1, substrate trust for DRM work

**Goal:** Shared device-service and runtime prerequisites are trustworthy enough for GPU validation.

**Must complete:** A1, A2, A3.

**Exit statement:** Shared GPU substrate is credible enough to support vendor DRM validation.

### Stage 2, shared DRM core trust

**Goal:** Shared DRM/KMS, GEM, PRIME, and bounded CS surfaces are stable and honestly documented.

**Must complete:** B1, B2, B3, B4.

**Exit statement:** Red Bear has a stable shared DRM control plane and an explicit line between display proof and render proof.

### Stage 3, vendor display acceptance

**Goal:** Intel and AMD both achieve bounded, evidence-backed display/KMS validation.

**Must complete:** C1 and D1.

**Exit statement:** Both vendors can clear the same display acceptance bar on real hardware.

### Stage 4, vendor render-path closure

**Goal:** Intel and AMD both close their backend-specific command submission and fence gaps enough to support hardware render claims.

**Must complete:** C2, C3, C4, D2, D3, D4, plus shared fence model work from B3.

**Exit statement:** Both vendors have a real render path, not just display and buffer-sharing support.

### Stage 5, userland hardware rendering proof

**Goal:** Mesa, GBM, EGL, and compositor/session layers can exercise the hardware path honestly.

**Must complete:** E1, E2, E3, E4, plus at least one bounded compositor proof on each vendor.

**Exit statement:** The Red Bear desktop path can consume real hardware rendering rather than software fallback.

### Stage 6, support-language cleanup and maintenance mode

**Goal:** Remove temporary shims, stale claims, duplicated policy, and documentation drift left over from bring-up.

**Must complete:** cleanup priorities below.

**Exit statement:** Support claims, code ownership, and docs all describe the same reality.

## Validation matrix

### Evidence classes

| Evidence class | Meaning | Can it support a support claim? |
|---|---|---|
| Builds | code compiles and links | No |
| Bounded runtime | daemon or backend starts and answers limited queries | Not by itself |
| Real display proof | real hardware modes, connectors, and bounded modeset evidence | Yes, for display only |
| Real render proof | real hardware renderer path, command submission, completion, visible client rendering | Yes, for render |
| Regression coverage | repeatable validation that protects the claim | Required to keep the claim |

### Acceptance matrix

| Surface | Shared core | Intel | AMD | Userland |
|---|---|---|---|---|
| Scheme registration | required | inherits | inherits | n/a |
| Connector and mode queries | required | real hardware proof required | real hardware proof required | consumed through libdrm |
| Modeset | required | real hardware proof required | real hardware proof required | compositor-visible proof required |
| GEM lifecycle | required | runtime proof required | runtime proof required | Mesa/libdrm use must match |
| PRIME import/export | required | runtime proof required | runtime proof required | zero-copy handoff proof required |
| Command submission | shared contract required | real backend proof required | real backend proof required | hardware renderer proof required |
| Fence and wait semantics | shared contract required | runtime proof required | runtime proof required | compositor and client sync proof required |
| Hardware-backed renderer | n/a | required for Intel render claim | required for AMD render claim | must be visible as non-LLVMpipe |

## Explicit Intel and AMD parity criteria

Modern Intel and AMD support are at parity only when **both** vendors satisfy all of the following.

### Display parity criteria

- real hardware device detection on the vendor path
- real connector discovery and stable mode enumeration
- bounded modeset proof on real hardware
- bounded post-modeset framebuffer transition evidence on real hardware
- no dependence on unsupported or fake runtime shortcuts for the claim

### Render parity criteria

- real backend command submission path exists and is exercised
- completion and wait semantics are real, not stubbed
- imported and local buffers follow the same lifetime rules the shared DRM core documents
- Mesa or equivalent userland path can reach a hardware-backed renderer on that vendor
- compositor or graphics client proof shows hardware path, not LLVMpipe fallback

### Evidence parity criteria

- same evidence class on both vendors for each claim surface
- same claim discipline in docs and status files
- same requirement for repeatable validation artifacts before broad support language is used

### Non-goals for parity

Parity does **not** require:

- equal line counts
- equal implementation strategy
- equal schedule length
- identical hardware-family coverage on day one

It does require equal honesty.

## Cleanup priorities

### Priority 1, remove claim drift

- update status language anywhere display progress might be read as hardware render support
- keep `local/docs/HARDWARE-3D-ASSESSMENT.md`, this plan, and `local/docs/DESKTOP-STACK-CURRENT-STATUS.md` aligned
- keep Track C language in `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` aligned with this DRM plan

### Priority 2, converge on one shared policy source

- keep quirk policy centered in `redox-driver-sys`
- avoid per-driver policy drift for IRQ, firmware, and accel-disable behavior
- keep `linux-kpi` as a compatibility layer, not a second policy authority

### Priority 3, retire bring-up-only abstractions once real ones exist

- remove temporary bounded CS paths once real backend submission paths replace them
- remove any stale support wording attached to compile-only features
- collapse duplicate validation helpers once vendor/runtime coverage is real and stable

### Priority 4, keep userland truth honest

- only expand Mesa driver enablement when the winsys and backend contracts are ready
- do not treat PRIME completion alone as hardware rendering completion
- keep compositor and session failures separate from missing DRM core work

## Recommended execution order

1. complete shared substrate trust work
2. stabilize shared DRM core contracts
3. validate display/KMS on real Intel and AMD hardware at the same acceptance bar
4. close backend-specific render submission and fence gaps
5. enable userland hardware rendering path honestly
6. clean up temporary bring-up surfaces and support language

## Relationship to existing docs

| Document | Role relative to this plan |
|---|---|
| `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` | Canonical desktop execution plan. This DRM plan is a lower-level execution plan for its hardware GPU track. |
| `local/docs/HARDWARE-3D-ASSESSMENT.md` | Current factual assessment of the render-path gap. |
| `local/docs/DMA-BUF-IMPROVEMENT-PLAN.md` | Detailed buffer-sharing and PRIME work beneath the render path. |
| `local/docs/DESKTOP-STACK-CURRENT-STATUS.md` | Current truth summary for package, runtime, and session state. |

## Final operating rule

Red Bear should speak about Intel and AMD modern DRM support in the same way it speaks about any other first-class subsystem.

Code presence is not support.
Build success is not support.
Modeset proof is not render proof.
One vendor passing does not cover the other.

The claim bar is shared. The implementation paths can differ. The evidence bar cannot.
