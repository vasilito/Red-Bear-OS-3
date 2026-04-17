# Red Bear OS Wi-Fi Implementation Plan

## Purpose

This document describes the current Wi-Fi state in Red Bear OS and the path from the existing
bounded Intel bring-up scaffold to validated wireless connectivity.

Wi-Fi does not provide working connectivity yet. What exists is a structurally complete,
host-tested Intel transport layer and native control plane, awaiting real hardware + firmware
validation.

## Validation States

| State | Meaning |
|---|---|
| **builds** | Compiles in-tree |
| **host-tested** | Tests pass on Linux host with synthesized fixtures |
| **validated** | Behavior confirmed with real hardware evidence |
| **experimental** | Available for bring-up, not support-promised |
| **missing** | No in-tree implementation |

## Current State

### Status Matrix

| Area | State | Detail |
|---|---|---|
| Intel PCIe transport | **builds, host-tested** | `redbear-iwlwifi`: ~2450 lines C transport + ~1550 lines Rust CLI. Real 802.11 RX frame parsing, DMA ring management, TX reclaim, ISR/tasklet dispatch, command response parsing, mac80211 ops, station state transitions, key management. Commands time out without real firmware — by design. |
| LinuxKPI compatibility | **builds, host-tested** | `linux-kpi`: 17 Rust modules, 93 tests. cfg80211/wiphy/mac80211 registration, ieee80211_ops 12-callback dispatch, PCI MSI/MSI-X, DMA pool, sk_buff, NAPI poll, list_head, atomic_t, completion, IO barriers, BSS/channel/band/rate, scan/connect/disconnect events, BSS registry with reference release. |
| IRQ dispatch | **builds, host-tested** | `request_irq`/`free_irq`/`disable_irq`/`enable_irq` fully implemented with real `scheme:irq/{}` integration, thread-based dispatch, and mask/unmask support. |
| Test coverage | **119 tests pass** | 93 linux-kpi + 8 redbear-iwlwifi + 18 redbear-wifictl. No production `unwrap()` in Wi-Fi daemon request loop (startup uses `expect()`). Host-tested; Redox-only C transport paths are compile-tested but not directly exercised by host tests. |
| Firmware loading | **partial** | `firmware-loader` can serve blobs generically. |
| Control plane | **host-tested** | `redbear-wifictl` daemon + `/scheme/wifictl` scheme with stub and Intel backends, state-machine enforcement, firmware-family reporting. Daemon request loop has graceful shutdown on socket errors. |
| Profile orchestration | **host-tested** | `redbear-netctl` Wi-Fi profiles (SSID/Security/Key), bounded prepare→init-transport→activate-nic→connect→disconnect flow, DHCP handoff. |
| Runtime diagnostics | **host-tested** | `redbear-info` Wi-Fi surfaces, packaged validators (`redbear-phase5-wifi-check/run/capture/analyze`). |
| Real hardware validation | **missing** | No Intel Wi-Fi device has been exercised. Transport is structurally correct but functionally unproven. |
| Desktop Wi-Fi API | **missing** | No NetworkManager-like or D-Bus Wi-Fi surface. |

### Transport Quality (from hardening pass)

The iwlwifi transport has been hardened with these specific improvements:

- **Atomic command state**: `command_complete`, `last_cmd_id`, `last_cmd_cookie`, `last_cmd_status` use `__atomic_store_n`/`__atomic_load_n` with `__ATOMIC_SEQ_CST` — no torn reads between ISR and command submission.
- **Stale response sentinel** (0xFFFF): After command timeout, the response fields are poisoned. Late-arriving firmware responses and id/cookie mismatches are discarded entirely without completing the waiter — prevents stale responses from completing the wrong in-flight command.
- **Command queue space management**: `iwl_pcie_send_cmd` reclaims completed TX descriptors before submitting each command. If the command queue is still full after reclaim, the command fails immediately rather than entering the overflow queue — commands are synchronous and one-at-a-time, so overflow queuing would create ownership ambiguity.
- **DMA read barrier**: `rmb()` added after `dma_sync_single_for_cpu()` and before parsing RX frame data — ensures correct ordering on weakly-ordered architectures.
- **TX queue selection safety**: `rb_iwlwifi_choose_txq()` returns -1 when no data queue is active instead of falling back to the command queue — data frames never use the command queue.
- **TX error handling**: `iwl_ops_tx` now properly frees the skb on failure and logs warnings instead of silently swallowing errors.
- **Association BSSID guard**: BSSID from association-response frames is only copied to transport state when `trans->connecting` is set — prevents stale frames from corrupting connection state.
- **TXQ stuck detection fix**: Removed `trans->irq <= 0` from stuck detection — queue stuckness is independent of IRQ allocation state.
- **RX drain**: Parses 802.11 frame_control type/subtype before freeing — distinguishes data, management, and control frames instead of blind disposal.
- **RX restock**: Write pointer pushed to hardware in both restock and start_dma paths — prevents DMA ring starvation.
- **TX reclaim**: Full DMA unmap cycle — no leaked mappings.
- **BSS registry cleanup**: `cfg80211_put_bss()` now removes entries from the BSS registry and cleans up associated IEs — no memory leak on repeated scans.

### LinuxKPI Compat Layer Improvements

The linux-kpi compatibility layer has been enhanced with real frame delivery and statistics:

- **RX callback mechanism**: `ieee80211_register_rx_handler(hw, callback)` registers a per-hw
  callback that receives drained RX frames. When `ieee80211_rx_drain` processes queued frames,
  it delivers them to the registered callback instead of logging and freeing. This allows the
  upper layer (e.g., a Redox wireless daemon) to consume frames in real time.
- **TX statistics tracking**: `ieee80211_get_tx_stats(hw)` returns per-hw TX completion counters
  (total, acked, nacked). `ieee80211_tx_status` increments these on every TX completion.
- **Full frame data in cfg80211 events**: `cfg80211_rx_mgmt` now stores complete frame data (not
  just metadata) in the wireless event state, enabling later consumption by the native wireless
  stack. `cfg80211_mgmt_tx_status` similarly stores full TX frame data.
- **IRQ dispatch confirmed real**: `request_irq`/`free_irq`/`disable_irq`/`enable_irq` use real
  `scheme:irq/{}` integration with thread-based dispatch and mask/unmask support — not stubs.
- **119 tests pass**: 93 linux-kpi + 8 redbear-iwlwifi + 18 redbear-wifictl.

### Honest Assessment

Without real hardware + firmware:
- Command submission times out (no firmware alive response)
- Scan returns no results (no firmware scan response)
- Association does not complete
- RX frames are never processed

The code reports these states honestly (timeout, no results) rather than fabricating success.
Hardware runtime validation is the required next gate.

## Architecture

### Subsystem Boundaries

```
User-facing
  redbear-netctl (profiles, CLI)
  redbear-netctl-console (ncurses TUI)
       │
       ▼
  /scheme/wifictl (redbear-wifictl daemon)
       │  scan / auth / association / link state / credentials
       ▼
  redbear-iwlwifi (driver daemon)
       │  PCIe transport / firmware / DMA / IRQ
       ▼
  linux-kpi (compatibility glue)
       │  PCI / MMIO / IRQ / DMA / sk_buff / mac80211 ops
       ▼
  redox-driver-sys (scheme:memory, scheme:irq, scheme:pci)
       │
  firmware-loader (scheme:firmware)
       │
Kernel: scheme-based primitives only

Post-association IP path:
  smolnetd → netcfg → dhcpd → redbear-netctl
```

### Key Design Decisions

1. **Native control plane above the driver** — `redbear-wifictl` owns scan/auth/association, not `redbear-netctl`.
2. **Bounded Intel transport port below that boundary** — reuse Linux-facing firmware/PCI/MMIO logic where it lowers cost.
3. **No full Linux wireless stack port** — cfg80211/mac80211/nl80211 are out of scope for the first milestone.
4. **`redbear-netctl` is the profile manager, not the supplicant** — it hands off to `/scheme/wifictl`, which hands off to the driver.

### Port vs Rewrite

The chosen approach is a **bounded transport-layer port with native control-plane rewrite above it**:
- Port and reuse transport-layer and firmware-facing logic from Linux `iwlwifi`
- Keep the native Red Bear control plane above that boundary
- Do not import the whole Linux wireless stack in one step

## Hardware Strategy

- **Target**: Intel Wi-Fi chips on Arrow Lake and older Intel client platforms
- **Driver family**: `iwlwifi`-class (7000/8000/9000/AX210/BZ)
- **Security scope**: Open networks + WPA2-PSK only (phase 1)
- **Out of scope**: WPA3, 802.1X, AP mode, roaming, monitor mode, suspend/resume, multi-BSS

## Implementation Phases

### Phase W0 — Scope Freeze ✅ Complete

- Intel target scope frozen
- Security scope frozen (open + WPA2-PSK)
- `redbear-wifi-experimental` config slice defined (`config/redbear-wifi-experimental.toml`)
- Unsupported features documented

### Phase W1 — Intel Driver Substrate Fit ✅ Complete (build-side)

- Intel device family mapped onto `redox-driver-sys` primitives
- Firmware naming/fetch path wired through `firmware-loader`
- Minimum `linux-kpi` additions identified and implemented (93 tests)
- All additions stay below the wireless control-plane boundary

**Exit criteria met (build-side)**: Intel target device can be discovered, initialized, and paired
with its firmware-loading path — in compiled/host-tested code. Real hardware validation still pending.

### Phase W2 — Native Wireless Control Plane ✅ Complete (host-tested)

- `redbear-wifictl` daemon with `/scheme/wifictl` scheme
- Stub backend for end-to-end control-plane validation
- Intel backend: device detection, firmware-family reporting, transport-readiness, state machine
- `redbear-netctl` Wi-Fi profile support (SSID/Security/Key)
- Bounded prepare→init-transport→activate-nic→scan→connect→disconnect flow
- `redbear-netctl-console` ncurses TUI client

**Exit criteria met (host-tested)**: Daemon reports scan results and link state honestly in
host-side tests. Runtime validation pending.

### Phase W3 — Network Stack for Post-Association Handoff ✅ Complete (build-side)

- `netcfg` exposes per-device interface nodes dynamically (not hard-coded `eth0`)
- `redbear-netctl` performs DHCP handoff for Wi-Fi profiles
- Native IP plumbing can consume a post-association Wi-Fi interface

**Exit criteria met (build-side)**: A connected Wi-Fi link can be handed off to the existing IP
path without treating it as raw Ethernet. Runtime validation pending.

### Phase W4 — First Association Milestone 🚧 Not started (blocked on hardware)

**Goal**: One real Wi-Fi connection under phase-1 scope.

**What to do**:
1. Obtain an Intel Wi-Fi device (iwlwifi-class) for bare-metal or VFIO passthrough testing
2. Boot Red Bear on hardware with the Intel Wi-Fi PCI function visible
3. Verify firmware loads via `firmware-loader`
4. Verify transport init succeeds (command queue alive, firmware responds)
5. Scan for one real SSID
6. Join one test network (open or WPA2-PSK)
7. Hand off to DHCP or static IP
8. Confirm bidirectional connectivity

**Exit criteria**: One Intel device family reaches usable network connectivity on a real network.

**Prerequisites**:
- Intel Wi-Fi PCI device available for testing
- `low-level controller` / IRQ quality validated (current blocker chain)
- Firmware blobs for the target device family

### Phase W5 — Runtime Reporting and Recovery (After W4)

> **Status note:** This Phase **W5** is not the same as the bounded `redbear-phase5-network-check`
> QEMU plumbing proof on `redbear-full`. W5 here remains a later real-hardware reporting/recovery
> milestone.

- Extend `redbear-info` with real Wi-Fi runtime evidence (not just bounded surfaces)
- Reconnect after disconnect
- Failure-state reporting and retry
- `redbear-phase5-wifi-check/run/capture/analyze` validated against real hardware

**Exit criteria**: Users can see whether hardware is present, firmware is loaded, scans succeed,
and association has succeeded or failed — backed by real hardware evidence.

### Phase W6 — Desktop Compatibility (Later)

- If KDE or desktop workflows require it, add a compatibility shim over the native Wi-Fi service
- Keep the shim above the native control plane, not in place of it

### Phase W7 — Broader Hardware Reassessment (Later)

- After one bounded Intel path is validated, reassess whether wider multi-family or deeper
  `linux-kpi` growth is justified
- Do not assume this is automatically warranted

## Validation Gates

Wi-Fi should not be described as supported until these gates pass in order:

1. ✅ Hardware detected via PCI scheme
2. 🚧 Firmware loads successfully
3. 🚧 Driver/daemon initializes and reports link state
4. 🚧 Scan sees a real SSID
5. 🚧 Association succeeds for one supported network type
6. 🚧 DHCP or static IP handoff succeeds
7. 🚧 Reconnect works after disconnect or reboot
8. 🚧 `redbear-info` reports all states honestly with real evidence

Until all gates pass, support language stays under `redbear-wifi-experimental`.

## Current Blockers

1. **No Intel Wi-Fi hardware available for testing** — the current host has a MediaTek MT7921K
   (`14c3:0608`), not an Intel `iwlwifi` device
2. **Low-level controller / IRQ quality** — must be validated before driver bring-up is reliable
3. **VFIO not loaded on current host** — passthrough path requires `vfio_pci` module and compatible IOMMU groups

## Scripts and Validation Tools

| Script | Purpose |
|---|---|
| `test-iwlwifi-driver-runtime.sh` | Bounded Intel driver lifecycle check in target runtime |
| `test-wifi-control-runtime.sh` | Bounded Wi-Fi control/profile runtime check |
| `test-wifi-baremetal-runtime.sh` | Strongest in-repo Wi-Fi runtime check on real Red Bear target |
| `test-wifi-passthrough-qemu.sh` | QEMU/VFIO Wi-Fi validation with in-guest checks |
| `validate-wifi-vfio-host.sh` | Host-side VFIO passthrough readiness check |
| `prepare-wifi-vfio.sh` | Bind/unbind Intel Wi-Fi PCI function for VFIO |
| `run-wifi-passthrough-validation.sh` | One-shot host wrapper for full passthrough validation |
| `package-wifi-validation-artifacts.sh` | Package validation artifacts into host-side tarball |
| `summarize-wifi-validation-artifacts.sh` | Summarize captured artifacts for quick triage |
| `finalize-wifi-validation-run.sh` | Analyze capture bundle and package final evidence set |

Packaged validators (inside target runtime):
- `redbear-phase5-wifi-check` — bounded in-target Wi-Fi validation
- `redbear-phase5-wifi-run` — run bounded Wi-Fi lifecycle
- `redbear-phase5-wifi-capture` — capture runtime evidence bundle
- `redbear-phase5-wifi-analyze` — analyze captured evidence
- `redbear-phase5-wifi-link-check` — link-level validation

## Related Documents

- `local/docs/WIFI-VALIDATION-RUNBOOK.md` — canonical operator runbook for bare-metal and VFIO validation
- `local/docs/WIFI-VALIDATION-ISSUE-TEMPLATE.md` — issue template for validation failures
- `local/docs/WIFICTL-SCHEME-REFERENCE.md` — `/scheme/wifictl` protocol reference
- `docs/04-LINUX-DRIVER-COMPAT.md` — linux-kpi and redox-driver-sys architecture

## Summary

The best Red Bear Wi-Fi path is **native-first**:

- Native wireless control plane (`redbear-wifictl` + `redbear-netctl`)
- One experimental Intel family path first (`redbear-iwlwifi`)
- `firmware-loader` + `redox-driver-sys` underneath
- Narrow `linux-kpi` glue only where useful (93 tests, 17 modules)
- Native `smolnetd` / `netcfg` / `dhcpd` reused after association

The codebase has 119 tests passing (93 linux-kpi + 8 redbear-iwlwifi + 18 redbear-wifictl), no production `unwrap()` in the Wi-Fi daemon request loop (startup uses `expect()`), atomic command
handling, proper timer cancellation, honest timeout reporting, and real 802.11 frame parsing.
The structural skeleton is solid. The next required step is **real hardware validation** with an
Intel Wi-Fi device — everything else is gated on that.
