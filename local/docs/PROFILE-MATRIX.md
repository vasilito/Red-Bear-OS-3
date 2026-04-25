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

## Compile Targets

> **Phase numbering note:** phase labels below use the v2.0 desktop plan phases from
> `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md`. Scripts and older docs may reference the
> historical P0–P6 hardware-enablement sequence — those are not the same numbering.

| Profile | Intent | Key Fragments | Current support language |
|---|---|---|---|
| `redbear-mini` | Console + storage + wired-network baseline | `minimal.toml`, `redbear-legacy-base.toml`, `redbear-device-services.toml`, `redbear-netctl.toml` | builds / primary validation baseline / DHCP boot profile enabled / input-runtime substrate wired / USB: daemons built via base and targeted for bounded mini-profile validation |
| `redbear-grub` | Text-only with GRUB boot manager | `redbear-mini.toml`, `redbear-grub-policy.toml` | builds / live media variant with GRUB chainload for real bare metal / desktop graphics intentionally absent |
| `redbear-full` | Desktop/network/session plumbing target | `desktop.toml`, `redbear-legacy-base.toml`, `redbear-legacy-desktop.toml`, `redbear-device-services.toml`, `redbear-netctl.toml`, `redbear-greeter-services.toml` | builds / boots in QEMU / active desktop-capable compile target / support claims remain evidence-qualified |

## Profile Notes

### `redbear-mini`

- First place to validate repository discipline and profile reproducibility.
- Should stay smaller and less assumption-heavy than the graphics profiles.
- Enables the shared `wired-dhcp` netctl profile by default for the VM/wired baseline.
- Ships the shared firmware/input runtime service prerequisites so the early substrate can be tested on the smallest profile as well.

### Historical and experimental overlays

- Experimental overlays such as `redbear-bluetooth-experimental` and `redbear-wifi-experimental`
  are bounded validation slices layered on top of the tracked compile targets, not additional
  compile targets.

### `redbear-grub`

- Text-only console/recovery target with GRUB boot manager for multi-boot bare-metal workflows.
- Inherits the same non-graphics intent as `redbear-mini`, but with GRUB chainload ESP layout.
- Should not grow desktop/session assumptions.

### `redbear-full`

- Desktop-capable tracked target for the current Red Bear session/network/runtime plumbing surface.
- Carries the broader D-Bus, greeter, seat, and desktop-oriented service surface.

### Historical notes

- Older names such as `redbear-minimal`, `redbear-desktop`, `redbear-wayland`, `redbear-kde`,
  `redbear-live`, `redbear-live-mini`, and `redbear-live-full` remain in older docs and some
  implementation details, but they are not the current supported compile-target surface.

### `redbear-bluetooth-experimental`

- Standalone tracked profile for the first in-tree Bluetooth slice instead of a blanket claim about
  all Red Bear images.
- Extends `redbear-mini` so the baseline runtime tooling is already present, then adds only the
  bounded Bluetooth pieces on top.
- Current path under active validation: QEMU/UEFI boot to login prompt plus guest-side `redbear-bluetooth-battery-check`, targeting repeated in-boot reruns, daemon-restart coverage, and one experimental battery-sensor Battery Level read-only workload.
- Current support language is intentionally narrow: explicit-startup only, USB-attached transport,
  BLE-first CLI/scheme surface, one experimental battery-sensor Battery Level read-only workload,
  and no USB-class autospawn claim yet.

### `redbear-wifi-experimental`

- Standalone tracked profile for the current bounded Intel Wi-Fi slice instead of implying that the
  wider desktop profiles already carry the full driver stack.
- Extends `redbear-mini` so the baseline firmware/input/reporting/profile-manager surface stays
  inherited while the Intel Wi-Fi driver package and bounded validation role remain isolated here.
- Includes the Intel driver package (`redbear-iwlwifi`) in addition to the shared firmware,
  control-plane, reporting, and profile-manager pieces.
- Current support language is intentionally narrow: bounded probe/prepare/init/activate/scan/
  connect/disconnect lifecycle, packaged in-target validation and capture commands, and no claim yet
  of validated real AP association or end-to-end Wi-Fi connectivity.

## Bluetooth Note

- `redbear-bluetooth-experimental` is now the tracked first Bluetooth-specific profile.
- Its support language remains experimental and bounded; it should not be used to imply Bluetooth
  support across the wider Red Bear profile set.
- The current bounded BLE workload is one read-only battery-sensor Battery Level interaction; this
  profile still does not claim generic GATT, write, or notify support.
- The current validation claim is QEMU-scoped and packaged-checker-scoped, not a blanket claim
  about real hardware Bluetooth maturity.

## USB Note

- `redbear-mini` is the preferred non-graphics target for bounded USB validation because these
  proofs do not require the full desktop graphics/session surface.
- USB validation is QEMU-only (`test-usb-qemu.sh --check`). No profile makes a real hardware USB
  support claim.
- USB error handling and correctness carry significant Red Bear patches over upstream; see
  `local/patches/base/redox.patch` and `local/docs/USB-IMPLEMENTATION-PLAN.md` for details.
- The in-tree mini image is still assembled through legacy `redbear-minimal*` config files in some
  places, but the supported compile-target names are `redbear-mini` and `redbear-grub`.
- `redbear-bluetooth-experimental` uses USB only as a transport for BLE dongles; it does not make a
  general USB-class-autospawn claim.
