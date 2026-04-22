# USB Boot Input Plan

## Goal

Make external USB keyboards a reliable bare-metal boot fallback for Red Bear OS.

This is a boot-resilience requirement, not optional polish. A system that reaches early
boot but cannot accept keyboard input on modern hardware is not a complete live/recovery
environment.

## Current Assessment

### What works today

- `xhcid` is the only host-controller path with a real runtime device model.
- `xhcid` spawns `usbhubd` and `usbhidd` via class matching.
- `usbhidd` reads HID input reports and forwards keyboard/mouse events into `inputd`.

This means USB keyboard input can work today only when the keyboard is reached through the
`xHCI -> usbhubd/usbhidd -> inputd` path.

### What does not work today

- `ehcid` is still an ownership / handoff / port-state daemon, not a real runtime host stack.
- `uhcid` is still ownership + port reset + logging only; full scheduling/enumeration is explicitly
  not implemented.
- `ohcid` is in the same state as `uhcid`.

The code is explicit about this:

- `ehcid`: connected EHCI-owned ports still fail with "EHCI enumeration is still not implemented"
- `uhcid`: connected ports still fail with "runtime enumeration is still not implemented"
- `ohcid`: connected ports still fail with "OHCI enumeration is still not implemented"

### Important practical consequence

An external USB keyboard on bare metal is **not guaranteed** to appear through `xHCI`.

It may instead land on:

- an EHCI root-hub path
- a UHCI/OHCI companion path after EHCI handoff
- a firmware/routing topology where low/full-speed devices do not end up on the `xHCI` runtime path

On such systems, the current code can detect controller ownership and connected ports, but still
cannot produce a real keyboard input path.

### LED state is a separate and weaker path

`usbhidd` now has a bounded best-effort HID output-report path for keyboard LEDs. It toggles
`Caps Lock`, `Num Lock`, and `Scroll Lock` locally on keydown and sends a one-byte HID output
report via `SET_REPORT`.

This is useful, but it is **not** the same as a complete global keyboard lock-state authority:

- it is per-device, not system-global
- it is best-effort and disables itself after the first device-side failure
- it does not solve missing USB enumeration on non-xHCI host-controller paths

So dead `Caps Lock` / `Num Lock` indicators still do **not** prove that keyboard transport is dead,
and working LEDs do **not** prove that the external USB keyboard fallback problem is solved.

## Root-Cause Summary For Current Bare-Metal Symptom

When a USB-attached keyboard does not bring up input during boot, the most likely causes are:

1. the keyboard is not on the `xHCI` runtime path
2. it lands on `EHCI/UHCI/OHCI`, where enumeration is not implemented yet
3. even if input later works, keyboard LEDs may still be misleading because LED sync is only a
   bounded per-device best-effort path

## Current Structural Gap

There is also a policy gap:

- `ehcid`, `uhcid`, and `ohcid` contain `--strict-boot` logic
- and the current boot path still does **not** hardcode `--strict-boot` in initfs driver command lines
- however, strict mode can now be enabled through `REDBEAR_STRICT_USB_BOOT=1`, which is inherited by
  `pcid-spawner` service units and then by legacy USB controller daemons

So the code contains a boot-guard concept that is currently not activated by the initfs spawn path.

This does not create input support by itself, but it does matter for observability and boot policy.

## Execution Order

### Phase U-B1: Make boot policy honest

Deliverables:

- decide whether initfs should pass `--strict-boot` to legacy USB host daemons
- provide a non-invasive runtime toggle for strict mode during bring-up
- if enabled, make the failure mode explicit and bounded
- if not enabled, log clearly that legacy USB ownership exists without runtime enumeration

Acceptance:

- the boot log makes it obvious whether the system has a usable USB keyboard path or only controller
  ownership
- strict mode can be enabled without rewriting driver command lines

### Phase U-B2: Finish legacy host runtime enumeration

Deliverables:

- implement real device enumeration for `uhcid`
- implement real device enumeration for `ohcid`
- implement real runtime ownership of low/full-speed devices behind `ehcid` companion routing

Acceptance:

- a low/full-speed USB keyboard on bare metal can reach `usbhidd` through the legacy host path

### Phase U-B3: Keep one HID class path

Deliverables:

- avoid inventing a second HID stack just for legacy controllers
- make legacy host controllers feed the existing USB class-driver model
- keep `usbhidd` and `usbhubd` as the class daemons above controller-specific ownership

Acceptance:

- keyboard class handling is shared regardless of host controller family

### Phase U-B4: Implement keyboard LED output

Deliverables:

- keep the new HID output-report support in `usbhidd` bounded and non-fatal
- decide whether the current per-device local toggle model is sufficient, or whether Red Bear
  later needs a system-authoritative lock-state surface
- preserve the rule that LED sync must never block or destabilize keyboard input

Acceptance:

- LED state tracks keyboard lock state on at least one supported USB keyboard in the current
  bounded per-device model

### Phase U-B5: Validation

Deliverables:

- QEMU validation for xHCI remains
- add bounded validation for legacy host-controller paths where feasible
- require bare-metal validation on systems where external USB keyboard currently fails

Acceptance:

- one xHCI bare-metal proof
- one EHCI/UHCI/OHCI-involved bare-metal proof
- explicit evidence that external USB keyboard input reaches login

## Design Rules

- do not treat controller ownership as equivalent to device enumeration
- do not treat keyboard LED state as equivalent to keyboard input health
- reuse the existing HID class-driver path instead of splitting per-controller userland stacks
- prefer bounded boot-policy checks and explicit failure logs over silent partial bring-up

## Priority Judgment

For bare-metal boot resilience, the correct order is:

1. finish legacy USB host runtime enumeration
2. then add keyboard LED output reports
3. in parallel, continue `I2C-HID` for internal modern laptop keyboards/touchpads

External USB keyboard fallback and internal `I2C-HID` are complementary. Red Bear needs both.
