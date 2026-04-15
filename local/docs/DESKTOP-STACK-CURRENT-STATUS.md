# Red Bear OS Desktop Stack — Current Status

## Purpose

This document is the **current build/runtime truth summary** for the Red Bear desktop stack.

It is intentionally narrower than the historical Wayland and KDE roadmap docs. Its job is to answer:

- what the current desktop stack actually builds,
- what the tracked desktop profiles currently expose,
- what is only build-visible,
- what is runtime-proven,
- and what still blocks a trustworthy Wayland/KDE session claim.

Use this document as the current-state summary. Use `docs/03-WAYLAND-ON-REDOX.md` and
`docs/05-KDE-PLASMA-ON-REDOX.md` mainly as design history, rationale, and deeper porting notes.

## Current State Summary

The Red Bear desktop stack is no longer blocked on basic Qt/Wayland package availability.

The repo currently proves:

- `libwayland` builds successfully against the current relibc/Red Bear compatibility surface
- Qt6 core modules build (`qtbase`, `qtdeclarative`, `qtsvg`, `qtwayland`)
- the current relibc overlay and its fresh-source reapply workflow are strong enough to support the
  rebuilt Qt/Wayland stack
- D-Bus builds and is wired into desktop-facing profiles
- `seatd` builds and is wired into the KDE-facing runtime profile
- the `redbear-wayland`, `redbear-full`, and `redbear-kde` profiles exist as real tracked product
  surfaces

The repo does **not** yet prove a generally trustworthy desktop runtime.

The main gap is no longer “can we build the packages?” The main gap is “which parts of the desktop
stack are runtime-trusted rather than just build-visible?”

## Status Matrix

| Area | Current state | What that means |
|---|---|---|
| `libwayland` | **builds** | relibc/Wayland-facing compatibility is materially better than before |
| Qt6 core stack | **builds** | `qtbase`, `qtdeclarative`, `qtsvg`, `qtwayland` are in-tree build surfaces |
| KF6 frameworks | **mixed but strong build progress** | many frameworks build; some higher-level pieces still rely on bounded or reduced recipes |
| KWin / Plasma session | **experimental / incomplete runtime** | recipe/config wiring exists, but runtime trust still trails build success |
| Mesa / hardware graphics path | **partial** | software path exists; hardware-validated Wayland graphics path still lags |
| Input stack | **build-visible and partly wired** | `evdevd`, `libevdev`, `libinput`, `seatd` are present, but runtime trust is still narrower than full desktop support |
| D-Bus session/system plumbing | **builds / wired into profiles** | present in desktop-facing profiles, but not equal to full desktop integration completeness |

## Profile View

### `redbear-wayland`

Role:

- narrow runtime validation profile for Wayland bring-up

Current truth:

- it is the current first-class profile for a bounded Wayland runtime path
- it should be used for small-scope compositor/runtime validation, not broad desktop claims

### `redbear-full`

Role:

- broader desktop/network/session plumbing profile

Current truth:

- it carries D-Bus and broader desktop integration pieces
- it is stronger than `redbear-wayland` for general integration, but still not the same as a stable
  KDE session claim

### `redbear-kde`

Role:

- KDE/Plasma session-surface profile

Current truth:

- it carries the KWin/session wiring and the KDE-facing package set
- it should still be described as experimental until runtime evidence catches up with build success

## Current Blockers

### 1. Runtime trust still trails build success

The project now has real build-visible desktop progress, but build success still exceeds runtime
confidence.

That gap is the main thing older docs sometimes blur.

### 2. Graphics/runtime validation is still thinner than package progress

The software-rendered stack is much further along than the hardware-validated stack.

The desktop stack therefore should not over-claim hardware-ready Wayland/KDE support yet.

### 3. KDE build progress is ahead of session maturity

KDE package and framework progress is real, but the session surface should still be described as an
experimental bring-up target rather than a broadly working desktop.

### 4. Input and seat management are present but not yet a final confidence story

`libinput`, `seatd`, and related runtime pieces matter, but they should still be treated as part of
the runtime-proof gap rather than as already-settled desktop infrastructure.

## Canonical Document Roles

Use the desktop-related docs this way:

- `local/docs/DESKTOP-STACK-CURRENT-STATUS.md` — current build/runtime truth summary
- `local/docs/QT6-PORT-STATUS.md` — Qt/KF6 package-level build/status truth
- `docs/03-WAYLAND-ON-REDOX.md` — historical Wayland implementation path + deeper rationale
- `docs/05-KDE-PLASMA-ON-REDOX.md` — historical KDE implementation path + deeper rationale
- `local/docs/PROFILE-MATRIX.md` — profile role and support-language reference

## Bottom Line

The current Red Bear desktop stack is in a transition phase:

- no longer blocked on basic Qt/Wayland package availability,
- materially stronger on relibc/Wayland-facing compatibility than before,
- but still short of a broad runtime-trusted desktop claim.

That is the current truth this repo should present.
