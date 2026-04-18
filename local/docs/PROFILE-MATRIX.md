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

> **Phase numbering note:** phase labels below use the v2.0 desktop plan phases from
> `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md`. Scripts and older docs may reference the
> historical P0–P6 hardware-enablement sequence — those are not the same numbering.

| Profile | Intent | Key Fragments | Current support language |
|---|---|---|---|
| `redbear-minimal` | Console + storage + wired-network baseline | `minimal.toml`, `redbear-legacy-base.toml`, `redbear-device-services.toml`, `redbear-netctl.toml` | builds / primary validation baseline / DHCP boot profile enabled / input-runtime substrate wired / USB: daemons built via base but not validated or support-scoped for this profile |
| `redbear-bluetooth-experimental` | First bounded Bluetooth validation profile | `redbear-bluetooth-experimental.toml`, `redbear-bluetooth-services.toml`, `redbear-minimal.toml` | builds / boots in QEMU / packaged Battery Level checker + QEMU harness present / QEMU validation still in progress / explicit-startup USB BLE-first only / not generic GATT / not USB-class-autospawn |
| `redbear-wifi-experimental` | First bounded Intel Wi-Fi validation profile | `redbear-wifi-experimental.toml`, `redbear-device-services.toml`, `redbear-netctl.toml` | builds / experimental bounded Intel Wi-Fi slice / driver + control/profile/reporting stack present / packaged in-target validation and capture commands available / real hardware connectivity still unproven |
| `redbear-desktop` | Supplementary Red Bear integration support profile without KDE-specific session wiring | `desktop.toml`, `redbear-legacy-base.toml`, `redbear-legacy-desktop.toml`, `redbear-device-services.toml`, `redbear-netctl.toml` | builds / input-runtime substrate wired / runtime reporting installed / USB: xHCI host present + HID keyboard/mouse usable + mass storage autospawns in QEMU / QEMU-validated only / no real hardware USB claim |
| `redbear-wayland` | v2.0 Phase 2 Wayland compositor validation profile | `wayland.toml` | builds / boots in QEMU / experimental software-path graphics-runtime slice / validation-only |
| `redbear-full` | Broader desktop/network/session plumbing (spans v2.0 Phases 2–3) | `desktop.toml`, `redbear-legacy-base.toml`, `redbear-legacy-desktop.toml`, `redbear-device-services.toml`, `redbear-netctl.toml` | builds / boots in QEMU / D-Bus system bus wired / experimental runtime path |
| `redbear-kde` | v2.0 Phases 3–4 KWin Wayland target desktop profile | `desktop.toml`, `redbear-legacy-base.toml`, `redbear-legacy-desktop.toml`, `redbear-device-services.toml`, `redbear-netctl.toml` | builds / tracked KWin desktop direction / D-Bus+seatd+sessiond+KWin session surface wired |
| `redbear-live` | Live and recovery image layered on desktop | `redbear-kde.toml` | builds / follows the tracked KWin desktop target |

## Profile Notes

### `redbear-minimal`

- First place to validate repository discipline and profile reproducibility.
- Should stay smaller and less assumption-heavy than the graphics profiles.
- Enables the shared `wired-dhcp` netctl profile by default for the VM/wired baseline.
- Ships the shared firmware/input runtime service prerequisites so the early substrate can be tested on the smallest profile as well.

### `redbear-bluetooth-experimental`

- Standalone tracked profile for the first in-tree Bluetooth slice instead of a blanket claim about
  all Red Bear images.
- Extends `redbear-minimal` so the baseline runtime tooling is already present, then adds only the
  bounded Bluetooth pieces on top.
- Current path under active validation: QEMU/UEFI boot to login prompt plus guest-side `redbear-bluetooth-battery-check`, targeting repeated in-boot reruns, daemon-restart coverage, and one experimental battery-sensor Battery Level read-only workload.
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

- Carries the standard Red Bear integration package additions.
- Inherits shared behavior while avoiding the heavier KDE session-specific wiring.
- Now includes the shared firmware/input runtime service fragment used by the wider desktop bring-up path.
- Also includes `redbear-info`, making this profile a main runtime-reporting integration environment.
- This remains available as a supplementary integration support profile.

### `redbear-wayland`

- Wraps the repo's existing `wayland.toml` into a tracked Red Bear validation target.
- Serves as the v2.0 Phase 2 compositor validation surface.
- Current verified path: QEMU/UEFI boot to login prompt plus guest-side `redbear-phase4-wayland-check`, with the compositor reaching xkbcommon initialization and EGL platform selection on Redox.
- Current QEMU renderer evidence is still software-based (`llvmpipe` on the current `-vga std` harness), so this profile must not be described as a hardware-accelerated desktop proof yet.
- Treat this profile as the bounded Wayland/Qt regression harness.
- The intended desktop direction is `redbear-kde` with KWin Wayland.

### `redbear-full`

- Used for broader desktop/session plumbing after the narrower `redbear-wayland` validation slice.
- Current role: carry D-Bus system-bus plumbing together with the native Red Bear network stack (spans v2.0 Phases 2–3).
- Current verified path: QEMU/UEFI boot to login prompt plus guest-side `redbear-phase5-network-check`, with functional VirtIO networking, `DBUS_SYSTEM_BUS=present`, and bounded UPower/UDisks2 runtime-backed enumeration.
- Should not be described as fully supported until runtime validation is evidence-backed.
- This bounded QEMU Phase 5 proof is not the same thing as the Wi-Fi plan's later Phase W5 real-hardware runtime-reporting-and-recovery exit criteria.

### `redbear-kde`

- Dedicated profile for the intended KWin Wayland desktop path.
- Keep KDE-specific service wiring here instead of leaking it into the generic desktop profile.
- Current role: carry the KWin session launch surface and its D-Bus/seatd dependencies in one image (v2.0 Phases 3–4).
- This is the tracked compositor/session direction.

### `redbear-live`

- Intended for install, demo, and recovery workflows.
- Should inherit only stable desktop-profile assumptions unless explicitly documented.
- It now inherits `redbear-kde` so the live image follows the tracked desktop direction.

## Bluetooth Note

- `redbear-bluetooth-experimental` is now the tracked first Bluetooth-specific profile.
- Its support language remains experimental and bounded; it should not be used to imply Bluetooth
  support across the wider Red Bear profile set.
- The current bounded BLE workload is one read-only battery-sensor Battery Level interaction; this
  profile still does not claim generic GATT, write, or notify support.
- The current validation claim is QEMU-scoped and packaged-checker-scoped, not a blanket claim
  about real hardware Bluetooth maturity.

## USB Note

- `redbear-desktop` is the primary profile carrying USB stack components (xHCI, HID, mass storage)
  and the only profile where USB is validated and support-scoped.
- USB validation is QEMU-only (`test-usb-qemu.sh --check`). No profile makes a real hardware USB
  support claim.
- USB error handling and correctness carry significant Red Bear patches over upstream; see
  `local/patches/base/redox.patch` and `local/docs/USB-IMPLEMENTATION-PLAN.md` for details.
- `redbear-minimal` inherits the base recipe which builds `xhcid`, `usbhidd`, `usbhubd`, `usbscsid`,
  and `usbctl`. These binaries are present in the image but USB is not validated or support-scoped
  for this profile.
- `redbear-bluetooth-experimental` uses USB only as a transport for BLE dongles; it does not make a
  general USB-class-autospawn claim.
