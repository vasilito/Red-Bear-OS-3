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

- `redbear-mini`
- `redbear-full`
- `redbear-grub`
- `redbear-bluetooth-experimental`
- `redbear-wifi-experimental`

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

### 6. Resilience policy: local-first package sources

- Red Bear builds must remain resilient when access to upstream Redox infrastructure is degraded or
  unavailable.
- Local package/source copies are the default operational source of truth for builds.
- Upstream fetch/refresh is opt-in and must be explicitly requested by the operator (for example via
  an explicit `--upstream` workflow).
- After an explicit upstream refresh, local durable overlays (`local/patches`, `local/recipes`) stay
  authoritative until a conscious reevaluation/promotion decision is made.

## Profile Intent

### `redbear-mini`

Primary validation baseline: console, storage, package flow, and wired networking.

### `redbear-bluetooth-experimental`

First bounded Bluetooth validation profile: explicit-startup, USB-attached, BLE-first, and
experimental only.

### `redbear-full`

Desktop-capable tracked target for the current Red Bear session/network/runtime plumbing surface,
including graphics-path bring-up beneath the tracked KWin direction.

### `redbear-grub`

Text-only console/recovery target with GRUB boot manager for bare-metal multi-boot workflows.

### `redbear-wifi-experimental`

Bounded Intel Wi-Fi validation profile layered on the mini baseline.

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
