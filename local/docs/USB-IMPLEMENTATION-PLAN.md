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

USB in Red Bear OS is **present but incomplete**.

The current repo supports a real host-side USB path built around the userspace `xhcid` controller
daemon, hub and HID class spawning, native USB observability (`lsusb`, `usbctl`, `redbear-info`),
and a low-level userspace client API through `xhcid_interface`.

The current limitations are material:

- xHCI no longer hard-forces polling; it uses the existing interrupt-mode selection path again, but
  interrupt-driven behavior is still only lightly validated under runtime load
- checked-in event-ring growth support now exists, but it still needs stronger runtime validation
- USB support varies by machine, including known `xhcid` panic cases
- hub/topology handling is partial
- HID is still wired through the legacy mixed-stream `inputd` path
- USB mass storage exists in-tree and now autospawns successfully in the current QEMU validation
  path, but broader runtime stability and wider class/topology validation are still open.
- there is no evidence of validated support for broader USB classes or modern USB-C / dual-role
  scope

### Current Status Matrix

| Area | State | Notes |
|---|---|---|
| Host mode | **usable / experimental** | Real host-side stack exists, but not broadly validated |
| xHCI controller | **builds / enumerates / usable on some hardware** | Interrupt-mode selection restored, hardware-variable, event-ring growth exists in-tree but still needs stronger runtime validation |
| Hub handling | **builds / partial usable** | `usbhubd` exists, USB 3 hub limitations remain |
| HID | **builds / usable in narrow path** | `usbhidd` handles keyboard/mouse/button/scroll via legacy input path |
| Mass storage | **builds / autospawns in QEMU** | `usbscsid` now spawns from the xHCI class-driver table, but runtime stability past spawn still needs work |
| Native tooling | **builds / enumerates** | `lsusb`, `usbctl`, `redbear-info` provide partial observability |
| Low-level userspace API | **builds** | `xhcid_interface` exists, but not a mature general userspace USB story |
| libusb | **builds / experimental** | WIP, compiled but not tested |
| usbutils | **broken / experimental** | WIP, compilation error |
| EHCI/OHCI/UHCI | **absent / undocumented** | No evidence present in-tree |
| USB networking/audio/video/Bluetooth classes | **partial / experimental** | Broad class support remains incomplete, but one bounded explicit-startup USB-attached Bluetooth slice now exists |
| Device mode / OTG / dual-role / USB-C / PD / alt-modes / USB4 | **absent / undocumented** | No evidence present |

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

- partially restored interrupt-driven behavior without complete event-ring growth support
- incorrect or incomplete speed handling for child devices
- TODOs around configuration choice and alternate settings
- TODOs around endpoint selection across interfaces
- incomplete BOS / SuperSpeed / SuperSpeedPlus handling

This means the current stack is more than a bring-up stub, but still below the bar for a reliable,
future-proof USB controller foundation.

### 2. Topology and hotplug maturity are partial

The stack can enumerate ports and descendants, but the code still carries explicit TODOs around hub
behavior and USB 3 hub handling.

The current repo does not justify a claim that attach, detach, reset, reconfigure, and hub-chained
topologies are runtime-proven in a broad sense.

### 3. HID works through a legacy path

`usbhidd` exists and is meaningful evidence that USB HID is not hypothetical.

However, the current HID path is still tied to the older anonymous `inputd` producer model.
`local/docs/INPUT-SCHEME-ENHANCEMENT.md` already defines the needed next step: named producers,
per-device streams, and explicit hotplug events.

### 4. Storage is present in-tree but not a current support claim

`usbscsid` is a real driver and the xHCI class-driver table now spawns it again during QEMU USB
storage validation. The current blocker is not matching or spawn, but transport/runtime stability
after spawn.

That means Red Bear should document USB storage as **implemented in-tree but not currently enabled
as a default working class path**.

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

**Goal**: Make the USB tree and device configuration path correct enough for real-world devices.

**What to do**:

- fix USB 3 hub stall cases and other known hub limitations
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
- “USB storage support exists in-tree and is QEMU-proven for the current validation path, but is
  not yet a broad hardware support claim”

## Summary

USB in Red Bear today is not missing. It is a real userspace host-side subsystem with meaningful
enumeration, runtime observability, hub/HID infrastructure, and a low-level userspace API.

It is also not complete. The current gaps are no longer “does Red Bear have any USB code at all?”
but rather:

- controller correctness and interrupt maturity
- topology and configuration correctness
- HID modernization
- re-enabling and validating storage
- defining a coherent userspace USB API strategy
- deciding how much modern USB scope Red Bear actually wants
- building a real USB validation surface

That is the correct framing for a modern, future-proof USB implementation plan in this repo.
