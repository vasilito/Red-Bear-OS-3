# Red Bear OS WIP Migration Ledger

## Purpose

This ledger records how Red Bear treats upstream WIP areas under the overlay policy.

The goal is to keep one compact, current view of whether a major WIP subsystem is:

- still consumed mainly from upstream WIP,
- mirrored locally and shipped from the Red Bear overlay,
- or mature enough upstream that Red Bear should prefer the upstream version.

This is a repo-governance document, not a subsystem deep dive.

## Status Labels

- **upstream-wip-input** — upstream WIP still exists and is useful as an input/reference, but Red Bear
  does not treat it as the durable shipping source of truth
- **local-overlay-owner** — Red Bear currently owns the shipping/integration burden locally
- **mixed-transition** — both upstream WIP and local overlay matter; Red Bear is still evaluating what
  to keep locally versus what to prefer upstream
- **prefer-upstream** — upstream is now first-class enough that Red Bear should default to upstream and
  keep only a narrow local integration delta if still needed

## Current Ledger

| Area | Current status | Current preferred shipping source | Notes |
|---|---|---|---|
| Qt6 base stack (`qtbase`, `qtdeclarative`, `qtsvg`, `qtwayland`) | **mixed-transition** | local overlay + upstream WIP inputs | Upstream WIP remains useful input, but Red Bear still carries recipe/integration fixes and validation locally. |
| KDE Frameworks / Plasma / KWin | **local-overlay-owner** | local overlay | Current KDE/Plasma recipe tree under `local/recipes/kde/` is the practical shipping source for Red Bear. |
| Wayland compositor/session stack | **mixed-transition** | local overlay for shipping decisions | Upstream WIP recipes remain inputs, but runtime-trusted Red Bear delivery still depends on local validation and local recipe ownership where needed. |
| `libinput` / desktop input userland | **mixed-transition** | local decision pending | Upstream WIP recipe exists, but Red Bear still treats this as a local validation and integration concern rather than a trusted upstream shipping surface. |
| `seatd` runtime path | **mixed-transition** | recipe-level decision still local | It builds and is integrated into KDE-facing configs, but runtime trust still trails the packaging story. |
| `redox-driver-sys` | **local-overlay-owner** | local overlay | Red Bear-owned driver substrate. |
| `linux-kpi` | **local-overlay-owner** | local overlay | Red Bear-owned compatibility layer. |
| `redox-drm` / `amdgpu` | **local-overlay-owner** | local overlay | Red Bear-owned graphics/driver work. |
| `firmware-loader` | **local-overlay-owner** | local overlay | Red Bear-owned runtime infrastructure. |
| relibc compatibility overlays | **mixed-transition** | upstream + local overlay | Prefer upstream where available; keep only the overlays that still prove necessary after fresh-source reapply and downstream rebuild. |

## Decision Rules

### When to stay local

Stay local when one or more of the following is true:

- upstream still marks the recipe/subsystem WIP,
- Red Bear still needs local fixes to build or ship it,
- Red Bear is carrying the validation burden that upstream has not yet established,
- the local version is the only version that currently integrates correctly with tracked Red Bear profiles.

### When to move back toward upstream

Prefer upstream when all of the following become true:

- upstream no longer treats the area as WIP,
- upstream solves the same problem adequately,
- refreshed upstream source + minimal Red Bear integration still rebuilds the affected profiles,
- keeping the local overlay would no longer provide unique value.

## Review Trigger

Reevaluate an entry in this ledger whenever:

- upstream removes WIP status from the recipe/subsystem,
- Red Bear finishes a fresh-source reapply + rebuild proof,
- a local overlay shrinks substantially because upstream caught up,
- or the shipping profile set starts depending on a WIP area more heavily than before.
