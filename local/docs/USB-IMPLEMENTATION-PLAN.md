# Red Bear OS USB Implementation Plan

## Purpose

This document defines the current state, completeness, and implementation path for USB in Red Bear
OS.

The goal is to describe USB in terms of **what is built**, **what is runtime-wired**, **what is
actually usable**, and **what still needs to be implemented** before Red Bear can honestly claim a
modern, future-proof USB stack.

This document is Red Bear-specific. It uses current repo evidence from code, configs, runtime
tooling, and status docs instead of assuming inherited upstream documentation is fully current.

## Validation States

- **builds** — code exists in-tree and is expected to compile
- **enumerates** — runtime surfaces can discover controllers, ports, or descriptors
- **usable** — a specific controller/class path works in a limited real scenario
- **validated** — behavior has been exercised with explicit evidence for the claimed scope
- **experimental** — available for bring-up, but not support-promised

This repo should not treat **builds** or **enumerates** as equivalent to **validated**.

## Current Repo State

### Summary

USB in Red Bear OS is **present and improving**.

The current repo supports a real host-side USB path built around the userspace `xhcid` controller
daemon, hub and HID class spawning, native USB observability (`lsusb`, `usbctl`, `redbear-info`),
and a low-level userspace client API through `xhcid_interface`.

Completed work:

- BOS/SuperSpeed descriptor fetching wired up — `xhcid` fetches and parses BOS capability
  descriptors during device enumeration, with bounds-checked slicing and graceful USB 2 fallback
- Speed detection for hub child devices — `usbhubd` extracts child device speed from hub port
  status via `UsbSpeed` enum (`#[repr(u8)]` with `TryFrom<u8>`) and passes it through
  `attach_with_speed()` protocol; server maps to PSIV via `lookup_speed_category()`
- Interrupt-driven operation restored — `main.rs` calls `get_int_method()` instead of hard-coded
  `(None, Polling)`; MSI/MSI-X/INTx paths re-enabled
- Event ring growth implemented — `grow_event_ring()` doubles ring size (up to 4096 cap),
  allocates new DMA ring, preserves dequeue pointer, updates ERDP/ERSTBA hardware registers
- USB 3 hub endpoint configuration — `SET_INTERFACE` always sent; stall on `(0,0)` tolerated
  with debug log and graceful continuation
- Hub interrupt EP1 status change detection replacing full polling loop in `usbhubd`
- Hub change bit clearing on all port paths — `clear_port_changes` sends
  `ClearFeature(C_PORT_CONNECTION, C_PORT_ENABLE, C_PORT_RESET, C_PORT_OVER_CURRENT)` plus
  USB3-specific features (`C_PORT_LINK_STATE`, `C_PORT_CONFIG_ERROR`) after every port status read
- Runtime panic reduction across USB daemons — `device_enumerator.rs`, `irq_reactor.rs`,
  `mod.rs`, `scheme.rs`, `usbhubd/main.rs`, `usbhidd/main.rs` converted from `panic!/expect`
  to `log + continue/return` or `ok_or` in most hot paths; mutex poison recovery on all hot-path
  locks; `scsi/mod.rs` block descriptor parsing returns errors instead of panicking;
  `xhci/scheme.rs` uses `ok_or` for device descriptor and DMA buffer access
- `usbhidd` no longer panics on malformed report data — proper `Result` propagation
- `usbscsid` panic paths eliminated in BOT transport — all 4 `panic!()` calls replaced with
  stall recovery (`clear_stall` + `reset_recovery`) and `ProtocolError` returns; SCSI
  `get_mode_sense10` failure returns error instead of panicking; `main.rs` uses
  `unwrap_or_else` with `eprintln` + `exit(1)` instead of `expect()`; startup sector read
  failure logs and continues instead of panicking; event loop handles errors gracefully
- Empty UAS module stub removed from `usbscsid`; `protocol::setup` returns `None` gracefully
  for unsupported protocols instead of unwrapping
- BOT transport correctness fixes — `CLEAR_FEATURE(ENDPOINT_HALT)` now uses USB endpoint
  address from descriptor (`bEndpointAddress`) instead of driver endpoint index; `get_max_lun`
  sends correct interface number; `early_residue` correctly computes `expected - transferred`
  for short packets; CSW read uses iterative bounded loop instead of unbounded recursion
- USB validation harness (`test-usb-qemu.sh`) with 6-check QEMU validation
- In-guest USB checker binary (`redbear-usb-check`) walking scheme tree
- USB validation runbook for operators
- All changes mirrored to `local/patches/base/redox.patch` for upstream refresh survival

The remaining limitations are:

- HID is still wired through the legacy mixed-stream `inputd` path
- SuperSpeedPlus differentiation requires Extended Port Status (not yet implemented)
- TTT (Think Time) in Slot Context hardcoded to 0 — needs parent hub descriptor propagation
- Composite devices and non-default alternate settings use first-match only (`//TODO: USE ENDPOINTS FROM ALL INTERFACES`)
- `grow_event_ring()` swaps to a new ring but does not copy pending TRBs from the old one; under sustained event-ring-full conditions this may lose in-flight events
- `usbhubd` startup uses `unwrap_or_else` with graceful exit (not panics), but per-child-port handle creation now skips failed ports with error logging
- there is no evidence of validated support for broader USB classes or modern USB-C / dual-role
  scope

### Identified Correctness Issues (from audit)

A comprehensive audit of the xHCI driver identified these correctness issues. Fixes are being
applied through `local/patches/base/redox.patch`:

- **ERDP read pointer bug** (`event.rs`): `erdp()` returns the software producer pointer from the
  ring state instead of reading the actual hardware dequeue pointer from the ERDP runtime register.
  Per XHCI spec §4.9.3, the ERDP must reflect where hardware has finished reading, not where
  software enqueues new entries. This causes the event ring dequeue pointer to be incorrect after
  processing events, potentially leading to missed or double-processed events.
- **Mutex poisoning panics**: ~37 `unwrap()` calls on mutex locks across `mod.rs`, `irq_reactor.rs`,
  `scheme.rs`, and `ring.rs` will panic if a thread holding the lock panics. All should use
  `unwrap_or_else(|e| e.into_inner())` for poisoning recovery. Additionally, ~22 `expect()` calls
  need proper error handling.
- **Ring `panic!()` in `trb_phys_ptr()`**: `ring.rs` contains a direct `panic!()` on invalid state
  instead of returning an error.

### Current Status Matrix

| Area | State | Notes |
|---|---|---|
| Host mode | **usable / experimental** | Real host-side stack exists, interrupt-driven, not broadly validated on hardware |
| xHCI controller | **builds / usable on some hardware** | Interrupt delivery restored (MSI/MSI-X/INTx), event ring growth, CLEAR_FEATURE uses USB endpoint address; mutex poison recovery on all hot-path locks in scheme.rs and mod.rs |
| Hub handling | **builds / improving** | `usbhubd` uses interrupt EP1, change bits cleared, USB 3 speed-aware attach |
| HID | **builds / usable in narrow path** | `usbhidd` handles keyboard/mouse/button/scroll via legacy input path, no panics in report loop |
| Mass storage | **builds / improving** | `usbscsid` BOT transport has graceful error handling; endpoint addresses corrected; event loop handles errors; `plain::from_bytes`/`slice_from_bytes` error mapping in bot.rs and scsi/mod.rs block descriptors with bounds checks; runtime I/O validation still needed |
| Native tooling | **builds / enumerates** | `lsusb`, `usbctl`, `redbear-info`, `redbear-usb-check` provide observability |
| Low-level userspace API | **builds** | `xhcid_interface` with `UsbSpeed` enum, `attach_with_speed()` |
| Validation | **builds** | `test-usb-qemu.sh` + `redbear-usb-check` + USB-VALIDATION-RUNBOOK.md |

## Evidence Already In Tree

### Built and wired components

- `recipes/core/base/recipe.toml` builds `xhcid`, `usbctl`, `usbhidd`, `usbhubd`, and `usbscsid`
- `recipes/core/base/source/drivers/usb/xhcid/config.toml` autoloads `xhcid` by PCI class match
- `recipes/core/base/source/drivers/usb/xhcid/drivers.toml` enables hub and HID subdrivers by
  default

### Runtime and API surfaces

- `README.md` documents native `usb.*` schemes and Red Bear's `lsusb`
- `local/recipes/system/redbear-hwutils/source/src/bin/lsusb.rs` walks `usb.*` schemes, reads port
  topology, parses descriptors, and falls back to reporting port state when full descriptors fail
- `local/recipes/system/redbear-info/source/src/main.rs` reports USB-controller visibility through
  passive runtime probing
- `recipes/core/base/source/drivers/usb/xhcid/src/lib.rs` and `driver_interface.rs` define a real
  userspace client interface for the xHCI daemon
- `recipes/core/base/source/drivers/usb/usbctl/src/main.rs` is a low-level CLI over that client API

### Negative and cautionary evidence

- `HARDWARE.md` says USB support varies by machine and records systems where USB input or USB more
  broadly does not work, plus known `xhcid` panic cases
- `local/docs/AMD-FIRST-INTEGRATION.md` marks USB as **variable**
- `recipes/core/base/source/drivers/usb/xhcid/src/xhci/irq_reactor.rs` now contains event-ring growth logic, but the restored interrupt path still needs stronger validation under sustained runtime load
- `recipes/core/base/source/drivers/usb/xhcid/drivers.toml` now re-enables USB SCSI autospawn with
  explicit protocol matching for BOT (`0x50`)
- `recipes/core/base/source/drivers/COMMUNITY-HW.md` is a historical/community request ledger and
  cannot be treated as a canonical current-state source for xHCI support

## Current Gaps and Limits

### 1. Controller correctness is still incomplete

`xhcid` is real, but it is not yet mature enough to anchor broad support claims.

Current repo-visible issues include:

- TODOs around configuration choice and alternate settings
- TODOs around endpoint selection across interfaces
- TTT (Think Time) hardcoded to 0 in Slot Context — needs parent hub descriptor propagation

This means the current stack is more than a bring-up stub, but still below the bar for a reliable,
future-proof USB controller foundation.

### 2. Topology and hotplug maturity are partial

The stack can enumerate ports and descendants. USB 3 hub endpoint configuration now works without
stalling, and child device speed detection is correct when devices attach through hubs.

The current repo does not justify a claim that attach, detach, reset, reconfigure, and hub-chained
topologies are runtime-proven in a broad sense.

### 3. HID works through a legacy path

`usbhidd` exists and is meaningful evidence that USB HID is not hypothetical.

However, the current HID path is still tied to the older anonymous `inputd` producer model.
`local/docs/INPUT-SCHEME-ENHANCEMENT.md` already defines the needed next step: named producers,
per-device streams, and explicit hotplug events.

### 4. Storage is present in-tree, improving, but not yet validated

`usbscsid` is a real driver and the xHCI class-driver table spawns it during QEMU USB storage
validation. All BOT transport `panic!()` paths have been replaced with proper stall recovery and
error returns. The `main.rs` initialization path uses graceful error handling instead of `expect()`.

The remaining gap is runtime validation: proving that stall recovery actually works under real
device I/O, and that multi-LUN devices configure correctly.

Red Bear should document USB storage as **implemented in-tree with improved error handling, but not yet
runtime-validated on hardware**.

### 5. The userspace USB story is still low-level

Red Bear already has:

- scheme-level access via `usb.*`
- descriptor/request/configuration APIs through `xhcid_interface`
- low-level inspection through `usbctl`

What it does not yet have is a mature, validated general-purpose userspace USB model that desktop
or application ports can rely on with confidence. The WIP `libusb` and broken `usbutils` recipes
are the clearest sign of that gap.

### 6. Modern USB scope is still undecided / absent

There is currently no repo evidence for:

- device mode / gadget mode
- OTG or dual-role support
- USB-C policy handling
- USB Power Delivery
- alternate modes
- USB4 / Thunderbolt-class integration

Those are not small omissions. They are the difference between a partial host USB stack and a
future-proof USB platform.

## Implementation Plan

### Repo-fit note

Some of the implementation targets below live in upstream-managed trees such as
`recipes/core/base/source/...`.

In Red Bear, work against those paths should be carried through the appropriate patch carrier under
`local/patches/` until it is intentionally upstreamed. This plan names the technical target path,
not a recommendation to bypass Red Bear's overlay/patch discipline.

### Phase U0 — Support Model and Scope Freeze

**Goal**: Make USB claims honest and reproducible before widening implementation scope.

**What to do**:

- Define USB support labels per profile: `builds`, `enumerates`, `usable`, `validated`
- Declare Red Bear's near-term USB scope explicitly as **host-first**
- Record that device mode / USB-C / PD / alt-modes / USB4 are later decision points, not implied
  current scope
- Add USB status guidance to the profile/support-language discipline used elsewhere in Red Bear

**Where**:

- `local/docs/PROFILE-MATRIX.md`
- `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md`
- this document

**Exit criteria**:

- USB claims are tied to a named profile or package-group slice
- no doc implies broad USB support without a matching validation label

---

### Phase U1 — xHCI Controller Baseline

**Status**: Partially complete.

**Completed**:
- BOS/SuperSpeed descriptor fetching wired up in `get_desc()` — `fetch_bos_desc()` called,
  `bos_capability_descs()` iterator parsed, `supports_superspeed`/`supports_superspeedplus` stored
  in `DevDesc`
- Speed detection for hub child devices fixed — `UsbSpeed` enum with `from_v2_port_status()` and
  `from_v3_port_status()` mapping, passed via `attach_with_speed()` protocol from `usbhubd`
- `attach_device_with_speed()` accepts optional speed override byte, maps to PSIV via
  `lookup_speed_category()`

**Remaining**:
- Validate one controller family as the first real support target
- Tighten controller-state correctness under sustained load

**Goal**: Turn `xhcid` from partial bring-up into a dependable baseline on at least one controller
family.

**What to do**:

- Restore interrupt-driven operation or explicitly justify continued polling with measured behavior
- Eliminate current crash-class regressions and known panic paths
- Validate one controller family as the first real support target
- Tighten speed detection and controller-state correctness

**Where**:

- `recipes/core/base/source/drivers/usb/xhcid/src/main.rs`
- `recipes/core/base/source/drivers/usb/xhcid/src/xhci/`
- `HARDWARE.md`

**Exit criteria**:

- one target controller family repeatedly boots without `xhcid` panic
- controller can enumerate attached devices reliably across repeated boot cycles
- interrupt strategy is no longer a TODO-level gap

---

### Phase U2 — Topology, Configuration, and Hotplug Correctness

**Status**: Partially complete.

**Completed**:
- USB 3 hub endpoint configuration stall handled — `SET_INTERFACE` is always sent; stall on
  `(0, 0)` is tolerated with debug log and graceful continuation
- `usbhubd` now passes `interface_desc` and `alternate_setting` to `configure_endpoints`

**Remaining**:
- validate repeated attach/detach/reset behavior
- support non-default configurations and alternate settings where needed
- improve composite-device handling and endpoint selection across interfaces
- separate "enumerates" from "stays correct under topology changes"

**Goal**: Make the USB tree and device configuration path correct enough for real-world devices.

**What to do**:

- USB 3 hub stall handling completed — SET_INTERFACE always sent with (0,0) stall tolerance
- validate repeated attach/detach/reset behavior
- support non-default configurations and alternate settings where needed
- improve composite-device handling and endpoint selection across interfaces
- separate “enumerates” from “stays correct under topology changes”

**Where**:

- `recipes/core/base/source/drivers/usb/usbhubd/`
- `recipes/core/base/source/drivers/usb/xhcid/src/xhci/mod.rs`
- `recipes/core/base/source/drivers/usb/xhcid/src/xhci/scheme.rs`

**Exit criteria**:

- repeated hub and hotplug scenarios complete without stale topology state
- at least one composite device configures correctly beyond the simplest path
- non-default configuration/alternate-setting paths are either implemented or explicitly scoped out

---

### Phase U3 — HID Modernization

**Status**: Partially complete.

**Completed**:
- `usbhidd` error handling improved — `assert_eq!` replaced with `anyhow::bail!`, `.expect()` in
  main loop replaced with `match` + `continue` for graceful recovery

**Remaining**:
- migrate `usbhidd` toward named producers and per-device streams
- expose hotplug add/remove behavior cleanly to downstream consumers
- align USB HID with the `inputd` enhancement design already documented in-tree

**Goal**: Move USB HID from legacy mixed-stream input to a modern per-device runtime path.

**What to do**:

- migrate `usbhidd` toward named producers and per-device streams
- expose hotplug add/remove behavior cleanly to downstream consumers
- keep compatibility with existing consumers while widening capability
- align USB HID with the `inputd` enhancement design already documented in-tree

**Where**:

- `recipes/core/base/source/drivers/input/usbhidd/`
- `recipes/core/base/source/drivers/inputd/`
- `local/docs/INPUT-SCHEME-ENHANCEMENT.md`

**Exit criteria**:

- two independent USB HID devices appear as separate input sources
- hot-unplug and replug do not collapse all USB HID into one anonymous stream

---

### Phase U4 — Storage, Userspace API, and Class Expansion

**Goal**: Turn USB from a controller/HID substrate into a broader usable host subsystem.

**What to do**:

- stabilize USB mass-storage after autospawn (BOT transport / SCSI runtime path)
- decide whether BOT-only is sufficient short-term or whether UAS is part of the next step
- bring `libusb` to a runtime-tested state or explicitly replace it with a Red Bear-native API
  strategy
- either fix `usbutils` or document native tools as the intended replacement
- choose the next USB class families explicitly instead of implying broad support

**Suggested class priority**:

1. storage baseline
2. generic userspace API story
3. USB networking or Bluetooth dongle path
4. audio/video only after controller and transfer maturity justify it

**Where**:

- `recipes/core/base/source/drivers/storage/usbscsid/`
- `recipes/wip/libs/other/libusb/`
- `recipes/wip/sys-info/usbutils/`
- `local/recipes/system/redbear-hwutils/`

**Exit criteria**:

- one USB storage path is validated on the target profile
- one coherent userspace USB API story is documented and works in practice
- next supported class families are named explicitly in docs and support labels

---

### Phase U5 — Modern USB Scope Decision Gate

**Goal**: Decide whether Red Bear remains a host-only USB system or grows toward a modern USB
platform.

**What to decide**:

- host-only versus device mode / gadget support
- whether OTG / dual-role matters for target hardware
- whether USB-C / PD / alt-mode policy belongs in Red Bear's target platform story
- whether USB4 / Thunderbolt-class behavior is in scope or explicitly excluded

**Why this phase exists**:

These are architectural choices, not small driver add-ons. A future-proof stack cannot leave them
implicit forever.

**Exit criteria**:

- a written architecture decision exists for included and excluded modern USB scope

---

### Phase U6 — Validation Slices and Support Claims

**Status**: Partially complete.

**Completed**:
- `local/scripts/test-usb-qemu.sh` — Full USB stack validation harness that boots with xHCI +
  keyboard + tablet + mass storage, then checks for xHCI interrupt mode, HID spawn, SCSI spawn,
  BOS processing, and no crash-class errors

**Remaining**:
- add hardware-matrix coverage for target controllers and class families
- extend `redbear-info` only where passive probing can be honest
- tie support claims to a concrete profile or package-group slice

**Goal**: Turn USB from a collection of partial capabilities into an evidence-backed support story.

**What to do**:

- create USB-focused validation helpers instead of relying only on other profile scripts that happen
  to include `qemu-xhci`
- add hardware-matrix coverage for target controllers and class families
- extend `redbear-info` only where passive probing can be honest
- tie support claims to a concrete profile or package-group slice

**Where**:

- `local/scripts/`
- `local/recipes/system/redbear-info/`
- `HARDWARE.md`
- `local/docs/PROFILE-MATRIX.md`

**Exit criteria**:

- at least one profile can honestly claim a validated USB baseline for named controller/class scope
- USB support language in docs matches real test evidence

## Support-Language Guidance

Until U1 through U3 are substantially complete, Red Bear should avoid broad phrases such as:

- “USB support works”
- “USB storage is supported”
- “USB is complete”

Prefer language such as:

- “xHCI host support is present but experimental”
- “USB enumeration and HID-adjacent host paths exist in-tree”
- “USB support remains controller-variable”
- “USB storage support exists in-tree with improved error handling, but is not yet a broad hardware support claim”

## Summary

USB in Red Bear today is not missing. It is a real userspace host-side subsystem with meaningful
enumeration, runtime observability, hub/HID infrastructure, and a low-level userspace API.

Recent work has closed several specific gaps: BOS/SuperSpeed descriptor handling, hub child speed
detection, USB 3 hub configuration stalls, HID error handling, and a comprehensive QEMU validation
harness.

The remaining gaps are:

- controller interrupt maturity under sustained load
- topology and configuration correctness under attach/detach stress
- HID modernization toward named producers and per-device streams
- re-enabling and validating storage runtime stability
- defining a coherent userspace USB API strategy
- deciding how much modern USB scope Red Bear actually wants
- building broader USB validation coverage

That is the correct framing for a modern, future-proof USB implementation plan in this repo.
