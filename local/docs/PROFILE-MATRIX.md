# Red Bear OS Profile Matrix

## Purpose

This matrix makes the tracked Red Bear profiles explicit so support claims map to a concrete build
target instead of a vague feature list.

## Validation Labels

- **builds** — configuration and packages are expected to compile
- **boots** — image is expected to reach a usable boot state
- **validated** — behavior has been tested on the claimed profile
- **experimental** — available for bring-up, but not support-promised

## Tracked Profiles

| Profile | Intent | Key Fragments | Current support language |
|---|---|---|---|
| `redbear-minimal` | Console + storage + wired-network baseline | `minimal.toml`, `redbear-legacy-base.toml`, `redbear-device-services.toml`, `redbear-netctl.toml` | builds / primary validation baseline / DHCP boot profile enabled |
| `redbear-desktop` | Main Red Bear desktop integration profile without KDE-specific session wiring | `desktop.toml`, `redbear-netctl.toml` | builds |
| `redbear-full` | Expanded graphics/input/Qt integration target | `desktop.toml`, `redbear-legacy-base.toml`, `redbear-legacy-desktop.toml`, `redbear-device-services.toml`, `redbear-netctl.toml` | builds / experimental runtime path |
| `redbear-kde` | KDE Plasma bring-up profile | `desktop.toml`, `redbear-legacy-base.toml`, `redbear-legacy-desktop.toml`, `redbear-device-services.toml`, `redbear-netctl.toml` | builds / experimental desktop path |
| `redbear-live` | Live and recovery image layered on desktop | `redbear-desktop.toml` | builds |

## Profile Notes

### `redbear-minimal`

- First place to validate repository discipline and profile reproducibility.
- Should stay smaller and less assumption-heavy than the graphics profiles.
- Enables the shared `wired-dhcp` netctl profile by default for the Phase 2 VM/wired baseline.

### `redbear-desktop`

- Carries the standard Red Bear desktop-facing package additions.
- Inherits desktop behavior but avoids the heavier KDE session-specific wiring.

### `redbear-full`

- Used for broader integration work that combines graphics, input, and Qt runtime pieces.
- Should not be described as fully supported until runtime validation is evidence-backed.

### `redbear-kde`

- Dedicated profile for Plasma/KWin session bring-up.
- Keep KDE-specific service wiring here instead of leaking it into the generic desktop profile.

### `redbear-live`

- Intended for install, demo, and recovery workflows.
- Should inherit only stable desktop-profile assumptions unless explicitly documented.
