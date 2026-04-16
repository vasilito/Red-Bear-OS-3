# Red Bear OS Wi-Fi Implementation Plan

## Purpose

This document defines the current Wi-Fi state in Red Bear OS and lays out the recommended path for
integrating Wi-Fi drivers and a usable wireless control plane.

The goal is not to imply that working Wi-Fi already exists. The goal is to describe what the repo
currently proves, what `linux-kpi` can and cannot realistically provide, and how Red Bear can grow
from a **bounded experimental Intel Wi-Fi scaffold** to one experimental, validated Wi-Fi path that
fits the existing Redox / Red Bear architecture.

## Validation States

- **builds** — code exists in-tree and is expected to compile
- **boots** — image or service path reaches a usable runtime state
- **reports** — runtime surfaces can honestly report current wireless state
- **validated** — behavior has been exercised with real evidence for the claimed scope
- **experimental** — available for bring-up, but not support-promised
- **missing** — no in-tree implementation path is currently present

This repo should not treat planned wireless scope as equivalent to implemented support.

## Current Repo State

### Summary

Wi-Fi is currently **not supported as working connectivity** in Red Bear OS.

There is still no complete in-tree cfg80211/mac80211/nl80211-compatible surface, no supplicant
path, and no profile that can honestly claim working Wi-Fi support. What now exists in-tree is a
bounded Intel bring-up slice: a driver-side package, a Wi-Fi control daemon/scheme, profile
plumbing, and host-validated LinuxKPI/CLI scaffolding below the real association boundary.

What the repo *does* have is a meaningful set of prerequisites:

- userspace drivers and schemes as the standard architectural model
- `redox-driver-sys` for PCI/MMIO/IRQ/DMA primitives
- `linux-kpi` as a limited low-level C-driver compatibility layer
- `firmware-loader` for blob-backed devices
- a working native wired network path through `network.*`, `smolnetd`, `dhcpd`, and `netcfg`
- profile/package-group discipline, including the reserved `net-wifi-experimental` slice

### Current Status Matrix

| Area | State | Notes |
|---|---|---|
| Wi-Fi controller support | **experimental bounded slice exists** | `redbear-iwlwifi` provides an Intel-only bounded driver-side package, not validated Wi-Fi connectivity |
| Linux wireless stack compatibility | **early compatibility scaffolding exists** | `linux-kpi` now carries initial `cfg80211` / `wiphy` / `mac80211` registration and station-mode compatibility scaffolding, but not a complete Linux wireless stack |
| Firmware loading | **partial prerequisite exists** | `firmware-loader` can serve firmware blobs generically |
| Wireless control plane | **experimental bounded slice exists** | `redbear-wifictl` and `redbear-netctl` expose bounded prepare/init/activate/scan orchestration, not real association support |
| Post-association IP path | **present** | Native `smolnetd` / `netcfg` / `dhcpd` / `redbear-netctl` path exists |
| Desktop Wi-Fi API | **missing** | No NetworkManager-like or D-Bus Wi-Fi surface |
| Runtime diagnostics | **experimental bounded slice exists** | `redbear-info` and runtime helpers expose Wi-Fi state surfaces, but not real Wi-Fi functionality proof |

## Evidence Already In Tree

### Direct current-state caution about supported connectivity

- `HARDWARE.md` says broad Wi-Fi and Bluetooth hardware support is still incomplete even though
  bounded in-tree scaffolding now exists
- `local/docs/AMD-FIRST-INTEGRATION.md` now treats `Wi-Fi/BT` as in progress with bounded wireless
  scaffolding present but validated connectivity still incomplete

### Positive driver-side prerequisites

- `docs/04-LINUX-DRIVER-COMPAT.md` documents `redox-driver-sys`, `linux-kpi`, and
  `firmware-loader`
- `local/recipes/drivers/redox-driver-sys/` provides userspace PCI/MMIO/IRQ/DMA primitives
- `local/recipes/drivers/linux-kpi/` provides a limited Linux-style compatibility subset
- `local/recipes/system/firmware-loader/` provides `scheme:firmware`

### Positive network/control-plane prerequisites

- `local/docs/NETWORKING-RTL8125-NETCTL.md` documents the native wired path:
  `pcid-spawner` → NIC daemon → `network.*` → `smolnetd` → `dhcpd` / `netcfg`
- `recipes/core/base/source/netstack/src/scheme/netcfg/mod.rs` shows route/address/resolver state
  is already exposed through a native control scheme
- `local/recipes/system/redbear-netctl/source/src/main.rs` shows Red Bear already uses a native
  network profile tool, even though it is currently wired-only
- `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md` reserves `net-wifi-experimental` as a package-group
  slot for future wireless work

## Feasibility Constraints

### 1. Wi-Fi is not just a driver

Wi-Fi in Red Bear cannot be treated as a single hardware daemon.

At minimum, a working Wi-Fi path needs:

- hardware transport and firmware bring-up
- scan/discovery
- authentication and association state
- link-state and disconnect handling
- credential storage
- post-association handoff into the native IP stack
- later desktop/user-facing integration if the repo wants it

This makes Wi-Fi more like a complete subsystem than a simple wired NIC driver.

### 2. `linux-kpi` is feasible only below the wireless control-plane boundary

Current `linux-kpi` is suitable for low-level driver-enablement work such as:

- PCI / IRQ / DMA / MMIO access
- firmware request glue
- workqueue-style helper logic
- C-driver compatibility for narrow hardware bring-up

Current `linux-kpi` is **not** a complete Wi-Fi architecture because the repo still has no in-tree,
complete:

- cfg80211
- mac80211
- nl80211
- wiphy model
- supplicant/control-plane compatibility layer

So `linux-kpi` is feasible only as a **partial low-level aid**, not as the primary Red Bear Wi-Fi
stack.

### 3. The current Red Bear control plane is Ethernet-specific

The current native network stack is useful, but not yet Wi-Fi-ready.

`redbear-netctl` now has a first Wi-Fi-facing profile layer, but only at the profile/orchestration
boundary.

Current `redbear-netctl` support now includes:

- `Connection=ethernet`
- `Connection=wifi`
- arbitrary `Interface=` values at the profile layer (for example `eth0`, `wlan0`)
- DHCP/static address, route, and DNS control after association
- Wi-Fi profile fields for `SSID`, `Security`, and `Key`/`Passphrase`
- a bounded native handoff to a future `/scheme/wifictl` control surface

The repo now also contains the first bounded implementation of that control surface:

- `local/recipes/system/redbear-wifictl/` provides a `redbear-wifictl` daemon and `/scheme/wifictl`
  scheme
- the current daemon supports a stub backend for end-to-end validation and an Intel-oriented backend
  boundary that detects Intel wireless-class PCI devices
- the current Intel backend is now firmware-aware: it reports candidate firmware families, selected
  firmware blobs when present, and supports a bounded `prepare` step before connect
- this is still not a full Intel association path, but it turns the control-plane contract into a
  real in-tree interface rather than a placeholder

This means `redbear-netctl` can now represent and start a Wi-Fi profile without pretending Wi-Fi is
just an Ethernet profile, but it still does **not** own scan/auth/association itself.

`netcfg` is no longer hard-wired to a single `eth0` node in the control scheme. The native control
surface can now expose per-device interface nodes dynamically from the current device list, which is
the first required step for post-association Wi-Fi handoff.

That means Red Bear can reuse its native IP plumbing **after association**, but not as the radio
control plane itself.

### 4. Intel target changes the first-driver strategy

The original version of this plan preferred a FullMAC-first path to avoid recreating Linux wireless
subsystem boundaries.

That is still the simplest architecture in the abstract, but the project target has now changed:
Red Bear must target **Intel Wi-Fi for Arrow Lake and older Intel client chips**.

That means the first realistic driver family is now Intel `iwlwifi`-class hardware rather than an
unspecified FullMAC family.

This changes the implementation burden materially:

- Intel `iwlwifi` is not a simple FullMAC path
- current Linux support is tightly coupled to `mac80211` / `cfg80211`
- firmware loading remains necessary but is not the hard part by itself
- Red Bear must plan for a bounded compatibility layer below the user-facing control plane

So the practical first target is now:

- **Intel `iwlwifi`-class devices, Arrow Lake and older**, with the understanding that this is a
  harder first driver family than a generic FullMAC-first strategy would have been

## Recommended Architecture

The best current Red Bear Wi-Fi architecture for the Intel target is:

1. **native Red Bear wireless control plane above the driver boundary**
2. **Intel-first low-level driver work below that boundary**
3. **reuse `firmware-loader` and `redox-driver-sys` wherever possible**
4. **accept bounded `linux-kpi` growth where Intel transport/firmware glue requires it**

### Build-note for the current Intel control-plane code

The earlier Redox-target source-level compile failure in `redbear-wifictl`'s Intel backend is now
fixed in-tree. If `cargo build --target x86_64-unknown-redox` still reports that
`x86_64-unknown-redox-gcc` is missing, check whether the repo-provided cross toolchain under
`prefix/x86_64-unknown-redox/sysroot/bin/` is on `PATH` before treating it as a fresh source-level
regression.

For repeatable local builds, use `local/scripts/build-redbear-wifictl-redox.sh`, which wires that
repo-provided toolchain path into the build invocation explicitly.
5. **reuse the existing native IP path only after association**

This is still a native-first architecture at the control-plane level, but it is no longer a pure
FullMAC-first plan.

### Subsystem boundary

The Wi-Fi subsystem should be split into these pieces:

- one **device transport / driver daemon** for the Intel target family
- one **firmware loading path** via `firmware-loader`
- one **Wi-Fi control daemon** for scan/auth/association/link state
- one **user-facing control tool** (`wifictl` or equivalent)
- one **post-association handoff** into `smolnetd` / `netcfg` / `dhcpd`
- one **later desktop shim** only if KDE/user-facing workflows require it

`redbear-netctl` should **not** become the supplicant. It can own profile orchestration and the
post-association IP handoff, but scan/auth/association should still live in a dedicated Wi-Fi
control daemon or scheme.

The current implementation now matches that boundary more closely:

- `redbear-netctl` can parse Wi-Fi profiles and hand credentials/intent to a native Wi-Fi control
  surface (`/scheme/wifictl`)
- `redbear-netctl` now also has a host-side CLI proof that starting a Wi-Fi profile drives the
  bounded driver/control actions and preserves the surfaced bounded connect metadata in status
  output; this is not yet proof of verified prepare/init/activate/connect execution order on a real
  associated link
- `redbear-netctl` stop now also drives the bounded disconnect path, so the current profile-manager
  slice covers start and stop instead of start-only behavior
- `redbear-wifictl` now exposes bounded connect and disconnect CLI flows, and the runtime checker
  now exercises the bounded connect step through the scheme surface
- the native IP path can address a non-`eth0` interface name after association
- `redbear-netctl` now also performs interface-specific DHCP handoff for Wi-Fi profiles and waits
  for the selected interface to receive an address in the bounded host/runtime validation path
- `local/recipes/system/redbear-netctl-console/` now adds a terminal UI client on top of the same
  `/scheme/wifictl` + `/etc/netctl` contract, so scan/select/edit/save/connect/disconnect workflows
  can be exercised without introducing a new daemon or bypassing profile semantics
- `local/scripts/test-wifi-baremetal-runtime.sh` now provides the strongest in-repo runtime
  validation path for this Wi-Fi slice on a real Red Bear OS target: driver probe, control probe,
  bounded connect/disconnect, profile start/stop, and `redbear-info --json` lifecycle reporting
- `redbear-phase5-wifi-check` now packages that bounded in-target validation flow as a first-class
  guest/runtime command, instead of leaving it only as a shell script
- that packaged runtime proof currently defaults to the bounded open-profile path; WPA2-PSK remains
  implemented and host/unit-verified elsewhere in-repo rather than equally packaged/runtime-validated
- `redbear-phase5-wifi-capture` now packages the corresponding runtime evidence bundle, so target
  runs can produce a single JSON artifact for debugging real hardware/passthrough failures;
  that bundle now includes command outputs, Wi-Fi scheme state, `netctl` profile state, active
  profile contents, interface listings, and `lspci` output
- `test-wifi-baremetal-runtime.sh` now writes that capture bundle to `/tmp/redbear-phase5-wifi-capture.json`
  as part of the target-side bounded validation flow
- `local/scripts/test-wifi-passthrough-qemu.sh` now provides the corresponding VFIO/QEMU harness for
  exercising the same bounded runtime path when an Intel Wi-Fi PCI function can be passed through to
  a Red Bear guest, including optional host-side extraction of the packaged Wi-Fi capture bundle
- `local/scripts/prepare-wifi-vfio.sh` now provides the matching host-side bind/unbind helper for
  moving an Intel Wi-Fi PCI function onto `vfio-pci` before passthrough validation and restoring it
  afterwards
- `local/scripts/run-wifi-passthrough-validation.sh` now wraps the whole host-side passthrough flow:
  bind to `vfio-pci`, run the packaged in-guest Wi-Fi validation path, collect the host-visible
  capture bundle, and restore the original host driver afterwards
- `local/scripts/validate-wifi-vfio-host.sh` now provides a read-only preflight for the same flow:
  PCI presence, current binding, UEFI firmware, image availability, QEMU/expect presence, VFIO
  module state, and visible IOMMU groups
- `local/docs/WIFI-VALIDATION-RUNBOOK.md` now ties the bare-metal path, VFIO path, packaged
  validators, and capture artifacts together into one operator runbook
- the control daemon exists now, and the first bounded driver-side package now exists as
  `local/recipes/drivers/redbear-iwlwifi/`
- `redbear-iwlwifi` now supports bounded `--probe` and `--prepare` driver-side actions for the
  current Intel family set
- `redbear-iwlwifi` now also supports bounded `--init-transport` and `--activate-nic` actions for
  the current Intel family set
- `redbear-iwlwifi` now also supports bounded `--scan` and `--retry` actions for the current Intel
  family set
- `redbear-iwlwifi` now also carries a first bounded `--connect` path that runs through the new
  LinuxKPI wireless compatibility scaffolding instead of stopping immediately at a hardcoded
  transport/association error
- `redbear-iwlwifi` now also carries a bounded `--disconnect` path so the current station-mode
  lifecycle is not connect-only anymore
- `redbear-iwlwifi --status` now reports the current bounded driver-side view directly
- the bounded driver-side action set can be exercised through the dedicated helper script
  `local/scripts/test-iwlwifi-driver-runtime.sh`
- on Redox targets, `redbear-iwlwifi` now also begins to use a `linux-kpi` C shim for firmware
  request and PCI/MMIO-facing prepare/transport actions instead of keeping those paths purely in
  Rust fallback code

### Port vs rewrite decision

For Arrow Lake-and-lower Intel Wi‑Fi, the current repo direction is:

- **do not** attempt a full Linux `mac80211` / `cfg80211` / `nl80211` port first,
- **do** create a bounded Intel driver/transport package below the native Red Bear Wi‑Fi control
  plane,
- **do** accept limited `linux-kpi` growth only where it materially reduces transport/firmware glue
  cost,
- keep `redbear-netctl` and `redbear-wifictl` as the native control-plane/user-facing layers above
  that driver boundary.

That means the repo is now following a **bounded transport-layer port with native control-plane
rewrite above it**, not a full Linux wireless stack port and not a pure greenfield driver rewrite.

### What this means in practical porting terms

The currently feasible interpretation of “use the real Linux Intel driver through `linux-kpi`” is:

- port and reuse **transport-layer and firmware-facing logic** where that lowers cost materially,
- keep the **native Red Bear control plane** above that boundary,
- and avoid treating a full `cfg80211` / `mac80211` / `nl80211` / `wiphy` port as the immediate
  first milestone.

In other words, Red Bear should not try to import the whole Linux wireless stack in one step.
Red Bear should instead pull over the **device-facing part** of the Intel stack in bounded layers.

### Boundary where `linux-kpi` is helpful

`linux-kpi` is most useful for:

- PCI helper semantics
- MMIO/IRQ/DMA glue
- firmware request/load glue
- workqueue-style deferred execution
- timer, mutex, and IRQ-critical-section helpers that transport-facing Linux Wi-Fi code expects
- low-level transport and reset sequences
- early packet-buffer / `net_device` / `wiphy` / registration scaffolding when Red Bear begins the
  first real Linux wireless-subsystem compatibility slice

That is the boundary where “run Linux driver code on Red Bear” is currently realistic.

The current tree now has the first explicit step in that direction as well:

- `linux-kpi` now carries initial `sk_buff`, `net_device`, `cfg80211`/`wiphy`, and `mac80211`
  registration scaffolding alongside the earlier firmware/timer/mutex/IRQ helpers
- that scaffolding now also includes the first station-mode compatibility types and hooks used by
  the bounded Intel scan/connect path: SSID/connect/station parameter structs plus basic
  `cfg80211_connect_bss` / ready-on-channel and `mac80211` VIF/STA/BSS-conf surfaces
- the bounded station-mode slice now also preserves real private-allocation sizes, exposes the
  common `sk_buff` reserve/push/pull/headroom/tailroom helpers, tracks `net_device`
  registration/setup, keeps carrier down until connect success, and routes
  `ieee80211_queue_work()` through the bounded LinuxKPI workqueue instead of silently dropping
  deferred work
- this new scaffolding is compile- and host-test-validated inside the `linux-kpi` crate
- this is still **not** a claim that Red Bear now has a working Linux wireless stack

### Boundary where a full Linux port becomes too expensive

A full Linux-style `iwlwifi` port becomes dramatically more expensive as soon as the code path
depends on the Linux wireless subsystem proper:

- `cfg80211`
- `mac80211`
- `nl80211`
- `wiphy` model and callbacks
- Linux regulatory integration
- Linux station/BSS bookkeeping and userspace-facing wireless semantics

The repo now has the earliest pieces of those subsystem layers, but still not anything close to a
complete Linux wireless stack. Building them out far enough to host Intel Wi‑Fi as a true Linux-like
solution still turns the effort from a bounded driver port into a much larger compatibility-stack
port.

### Chosen direction

The chosen direction for Arrow Lake-and-lower Intel Wi‑Fi is therefore:

1. keep the **native Red Bear control plane** (`redbear-netctl` + `redbear-wifictl`),
2. keep pushing the **hardware-facing Intel path** down into `redbear-iwlwifi`,
3. use `linux-kpi` for the low-level Linux-facing transport/runtime glue where that reduces effort,
4. avoid promising or attempting a full Linux wireless-stack port as the first milestone.

The current code now matches that decision more closely than before: `redbear-wifictl` remains the
native control plane, while `redbear-iwlwifi` is the place where Linux-facing firmware/PCI/MMIO
driver logic is starting to accumulate.

The current tree also now pushes more of that bounded Intel path through the actual LinuxKPI
surface instead of bespoke C declarations alone:

- `linux-kpi` now exports direct and async firmware request helpers for firmware-family workflows
- timer and IRQ save/restore bindings are exported through the Linux-facing headers instead of
  remaining header-only stubs
- `mutex_trylock()` is available to transport-facing code that needs bounded serialization without
  pretending the full Linux scheduler model exists
- the current `redbear-iwlwifi` C transport shim now includes the LinuxKPI headers directly and
  uses Linux-style firmware, timer, mutex, and IRQ helper entry points for prepare/probe/init/
  activate steps

This remains a bounded transport-layer port. It does **not** change the rule that cfg80211/
mac80211/nl80211 remain out of scope for the current milestone.

### Current validation status for this bounded LinuxKPI slice

The current validation story for this slice is intentionally narrow and should be described that
way:

- the `linux-kpi` host-side test suite now runs cleanly in this repo, including the Wi‑Fi-facing
  helper changes in this slice: `request_firmware_direct`, `request_firmware_nowait`,
  `mutex_trylock`, IRQ-depth tracking, variable private-allocation lifetime tracking, station-mode
  scan/connect/disconnect lifecycle assertions, workqueue-backed `ieee80211_queue_work()`, the new
  `sk_buff` headroom/tailroom helpers, and the existing memory tests
- `redbear-iwlwifi` host-side tests now smoke-test the bounded firmware/transport/activation/scan/
  retry actions used by the current Intel path
- `redbear-iwlwifi` also now has a binary-level host-side CLI smoke test for the current bounded
  Intel path against temporary PCI/firmware fixtures; this is not the same as a chained real-target
  transport→activation→association proof
- `redbear-wifictl` host-side tests pass for the bounded control-plane state propagation above that
  Intel path
- the packaged target-side Wi-Fi validators now also accept bounded `status=associating`/
  pending-connect output, so the in-target/runtime checks stay aligned with the current honest
  connect semantics instead of requiring a fake associated/connected result
- the default packaged bounded runtime profile is now `wifi-open-bounded`, separating lifecycle
  validation from the later DHCP-on-real-association gate

This does **not** mean Red Bear has validated a full Linux Wi‑Fi driver stack. The validated claim
is narrower: this repo now has tested, bounded LinuxKPI support for the current Intel transport-
facing helper slice, plus host-tested bounded CLI/control flows above it. Current bounded connect
results should still be read as pending/experimental lifecycle state, not proof of real AP
association.

In the current host environment used for this hardening pass, the Intel-specific VFIO runtime path
also remains blocked by prerequisites outside the repo changes themselves: the host validator sees a
MediaTek MT7921K (`14c3:0608`) instead of an Intel `iwlwifi` device on the available Wi‑Fi slot,
and `vfio_pci` is not loaded. That means the repo-side bounded runtime harness is present and the
Red Bear image/QEMU/OVMF/`expect` prerequisites are available, but a literal Intel passthrough run
still requires compatible host hardware and VFIO binding before it can be executed.

That is the current feasibility conclusion grounded in the codebase.

## Hardware Strategy

### Target hardware scope

The target scope for this plan is now:

- **Intel Wi-Fi chips used on Arrow Lake and older Intel client platforms**

That includes the practical `iwlwifi` family boundary, not an abstract FullMAC-first family chosen
for architectural neatness.

### What this means for phase 1

Phase 1 is no longer “pick any convenient Wi-Fi family.”

Phase 1 is now:

- prove one bounded Intel client Wi-Fi path,
- keep the support language experimental,
- and avoid promising the entire Linux wireless stack up front.

## Security Scope Freeze

### Phase-1 supported security

- open networks
- WPA2-PSK

### Explicitly out of initial scope

- WPA3
- 802.1X / enterprise Wi-Fi
- AP mode
- roaming
- monitor mode
- suspend/resume guarantees
- multi-BSS support
- sophisticated regulatory-domain handling

This scope freeze is required to keep the first milestone honest and achievable.

## Comprehensive Full Plan

## Current Implementation Progress

### Already landed in-tree

The current repo now contains a **bounded Phase W0/W2/W3 slice**:

- the plan target is explicitly Intel Arrow Lake and older Intel Wi-Fi chips
- `redbear-netctl` now supports Wi‑Fi profiles with `Connection=wifi`, `Interface=...`, `SSID`,
  `Security`, and `Key` / `Passphrase`
- `netctl` now performs a bounded `prepare` → `init-transport` → `connect` handoff into
  `/scheme/wifictl`
- that user-facing path now also includes a bounded `activate-nic` step before `connect`
- `netctl scan <profile|iface>` now uses the same `prepare` → `init-transport` ordering before the
  active `scan` action
- `netcfg` no longer hard-codes a single `eth0` interface node and can expose interfaces from the
  current device list dynamically
- `redbear-wifictl` now exists as a real package/daemon/scheme with:
  - a stub backend for end-to-end control-plane validation
  - an Intel-oriented backend boundary for Arrow Lake-and-lower families
  - firmware-family and firmware-presence reporting
  - a bounded `prepare` step before `connect`
  - transport-readiness reporting derived from PCI command/BAR/IRQ state
  - a bounded PCI transport-prep action that enables memory-space and bus-master bits before connect
- a bounded `scan` action with a working stub path and a bounded Intel scan/reporting path rather
  than the older explicit `not implemented yet` result
  - a bounded `init-transport` state boundary after preparation and before any future association path
  - a bounded `activate-nic` state boundary after `init-transport`
  - state-machine enforcement so Intel scan/connect refuse to proceed before `init-transport`
- `redbear-info` and the runtime helper scripts now expose the Wi‑Fi control-plane surfaces
- `redbear-info` now reports Wi‑Fi firmware status, transport status, activation status, and scan results from the
  primary Wi‑Fi control interface
- `redbear-info` and the runtime helper also now expose `transport-init-status`, which separates
  simple transport probing from an actual transport-initialization attempt
- on Redox runtime builds where `/usr/lib/drivers/redbear-iwlwifi` is present **and** at least one
  Intel Wi-Fi candidate is actually detectable, `redbear-wifictl` now auto-selects the Intel backend
  instead of silently falling back to the stub backend
- if the Intel driver package is present but no Intel Wi-Fi candidate is detected, `redbear-wifictl`
  now exposes a dedicated no-device fallback rather than a synthetic stub `wlan0`, so the runtime
  does not pretend the Intel path is usable

### What this means

This does **not** mean Red Bear has working Intel Wi‑Fi connectivity yet.

It means the repo now has:

- a real Wi‑Fi profile model,
- a real Wi‑Fi control-plane daemon and scheme,
- a first dedicated Intel Wi‑Fi driver-side package (`redbear-iwlwifi`),
- a runtime helper for the bounded Intel driver probe path (`local/scripts/test-iwlwifi-driver-runtime.sh`),
- a runtime check that the Wi‑Fi control daemon selects the Intel backend only when Intel Wi‑Fi
  candidates are actually present,
- a native post-association IP handoff path that can address non-`eth0` interfaces,
- and a firmware-aware, transport-aware Intel backend boundary.
- and a bounded active scan surface.
- and a bounded transport-initialization surface.

The current bounded implementation is therefore no longer just static plumbing. It now has a real
user-facing Wi‑Fi orchestration flow through `netctl`, a real control daemon state machine, and a
real Intel-targeted firmware/transport preparation boundary.

That is the first substantial Wi‑Fi bring-up slice, but not the final result.

### Still missing after the current slice

- real Intel transport initialization
- actual firmware loading/prepare action on Redox target hardware
- scan implementation against real hardware
- authentication and association
- WPA2 key negotiation on a real link
- DHCP/static IP handoff on a real associated wireless interface
- runtime validation on Intel hardware or a realistic guest path

### Phase W0 — Scope Freeze and Package-Group Definition

**Goal**: Define the first Wi-Fi milestone precisely before implementation starts.

**What to do**:

- freeze the target scope to Intel Arrow Lake and older Intel Wi-Fi chips
- freeze security scope to open + WPA2-PSK
- define `net-wifi-experimental` as the package/config slice for first Wi-Fi support
- document unsupported wireless features explicitly

**Exit criteria**:

- Intel target scope is explicit
- support language and non-goals are written down
- the repo has a standalone tracked Wi-Fi experimental profile (`config/redbear-wifi-experimental.toml`) extending the minimal Red Bear baseline

---

### Phase W1 — Intel Driver Substrate Fit

**Goal**: Prove the Intel target family can fit Red Bear’s existing driver primitives and identify
the minimum additional compatibility surface required.

**What to do**:

- map the Intel target family onto `redox-driver-sys`
- verify firmware naming and fetch path through `firmware-loader`
- identify exactly which `linux-kpi` additions are mandatory for Intel transport/firmware bring-up
- keep those additions below the wireless control-plane boundary

**Exit criteria**:

- one Intel target device can be discovered, initialized, and paired with its firmware-loading path

---

### Phase W2 — Native Wireless Control Plane

**Goal**: Add a Red Bear-native wireless daemon and control interface.

**What to do**:

- implement a Wi-Fi daemon that owns:
  - scan state
  - auth/association state
  - link state
  - disconnect/retry behavior
  - credential ownership
- add a user-facing `wifictl`-style control surface

**What not to do**:

- do not push supplicant logic into `redbear-netctl`
- do not model Wi-Fi as “just another Ethernet profile” at this phase

**Exit criteria**:

- the daemon can report scan results and current link state honestly

---

### Phase W3 — Network Stack Refactor for Post-Association Handoff

**Goal**: Make the native IP stack accept Wi-Fi as a first-class post-association interface.

**What to do**:

- generalize current `eth0` / Ethernet assumptions where needed
- allow the native stack to consume a post-association Wi-Fi interface state
- keep route/address/DNS handling in native `netcfg` / `smolnetd` plumbing after association

**Exit criteria**:

- a connected Wi-Fi link can be handed off to the existing IP path without pretending it is merely a
  raw Ethernet control-plane object

---

### Phase W4 — First Association Milestone

**Goal**: Achieve one real Wi-Fi connection under the frozen phase-1 scope.

**What to do**:

- scan for one real SSID
- join one test network
- complete open or WPA2-PSK association
- hand off to DHCP or static IP configuration

**Exit criteria**:

- one chosen device family reaches usable network connectivity on a real network

---

### Phase W5 — Runtime Reporting and Recovery

**Goal**: Make Wi-Fi support diagnosable and honest.

**What to do**:

- extend `redbear-info` with Wi-Fi-specific runtime reporting
- add reconnect and failure-state reporting
- keep all support labels experimental

**Exit criteria**:

- users can see whether hardware is present, firmware is loaded, scans succeed, and association has
  succeeded or failed

---

### Phase W6 — Desktop Compatibility (Later)

**Goal**: Add desktop-oriented control only after native Wi-Fi works.

**What to do**:

- if KDE or desktop workflows require it, add a small compatibility shim over the native Wi-Fi
  service
- keep that shim above the native control plane, not in place of it

**Exit criteria**:

- desktop Wi-Fi workflows become possible without changing the native subsystem boundaries

---

### Phase W7 — Broader Hardware and `linux-kpi` Reassessment

**Goal**: Reassess whether Red Bear wants to widen Wi‑Fi support after one bounded Intel path works.

**What to do**:

- only after one bounded Intel transport/association path is validated, decide whether a wider
  multi-family or deeper `linux-kpi` path is worth the cost
- do not assume this is automatically justified

**Exit criteria**:

- Red Bear either keeps the narrow native-first architecture, or consciously chooses a larger Linux
  wireless-compat effort with full awareness of the cost

## Validation Gates

Wi-Fi should not be described as supported until these gates are passed in order:

1. hardware is detected
2. firmware loads successfully
3. the driver/daemon initializes and reports link state
4. scan sees a real SSID
5. association succeeds for one supported network type
6. DHCP or static IP handoff succeeds through the native network stack
7. reconnect works after disconnect or reboot
8. `redbear-info` and profile docs report supported and unsupported states honestly

Until then, support language should remain under `net-wifi-experimental` only.

## Support-Language Guidance

Until the validation gates above are passed, Red Bear should use language such as:

- “Wi-Fi is not supported yet”
- “Wi-Fi remains experimental and hardware-specific”
- “The current wireless path is an experimental Intel bounded-transport bring-up”

Avoid language such as:

- “Linux Wi‑Fi drivers are supported”
- “wireless support works”
- “Wi-Fi is generally available”

unless profile-scoped validation evidence exists.

## Summary

The best Red Bear Wi-Fi path is **native-first**:

- native wireless control plane
- one experimental bounded Intel family path first
- `firmware-loader` + `redox-driver-sys` underneath
- optional narrow `linux-kpi` glue only where useful
- native `smolnetd` / `netcfg` / `redbear-netctl` reused only after association

`linux-kpi` is therefore **feasible only in a narrow sense**. It is useful as a low-level helper
for driver bring-up, but it is not currently a viable full Wi‑Fi architecture for Red Bear OS.

That is the most realistic way to integrate Wi‑Fi into Red Bear while keeping the design aligned
with the repo’s current userspace-driver and profile-based architecture.
