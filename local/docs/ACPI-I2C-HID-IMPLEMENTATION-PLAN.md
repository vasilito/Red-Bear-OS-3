# ACPI I2C / I2C-HID Implementation Plan

## Goal

Implement a real laptop-class ACPI I2C stack for Red Bear OS, with `I2C-HID via ACPI`
as the first user-visible deliverable. This is required for modern touchpads, keyboards,
and other embedded input devices that are no longer exposed via PS/2.

The shortest correct path is:

`ACPI _CRS decode` -> `I2C controller ownership` -> `I2C bus API/scheme` -> `i2c-hidd` ->
`inputd integration`

This work must be treated as bare-metal boot-critical substrate, not as optional polish.

## Current State

What already exists:

- `acpid` has AML evaluation and a scheme surface for tables, AML symbols, DMI, power,
  reboot, and PCI registration.
- `hwd` already recognizes `PNP0C50` as `I2C HID` during ACPI probe, but only as a label.
- `amlserde` can already carry raw AML buffers and the relevant opregion kinds.

What is missing:

- no decoded `_CRS` resource parser for ACPI devices
- no `/scheme/acpi/...` API for decoded `I2cSerialBus`, `GpioInt`, `GpioIo`, or IRQ data
- no native I2C controller subsystem
- no native I2C controller drivers for Intel LPSS / AMD laptop paths
- no `i2c-hidd`
- no completed input path for laptop-class ACPI-attached keyboards and touchpads

## Reference Carriers In Local Tree

These Linux sources are reference carriers only. They should guide design and descriptor
semantics, but should not be transliterated blindly.

- `build/linux-kernel-cache/linux-7.0/drivers/hid/i2c-hid/i2c-hid-acpi.c`
- `build/linux-kernel-cache/linux-7.0/drivers/hid/i2c-hid/i2c-hid-core.c`
- `build/linux-kernel-cache/linux-7.0/drivers/i2c/i2c-core-acpi.c`
- `build/linux-kernel-cache/linux-7.0/drivers/acpi/resource.c`
- `build/linux-kernel-cache/linux-7.0/drivers/mfd/intel-lpss-pci.c`
- `build/linux-kernel-cache/linux-7.0/drivers/mfd/intel-lpss-acpi.c`
- `build/linux-kernel-cache/linux-7.0/drivers/i2c/busses/i2c-designware-amdpsp.c`
- `build/linux-kernel-cache/linux-7.0/drivers/i2c/busses/i2c-amd-mp2-pci.c`

## Execution Order

### Phase A: ACPI `_CRS` substrate

Deliverables:

- add decoded ACPI resource support in `acpid`
- expose decoded device resources through `/scheme/acpi`
- support at minimum:
  - IRQ
  - Extended IRQ
  - GPIO interrupt
  - GPIO I/O
  - `I2cSerialBus`

Acceptance:

- a consumer can query decoded resources for a device path without reimplementing AML
  resource decoding
- known laptop devices show valid controller link, slave address, and interrupt metadata

### Phase B: Native I2C substrate

Deliverables:

- add a small `i2cd` scheme / API
- support controller registration, transfers, and per-device addressing
- keep scope tight; do not clone Linux I2C core complexity

Acceptance:

- a userspace daemon can open an adapter and issue I2C transfers using a stable Red Bear API

### Phase C: Intel laptop controller path

Deliverables:

- add Intel LPSS / Serial IO I2C controller ownership first

Why first:

- this is the most common modern Intel laptop path for touchpads and keyboards
- it directly unblocks `I2C-HID` on many real machines

Acceptance:

- at least one Intel bare-metal laptop registers a usable I2C adapter from ACPI-described
  hardware

### Phase D: `i2c-hidd`

Deliverables:

- bind ACPI `PNP0C50` / `ACPI0C50`
- evaluate `_DSM` using the HID-over-I2C GUID to retrieve the HID descriptor address
- fetch HID descriptor and report descriptor via I2C
- stream input reports into `inputd`

Acceptance:

- at least one laptop touchpad or keyboard produces usable events

### Phase E: AMD controller path

Deliverables:

- add AMD laptop-class I2C controller support
- likely DesignWare / MP2 mediated paths depending on platform

Acceptance:

- at least one AMD laptop reaches a functioning internal input device through ACPI I2C

### Phase F: Remaining ACPI I2C functions

Deliverables:

- `_STA` gating before bind
- `_INI` where required
- `_PS0` / `_PS3` best-effort device power transitions
- `GpioInt` and `GpioIo` semantics for reset, wake, and power sequencing
- `_S0W` / wake-capable handling where hardware requires it
- GenericSerialBus / SMBus opregion support only where firmware actually needs it

Acceptance:

- runtime bring-up no longer depends on USB or PS/2 fallback for supported laptops

## Design Rules

- prefer a small, explicit Red Bear userspace API over Linux-core emulation
- decode ACPI resources once in `acpid`; do not duplicate `_CRS` parsing in every consumer
- make controller ownership data-driven through decoded ACPI resources where possible
- keep laptop input as a boot-resilience feature, not a desktop-only feature
- treat Intel and AMD laptops as equal-priority hardware targets

## Other Boot-Relevant I2C Device Classes

`I2C-HID` is the first and most important I2C deliverable, but it is not the only I2C-related
surface that can matter during boot on modern bare metal.

### Highest priority after `I2C-HID`

- GPIO expanders used to expose reset, enable, interrupt, or wake lines for input devices
- platform-specific I2C controller companions that gate access to the actual `I2C-HID` device

These are not always directly user-visible as "devices", but they are boot-relevant whenever the
keyboard/touchpad path depends on them.

### Sometimes boot-relevant

- USB-C / UCSI / PD related I2C-attached endpoints on platforms where a USB-C attached keyboard or
  dock path is firmware-mediated and not available without those services
- embedded controller-adjacent I2C peripherals that gate keyboard/touchpad power or wake routing

These should be treated as platform-dependent bring-up work, not as universal phase-1 targets.

### Not first-order blockers for reaching login

- sensors (accelerometer, gyro, ambient light)
- battery / charger / fuel-gauge devices
- camera-side I2C devices
- most audio codecs and amplifier control devices
- thermal and fan-adjacent I2C sensors

These matter for full laptop support, but they do not outrank keyboard/touchpad bring-up for live
boot and recovery.

## Boot Priority Order

For boot-to-login on modern laptops, the correct priority is:

1. `I2C-HID` keyboards and touchpads
2. any GPIO-expander or companion I2C devices required to make those devices usable
3. platform-specific USB-C / UCSI I2C surfaces only on machines that actually depend on them for
   input availability
4. all other I2C-attached peripherals

## Immediate Next Steps

1. land `_CRS` decoding in `acpid`
2. expose decoded resources under `/scheme/acpi`
3. validate decoded `I2cSerialBus` and GPIO/IRQ data on real hardware logs
4. introduce the minimal native I2C userspace substrate
5. implement Intel LPSS controller ownership
6. implement `i2c-hidd`
