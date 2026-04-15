# Red Bear OS Profile Matrix

## Purpose

This matrix makes the tracked Red Bear profiles explicit so support claims map to a concrete build
target instead of a vague feature list.

## Validation Labels

- **builds** — configuration and packages are expected to compile
- **boots** — image is expected to reach a usable boot state
- **validated** — behavior has been tested on the claimed profile
- **experimental** — available for bring-up, but not support-promised

Subsystem plans may add narrower intermediate labels when `boots` is too coarse. In particular, the
USB plan uses:

- **enumerates** — runtime surfaces can discover controllers, ports, or descriptors
- **usable** — a specific controller/class path works in a limited real scenario

## Tracked Profiles

| Profile | Intent | Key Fragments | Current support language |
|---|---|---|---|
| `redbear-minimal` | Console + storage + wired-network baseline | `minimal.toml`, `redbear-legacy-base.toml`, `redbear-device-services.toml`, `redbear-netctl.toml` | builds / primary validation baseline / DHCP boot profile enabled / input-runtime substrate wired |
| `redbear-desktop` | Main Red Bear desktop integration profile without KDE-specific session wiring | `desktop.toml`, `redbear-legacy-base.toml`, `redbear-device-services.toml`, `redbear-netctl.toml` | builds / input-runtime substrate wired / runtime reporting installed |
| `redbear-wayland` | Phase 4 Wayland runtime validation profile | `wayland.toml` | builds / boots in QEMU / experimental graphics-runtime path |
| `redbear-full` | Phase 5 desktop/network plumbing profile | `desktop.toml`, `redbear-legacy-base.toml`, `redbear-legacy-desktop.toml`, `redbear-device-services.toml`, `redbear-netctl.toml` | builds / boots in QEMU / D-Bus system bus wired / experimental runtime path |
| `redbear-kde` | Phase 6 KDE session-surface profile | `desktop.toml`, `redbear-legacy-base.toml`, `redbear-legacy-desktop.toml`, `redbear-device-services.toml`, `redbear-netctl.toml` | builds / experimental desktop path / D-Bus+seatd+KWin session surface wired |
| `redbear-live` | Live and recovery image layered on desktop | `redbear-desktop.toml` | builds |

## Profile Notes

### `redbear-minimal`

- First place to validate repository discipline and profile reproducibility.
- Should stay smaller and less assumption-heavy than the graphics profiles.
- Enables the shared `wired-dhcp` netctl profile by default for the Phase 2 VM/wired baseline.
- Ships the shared firmware/input runtime service prerequisites so the early substrate can be tested on the smallest profile as well.

### `redbear-desktop`

- Carries the standard Red Bear desktop-facing package additions.
- Inherits desktop behavior but avoids the heavier KDE session-specific wiring.
- Now includes the shared firmware/input runtime service fragment used by the wider desktop bring-up path.
- Also includes `redbear-info`, making the desktop profile the main runtime-reporting integration environment.

### `redbear-wayland`

- Wraps the repo's existing `wayland.toml` into a first-class Red Bear build target.
- Serves as the Phase 4 runtime-validation surface for `orbital-wayland` and `smallvil`.
- Current verified path: QEMU/UEFI boot to login prompt plus guest-side `redbear-phase4-wayland-check`, with `smallvil` reaching xkbcommon initialization and EGL platform selection on Redox.

### `redbear-full`

- Used for broader desktop/session plumbing after the narrower `redbear-wayland` validation slice.
- Current Phase 5 role: carry D-Bus system-bus plumbing together with the native Red Bear network stack.
- Current verified path: QEMU/UEFI boot to login prompt plus guest-side `redbear-phase5-network-check`, with functional VirtIO networking and `DBUS_SYSTEM_BUS=present`.
- Should not be described as fully supported until runtime validation is evidence-backed.

### `redbear-kde`

- Dedicated profile for Plasma/KWin session bring-up.
- Keep KDE-specific service wiring here instead of leaking it into the generic desktop profile.
- Current Phase 6 role: carry the KWin session launch surface and its D-Bus/seatd dependencies in one image.

### `redbear-live`

- Intended for install, demo, and recovery workflows.
- Should inherit only stable desktop-profile assumptions unless explicitly documented.
