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
| `redbear-bluetooth-experimental` | First bounded Bluetooth validation profile | `redbear-bluetooth-experimental.toml`, `redbear-minimal.toml` | builds / boots in QEMU / validated bounded Battery Level slice via `redbear-bluetooth-battery-check` and `test-bluetooth-qemu.sh --check` / explicit-startup USB BLE-first only / repeated helper + restart cleanup covered / not generic GATT / not USB-class-autospawn |
| `redbear-wifi-experimental` | First bounded Intel Wi-Fi validation profile | `redbear-wifi-experimental.toml`, `redbear-device-services.toml`, `redbear-netctl.toml` | builds / experimental bounded Intel Wi-Fi slice / driver + control/profile/reporting stack present / packaged in-target validation and capture commands available / real hardware connectivity still unproven |
| `redbear-desktop` | Main Red Bear desktop integration profile without KDE-specific session wiring | `desktop.toml`, `redbear-legacy-base.toml`, `redbear-device-services.toml`, `redbear-netctl.toml` | builds / input-runtime substrate wired / runtime reporting installed |
| `redbear-wayland` | Phase 4 Wayland runtime validation profile | `wayland.toml` | builds / boots in QEMU / experimental software-path graphics-runtime slice / not QEMU hardware-acceleration proof |
| `redbear-full` | Phase 5 desktop/network plumbing profile | `desktop.toml`, `redbear-legacy-base.toml`, `redbear-legacy-desktop.toml`, `redbear-device-services.toml`, `redbear-netctl.toml` | builds / boots in QEMU / D-Bus system bus wired / experimental runtime path |
| `redbear-kde` | Phase 6 KDE session-surface profile | `desktop.toml`, `redbear-legacy-base.toml`, `redbear-legacy-desktop.toml`, `redbear-device-services.toml`, `redbear-netctl.toml` | builds / experimental desktop path / D-Bus+seatd+KWin session surface wired |
| `redbear-live` | Live and recovery image layered on desktop | `redbear-desktop.toml` | builds |

## Profile Notes

### `redbear-minimal`

- First place to validate repository discipline and profile reproducibility.
- Should stay smaller and less assumption-heavy than the graphics profiles.
- Enables the shared `wired-dhcp` netctl profile by default for the Phase 2 VM/wired baseline.
- Ships the shared firmware/input runtime service prerequisites so the early substrate can be tested on the smallest profile as well.

### `redbear-bluetooth-experimental`

- Standalone tracked profile for the first in-tree Bluetooth slice instead of a blanket claim about
  all Red Bear images.
- Extends `redbear-minimal` so the baseline runtime tooling is already present, then adds only the
  bounded Bluetooth pieces on top.
- Current verified path: QEMU/UEFI boot to login prompt plus guest-side `redbear-bluetooth-battery-check`, with repeated in-boot reruns, daemon-restart coverage, and one experimental battery-sensor Battery Level read-only workload.
- Current support language is intentionally narrow: explicit-startup only, USB-attached transport,
  BLE-first CLI/scheme surface, one experimental battery-sensor Battery Level read-only workload,
  and no USB-class autospawn claim yet.

### `redbear-wifi-experimental`

- Standalone tracked profile for the current bounded Intel Wi-Fi slice instead of implying that the
  wider desktop profiles already carry the full driver stack.
- Extends `redbear-minimal` so the baseline firmware/input/reporting/profile-manager surface stays
  inherited while the Intel Wi-Fi driver package and bounded validation role remain isolated here.
- Includes the Intel driver package (`redbear-iwlwifi`) in addition to the shared firmware,
  control-plane, reporting, and profile-manager pieces.
- Current support language is intentionally narrow: bounded probe/prepare/init/activate/scan/
  connect/disconnect lifecycle, packaged in-target validation and capture commands, and no claim yet
  of validated real AP association or end-to-end Wi-Fi connectivity.

### `redbear-desktop`

- Carries the standard Red Bear desktop-facing package additions.
- Inherits desktop behavior but avoids the heavier KDE session-specific wiring.
- Now includes the shared firmware/input runtime service fragment used by the wider desktop bring-up path.
- Also includes `redbear-info`, making the desktop profile the main runtime-reporting integration environment.

### `redbear-wayland`

- Wraps the repo's existing `wayland.toml` into a first-class Red Bear build target.
- Serves as the Phase 4 runtime-validation surface for `orbital-wayland` and `smallvil`.
- Current verified path: QEMU/UEFI boot to login prompt plus guest-side `redbear-phase4-wayland-check`, with `smallvil` reaching xkbcommon initialization and EGL platform selection on Redox.
- Current QEMU renderer evidence is still software-based (`llvmpipe` on the current `-vga std` harness), so this profile must not be described as a hardware-accelerated desktop proof yet.
- Treat this profile as the bounded Phase 4 Wayland/Qt regression harness; the final hardware-desktop claim still belongs to the bare-metal accelerated graphics path.

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

## Bluetooth Note

- `redbear-bluetooth-experimental` is now the tracked first Bluetooth-specific profile.
- Its support language remains experimental and bounded; it should not be used to imply Bluetooth
  support across the wider Red Bear profile set.
- The current bounded BLE workload is one read-only battery-sensor Battery Level interaction; this
  profile still does not claim generic GATT, write, or notify support.
- The current validation claim is QEMU-scoped and packaged-checker-scoped, not a blanket claim
  about real hardware Bluetooth maturity.
