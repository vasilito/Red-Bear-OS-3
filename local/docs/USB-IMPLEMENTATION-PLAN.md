# Red Bear OS USB Implementation Plan

## Purpose

This document defines the current state, completeness, and implementation path for USB in Red Bear
OS. It distinguishes between the **upstream source** (unpatched) and the **Red Bear state** (after
applying `local/patches/base/redox.patch`).

The goal is to describe USB in terms of **what is built**, **what is patched**, **what is
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

## Source Model

USB driver code lives in `recipes/core/base/source/drivers/usb/`, which is an upstream-managed git
working copy. Red Bear carries all USB modifications through `local/patches/base/redox.patch`
(currently 5029 lines, 23 diff sections, 16 USB/HID/storage-related).

**Upstream state** — the unpatched source snapshot that `make fetch` produces — has significant
error handling gaps and several correctness bugs. Red Bear's patch layer fixes these, but the fixes
are only visible after patch application. This document describes the **Red Bear state** unless
explicitly noted.

## Current Repo State

### Summary

USB in Red Bear OS is **present and improving**.

The Red Bear USB stack consists of:

- a host-side xHCI controller daemon (`xhcid`) with Red Bear patches for error handling,
  correctness, and robustness
- hub and HID class daemons with Red Bear patches
- a mass-storage BOT daemon with Red Bear patches
- native USB observability (`lsusb`, `usbctl`, `redbear-info`)
- a low-level userspace client API through `xhcid_interface`
- a hardware quirks system that applies USB device-specific workarounds at runtime
- three QEMU validation harnesses covering interrupt delivery, full stack, and storage autospawn
- an in-guest scheme-tree checker (`redbear-usb-check`)

### Red Bear xHCI Patch Layer

The Red Bear patch at `local/patches/base/redox.patch` carries these changes over the upstream
source:

**Error handling (88 fixes):**
- `unwrap()` on mutex locks replaced with `unwrap_or_else(|e| e.into_inner())` across `scheme.rs`,
  `mod.rs`, `irq_reactor.rs`, and `ring.rs` — mutex poisoning no longer panics any hot-path lock
- `expect()` calls replaced with proper `Result` propagation, logged errors, or fallible helpers
- `trb_phys_ptr()` returns `Result<u64>` instead of panicking on invalid TRB pointers
- `panic!()` in `irq_reactor.rs` replaced with error returns where possible
- `device_enumerator.rs` panics replaced with error logging and graceful handling

**Correctness fixes:**
- **ERDP split**: upstream has a single `erdp()` method that conflates the software dequeue pointer
  with the hardware register read. Red Bear splits this into `dequeue_ptr()` (software ring
  position) and `erdp(&RuntimeRegs)` (actual hardware register read, per XHCI spec §4.9.3)
- **endp_direction off-by-one**: upstream uses `endp_num as usize` to index into the endpoints Vec,
  but USB endpoints are 1-indexed. Red Bear uses `endp_num.checked_sub(1)` for correct 0-based
  indexing
- **cfg_idx ordering**: upstream sets `port_state.cfg_idx` before validating the config descriptor.
  Red Bear moves the assignment after validation succeeds
- **CLEAR_FEATURE endpoint address**: upstream uses the driver-internal endpoint index for
  `CLEAR_FEATURE(ENDPOINT_HALT)`. Red Bear uses the USB endpoint address from the descriptor
  (`bEndpointAddress`)
- **usbhubd status_change_buf**: upstream has off-by-one bitmap sizing and bit-position parsing.
  Red Bear sizes the buffer correctly and computes port bit positions explicitly

**Functional additions:**
- **Event ring growth**: upstream has a stub `grow_event_ring()` that logs "TODO". Red Bear
  implements real ring doubling (up to 4096 cap), new DMA allocation, dequeue pointer preservation,
  ERDP/ERSTBA register updates, and DCS bit handling
- **BOS/SuperSpeed descriptor fetching**: `fetch_bos_desc()` called during device enumeration with
  bounds-checked slicing and graceful USB 2 fallback
- **Speed detection for hub child devices**: `UsbSpeed` enum with `from_v2_port_status()` /
  `from_v3_port_status()` mapping, passed via `attach_with_speed()` from `usbhubd`
- **Interrupt-driven operation restored**: `get_int_method()` replaces hardwired polling; MSI/MSI-X/
  INTx paths re-enabled
- **Hub interrupt EP1**: `usbhubd` reads status change via interrupt endpoint instead of polling
- **USB 3 hub endpoint configuration**: `SET_INTERFACE` always sent; stall on `(0,0)` tolerated
- **Hub change bit clearing**: `clear_port_changes` sends all relevant `ClearFeature` requests
  including USB3-specific features after every port status read
- **HID error handling**: `usbhidd` uses `anyhow::Result` with context, no panics in report loop
- **BOT transport robustness**: `usbscsid` replaces all `panic!()` with stall recovery and error
  returns; iterative bounded CSW read loop instead of unbounded recursion; correct early_residue
  computation

### Remaining Limitations

Even with the Red Bear patch applied:

- HID is still wired through the legacy mixed-stream `inputd` path
- SuperSpeedPlus differentiation requires Extended Port Status (not yet implemented)
- TTT (Think Time) in Slot Context hardcoded to 0 — needs parent hub descriptor propagation
- Composite devices and non-default alternate settings use first-match only
  (`//TODO: USE ENDPOINTS FROM ALL INTERFACES`)
- `grow_event_ring()` swaps to a new ring but does not copy pending TRBs from the old one; under
  sustained event-ring-full conditions this may lose in-flight events
- ~57 TODO/FIXME comments remain across xHCI driver files
- usbhubd: interrupt-driven change detection implemented; 1-second polling retained as fallback
- usbscsid: `ReadCapacity16` now implemented with automatic fallback from `ReadCapacity10`
- No real hardware USB validation — all testing is QEMU-only
- No hot-plug stress testing
- No USB storage data I/O validation (autospawn checked, but no read/write tested)
- USB quirk table expanded from 8 to 146 entries mined from Linux 7.0
- USB quirk flags expanded from 9 to 22 (13 new flags from Linux 7.0 including NO_BOS, HUB_SLOW_RESET)
- Terminus hub (0x1A40:0x0101) corrected from `no_lpm` to `hub_slow_reset` per Linux semantics

### Current Status Matrix

| Area | State | Notes |
|---|---|---|
| Host mode | **builds / QEMU-validated** | Real host-side stack, interrupt-driven, QEMU-validated only |
| xHCI controller | **builds / QEMU-validated** | Red Bear patch: 88 error handling fixes, ERDP split, endp_direction fix, cfg_idx fix, real grow_event_ring, mutex poison recovery on all hot-path locks; no real hardware validation yet |
| Hub handling | **builds / good quality** | `usbhubd`: all `expect()` eliminated, interrupt-driven change detection with polling fallback, graceful per-port error handling |
| HID | **builds / QEMU-validated in narrow path** | `usbhidd` handles keyboard/mouse/button/scroll via legacy input path, no panics in report loop |
| Mass storage | **builds / good quality** | `usbscsid`: typed `ScsiError`, fallible parsing, `ReadCapacity16` for >2TB, stall recovery, resilient event loop |
| Native tooling | **builds / enumerates** | `lsusb`, `usbctl`, `redbear-info`, `redbear-usb-check` provide observability |
| Low-level userspace API | **builds** | `xhcid_interface` with `UsbSpeed` enum, `attach_with_speed()` |
| Validation | **builds / QEMU-only** | 3 harness scripts + in-guest checker; no real hardware validation scripts |
| Hardware quirks | **builds** | `redox-driver-sys` quirk tables with 146 compiled-in USB quirk entries (mined from Linux 7.0) + 22 USB quirk flags; runtime TOML loading for `/etc/quirks.d/` |

## Code Quality by Daemon

### xHCI driver (`xhcid/src/xhci/`)

**Upstream state** — 91 `unwrap()`, 25 `expect()`, 7 `panic!()`, ~57 TODO/FIXME across ~6000
lines of Rust.

**Red Bear state** — mutex poisoning eliminated on all hot-path locks; `trb_phys_ptr()` returns
`Result`; critical correctness bugs fixed; ~57 TODOs remain as design notes.

Key files and their sizes:

| File | Lines (approx) | Upstream Issues | Red Bear Fix Status |
|---|---|---|---|
| `scheme.rs` | ~2800 | 36 unwrap, 14 expect, 2 panic | All unwrap/expect on hot paths fixed; endp_direction, cfg_idx, CLEAR_FEATURE fixed |
| `mod.rs` | ~1500 | 38 unwrap, 5 expect | All mutex-related unwrap fixed |
| `irq_reactor.rs` | ~750 | 17 unwrap, 6 expect, 4 panic | All fixed; grow_event_ring fully implemented |
| `ring.rs` | ~200 | 1 panic (trb_phys_ptr) | Returns Result instead of panicking |
| `event.rs` | ~60 | 1 TODO | ERDP split into dequeue_ptr() + erdp(&RuntimeRegs) |

### Class drivers

| Daemon | Lines | Error Handling Quality | Remaining unwrap/expect | Key Gaps |
|---|---|---|---|---|
| `usbhubd` | ~430 | **Good** — `Result<(), Box<dyn Error>>`, all `expect()` eliminated, interrupt-driven change detection | 0 | 1-second polling fallback if interrupt EP unavailable |
| `usbhidd` | 576 | **Good** — `anyhow::Result` with context, zero `unwrap()`/`expect()` | 0 | Hardcoded 1ms poll rate; mouse ×2 multiplier workaround; X scroll missing |
| `usbscsid` | ~1800 | **Good** — `ScsiError` typed errors, fallible `parse_bytes`/`parse_mut_bytes` helpers, resilient event loop, `ReadCapacity16` | 0 | — |

## Validation Infrastructure

### Host-side QEMU harnesses

| Script | What it tests | Limitations |
|---|---|---|
| `test-usb-qemu.sh --check` | Full stack: xHCI interrupt mode, HID spawn, SCSI spawn, BOS processing, crash errors (6 checks) | QEMU-only; log-grep based; no runtime I/O |
| `test-usb-storage-qemu.sh` | USB mass storage autospawn + crash patterns | No actual read/write; no multi-LUN; no UAS |
| `test-xhci-irq-qemu.sh --check` | xHCI interrupt delivery mode (MSI/MSI-X/INTx) | No devices attached during check; single log grep |

### In-guest tooling

| Tool | What it does | Installation |
|---|---|---|
| `lsusb` | Walks `/scheme/usb.*`, reads descriptors, shows vendor:product + quirks | Installed via `redbear-hwutils` recipe |
| `redbear-usb-check` | Scheme tree walk with pass/fail exit code | Installed via `redbear-hwutils` recipe |
| `redbear-info --verbose` | Reports USB controller count and integration status | Installed via `redbear-info` recipe |

### Runbook

`local/docs/USB-VALIDATION-RUNBOOK.md` documents two operator paths:
- **Path A**: Host-side QEMU validation via `test-usb-qemu.sh --check`
- **Path B**: Interactive guest validation via `redbear-usb-check`

### What is NOT validated

- Real hardware USB controllers (QEMU `qemu-xhci` only)
- Hub topology (direct-attached devices only)
- USB 3 SuperSpeed data paths
- Isochronous or streaming transfers
- Hot-plug stress testing
- USB storage data I/O (read/write to block device)
- USB device mode / OTG / USB-C

## Implementation Plan

### Repo-fit note

Some implementation targets live in upstream-managed trees such as
`recipes/core/base/source/...`. In Red Bear, work against those paths is carried through the
appropriate patch carrier under `local/patches/` until intentionally upstreamed. This plan names
the technical target path, not a recommendation to bypass Red Bear's overlay/patch discipline.

### Phase U0 — Support Model and Scope Freeze

**Goal**: Make USB claims honest and reproducible before widening implementation scope.

**What to do**:

- Define USB support labels per profile: `builds`, `enumerates`, `usable`, `validated`
- Declare Red Bear's near-term USB scope explicitly as **host-first**
- Record that device mode / USB-C / PD / alt-modes / USB4 are later decision points, not implied
  current scope
- Add USB status guidance to the profile/support-language discipline used elsewhere in Red Bear

**Where**: `local/docs/PROFILE-MATRIX.md`, `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md`, this
document.

**Exit criteria**: USB claims are tied to a named profile or package-group slice; no doc implies
broad USB support without a matching validation label.

---

### Phase U1 — xHCI Controller Baseline

**Status**: Substantially complete in the Red Bear patch layer. Runtime validation still QEMU-only.

**Completed (Red Bear patch)**:
- BOS/SuperSpeed descriptor fetching wired up
- Speed detection for hub child devices with `UsbSpeed` enum
- Interrupt-driven operation restored (MSI/MSI-X/INTx)
- Event ring growth fully implemented (ring doubling, DMA, ERDP/ERSTBA, DCS)
- 88 error handling fixes across scheme.rs, mod.rs, irq_reactor.rs, ring.rs
- ERDP split into `dequeue_ptr()` + `erdp(&RuntimeRegs)`
- `trb_phys_ptr()` returns `Result<u64>`
- Mutex poisoning recovery on all hot-path locks

**Remaining**:
- Validate one controller family on real hardware (requires hardware)
- Tighten controller-state correctness under sustained load (requires hardware)
- Address remaining ~57 TODO/FIXME design notes (ongoing, not blocking)
- SuperSpeedPlus differentiation via Extended Port Status (xHCI spec extension)
- TTT (Think Time) propagation from parent hub descriptor into Slot Context
- Event ring growth: copy pending TRBs from old ring to avoid losing in-flight events under sustained load

**Where**: `recipes/core/base/source/drivers/usb/xhcid/` (via `local/patches/base/redox.patch`)

**Exit criteria**: one target controller family repeatedly boots without `xhcid` panic on real
hardware; controller enumerates attached devices reliably across repeated boot cycles.

---

### Phase U2 — Topology, Configuration, and Hotplug Correctness

**Status**: Partially complete.

**Completed (Red Bear patch)**:
- USB 3 hub endpoint configuration stall handled
- `endp_direction` off-by-one fixed (`checked_sub(1)`)
- `cfg_idx` assigned after validation
- `CLEAR_FEATURE` uses correct USB endpoint address from descriptor
- `usbhubd` status_change_buf sizing and bitmap parsing fixed
- Hub interrupt EP1 status change detection replacing polling
- `usbhubd` error handling improved — all ~22 `expect()` eliminated, `Result` return type, graceful per-port failure handling
- `usbhubd` interrupt-driven change detection — reads hub interrupt IN endpoint for status change bitmap; falls back to 1-second polling if endpoint unavailable; initial full scan preserved at startup

**Remaining**:
- Validate repeated attach/detach/reset behavior under stress (requires real hardware)
- Support non-default configurations and alternate settings (requires xHCI config logic in scheme.rs)
- Improve composite-device handling and endpoint selection across interfaces (requires xHCI config logic in scheme.rs)

**Where**: `recipes/core/base/source/drivers/usb/usbhubd/`, `xhcid/src/xhci/scheme.rs`

**Exit criteria**: repeated hub and hotplug scenarios complete without stale topology state; at
least one composite device configures correctly beyond the simplest path.

---

### Phase U3 — HID Modernization

**Status**: Partially complete.

**Completed (Red Bear patch)**:
- `usbhidd` error handling improved — `anyhow::Result` with context, no panics in report loop, zero `unwrap()`/`expect()` calls
- `assert_eq!` replaced with `anyhow::bail!`
- Display write failures logged as warnings instead of panicking

**Remaining** (all require architectural changes to `inputd`, not USB-internal code):
- Migrate `usbhidd` toward named producers and per-device streams (requires inputd redesign)
- Expose hotplug add/remove behavior cleanly to downstream consumers (requires inputd redesign)
- Align USB HID with the `inputd` enhancement design already documented in-tree (cross-cutting)

**Where**: `recipes/core/base/source/drivers/input/usbhidd/`, `inputd/`,
`local/docs/INPUT-SCHEME-ENHANCEMENT.md`

**Exit criteria**: two independent USB HID devices appear as separate input sources; hot-unplug and
replug do not collapse all USB HID into one anonymous stream.

---

### Phase U4 — Storage, Userspace API, and Class Expansion

**Status**: Storage quality improved; userspace API story still low-level.

**Completed (Red Bear patch)**:
- `usbscsid` BOT transport: all `panic!()` replaced with stall recovery and error returns
- Correct endpoint addresses for `CLEAR_FEATURE` and `get_max_lun`
- Iterative bounded CSW read loop
- SCSI block descriptor parsing with bounds checks
- `usbscsid` SCSI layer: `plain::from_bytes().unwrap()` replaced with typed `ScsiError` and fallible `parse_bytes`/`parse_mut_bytes` helpers
- `usbscsid` main.rs: fallible `run()` helper, event loop continues on individual failures
- `ReadCapacity16` implemented with automatic fallback when `ReadCapacity10` returns max LBA (0xFFFFFFFF)

**Remaining** (all require hardware or design decisions):
- Runtime I/O validation: prove stall recovery works under real device I/O (requires hardware)
- Decide whether BOT-only is sufficient short-term or UAS is needed (design decision)
- Bring `libusb` to a runtime-tested state or replace with Red Bear-native API (large scope, deferred)
- Choose the next USB class families explicitly (design decision)

**Suggested class priority**: storage baseline → generic userspace API → USB networking or
Bluetooth dongle → audio/video only after controller maturity justifies it

**Where**: `recipes/core/base/source/drivers/storage/usbscsid/`, `recipes/wip/libs/other/libusb/`,
`local/recipes/system/redbear-hwutils/`

**Exit criteria**: one USB storage path validated on target profile; one coherent userspace USB API
story documented and works in practice; next supported class families named explicitly.

---

### Phase U5 — Modern USB Scope Decision Gate

**Goal**: Decide whether Red Bear remains a host-only USB system or grows toward a modern USB
platform.

**What to decide**:
- Host-only versus device mode / gadget support
- Whether OTG / dual-role matters for target hardware
- Whether USB-C / PD / alt-mode policy belongs in Red Bear's target platform story
- Whether USB4 / Thunderbolt-class behavior is in scope or explicitly excluded

**Why this phase exists**: These are architectural choices, not small driver add-ons. A
future-proof stack cannot leave them implicit forever.

**Exit criteria**: a written architecture decision exists for included and excluded modern USB
scope.

---

### Phase U6 — Validation Slices and Support Claims

**Status**: Partially complete.

**Completed**:
- `test-usb-qemu.sh` — full USB stack validation harness (6 checks)
- `test-usb-storage-qemu.sh` — USB mass storage autospawn check
- `test-xhci-irq-qemu.sh` — xHCI interrupt delivery mode check
- `USB-VALIDATION-RUNBOOK.md` — operator documentation with Paths A and B
- `redbear-usb-check` — in-guest scheme-tree checker (now installed in image)
- `lsusb` — full USB scheme walk with descriptor parsing and quirks integration
- `redbear-info` — passive USB controller reporting

**Remaining** (all require hardware):
- Add hardware-matrix coverage for target controllers and class families
- Add USB storage data I/O validation (read/write to block device)
- Add hot-plug stress testing harness

**Exit criteria**: at least one profile can honestly claim a validated USB baseline for named
controller/class scope; USB support language in docs matches real test evidence.

## Support-Language Guidance

Until U1 through U3 are substantially complete, Red Bear should avoid broad phrases such as:

- "USB support works"
- "USB storage is supported"
- "USB is complete"

Prefer language such as:

- "xHCI host support is present but experimental"
- "USB enumeration and HID-adjacent host paths exist in-tree"
- "USB support remains controller-variable"
- "USB storage support exists in-tree with improved error handling, but is not yet a broad hardware
  support claim"
- "USB error handling and correctness carry significant Red Bear patches over upstream; see
  `local/patches/base/redox.patch` for details"

## Linux Kernel USB Data Mining

### linux-kpi Scope Clarification

The `linux-kpi` compatibility layer (`local/recipes/drivers/linux-kpi/`) is used **exclusively for
GPU and Wi-Fi drivers** — it provides Linux kernel API headers and Rust FFI implementations for
porting Linux C drivers in those domains to Redox. It does **not** cover USB and contains no USB
headers, USB device ID tables, or USB driver implementations.

The linux-kpi header inventory (`src/c_headers/`) covers: PCI, DMA, IRQ, firmware, networking
(netdevice, skbuff, ieee80211, nl80211, cfg80211, mac80211), DRM, workqueue, timer, wait, sync,
memory, and related kernel infrastructure — but zero USB content. This is documented globally in
`AGENTS.md` and `local/AGENTS.md`.

### Linux 7.0 Source Availability

Linux kernel 7.0 (stable, released 2026-04-13) is extracted at
`build/linux-kernel-cache/linux-7.0/` for USB data mining purposes. This is a build cache, not a
tracked source tree — it can be re-fetched from `cdn.kernel.org` at any time.

```bash
# Re-fetch if needed:
curl -L -o build/linux-kernel-cache/linux-7.0.tar.xz \
  "https://cdn.kernel.org/pub/linux/kernel/v7.x/linux-7.0.tar.xz"
tar xf build/linux-kernel-cache/linux-7.0.tar.xz -C build/linux-kernel-cache/
```

### Mining Inventory — What Linux 7.0 Contains

| Data Source | Linux Path | Entries | Lines | Relevance |
|---|---|---|---|---|
| USB device quirks | `drivers/usb/core/quirks.c` | 64 device + 5 AMD-resume + 4 endpoint-ignore | 800 | Directly feed our quirk tables |
| USB quirk flag definitions | `include/linux/usb/quirks.h` | 19 flags | 84 | We have 9 of 19; 10 missing |
| USB storage unusual devices | `drivers/usb/storage/unusual_devs.h` | 323 entries | 2513 | Mass storage device workarounds |
| USB hub driver | `drivers/usb/core/hub.c` | — | 6567 | TT handling, hub descriptor parsing |
| xHCI host driver | `drivers/usb/host/xhci*.c/h` | ~15 files | ~30000 | Controller quirks, TRB handling |
| SCSI disk driver | `drivers/scsi/sd.c` | — | 4467 | SCSI command support tables |
| USB core headers | `include/linux/usb/*.h` | 75 headers | — | ch9.h (descriptors), hcd.h, storage.h, uas.h |

### Extraction Tool

`local/scripts/extract-linux-quirks.py` parses Linux kernel source and generates Red Bear TOML
quirk entries. Handles three source formats:
- `drivers/usb/core/quirks.c` → `[[usb_quirk]]` TOML entries (146 entries from Linux 7.0)
- `drivers/usb/storage/unusual_devs.h` → `[[usb_storage_quirk]]` TOML entries (214 entries from Linux 7.0)
- `drivers/pci/quirks.c` → `[[pci_quirk]]` TOML entries (heuristic flag mapping, requires review)

USB quirk extraction is direct and does not require review. PCI quirk extraction is heuristic and
requires manual review before committing.

The extraction script needs extension to also handle `drivers/usb/storage/unusual_devs.h` for mass
storage device entries (323 entries, different macro format `UNUSUAL_DEV`).

### Flag Gap Analysis

**Flags we have (22, fully aligned with Linux 7.0):** `NO_STRING_FETCH`, `RESET_DELAY`, `NO_USB3`,
`NO_SET_CONFIG`, `NO_SUSPEND`, `NEED_RESET`, `BAD_DESCRIPTOR`, `NO_LPM`, `NO_U1U2`,
`NO_SET_INTF`, `CONFIG_INTF_STRINGS`, `NO_RESET`, `HONOR_BNUMINTERFACES`, `DEVICE_QUALIFIER`,
`IGNORE_REMOTE_WAKEUP`, `DELAY_CTRL_MSG`, `HUB_SLOW_RESET`, `NO_BOS`,
`SHORT_SET_ADDR_TIMEOUT`, `FORCE_ONE_CONFIG`, `ENDPOINT_IGNORE`, `LINEAR_FRAME_BINTERVAL`

**All 19 Linux 7.0 USB_QUIRK flags are now covered.** The mapping table below documents the
correspondence for future reference.

| Linux Flag | Purpose | Impact | Mapping Notes |
|---|---|---|---|
| `USB_QUIRK_RESET_RESUME` | Device can't resume, needs reset instead | High — many devices | Roughly maps to our `NEED_RESET` |
| `USB_QUIRK_NO_SET_INTF` | Device can't handle SetInterface requests | Medium — composite devices | Our `NO_SET_CONFIG` targets SET_CONFIGURATION, not SET_INTERFACE |
| `USB_QUIRK_CONFIG_INTF_STRINGS` | Device can't handle config/interface strings | Low — enumeration robustness | New concept |
| `USB_QUIRK_RESET` | Device can't be reset at all | Medium — prevents crashes on morph devices | No equivalent |
| `USB_QUIRK_HONOR_BNUMINTERFACES` | Wrong interface count in descriptor | Medium — composite devices | New concept |
| `USB_QUIRK_DEVICE_QUALIFIER` | Device can't handle device_qualifier descriptor | Low — skip descriptor fetch | New concept |
| `USB_QUIRK_IGNORE_REMOTE_WAKEUP` | Device generates spurious wakeup | Low — power management | New concept |
| `USB_QUIRK_DELAY_CTRL_MSG` | Device needs pause after every control message | Medium — prevents timeouts | New concept |
| `USB_QUIRK_HUB_SLOW_RESET` | Hub needs extra delay after port reset | High — our Terminus hub entry (0x1A40:0x0101) currently has `no_lpm` but Linux marks it `HUB_SLOW_RESET` | New concept |
| `USB_QUIRK_NO_BOS` | Skip BOS descriptor (hangs at SuperSpeedPlus) | High — we added BOS fetching, some devices hang | New concept |
| `USB_QUIRK_SHORT_SET_ADDRESS_REQ_TIMEOUT` | Short timeout for SET_ADDRESS | Low — controller-specific | New concept |
| `USB_QUIRK_FORCE_ONE_CONFIG` | Device claims zero configs, force to 1 | Low — edge case | New concept |
| `USB_QUIRK_ENDPOINT_IGNORE` | Device has endpoints that should be ignored | Medium — audio devices | New concept |
| `USB_QUIRK_LINEAR_FRAME_INTR_BINTERVAL` | bInterval is linear frames, not exponential | Low — interrupt endpoint timing | Related to our `BAD_DESCRIPTOR` |

Note: Some Linux flags overlap semantically with our existing flags. The exact mapping requires a
per-flag design decision — either extend existing flags with clarified semantics or add new parallel
flags.

### Duplicate Quirk Table Problem

`xhcid` carries its own copy of the USB quirk table at
`recipes/core/base/source/drivers/usb/xhcid/src/usb_quirks.rs`. The canonical table is in
`local/recipes/drivers/redox-driver-sys/source/src/quirks/usb_table.rs`.

Both tables now carry the expanded 22-flag set and synchronized entries. The xhcid copy contains a
representative subset of the most common entries (early-boot fallback when `/etc/quirks.d/` is not
yet mounted), while the full 146-entry table and TOML runtime loading serve as the complete
runtime source.

**Long-term resolution:** xhcid should import from redox-driver-sys directly rather than
maintaining a duplicate. Until then, both must be kept in sync when adding new entries.

### Prioritized Mining Targets

**Tier 1 — COMPLETED:**

1. ✅ **USB device quirk table expansion** — All 146 entries from Linux 7.0 `quirks.c` extracted
   into `usb_table.rs` and `20-usb.toml`. Covers HP, Microsoft, Logitech, Lenovo, SanDisk,
   Corsair, Realtek, NVIDIA, ASUS, Dell, Elan, Genesys, Razer, and others.

2. ✅ **`USB_QUIRK_NO_BOS` flag** — Added. 4 devices that hang at SuperSpeedPlus BOS fetch are
   now flagged: ASUS TUF 4K PRO (0x0B05:0x1AB9), Avermedia GC553G2 (0x07CA:0x2553), Elgato 4K X
   (0x0FD9:0x009B), UGREEN 35871 (0x2B89:0x5871), ezcap401 (0x32ED:0x0401).

3. ✅ **`USB_QUIRK_HUB_SLOW_RESET` flag** — Added. Terminus hub (0x1A40:0x0101) corrected from
   `no_lpm` to `hub_slow_reset`.

4. ✅ **Flag gap closed** — All 19 Linux 7.0 USB_QUIRK flags now mapped. 13 new flags added:
   `NO_SET_INTF`, `CONFIG_INTF_STRINGS`, `NO_RESET`, `HONOR_BNUMINTERFACES`,
   `DEVICE_QUALIFIER`, `IGNORE_REMOTE_WAKEUP`, `DELAY_CTRL_MSG`, `HUB_SLOW_RESET`, `NO_BOS`,
   `SHORT_SET_ADDR_TIMEOUT`, `FORCE_ONE_CONFIG`, `ENDPOINT_IGNORE`, `LINEAR_FRAME_BINTERVAL`.

5. ✅ **Duplicate quirk tables synchronized** — Both `usb_table.rs` (redox-driver-sys) and
   `usb_quirks.rs` (xhcid) now carry the expanded flag set and synchronized entries.

6. ✅ **USB storage unusual_devs.h** — 214 entries extracted from Linux 7.0 into
   `local/recipes/system/redbear-quirks/source/quirks.d/30-storage.toml` (1716 lines). Extraction
   script extended to handle `UNUSUAL_DEV` macro format. Most common flags: `ignore_residue` (46),
   `fix_capacity` (34), `single_lun` (28), `max_sectors_64` (22), `fix_inquiry` (22). Includes
   `initial_read10` entries for Feiya SD/SDHC reader and Corsair Padlock v2.

7. ✅ **usbscsid storage quirk integration** — Storage quirks are now active at runtime.
   `usbscsid/src/quirks.rs` reads `[[usb_storage_quirk]]` entries from `/etc/quirks.d/*.toml`
   and applies them to the BOT transport and SCSI command layers. Active behavioral flags:
   - `IGNORE_RESIDUE`: suppresses CSW residue in BOT `send_command`
   - `FIX_CAPACITY`: adjusts block count from READ CAPACITY(10) by -1
   - `SINGLE_LUN`: enforces LUN=0 in CBW (future-proof for multi-LUN support)
   - `MAX_SECTORS_64`: clamps transfer length to 64 sectors in SCSI read/write
   - `INITIAL_READ10`: uses READ(10)/WRITE(10) instead of READ(16)/WRITE(16)
   Vendor/product IDs are extracted from `DevDesc` at daemon startup. A compiled-in fallback
   table covers 5 common devices for early-boot correctness.

8. ✅ **xhcid USB device quirk consumption** — xhcid now stores per-device `UsbQuirkFlags` in
   `PortState` and applies them during enumeration and runtime requests. Active behavioral flags:
    - `NO_STRING_FETCH`: skips manufacturer/product/serial/configuration string fetches
    - `BAD_DESCRIPTOR`: tolerates language/string descriptor fetch failures and continues interface parsing when malformed endpoint descriptors appear
    - `RESET_DELAY`: extends first-touch post-reset settle time via early `PortId`-based lookup
    - `HUB_SLOW_RESET`: uses a longer hub-oriented reset settle time via early `PortId`-based lookup
     - `NO_BOS`: skips BOS descriptor fetch and leaves superspeed capability detection false
   - `SHORT_SET_ADDR_TIMEOUT`: uses a shorter `Address Device` command timeout via early `PortId`-based lookup
   - `FORCE_ONE_CONFIG`: limits enumeration to configuration index 0 (configuration value 1 path)
   - `HONOR_BNUMINTERFACES`: stops interface parsing at `bNumInterfaces`
   - `DELAY_CTRL_MSG`: inserts a short post-control-transfer delay
   - `NO_SET_CONFIG`: skips `SET_CONFIGURATION`
   - `NO_SET_INTF`: skips `SET_INTERFACE`
   - `NEED_RESET`: issues xHC `Reset Device` automatically after transfer failures
    The early-enumeration timing path now uses optional TOML `port = "<root>[.<route>...]"`
    selectors in `[[usb_quirk]]` entries for quirks that must act before vendor/product are known.

9. ✅ **xhcid suspend/resume API skeleton** — xhcid now exposes explicit `port<n>/suspend` and
   `port<n>/resume` endpoints plus matching `XhciClientHandle::{suspend_device,resume_device}`
   helpers. `PortState` now tracks `PortPmState::{Active,Suspended}` and xhcid enforces
   `NO_SUSPEND` by rejecting suspend with `EOPNOTSUPP`. While suspended, control/data/reset
   activity returns `EBUSY`.

10. ✅ **usbhubd suspend coordination slice** — `usbhubd` now tracks downstream child suspend
    state and mirrors USB 2 hub-port suspend status into child xhcid devices via
    `suspend_device()` / `resume_device()`. This gives us the first real cross-layer coordination
    path for hub-attached devices without inventing a separate PM daemon. Remaining gap: suspend
    policy/origination is still external, and USB 3 link-state-driven coordination is not yet
    implemented.

**Tier 2 — Medium-term (improves robustness):**

5. **TT handling from hub.c** — Linux's hub driver reads `wHubDelay` and `bNbrPorts` from hub
   descriptors to populate TT think time and MTT capability. Our xHCI driver hardcodes `ttt = 0`
   and `mtt = false`. Mining the hub descriptor parsing logic from `hub.c` would replace these
   stubs with correct values.

6. **xHCI controller quirks from xhci-pci.c** — Linux has per-vendor controller workarounds
   (Intel PCH, AMD, Etron, Fresco, VIA). Our driver has no controller-specific paths. Mining the
   quirk table and applying it through our existing PCI quirk system would add real-hardware
   robustness.

 7. **SCSI command selection from sd.c** — READ(10)/WRITE(10) support is now implemented
   (triggered by `INITIAL_READ10` quirk flag). Remaining: REPORT LUNS for multi-LUN devices,
   SYNCHRONIZE CACHE (triggered by `NEEDS_SYNC_CACHE` flag), and START STOP UNIT for power
   management.

**Tier 3 — Future (enables new device classes):**

8. **USB class/subclass/protocol tables from ch9.h** — Complete class code definitions for device
   matching in `drivers.toml`.

9. **USB endpoint descriptor parsing from message.c** — Extended endpoint type mapping for streams
   and isochronous support.

### Mining into the Build

The Linux kernel source at `build/linux-kernel-cache/` is a build cache, not a tracked dependency.
Mined data must be materialized into durable locations:

| Mined Data | Target Location | Format |
|---|---|---|
| USB device quirks | `local/recipes/system/redbear-quirks/source/quirks.d/20-usb.toml` | TOML (146 entries ✅) |
| USB compiled-in quirks | `local/recipes/drivers/redox-driver-sys/source/src/quirks/usb_table.rs` | Rust (146 entries ✅) |
| PCI controller quirks | `local/recipes/system/redbear-quirks/source/quirks.d/10-pci.toml` | TOML |
| Storage device flags | `local/recipes/system/redbear-quirks/source/quirks.d/30-storage.toml` | TOML (214 entries ✅, active at runtime ✅) |
| Flag definitions | `local/recipes/drivers/redox-driver-sys/source/src/quirks/mod.rs` | Rust bitflags (22 USB flags ✅) |

The extraction script at `local/scripts/extract-linux-quirks.py` should be extended to also handle
`drivers/usb/storage/unusual_devs.h` for mass storage device entries.

## Summary

USB in Red Bear today is not missing. It is a real userspace host-side subsystem with meaningful
enumeration, runtime observability, hub/HID infrastructure, and a low-level userspace API.

The Red Bear patch layer carries substantial error handling and correctness improvements over the
upstream source: 88 error handling fixes (mutex poisoning recovery, expect/panic replacement, Result
conversions), multiple correctness bug fixes, real event ring growth,
class driver error handling improvements (all three USB class daemons now use `Result` types with
zero `unwrap()`/`expect()` panics), interrupt-driven hub change detection, `ReadCapacity16`
for large disk support, and a USB quirk table expanded from 8 to 146 entries with 22 quirk flags
mined from Linux 7.0.

All validation is QEMU-only. No real hardware USB testing exists.

The remaining gaps fall into three categories:

**Still-open software work (implementable without hardware):**
- Composite-device endpoint selection across interfaces (xHCI scheme.rs — `//TODO: USE ENDPOINTS FROM ALL INTERFACES`)
- Non-default configuration and alternate-setting support (xHCI scheme.rs)
- SuperSpeedPlus differentiation via Extended Port Status
- TTT (Think Time) propagation from parent hub descriptor into Slot Context
- Event ring growth does not copy pending TRBs from old ring (may lose events under sustained load)

**Architectural redesign (cross-cutting, not USB-internal):**
- HID producer modernization: per-device streams, hotplug add/remove (requires inputd redesign)
- Userspace USB API: `libusb` WIP, no coherent native story

**Hardware-dependent or design decisions:**
- Real hardware validation: no controller tested outside QEMU
- Hot-plug stress testing
- Storage I/O validation (read/write to block device)
- usbhubd 1-second polling fallback (only exercisable with real hub hardware)
- Modern USB scope decision: device mode / USB-C / PD

Software items are tracked in Phase U1 (xHCI internals) and Phase U2 (configuration/composite).
Architectural and hardware items are tracked in Phase U1 (controller hardware validation), Phase U2
(hub polling fallback), Phase U3 (HID), Phase U4 (storage/API), Phase U5 (modern USB scope
decision), and Phase U6 (validation).

Linux kernel USB data mining is documented in the "Linux Kernel USB Data Mining" section above.
Linux 7.0 source is available at `build/linux-kernel-cache/linux-7.0/` with 146 USB device quirks,
22 quirk flags (all 19 Linux USB_QUIRK flags covered), 214 active storage device quirks
consumed at runtime by usbscsid, and extensive xHCI/hub/SCSI reference code ready for extraction.
