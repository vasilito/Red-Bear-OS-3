# Red Bear OS USB Validation Runbook

This runbook is the canonical operator path for exercising the USB stack on Red Bear OS.

It does not claim that USB is broadly solved. Its job is to make the current QEMU-validated USB
workload reproducible and honest.

## Goal

Produce one or both of the following:

- a successful USB stack validation via `redbear-usb-check` inside the guest
- a repeatable QEMU/UEFI validation log via `./local/scripts/test-usb-qemu.sh --check`
- a repeatable bounded xHCI lifecycle log via `./local/scripts/test-xhci-device-lifecycle-qemu.sh --check`

## Path A - Host-side QEMU validation

Use this when the host supports the repo's normal x86_64 QEMU/UEFI flow.

### On the host

Build the tracked mini profile first:

```bash
./local/scripts/build-redbear.sh redbear-mini
```

Then run the automated QEMU harness:

```bash
./local/scripts/test-usb-qemu.sh --check
./local/scripts/test-xhci-device-lifecycle-qemu.sh --check
```

What that harness does today:

1. boots `redbear-mini` in QEMU with `qemu-xhci`, USB keyboard, USB tablet, and USB mass storage
2. captures the full boot log over serial
3. checks for xHCI interrupt-driven mode in the log
4. checks for USB HID driver spawn
5. checks for USB SCSI driver spawn
6. checks for BOS descriptor processing (or graceful fallback for USB 2 devices)
7. checks that no crash-class errors appear in the log

What the lifecycle harness does today:

1. boots `redbear-mini` in QEMU with `qemu-xhci` and no pre-attached USB devices
2. logs into the guest over serial, then uses the QEMU monitor to hotplug a USB keyboard
3. requires xHCI attach and completion logs plus USB HID driver spawn evidence
4. uses one-shot guest-side `/tmp/xhcid-test-hook` commands to inject a bounded
   post-`SET_CONFIGURATION` failure and a delayed attach-commit timing case
5. hot-unplugs the keyboard and requires detach evidence
6. hotplugs and hot-unplugs a USB storage device and requires attach/detach plus SCSI driver spawn evidence
7. fails on panic-class or teardown-class xHCI errors in the captured log

### Artifact to preserve

- the full terminal log from `./local/scripts/test-usb-qemu.sh --check`
- the full terminal log from `./local/scripts/test-xhci-device-lifecycle-qemu.sh --check`

## Path B - Interactive guest validation

Use this when you want to inspect the runtime manually inside the guest.

### On the host

```bash
./local/scripts/test-usb-qemu.sh redbear-mini
```

### Inside the guest

Run the packaged checker directly:

```bash
redbear-usb-check
```

Expected output:

```
redbear-usb-check: found N usb scheme entries: [...]
redbear-usb-check:   xhci.0 -> M ports
redbear-usb-check:     port 1 -> vendor:product [device name]
redbear-usb-check:     port 2 -> vendor:product [device name] [SS]
redbear-usb-check: xhci controllers: ["xhci.0"]
redbear-usb-check: all checks passed
```

The checker walks `/scheme/usb/` and `/scheme/xhci/` to verify that the xHCI controller is
enumerated, ports have devices attached, and device descriptors are readable.

## What this validates

- xHCI controller initialization
- USB device enumeration and descriptor fetching
- BOS/SuperSpeed capability detection
- HID class driver spawning (keyboard/tablet)
- SCSI class driver spawning (mass storage)
- bounded xHCI hotplug attach/detach lifecycle behavior for HID and storage devices in QEMU
- No panic or crash-class errors in USB daemons

## What this does not validate

- Real hardware USB controllers (QEMU qemu-xhci only)
- Hub topology (direct-attached devices only in the default harness)
- USB 3 SuperSpeed data paths
- Isochronous or streaming transfers
- Broad hot-plug stress testing on real hardware
- USB device mode / OTG / USB-C

## Existing USB test scripts

| Script | What it tests |
|--------|---------------|
| `test-usb-qemu.sh --check` | Full USB stack (xHCI + HID + SCSI + bounded sector-0 readback + BOS + no crashes) |
| `test-xhci-device-lifecycle-qemu.sh --check` | Bounded xHCI hotplug lifecycle proof for HID + storage attach/detach |
| `test-usb-storage-qemu.sh` | USB mass storage autospawn + bounded sector-0 readback + crash pattern check |
| `test-xhci-irq-qemu.sh --check` | xHCI interrupt delivery mode (MSI/MSI-X/INTx) |
| `test-usb-maturity-qemu.sh` | Sequential wrapper for the bounded USB maturity checks |

In-guest quick checks:
- `lsusb` — walks `/scheme/usb.*`, reads descriptors, shows vendor:product + quirks
- `redbear-info --verbose` — reports USB controller count and integration status
- `redbear-usb-check` — scheme tree walk with pass/fail exit code

## Compile-target note

Red Bear has exactly four compile targets:

- `redbear-mini`
- `redbear-live-mini`
- `redbear-full`
- `redbear-live-full`

Older names such as `redbear-desktop`, `redbear-wayland`, `redbear-kde`, and `redbear-minimal` may
still appear in historical notes or implementation details, but they are not the supported
compile-target surface.
