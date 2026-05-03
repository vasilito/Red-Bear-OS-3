# ACPI I2C / I2C-HID Implementation Plan

## Goal

Implement a real laptop-class ACPI I2C stack for Red Bear OS, with `I2C-HID via ACPI`
as the first user-visible deliverable. This is required for modern touchpads, keyboards,
and other embedded input devices that are no longer exposed via PS/2.

The shortest correct path is:

`ACPI _CRS decode` -> `I2C controller ownership` -> `I2C bus API/scheme` -> `i2c-hidd` ->
`inputd integration`

This work must be treated as bare-metal boot-critical substrate, not as optional polish.

## Current State (updated 2026-04-22)

### What exists

- **`acpid`** has AML evaluation and a scheme surface for tables, AML symbols, DMI, power,
  reboot, and PCI registration. `acpid/src/resources.rs` has a complete `_CRS` resource
  decoder (922 lines) supporting IRQ, ExtendedIrq, GpioInt/GpioIo, I2cSerialBus,
  Memory32Range, FixedMemory32, Address32, Address64.
- **`/scheme/acpi/resources/<device>`** endpoint (IN PROGRESS) — acpid's `decode_resource_template()`
  exists but is not yet wired into the scheme surface. This is the #1 remaining gap.
- **`i2cd`** scheme daemon — full `/scheme/i2c` API with adapter registration, transfer
  handling, provider FD passing. Located at `drivers/i2c/i2cd/`.
- **`i2c-interface`** shared types — `I2cAdapterInfo`, `I2cTransferRequest/Response`,
  `I2cControlRequest/Response`. Located at `drivers/i2c/i2c-interface/`.
- **Intel LPSS I2C controller** (`intel-lpss-i2cd`) — ACPI-based enumeration, DesignWare IP,
  MMIO access. Registers as adapter with i2cd.
- **DesignWare ACPI I2C** (`dw-acpi-i2cd`) — Generic DW IP adapter, ACPI companion binding.
- **AMD MP2 I2C** (`amd-mp2-i2cd`) — AMD Picasso/Renoir platform I2C via MP2.
- **`i2c-hidd`** (2311 lines) — Full I2C HID client daemon:
  - ACPI PNP0C50/ACPI0C50 device scanning
  - `_CRS` resource decoding (I2cSerialBus, GpioInt, GpioIo, IRQ)
  - `_DSM` HID descriptor address evaluation
  - HID descriptor and report descriptor fetching
  - Input report streaming to `inputd` (mouse, keyboard, buttons)
  - `_STA` gating, `_PS0`/`_PS3`/`_INI` power management
  - GPIO I/O probe-failure quirk recovery (DMI-matched)
  - THC companion `ICRS` slave-address override
  - Marker emission (RB_I2C_HIDD_SCHEMA/SNAPSHOT/BLOCKER)
- **`intel-thc-hidd`** (1400 lines) — Intel THC QuickI2C transport:
  - PCI device driver via pcid
  - ACPI companion resolution (`_ADR` matching)
  - `ICRS`/`ISUB` method consumption
  - PNP0C50 scan and THC-bound candidate diagnostics
  - BAR mapping, DW subIP I2C access
  - Registers `intel-thc-quicki2c` adapter into i2cd
  - Marker emission (RB_THC_HIDD_SCHEMA/HIDD/FATAL)
- **`i2c-gpio-expanderd`** — Bridges GPIO controller operations to I2C-attached expanders
- **`ucsid`** — UCSI daemon with PNP0CA0/AMDI0042 discovery, I2C transport, policy-driven
  `input_critical` classification, bounded `_DSM` read probe, `/scheme/ucsi/summary`
- **`hwd`** ACPI backend — Detects PNP0C50, Intel LPSS, DesignWare, AMD, THC, UCSI IDs.
  Emits RB_THC_QUICKI2C, RB_UCSI_* markers. Consumes `/scheme/ucsi/summary`.
- **`amlserde`** — AML serialization/deserialization, including `AmlSerdeValue::Buffer`
  (needed for `_CRS`), `RegionSpace::GenericSerialBus` for I2C/SMBus opregions.
- **Init services** — `redbear-mini.toml` wires `i2cd`, `i2c-hidd`, `i2c-dw-acpi`,
  `i2c-gpio-expanderd`, `intel-gpiod`, `ucsid` with non-blocking startup ordering.

### What is missing (active gaps)

1. **`/scheme/acpi/resources/<device>` scheme endpoint** — `acpid` has the decoder
   (`decode_resource_template()`) but does not expose it through the scheme. Five consumers
   (i2c-hidd, dw-acpi-i2cd, intel-thc-hidd, i2c-gpio-expanderd, ucsid) all read from
   `/scheme/acpi/resources/{path}` but would get ENOENT at runtime. This is the #1 blocker.

2. **Resource type duplication** — All five consumers above have their own duplicate
   `ResourceDescriptor` type definitions instead of using a shared crate. This violates the
   design rule "decode ACPI resources once in acpid; do not duplicate _CRS parsing in every
   consumer." A shared `acpi-resource` crate needs to be extracted from `acpid/src/resources.rs`
   and adopted by all consumers.

3. **`_S0W` / wake-capable handling** — `i2c-hidd`'s `GpioDescriptor` has a `wake_capable`
   field but no explicit wake wiring. `_S0W` evaluation is not implemented. These are
   sleep/resume features, not boot-critical.

4. **GenericSerialBus / SMBus opregion support** — Not yet implemented. Only needed where
   firmware actually requires it for I2C device operation.

5. **Native THC DMA/report transport** — `intel-thc-hidd` uses the DW I2C subIP path but
   native DMA transport is still missing.

6. **Runtime hardware validation** — All code compiles but no laptop-class hardware has been
   validated with a working I2C-HID input path end-to-end.

### Design rule violation being fixed

The "decode once" principle is currently violated: five consumers each have their own
`ResourceDescriptor` types and `read_device_resources()` functions. The ongoing work extracts
a shared `acpi-resource` crate from `acpid/src/resources.rs` and refactors all consumers to
use it.

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

### Phase A: ACPI `_CRS` substrate — IN PROGRESS

Deliverables:

- add decoded ACPI resource support in `acpid`
- expose decoded device resources through `/scheme/acpi/resources/<device>`
- support at minimum:
  - IRQ
  - Extended IRQ
  - GPIO interrupt
  - GPIO I/O
  - `I2cSerialBus`

Current status:
- ✅ `acpid/src/resources.rs` — complete `_CRS` decoder (922 lines)
- ✅ `decode_resource_template()` function with all descriptor types
- 🚧 `/scheme/acpi/resources/<device>` endpoint — decoder exists but not wired into scheme
- 🚧 `acpi-resource` shared crate — extracting from acpid to eliminate duplication in 5 consumers

Acceptance:

- a consumer can query decoded resources for a device path without reimplementing AML
  resource decoding
- known laptop devices show valid controller link, slave address, and interrupt metadata

### Phase B: Native I2C substrate — COMPLETE

Deliverables:

- add a small `i2cd` scheme / API
- support controller registration, transfers, and per-device addressing
- keep scope tight; do not clone Linux I2C core complexity

Current status:
- ✅ `i2cd` — full `/scheme/i2c` scheme with adapter registry, transfers, provider FD passing
- ✅ `i2c-interface` — shared types (I2cAdapterInfo, I2cTransferRequest, I2cControlRequest)
- ✅ Controller registration and transfer API working

Acceptance:

- ✅ a userspace daemon can open an adapter and issue I2C transfers using a stable Red Bear API

### Phase C: Intel laptop controller path — COMPLETE

Deliverables:

- add Intel LPSS / Serial IO I2C controller ownership first

Current status:
- ✅ `intel-lpss-i2cd` — Intel LPSS/SerialIO I2C controller with DesignWare IP
- ✅ `dw-acpi-i2cd` — DesignWare ACPI-bound I2C adapter
- ✅ Both register with i2cd and provide transfer capability

Acceptance:

- compile-visible: ✅ at least one Intel controller driver registers a usable I2C adapter
- runtime: ❌ no bare-metal validation yet

### Phase D: `i2c-hidd` — COMPLETE (compile-visible)

Deliverables:

- bind ACPI `PNP0C50` / `ACPI0C50`
- evaluate `_DSM` using the HID-over-I2C GUID to retrieve the HID descriptor address
- fetch HID descriptor and report descriptor via I2C
- stream input reports into `inputd`

Current status:
- ✅ `i2c-hidd` — 2311-line daemon with full ACPI scanning, _DSM, HID protocol, input streaming
- ✅ `intel-thc-hidd` — 1400-line THC QuickI2C transport daemon
- ✅ Both have marker emission for boot-log diagnostics

Acceptance:

- compile-visible: ✅ all code builds
- runtime: ❌ no laptop touchpad or keyboard has produced usable events yet (blocked by Phase A)

### Phase E: AMD controller path — COMPLETE (compile-visible)

Deliverables:

- add AMD laptop-class I2C controller support
- likely DesignWare / MP2 mediated paths depending on platform

Current status:
- ✅ `amd-mp2-i2cd` — AMD MP2 I2C controller driver
- ✅ `dw-acpi-i2cd` also handles AMD DesignWare IDs (AMDI0010, AMDI0019, AMDI0510)

Acceptance:

- compile-visible: ✅ AMD controller driver exists and registers with i2cd
- runtime: ❌ no AMD laptop validated

### Phase F: Remaining ACPI I2C functions — PARTIALLY COMPLETE

Deliverables and status:

| Feature | Status | Detail |
|---------|--------|--------|
| `_STA` gating before bind | ✅ | `i2c-hidd:prepare_acpi_device()` checks presence bit |
| `_INI` where required | ✅ | Evaluated after `_PS0` in `prepare_acpi_device()` |
| `_PS0` / `_PS3` power transitions | ✅ | `prepare_acpi_device()` and `recover_acpi_device()` |
| `GpioInt`/`GpioIo` reset | ✅ | DMI-matched GPIO I/O probe-failure quirk recovery |
| `_S0W` / wake-capable | ❌ | `wake_capable` field exists but not wired; `_S0W` not evaluated |
| GpioInt wake wiring | ❌ | Wake interrupt path not implemented |
| GenericSerialBus opregion | ❌ | Not needed for boot; only where firmware requires it |

Acceptance:

- boot-critical items (STA, PS0, PS3, GPIO reset): ✅
- sleep/resume items (S0W, wake, opregion): ❌ deferred until sleep/resume is in scope

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

Concrete implementation carriers in local Linux reference tree:

- `drivers/hid/intel-thc-hid/intel-quicki2c/*` for Intel THC QuickI2C-backed HID paths
  (Lunar/Panther/Nova/Wildcat generations)
- `drivers/gpio/*` families used as ACPI `GpioInt`/`GpioIo` providers for input reset/wake rails
- `drivers/i2c/i2c-core-acpi.c` resource binding behavior for controller/device matching semantics

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

1. ~~land `_CRS` decoding in `acpid`~~ ✅ (decoder exists)
2. expose decoded resources under `/scheme/acpi/resources/` ← **ACTIVE WORK**
3. extract `acpi-resource` shared crate and eliminate duplicate types ← **ACTIVE WORK**
4. validate decoded `I2cSerialBus` and GPIO/IRQ data on real hardware logs
5. end-to-end I2C-HID input validation on bare metal

## Boot-Critical I2C Addendum (post-Phase D)

The remaining boot-critical order after initial `i2c-hidd` is:

1. Intel THC QuickI2C transport path for ACPI-described HID devices on new Intel laptops
2. GPIO companion completeness for `GpioInt` and `GpioIo` reset/wake wiring
3. platform-specific I2C controller companions only where they gate input availability

Anything outside this list should not preempt keyboard/touchpad path completion for boot-to-login.

### Concrete device classes to implement next (boot-first order)

| Priority | Device class | Linux carrier in tree | Red Bear status |
|---|---|---|---|
| P0 | Intel THC QuickI2C transport (`HID over THC`) | `drivers/hid/intel-thc-hid/intel-quicki2c/*` | ✅ detection, BAR mapping, DW subIP adapter registration landed; native DMA transport still missing |
| P1 | GPIO companions for `GpioInt`/`GpioIo` (reset/wake rails) | `drivers/gpio/*`, ACPI resource flow | ✅ GPIO I/O probe-failure quirk recovery landed; wake wiring still missing |
| P2 | Controller-companion ACPI methods (`_DSM/_DSD`) that gate input | `i2c-core-acpi.c`, QuickI2C ACPI helpers | ✅ ICRS/ISUB companion methods consumed; platform-specific gaps remain |
| P3 | USB-C/UCSI I2C only on machines where input depends on it | `drivers/usb/typec/ucsi/*` and ACPI glue | ✅ ACPI UCSI discovery + bounded I2C probe + `/scheme/ucsi/summary` landed; runtime UCSI transport/partner path still missing |

This order is strict for boot-to-login resilience on modern laptops.

## Marker Emission Summary

The I2C stack uses structured marker lines for CI/log scraping:

| Producer | Marker | Purpose |
|----------|--------|---------|
| `hwd` | `RB_THC_QUICKI2K_SCHEMA` / `RB_THC_QUICKI2K` | THC companion readiness |
| `hwd` | `RB_UCSI_SCHEMA` / `RB_UCSI_SNAPSHOT` / `RB_UCSI_SUMMARY` / `RB_UCSI_HEALTH` / `RB_UCSI_DEVICE` | UCSI topology readiness |
| `i2c-hidd` | `RB_I2C_HIDD_SCHEMA` / `RB_I2C_HIDD_BLOCKER` / `RB_I2C_HIDD_SNAPSHOT` | HID bind progress and blockers |
| `intel-thc-hidd` | `RB_THC_HIDD_SCHEMA` / `RB_THC_HIDD` / `RB_THC_HIDD_FATAL` | THC transport bring-up status |
| `ucsid` | `RB_UCSID_SCHEMA` / `RB_UCSID_SUMMARY` / `RB_UCSID_DEVICE` / `RB_UCSID_HEALTH` | UCSI daemon diagnostics |

All markers carry `generation=<n>` for cycle-level correlation across producers.

## Service Boot Ordering

```
00_base.target
  → 40_pcid.service (PCI enumeration)
  → 41_acpid.service (ACPI tables + AML evaluation)
  → 40_hwd.service (hardware discovery + markers)
  → 00_i2cd.service (I2C adapter registry)
  → 00_i2c-dw-acpi.service (DesignWare I2C controllers)
  → 00_intel-gpiod.service (Intel GPIO controller)
  → 00_i2c-gpio-expanderd.service (GPIO expander companion)
  → 00_i2c-hidd.service (I2C HID devices — touchpads, keyboards)
  → 00_ucsid.service (UCSI USB-C topology)
```

All I2C services use non-blocking (`oneshot_async`) startup so the boot path is not blocked
by any single service's probe latency.
