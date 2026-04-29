# Red Bear OS Wayland Implementation Plan

**Version:** 1.0 (2026-04-19)
**Status:** Canonical Wayland subsystem plan
**Supersedes:** `docs/03-WAYLAND-ON-REDOX.md` as the active Wayland planning document

## Purpose

This is the single authoritative Red Bear Wayland subsystem plan.

It replaces the planning role previously held by `docs/03-WAYLAND-ON-REDOX.md` and consolidates the
current Wayland story into one document that answers four questions clearly:

1. what in the Wayland stack actually builds,
2. what has runtime proof,
3. what still blocks a trustworthy compositor/session claim,
4. and what work must happen next, in what order, to close those gaps.

This plan is subordinate to the canonical desktop path in
`local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` and to the current build/runtime truth in
`local/docs/DESKTOP-STACK-CURRENT-STATUS.md`, but it is the canonical subsystem plan for the
Wayland layer beneath that desktop path.

## Truth Statement

Red Bear Wayland is **partially complete and still experimental**.

What is true today:

- the base package stack is substantially build-visible: `libwayland`, `wayland-protocols`, Mesa
  EGL/GBM/GLES2, Qt Wayland, libinput, seatd, and KWin-related package surfaces all build in some
  form,
- the tracked Wayland validation profile, `redbear-wayland`, builds and boots in QEMU,
- the bounded validation path reaches compositor early init, xkbcommon initialization, and Redox EGL
  platform selection,
- `qt6-wayland-smoke` is a real bounded client-side proof target,
- but there is still **no complete Wayland compositor session**, **no runtime-trusted input/session
  path**, and **no hardware-accelerated Wayland proof**.

This means Wayland is no longer blocked mainly by package absence. It is blocked by the gap between
**build-visible packaging** and **runtime-trusted compositor/session behavior**.

## Scope

This plan covers the Red Bear Wayland subsystem from protocol/runtime substrate up to a bounded
working compositor session, and then its handoff into the KWin desktop path.

In scope:

- `libwayland`, `wayland-protocols`, protocol generation, and residual patch reduction,
- the `redbear-wayland` validation profile,
- compositor runtime validation,
- evdevd / udev-shim / libinput / seatd integration as they affect Wayland,
- Mesa/GBM/EGL software-path proof and the Wayland-facing graphics runtime,
- KWin as the intended production Wayland compositor path,
- local overlay ownership decisions for Wayland components and validation harnesses.

Out of scope:

- full KDE Plasma session assembly beyond its Wayland-facing dependencies,
- hardware GPU render enablement strategy in detail (owned by the DRM plan),
- Wi-Fi, Bluetooth, USB, and low-level controller work except where they directly block Wayland
  runtime trust.

## Authority Chain

Use the doc set in this order:

1. `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` — top-level desktop sequencing authority
2. `local/docs/DESKTOP-STACK-CURRENT-STATUS.md` — current desktop/Wayland truth
3. `local/docs/WAYLAND-IMPLEMENTATION-PLAN.md` — Wayland subsystem plan beneath the desktop path
4. `local/docs/DRM-MODERNIZATION-EXECUTION-PLAN.md` — GPU/DRM execution detail
5. `local/docs/QT6-PORT-STATUS.md` — Qt/KF6/KWin package-level build status

The following are historical or reference-only after this plan:

- `docs/05-KDE-PLASMA-ON-REDOX.md` — historical KDE rationale
- older WIP compositor notes such as the `smallvil` path — historical bounded validation references

## Evidence Model

This plan uses the same strict evidence classes as the canonical desktop path:

| Class | Meaning | Safe to say | Not safe to say |
|---|---|---|---|
| **builds** | package compiles and stages | “builds” | “works” |
| **boots** | image reaches prompt or known runtime surface | “boots” | “desktop works” |
| **enumerates** | scheme/device node appears and answers bounded queries | “enumerates” | “usable end to end” |
| **usable** | bounded runtime path performs intended task | “usable for this path” | “broadly stable” |
| **validated** | repeated proof on intended target class | “validated” | “complete everywhere” |
| **experimental** | partial, scaffolded, or runtime-untrusted | “experimental” | “done” |

Rules:

- compile-only success is still only **builds**,
- QEMU-only success stays QEMU-bounded,
- a compositor that reaches early init but never completes a session is still **experimental**,
- KWin and Plasma build success does not imply Wayland session viability.

## Current State Assessment

### Stable enough to rely on for planning

| Area | Current state | Notes |
|---|---|---|
| `redbear-wayland` profile | builds, boots | bounded validation profile only |
| `libwayland` | builds | still carries Redox-specific recipe/source rewriting and residual patching |
| `wayland-protocols` | builds | protocol packaging is not the blocker |
| Qt6 Wayland client path | builds, partial runtime | `qt6-wayland-smoke` is installed, runs in the bounded harness, and leaves runtime markers; visible in-compositor window proof is still open |
| Mesa EGL + GBM + GLES2 | builds | software path via LLVMpipe proven in QEMU |
| evdevd / udev-shim / firmware-loader / redox-drm | builds, boots, enumerate | runtime trust still bounded |
| libinput | builds | udev disabled in recipe; runtime integration still open |
| seatd | builds | runtime trust still open; lease path still unproven |
| KWin | stub (cmake configs + wrapper scripts delegating to redbear-compositor) | real KWin build requires Qt6Quick/QML downstream proof |

### What remains incomplete

| Area | Current gap |
|---|---|
| Compositor runtime | no complete Wayland compositor session |
| Input path | no end-to-end proof that evdevd → libinput → compositor is trustworthy |
| Session path | no runtime-trusted seat/session proof for KWin path |
| Hardware graphics | no hardware-accelerated Wayland proof |
| KWin truthfulness | build is reduced and partially dependency-honest, but still not a runtime-ready session |
| WIP ownership | upstream WIP recipes and local overlays are mixed; forward path is not always explicit |

## Stability / Completeness Verdict

### Stability

Wayland is **not stable enough** for a broad support claim.

Reason:

- runtime proof is still limited to a bounded QEMU validation harness,
- the compositor path reaches early init but not a complete session,
- input/session integration is not yet runtime-trusted,
- the intended production path (KWin) is still runtime-incomplete.

### Completeness

Wayland is **build-substantially-complete but runtime-incomplete**.

The stack is no longer missing its main package layers. It is missing:

- complete compositor runtime proof,
- complete input/session integration proof,
- hardware-path proof,
- and a cleaner local ownership story for the forward path versus historical references.

## Main Gaps and Blockers

### G1. Runtime trust trails build success

This is the biggest real blocker.

Current examples:

- `libwayland` builds, but runtime behavior is not yet trusted as a full compositor foundation,
- libinput builds, but its runtime path through evdevd/udev-shim is still open,
- seatd builds, but the compositor/session path still lacks runtime proof,
- `redox-drm` enumerates and supports bounded display tooling, but Wayland compositor runtime is not
  yet trusted on top of it.

### G2. No complete compositor session

The bounded validation compositor path is still an **early-init harness**, not a working session.

Current proof stops at:

- launch surface present,
- xkbcommon init reached,
- Redox EGL platform selected,
- Qt smoke markers present.

That is useful, but it is still not the same thing as:

- a visible, durable Wayland session,
- a client that connects and stays usable,
- input routing proven through the compositor,
- or a trustworthy handoff into KWin session work.

### G3. KWin remains the intended path, but it is still runtime-incomplete

KWin is the forward compositor direction, not smallvil or COSMIC.

Current truth:

- the recipe exists,
- the reduced path is more honest than before,
- but it still carries disabled features and incomplete runtime/session proof,
- therefore it must not yet be described as a working compositor path.

### G4. The input/session stack is build-visible but still operationally incomplete

Key issues:

- libinput is still built with udev disabled,
- seatd runtime proof is still open,
- compositor-side device discovery and hotplug behavior remain bounded or incomplete,
- `seatd-redox` remains a live local TODO and not a closed runtime path.

### G5. Hardware GPU acceleration is downstream from honest software-path proof

The current Wayland subsystem must not absorb or hide GPU render-path incompleteness.

Current truth:

- software-path Mesa/GBM/EGL is the valid bounded proof path,
- hardware acceleration remains blocked on shared GPU/DRM work outside the Wayland package layer,
- therefore hardware claims must stay in the DRM plan, not be implied by Wayland package success.

## Ownership and Forward Path

### Red Bear-owned forward path

The forward path is now:

- `redbear-wayland` for bounded compositor/runtime validation,
- `redbear-kde` for the intended KWin Wayland desktop direction,
- local overlay ownership for validation harnesses and any shipping-critical Wayland recipe deltas.

### Historical or non-forward references

These should not be treated as the forward path:

- `smallvil` — historical bounded validation compositor reference only,
- the generic upstream WIP compositor set (`wlroots`, `sway`, `hyprland`, etc.) — useful inputs, not
  trusted Red Bear shipping surfaces,
- `docs/03-WAYLAND-ON-REDOX.md` — retired as a planning document.

## Implementation Plan

This plan keeps Wayland aligned with the canonical desktop path, but narrows the work specifically to
Wayland subsystem needs.

### Wave 1 — Runtime substrate closure for Wayland consumers

**Goal:** turn the Wayland substrate from build-visible into runtime-trusted.

**Must prove:**

1. `libwayland` runtime behavior against the current relibc event/fd surfaces,
2. evdevd → libinput → compositor-facing input viability,
3. udev-shim enumeration sufficient for current Wayland-facing consumers,
4. firmware-loader + `redox-drm` + bounded KMS/display evidence adequate for the validation path.

**Acceptance criteria:**

- bounded relibc/libwayland runtime smoke is repeatable,
- bounded input path reaches compositor-facing consumers without hand-wavy assumptions,
- bounded display path still passes the current runtime harness after the input/session wiring is
  tightened,
- no current claim depends on a package merely compiling.

### Wave 2 — Complete the bounded compositor validation path

**Goal:** convert the current early-init harness into a real bounded software compositor proof.

**What success means:**

- compositor runs for a bounded interval without crashing,
- `WAYLAND_DISPLAY` is live,
- a client connects and survives,
- the current `qt6-wayland-smoke` path remains a visible bounded proof target,
- input is proven through the active compositor surface, not just through lower-layer scheme checks.

**Important rule:**

This wave is still a **validation compositor** wave, not a claim that KWin or Plasma is working.

### Wave 3 — KWin runtime truthfulness

**Goal:** turn the current KWin stub into a real build with Qt6Quick/QML downstream proof.

**Required work:**

- keep dependency honesty explicit,
- prove which remaining stubs/shims are still acceptable for bounded runtime work,
- establish one bounded KWin session proof before any Plasma support claim,
- keep disabled features and bounded providers visible in the support language.

**Acceptance criteria:**

- KWin starts as the compositor on the tracked path,
- the runtime session survives for a bounded interval,
- session/login1/D-Bus surfaces needed by KWin are observable,
- support claims still remain profile-scoped and bounded.

### Wave 4 — Ownership cleanup and stale-path retirement

**Goal:** make the doc/recipe story match the real forward path.

**Required work:**

- retire old planning authority from historical Wayland docs,
- demote or remove stale historical compositor references from the active guidance path,
- make the WIP recipe guidance reflect current truth instead of older partial states,
- keep local overlay ownership explicit wherever Red Bear is still the effective shipping owner.

**Acceptance criteria:**

- one canonical Wayland subsystem plan exists,
- stale planning references are removed,
- historical references are clearly marked historical,
- no active doc suggests that smallvil or generic upstream WIP compositor recipes are the forward
  Red Bear desktop path.

## What This Plan Supersedes

This plan supersedes the active planning role previously held by:

- `docs/03-WAYLAND-ON-REDOX.md`

It also reduces ambiguity in these adjacent surfaces:

- `recipes/wip/AGENTS.md` Wayland status notes,
- `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md` Wayland references,
- current-status and canonical-plan references that still pointed to the old Wayland roadmap.

## Docs To Keep vs. Retire

### Keep

- `local/docs/WAYLAND-IMPLEMENTATION-PLAN.md` — canonical Wayland subsystem plan
- `local/docs/DESKTOP-STACK-CURRENT-STATUS.md` — current truth summary
- `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` — canonical desktop path
- `local/docs/DRM-MODERNIZATION-EXECUTION-PLAN.md` — GPU/DRM execution detail
- `local/docs/QT6-PORT-STATUS.md` — Qt/KF6/KWin package build status

### Retire or demote

- `docs/03-WAYLAND-ON-REDOX.md` — remove as an active planning document
- stale WIP Wayland status text that still implies `smallvil` is current or that package build status
  equals runtime viability

## Definition of Done

Wayland can be called substantially complete for the current subsystem scope only when all of the
following are true:

- the bounded Wayland runtime path completes a usable software compositor session,
- runtime input/session/device-enumeration behavior is trusted enough to support that claim,
- KWin has at least one honest bounded runtime proof path,
- current docs describe the same truth with no stale forward-path confusion,
- hardware acceleration remains either separately proven or explicitly outside the claim.

## Current Bottom Line

Red Bear Wayland is no longer blocked primarily by package absence. It is blocked by runtime trust,
compositor completion, session/input integration, and honest ownership of the forward path.

That is the real work. This plan makes that explicit.
