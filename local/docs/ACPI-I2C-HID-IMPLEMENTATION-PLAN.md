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

1. land `_CRS` decoding in `acpid`
2. expose decoded resources under `/scheme/acpi`
3. validate decoded `I2cSerialBus` and GPIO/IRQ data on real hardware logs
4. introduce the minimal native I2C userspace substrate
5. implement Intel LPSS controller ownership
6. implement `i2c-hidd`

## Boot-Critical I2C Addendum (post-Phase D)

The remaining boot-critical order after initial `i2c-hidd` is:

1. Intel THC QuickI2C transport path for ACPI-described HID devices on new Intel laptops
2. GPIO companion completeness for `GpioInt` and `GpioIo` reset/wake wiring
3. platform-specific I2C controller companions only where they gate input availability

Anything outside this list should not preempt keyboard/touchpad path completion for boot-to-login.

### Concrete device classes to implement next (boot-first order)

| Priority | Device class | Linux carrier in tree | Red Bear status |
|---|---|---|---|
| P0 | Intel THC QuickI2C transport (`HID over THC`) | `drivers/hid/intel-thc-hid/intel-quicki2c/*` | detection/parking landed, transport still missing |
| P1 | GPIO companions for `GpioInt`/`GpioIo` (reset/wake rails) | `drivers/gpio/*`, ACPI resource flow | partially landed, board-specific gaps remain |
| P2 | Controller-companion ACPI methods (`_DSM/_DSD`) that gate input | `i2c-core-acpi.c`, QuickI2C ACPI helpers | partially landed, still platform-dependent |
| P3 | USB-C/UCSI I2C only on machines where input depends on it | `drivers/usb/typec/ucsi/*` and ACPI glue | partial: ACPI UCSI (`PNP0CA0`/`AMDI0042`) discovery + bounded I2C probe/policy surface landed; runtime UCSI transport/partner path still missing |

This order is strict for boot-to-login resilience on modern laptops.

Current in-tree staging note:
- initfs boot ownership for ACPI/PIC is now explicit: `40_pcid.service` -> `41_acpid.service` -> `40_hwd.service` -> `40_pcid-spawner-initfs.service`; `hwd` no longer spawns `acpid` or `pcid` ad hoc.
- `redbear-live-mini` now enables `00_i2c-hidd.service` in non-blocking mode (`oneshot_async`) instead of masking it with `cmd = "true"`.
- `redbear-live-mini` now carries non-blocking `00_i2c-gpio-expanderd.service` and `00_ucsid.service` overlays so the boot-minimal image keeps companion GPIO and UCSI topology diagnostics aligned with the boot-critical I2C path.
- `intel-thc-hidd` now performs ACPI companion resolution, `_DSM` capability reads, `PNP0C50` scan, THC-bound candidate diagnostics, BAR mapping, and registers a minimal `intel-thc-quicki2c` adapter into `i2cd` with transfer handling through THC I2C subIP (DesignWare-style path).
- `intel-thc-hidd` now also consumes bounded ACPI controller-companion methods `ICRS` and `ISUB` when present, using them to refine adapter speed/addressing diagnostics and to apply ACPI timing overrides for DW SCL high/low counters.
- `intel-thc-hidd` now emits compact `RB_THC_HIDD_SCHEMA` / `RB_THC_HIDD` marker lines with THC-bound PNP0C50 candidate counts and status reasons (`available` vs `not-available`) so boot logs can distinguish missing ACPI binding surfaces from transport/runtime faults.
- `intel-thc-hidd` now canonicalizes the primary `RB_THC_HIDD` status field and warns/coerces unknown values to `error`, matching the parser-robust status policy used in `hwd`/`i2c-hidd`.
- `RB_THC_HIDD` / `RB_THC_HIDD_FATAL` markers now include `generation=<n>` for explicit correlation with other boot-readiness streams.
- `RB_THC_HIDD` status semantics now explicitly include `not-ready` (ACPI symbols not ready) and `error` (ACPI symbol scan failure), so init ordering and enumeration faults are disambiguated in CI logs.
- `intel-thc-hidd` now emits `RB_THC_HIDD_FATAL status=error ...` markers on hard-stop failures (BAR map/size failures, unexpected DW component type, i2cd registration failure, provider-loop failure) so fatal transport bring-up exits are machine-classifiable.
- `hwd` now emits compact `RB_THC_QUICKI2C status=not-ready ...` and UCSI `RB_UCSI_* status=not-ready ...` markers even when ACPI symbol enumeration returns `WouldBlock`, preserving machine-readable readiness signals during early init ordering windows.
- `hwd` now reports THC companion `ICRS`/`ISUB` method readiness counts (`thc_quicki2c_ready`) during ACPI probe so missing controller-companion surfaces are visible before driver bring-up.
- `hwd` now also emits compact `RB_THC_QUICKI2C_SCHEMA` / `RB_THC_QUICKI2C` marker lines (`status=absent|available|not-ready`) for THC companion readiness scraping in CI/log pipelines.
- `RB_THC_QUICKI2C` now also includes `generation=<n>` for consistent correlation semantics with other marker streams.
- `hwd` now assigns marker generation per ACPI probe pass and threads it through UCSI/THC fallback markers as well, eliminating `generation=0` ambiguity on local fallback paths.
- schema markers now also carry generation (`RB_UCSI_SCHEMA`, `RB_THC_QUICKI2C_SCHEMA`, `RB_UCSID_SCHEMA`, `RB_I2C_HIDD_SCHEMA`, `RB_THC_HIDD_SCHEMA`) so schema and data lines share the same correlation key shape.
- `i2c-hidd` now consults THC companion `ICRS` when bound through `intel-thc-quicki2c`, and performs a bounded slave-address override if HID `_CRS` I2C address and companion-method address disagree.
- `i2c-hidd` now emits compact `RB_I2C_HIDD_SCHEMA` and `RB_I2C_HIDD_BLOCKER` markers so unresolved THC resource-source adapter matches are machine-readable in boot logs (`reason=thc_transport_adapter_unavailable`).
- `i2c-hidd` marker emitters now canonicalize status fields (`RB_I2C_HIDD_SNAPSHOT`, `RB_I2C_HIDD_BLOCKER`) and warn/coerce unknown values to `error` for parser-safe output under schema drift.
- `RB_I2C_HIDD_BLOCKER` now also includes `generation=<n>`, aligned to the current scan cycle so blocker and snapshot events are directly correlatable.
- `RB_I2C_HIDD_BLOCKER` status semantics now also include `not-ready` (`acpi_symbols_not_ready`) and `error` (`acpi_symbol_scan_error`) on the ACPI symbol scan path, aligning HID-consumer readiness reporting with THC producer markers.
- `i2c-hidd` scan-state handling now preserves that distinction in snapshots: ACPI `WouldBlock` yields `RB_I2C_HIDD_SNAPSHOT status=not-ready reason=acpi_symbols_not_ready` (not `not-available`), avoiding false “no devices” classification during early init ordering.
- `i2c-hidd` now emits `RB_I2C_HIDD_SNAPSHOT` per scan cycle (`status`, `reason`, `devices`, `adapters`, `started`, `probe_ok`, `probe_failed`) so boot logs expose HID bring-up progress and failure density, not only blocker edges.
- `RB_I2C_HIDD_SNAPSHOT` now also carries `generation=<n>`, allowing cycle-level correlation with other readiness marker streams.
- the same `RB_I2C_HIDD_SNAPSHOT` stream now includes `status=error reason=scan_failed` on scan-cycle exceptions, preserving explicit machine-readable failure state even when scan aborts early.
- The in-tree bridge now derives adapter speed profile from ACPI-bound devices and includes bounded one-shot controller recovery/reinit on non-addressing transfer failures.
- `ucsid` now exposes `/scheme/ucsi` summary/device records with policy-driven `input_critical` classification, per-node probe policy, bounded AMDI0042 I2C probe telemetry, and PNP0CA0 ACPI `_DSM` capability-mask diagnostics (Linux carrier aligned with `ucsi_acpi.c` function-mask semantics).
- `ucsid` now also supports a policy/env-gated bounded PNP0CA0 `_DSM` function-2 read-call probe (`REDBEAR_UCSI_DSM_READ_PROBE` / `probe_dsm_read`) to surface ACPI transport readiness without enabling full UCSI command transport.
- when that bounded read probe returns a buffer payload, `ucsid` now surfaces a minimal UCSI header snapshot (`version_bcd` at offset 0 and `cci` at offset 4) for early transport-readiness diagnostics.
- `hwd` now opportunistically consumes `/scheme/ucsi/summary` (when available) and logs UCSI transport-readiness snapshot counts/devices on the ACPI probe path.
- `hwd` now classifies UCSI summary availability as `available` / `not-ready` / `not-available` / `error`, making initfs service-order gaps distinguishable from summary parse/probe failures.
- `hwd` now emits a compact `RB_UCSI_SNAPSHOT ...` key-value marker line for CI/log scrapers in addition to the human-readable UCSI status logs.
- `ucsid` now emits compact `RB_UCSID_SUMMARY ...` and per-device `RB_UCSID_DEVICE ...` marker lines so readiness can be scraped directly from daemon logs without scheme reads.
- `hwd` now mirrors per-device compact markers as `RB_UCSI_DEVICE ...` when `/scheme/ucsi/summary` is available, so CI can consume transport-readiness from the `hwd` boot stream as well.
- `ucsid` now classifies `transport_blocker` for `input_critical` devices that are not transport-ready, and exports `transport_blocked_input_critical` in summary markers for boot-priority triage.
- `ucsid`/`hwd` compact markers now include `health=ok|degraded` plus dedicated `RB_UCSID_HEALTH` / `RB_UCSI_HEALTH` lines keyed by `transport_blocked_input_critical`.
- `RB_UCSID_SUMMARY` and mirrored `RB_UCSI_SUMMARY` now carry `generation=<n>` so CI can correlate `hwd` snapshots with a specific `ucsid` scan cycle and detect stale reads.
- generation is now assigned before each `ucsid` scan pass and included on per-device markers (`RB_UCSID_DEVICE` / mirrored `RB_UCSI_DEVICE`) so device-level lines are cycle-correlated as well.
- `RB_UCSI_SNAPSHOT` is now self-contained with `generation`, `health`, and `transport_blocked_input_critical` when summary is available, while preserving explicit `status=*` for not-ready/not-available/error cases.
- `hwd` now emits `RB_UCSI_SNAPSHOT status=absent ...` when no ACPI UCSI candidates are discovered, so CI can distinguish true surface absence from service readiness failures.
- `ucsid` compact markers now emit explicit status on every scan cycle (`status=available|absent|not-ready|error`) via `RB_UCSID_SUMMARY` / `RB_UCSID_HEALTH`.
- `ucsid` now also emits `RB_UCSID_SUMMARY` / `RB_UCSID_HEALTH` with `status=error` and `health=unknown` on scan-cycle failures, so CI can distinguish explicit UCSI scan faults from stale/missing marker streams.
- on scan failure, `ucsid` now also resets shared `/scheme/ucsi/summary` counters to a clean `status=error` state for that generation (instead of carrying stale counters from the previous successful cycle).
- `ucsid` ACPI-symbol `WouldBlock` no longer collapses into `status=absent`; it now emits explicit `status=not-ready` markers so early-init readiness is not misclassified as true UCSI surface absence.
- `/scheme/ucsi/summary` now carries explicit producer status (`available|absent|not-ready|error`) and `hwd` consumes that field when mirroring `RB_UCSI_*`, preventing false `status=available` interpretations from zeroed summary counters during not-ready/error cycles.
- `hwd` now normalizes UCSI summary status parsing (trims whitespace and accepts case variants) and warns before coercing unknown statuses to `error`, improving resilience to producer/schema drift.
- `hwd` now also cross-checks status against summary counters: contradictory payloads (`available` with zero devices, or `absent` with nonzero devices) are warned and coerced to safe marker output (`absent` / `error`) instead of being mirrored verbatim.
- for `status=not-ready|not-available|error`, `hwd` now warns if nonzero payload counters/devices are present before emitting fallback status markers, making producer-status drift explicit in logs.
- `ucsid` startup default summary state is now explicitly `status=not-ready` (instead of implicit empty/default status), so early pre-scan reads of `/scheme/ucsi/summary` remain unambiguous.
- mirrored `hwd` markers now carry explicit status on `RB_UCSI_SUMMARY` / `RB_UCSI_HEALTH` (`available|absent|not-ready|not-available|error`), keeping status semantics aligned across both producers and fallback paths.
- mirrored `RB_UCSI_DEVICE` now carries richer readiness context (`i2c_backed`, DSM read support/probe flags) to match `ucsid` diagnostics from a single boot-log stream.
- for `status=not-ready|not-available|error`, `hwd` now emits fallback `RB_UCSI_SUMMARY` / `RB_UCSI_HEALTH` lines (not just `RB_UCSI_SNAPSHOT`) so CI can rely on the same marker keys in every status path.
- Native THC DMA/report transport is still missing.
