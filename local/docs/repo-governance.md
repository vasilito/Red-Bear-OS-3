# Red Bear OS Repository Governance

## Purpose

This document defines the repository-discipline rules for Red Bear OS so profile work stays
reproducible, reviewable, and upstream-friendly.

## Core Rules

### 1. Keep Red Bear work isolated

- Put Red Bear-specific source, recipes, scripts, and docs under `local/` whenever possible.
- Prefer patch files and symlinks over direct edits to upstream-managed source trees.
- Treat mainline Redox areas as upstream surfaces first, not as the default place for Red Bear
  customization.

### 2. Profiles are the support surface

Tracked Red Bear profiles are:

- `redbear-minimal`
- `redbear-bluetooth-experimental`
- `redbear-desktop`
- `redbear-full`
- `redbear-wayland`
- `redbear-kde`
- `redbear-live`

Every user-visible feature should name which profile(s) it belongs to.

### 3. Validation claims must be explicit

- `builds` means the package or profile compiles.
- `boots` means the image reaches a real bootable system state.
- `validated` means behavior has been tested on the claimed profile.
- `experimental` means present for bring-up but not support-promised.

Do not describe compile-only work as supported hardware or a working desktop path.

### 4. Prefer shared fragments over duplicated profile logic

- Shared profile file wiring belongs in reusable `config/redbear-*.toml` fragments.
- Avoid copy-pasting identical service definitions or file payloads across multiple Red Bear
  profiles.
- Keep profile-specific behavior in the profile file only when the runtime behavior is actually
  different.

### 5. Build helpers must match tracked profiles

If a profile is tracked in git, helper scripts and docs should either support it directly or state
why it is intentionally excluded.

## Profile Intent

### `redbear-minimal`

Primary validation baseline: console, storage, package flow, and wired networking.

### `redbear-bluetooth-experimental`

First bounded Bluetooth validation profile: explicit-startup, USB-attached, BLE-first, and
experimental only.

### `redbear-desktop`

Supplementary integration support profile for shared Red Bear runtime services beneath the tracked KWin target.

### `redbear-full`

Expanded integration slice that includes more runtime pieces and graphics-path bring-up beneath the tracked KWin target.

### `redbear-wayland`

Dedicated Wayland runtime validation profile layered above the current Red Bear service baseline and subordinate to the tracked KWin direction.

### `redbear-kde`

Dedicated KDE/Plasma bring-up profile and tracked forward desktop target.

### `redbear-live`

Live and recovery variant layered on top of the tracked KWin desktop target.

## Change Checklist

For any substantial Red Bear change, record:

- objective
- profile impact
- files touched
- validation level (`builds`, `boots`, `validated`, `experimental`)
- known limitations

## Upstream Sync Discipline

- Rebase/sync through `local/scripts/sync-upstream.sh`.
- Keep Red Bear-specific diffs easy to audit.
- Update profile docs when config inheritance or package composition changes.
