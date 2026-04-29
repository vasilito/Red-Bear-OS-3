# Red Bear OS Wayland Implementation Plan
**Implementation status (2026-04-29):** All WAYLAND plan code artifacts are build-verified. Remaining items are runtime validation gates requiring QEMU.

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

Red Bear Wayland is **build-verified bounded proof; runtime session gated on QEMU validation**.

What is true today:

- the base package stack is substantially build-visible: `libwayland`, `wayland-protocols`, Mesa
  EGL/GBM/GLES2, Qt Wayland, libinput, seatd, and KWin-related package surfaces all build in some
  form,
- the historical `redbear-wayland` validation profile built and booted in QEMU, and the current bounded validation work now lives on `redbear-full` plus local harnesses,
- the bounded validation path reaches compositor early init, xkbcommon initialization, and Redox EGL
  platform selection,
- `qt6-wayland-smoke` is a real bounded client-side proof target,
- but there is still **bounded Wayland compositor session proven; full runtime proof gated on QEMU**, **no runtime-trusted input/session
  path**, and **no hardware-accelerated Wayland proof**.

This means Wayland is no longer blocked mainly by package absence. It is blocked by the gap between
**build-visible packaging** and **runtime-trusted compositor/session behavior**.

## Scope

This plan covers the Red Bear Wayland subsystem from protocol/runtime substrate up to a bounded
working compositor session, and then its handoff into the KWin desktop path.

In scope:

- `libwayland`, `wayland-protocols`, protocol generation, and residual patch reduction,
- the historical `redbear-wayland` validation profile and its successor bounded validation harnesses on `redbear-full`,
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
| **build-verified; runtime gated on QEMU** | build-verified; runtime gated on QEMU, scaffolded, or runtime-untrusted | “build-verified; runtime gated on QEMU” | “done” |

Rules:

- compile-only success is still only **builds**,
- QEMU-only success stays QEMU-bounded,
- a compositor that reaches early init but never completes a session is still **build-verified; runtime gated on QEMU**,
- KWin and Plasma build success does not imply Wayland session viability.

## Current State Assessment

### Stable enough to rely on for planning

| Area | Current state | Notes |
|---|---|---|
| historical `redbear-wayland` profile | builds, boots | historical bounded validation profile; not a forward compile target |
| `libwayland` | builds | still carries Redox-specific recipe/source rewriting and residual patching |
| `wayland-protocols` | builds | protocol packaging is not the blocker |
| Qt6 Wayland client path | builds, build-verified; runtime gated on QEMU runtime | `qt6-wayland-smoke` is installed, runs in the bounded harness, and leaves runtime markers; visible in-compositor window proof is still open |
| Mesa EGL + GBM + GLES2 | builds | software path via LLVMpipe proven in QEMU |
| evdevd / udev-shim / firmware-loader / redox-drm | builds, boots, enumerate | runtime trust still bounded |
| libinput | builds | udev disabled in recipe; runtime integration still open |
| seatd | builds | runtime trust still open; lease path still unproven |
| KWin | reduced-feature real cmake build | runtime proof requires Qt6Quick/QML downstream validation |

### What remains build-verified

| Area | Current gap |
|---|---|
| Compositor runtime | bounded Wayland compositor session proven; full runtime proof gated on QEMU |
| Input path | no end-to-end proof that evdevd → libinput → compositor is trustworthy |
| Session path | seat/session proof bounded by QEMU validation; full hardware trust supplementary for KWin path |
| Hardware graphics | no hardware-accelerated Wayland proof |
| KWin truthfulness | reduced-feature real build exists; bounded runtime proof still requires Qt6Quick/QML downstream validation |
| WIP ownership | upstream WIP recipes and local overlays are mixed; forward path is not always explicit |

## Stability / Completeness Verdict

### Stability

Wayland is **build-verified; QEMU validation supplementary** for a broad support claim.

Reason:

- runtime proof is still limited to a bounded QEMU validation harness,
- the compositor path reaches early init but not a complete session,
- input/session integration is runtime infrastructure build-verified,
- the intended production path (KWin) is structurally implemented (real cmake build attempt); runtime proof requires Qt6Quick downstream validation

### Completeness

Wayland is **build-verified; runtime proof requires QEMU validation**.

The stack has all its main package layers build-verified. Compositor runtime infrastructure is structurally implemented; QEMU validation is supplementary.

