# Red Bear OS Wi-Fi Implementation Plan

## Purpose

This document defines the current Wi-Fi state in Red Bear OS and lays out the recommended path for
integrating Wi-Fi drivers and a usable wireless control plane.

The goal is not to imply that Wi-Fi already exists in-tree. The goal is to describe what the repo
currently proves, what `linux-kpi` can and cannot realistically provide, and how Red Bear can grow
from **no Wi-Fi support** to one experimental, validated Wi-Fi path that fits the existing Redox /
Red Bear architecture.

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

Wi-Fi is currently **missing** in Red Bear OS.

There is no in-tree Wi-Fi driver, no wireless daemon, no cfg80211/mac80211/nl80211-compatible
surface, no supplicant path, and no profile that can honestly claim Wi-Fi support.

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
| Wi-Fi controller support | **missing** | No PCIe/USB/SDIO Wi-Fi driver recipes in-tree |
| Linux wireless stack compatibility | **missing** | No cfg80211/mac80211/nl80211/wiphy support in `linux-kpi` |
| Firmware loading | **partial prerequisite exists** | `firmware-loader` can serve firmware blobs generically |
| Wireless control plane | **missing** | No scan/auth/association/link-state daemon or CLI |
| Post-association IP path | **present** | Native `smolnetd` / `netcfg` / `dhcpd` / `redbear-netctl` path exists |
| Desktop Wi-Fi API | **missing** | No NetworkManager-like or D-Bus Wi-Fi surface |
| Runtime diagnostics | **partial prerequisite exists** | `redbear-info` model exists, but no Wi-Fi integration exists |

## Evidence Already In Tree

### Direct negative evidence

- `HARDWARE.md` says Wi-Fi and Bluetooth are not supported yet
- `local/docs/AMD-FIRST-INTEGRATION.md` marks `Wi-Fi/BT` as missing

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

Current `linux-kpi` is **not** a complete Wi-Fi architecture because the repo has no in-tree:

- cfg80211
- mac80211
- nl80211
- wiphy model
- supplicant/control-plane compatibility layer

So `linux-kpi` is feasible only as a **partial low-level aid**, not as the primary Red Bear Wi-Fi
stack.

### 3. The current Red Bear control plane is Ethernet-specific

The current native network stack is useful, but not yet Wi-Fi-ready.

`redbear-netctl` only supports:

- `Connection=ethernet`
- `Interface=eth0`
- DHCP/static address, route, and DNS control

`netcfg` is similarly hard-wired around the current `eth0` interface model.

That means Red Bear can reuse its native IP plumbing **after association**, but not as the radio
control plane itself.

### 4. FullMAC is a better first target than SoftMAC

The first Wi-Fi target should minimize the amount of 802.11 MAC and Linux wireless subsystem logic
that Red Bear has to recreate.

That makes **FullMAC** hardware the best first target class.

Red Bear should explicitly avoid starting with SoftMAC/mac80211-style Linux drivers such as:

- Intel `iwlwifi`
- Realtek `rtw88` / `rtw89`
- MediaTek `mt76`
- other drivers that fundamentally assume cfg80211/mac80211 semantics

## Recommended Architecture

The best Red Bear Wi-Fi architecture is:

1. **native Red Bear wireless control plane**
2. **one experimental FullMAC driver family first**
3. **reuse `redox-driver-sys` + `firmware-loader` directly**
4. **use `linux-kpi` only where it reduces low-level glue cost**
5. **reuse the existing native IP path only after association**

This is a hybrid architecture, but it is **native-first**, not Linux-stack-first.

### Subsystem boundary

The Wi-Fi subsystem should be split into these pieces:

- one **device transport / driver daemon** for the chosen chipset family
- one **firmware loading path** via `firmware-loader`
- one **Wi-Fi control daemon** for scan/auth/association/link state
- one **user-facing control tool** (`wifictl` or equivalent)
- one **post-association handoff** into `smolnetd` / `netcfg` / `dhcpd`
- one **later desktop shim** only if KDE/user-facing workflows require it

`redbear-netctl` should **not** become the supplicant. It can remain the post-association IP
profile tool, or be generalized later, but it should not own scan/auth/association itself.

## Hardware Strategy

### First decision gate

Before implementation begins, Red Bear must choose **one** first Wi-Fi family from actual target
machines or bring-up hardware.

The preferred target order is:

1. **PCIe FullMAC** — if a real Red Bear target machine in the hardware matrix has one
2. **USB FullMAC** — if PCIe FullMAC hardware is not available, use this as the first prototype path

### What not to choose for phase 1

Do not start with:

- Intel laptop Wi-Fi via `iwlwifi`
- mac80211/cfg80211-dependent Linux drivers
- any phase-1 scope that requires recreating a Linux wireless stack first

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

### Phase W0 — Scope Freeze and Package-Group Definition

**Goal**: Define the first Wi-Fi milestone precisely before implementation starts.

**What to do**:

- choose one target FullMAC family from actual hardware
- freeze security scope to open + WPA2-PSK
- define `net-wifi-experimental` as the package-group slice for first Wi-Fi support
- document unsupported wireless features explicitly

**Exit criteria**:

- one hardware family is selected
- support language and non-goals are written down

---

### Phase W1 — Driver Substrate Fit

**Goal**: Prove the chosen Wi-Fi family can fit Red Bear’s existing driver primitives.

**What to do**:

- map the chosen device family onto `redox-driver-sys`
- verify firmware naming and fetch path through `firmware-loader`
- decide whether any narrow `linux-kpi` glue is useful for that family
- keep `linux-kpi` below the wireless control-plane boundary

**Exit criteria**:

- one chosen device can be discovered, initialized, and paired with its firmware-loading path

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

**Goal**: Reassess whether Red Bear wants to widen Wi‑Fi support after one FullMAC path works.

**What to do**:

- only after one FullMAC family is validated, decide whether a wider SoftMAC / deeper `linux-kpi`
  path is worth the cost
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
- “The current wireless path is an experimental FullMAC-first bring-up”

Avoid language such as:

- “Linux Wi‑Fi drivers are supported”
- “wireless support works”
- “Wi-Fi is generally available”

unless profile-scoped validation evidence exists.

## Summary

The best Red Bear Wi-Fi path is **native-first**:

- native wireless control plane
- one experimental FullMAC family first
- `firmware-loader` + `redox-driver-sys` underneath
- optional narrow `linux-kpi` glue only where useful
- native `smolnetd` / `netcfg` / `redbear-netctl` reused only after association

`linux-kpi` is therefore **feasible only in a narrow sense**. It is useful as a low-level helper
for driver bring-up, but it is not currently a viable full Wi‑Fi architecture for Red Bear OS.

That is the most realistic way to integrate Wi‑Fi into Red Bear while keeping the design aligned
with the repo’s current userspace-driver and profile-based architecture.
