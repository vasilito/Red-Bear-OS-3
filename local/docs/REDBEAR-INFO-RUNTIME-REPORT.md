# redbear-info Runtime Report

`redbear-info` is the canonical Red Bear OS runtime integration and debugging command.

## Purpose

The tool is intentionally passive. It reports what the running system can actually prove through
read-only runtime surfaces instead of flattening everything into a single “available” bit.

It is meant to answer:

- what Red Bear integrations are installed,
- what services or schemes are actually active,
- what integrations passed a safe read-only runtime probe,
- whether networking is configured, including IP, DNS, and default route,
- whether key hardware discovery surfaces (PCI, USB, DRM, RTL8125, VirtIO NICs) are visible.

## Output model

Each integration is reported with one of these layered states:

- `absent` — no artifact or runtime surface was observed
- `present` — an artifact or config exists, but there is no live runtime proof yet
- `active` — a live runtime surface exists, but the probe cannot honestly claim full working order
- `functional` — a safe read-only runtime probe succeeded
- `unobservable` — no honest runtime proof exists for a deeper claim

This distinction matters because some Red Bear integrations compile or package cleanly before they
are hardware-validated at runtime.

## Current sections

`redbear-info` reports:

- **Identity** — OS name, version, hostname
- **Networking** — stack state, connected flag, interface, MAC, IP/CIDR, DNS, default route,
  active `netctl` profile, visible `network.*` schemes, Wi-Fi control/firmware/transport surfaces,
  and bounded Bluetooth transport/control visibility
- **Hardware** — PCI device count, USB controller count, DRM card count, RTL8125 PCI visibility,
  VirtIO NIC visibility for VM baselines
- **Hardware** — bounded PCI interrupt-support summary (`none`, `legacy`, `msi`, `msix`) derived
  from the shared `redox-driver-sys` parser instead of a second local decoder
- **Integrations** — tools, daemons, and integration paths such as `lspci`, `lsusb`, `netctl`,
  `pcid-spawner`, `smolnetd`, `firmware-loader`, `udev-shim`, `evdevd`, `redox-drm`,
  `redbear-wifictl`, `redbear-btusb`, `redbear-btctl`, and the native RTL8125 and VirtIO
  networking paths

For Phase 3 runtime validation, `udev-shim` is expected at `/usr/bin/udev-shim` and `evdevd` is
expected at both `/usr/bin/evdevd` and `/usr/lib/drivers/evdevd` so service execution and runtime
reporting use the same binary identity.

## Commands

- `redbear-info` — human-readable report
- `redbear-info --verbose` — includes evidence and claim limits
- `redbear-info --json` — structured machine-readable output
- `redbear-info --test` — suggested follow-up diagnostic commands

## Maintenance rule

Whenever Red Bear adds or materially changes an integration, update `redbear-info` in the same
change set.

That includes new:

- user-facing tools
- scheme daemons
- services
- hardware integration paths
- configuration layers that users rely on to debug a running image

Recent examples include the Wi-Fi control-plane surfaces and the bounded Bluetooth first-slice
surfaces, both of which extend the runtime report without over-claiming hardware validation.

Recent PCI/IRQ examples now also include:

- aggregate interrupt-support counts in the hardware section,
- bounded `redbear-upower` integration reporting tied to the live `/scheme/acpi/power` surface,
- the distinction between “PCI device visible” and “PCI interrupt mode/runtime proof is still a
  bounded claim”,
- and normalized bounded proof outputs from the MSI-X and xHCI QEMU helpers (`IRQ_DRIVER`,
  `IRQ_MODE`, `IRQ_REASON`, `IRQ_LOG`) so the current proof surface says which mode was actually
  observed and why the driver believed it was using that path.

The goal is for `redbear-info` to remain the first command users run when they need to understand
the state of a Red Bear system.
