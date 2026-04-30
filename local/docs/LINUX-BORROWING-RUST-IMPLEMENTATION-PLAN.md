# Linux Borrowing and Rust Implementation Plan for Red Bear OS

**Date:** 2026-04-18
**Status:** Planning authority for Linux-derived borrowing boundaries and Rust rewrite guidance across low-level subsystem work. PCI/IRQ rollout authority lives in `local/docs/IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md`.
**Scope:** Hardware enablement, ACPI including suspend/resume, low-level startup/init, PCI, IRQ/MSI/MSI-X, PS/2 init, IOMMU, USB/xHCI/storage, bounded Wi-Fi transport reuse, and selective GPU/DRM orchestration reuse

## Intent

Which Linux kernel source and Linux documentation already present in this repo should be used as donor material for Red Bear OS, what should be rewritten into Rust, what should remain reference-only, and where should that logic live in Red Bear's architecture?

This plan is intentionally **Red Bear-native**. It does **not** propose importing Linux subsystem architecture into Red Bear.

## Current implementation status snapshot (2026-04-18)

The software-only, bounded slices from this plan that are now implemented in code are:

- **Phase A — PCI / IRQ substrate**
  - bounded shared substrate slices landed in code (`redox-driver-sys` capability-chain parsing,
    interrupt-support summary, and early `pcid` convergence)
  - the **canonical execution order, current robustness judgment, and remaining implementation work**
    for PCI/IRQ now live in `local/docs/IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md`
- **Phase B — ACPI / IOMMU groundwork**
  - `acpid` now carries an explicit userspace sleep-target model naming `S1` / `S3` / `S4` / `S5`
  - only `_S5` soft-off is materially wired today; non-`S5` targets remain groundwork-only
  - `iommu` now detects kernel ACPI `DMAR` presence as a convergence seam, but Intel VT-d runtime ownership is not yet cleanly closed outside `acpid`
- **Phase C — PS/2 / USB / storage**
  - `ps2d` now flushes stale controller output during probe and around core init/self-test
  - `xhcid` now tracks active alternate settings and resolves endpoint descriptors through that map
  - `usbscsid` now has a bounded `SYNCHRONIZE CACHE(10/16)` heuristic behind `needs_sync_cache`
- **Phase D — Wi-Fi / DRM shared-core**
  - `redbear-wifictl` transport probing now uses the shared PCI parser and interrupt-support summary
  - `redox-drm` now exposes queued shared hotplug/vblank events through a real scheme `EVENT_READ` surface

The work that still remains is the larger **vendor/backend maturation and hardware-validation** side:

- full ACPI sleep/resume implementation beyond groundwork
- full Intel VT-d runtime support beyond DMAR ownership discovery
- deeper PCI / `pcid` convergence on shared helpers
- broader PS/2 resume/wake policy
- broader USB architecture/runtime maturation beyond the bounded helper slices already implemented
- deeper Wi-Fi transport/helper extraction beyond probing
- Intel and AMD DRM backend maturation and real hardware validation

This document should therefore be read as:

- **implemented now** for the bounded shared-core and software-only slices listed above
- **still in progress** for backend maturation and hardware-backed acceptance phases

## Hard rules

1. **Linux suspend/resume is reference-only.** Red Bear should study Linux ordering and edge cases, but implement its own suspend/resume support in the Red Bear architecture.
2. **`linux-kpi` is GPU and Wi-Fi only.** It is not a general solution for ACPI, USB, input, startup, or general platform ownership.
3. **Do not copy Linux subsystem structure blindly.** Use Linux as an algorithm, quirk, parser, and sequencing donor; implement the resulting behavior in Red Bear’s own kernel/scheme/userspace-daemon model.
4. **Keep Red Bear ownership boundaries intact.** Kernel remains minimal; runtime/controller policy stays in userspace daemons; reusable low-level helpers converge into shared Rust crates.
5. **Respect provenance and license constraints.** Treat Linux driver code as reference/reverse-engineering input unless a bounded donor island already exists in-tree. Prefer datasheets when available.

## Repo-grounded evidence base

### Actual Linux-derived material in this repo

- Imported AMDGPU/DC tree: `local/recipes/gpu/amdgpu-source/`
- Bounded Intel Wi-Fi transport donor: `local/recipes/drivers/redbear-iwlwifi/source/src/linux_port.c`
- Linux compatibility layer for bounded donor ports: `local/recipes/drivers/linux-kpi/`
- Red Bear-native low-level substrate: `local/recipes/drivers/redox-driver-sys/`
- Linux-mined quirk system and tables:
  - `local/recipes/drivers/redox-driver-sys/source/src/quirks/*`
  - `local/docs/QUIRKS-SYSTEM.md`
- Linux source cache used as donor/reference material:
  - `build/linux-kernel-cache/linux-7.0/`

### Linux source files directly relevant to this plan

- ACPI sleep and PM ordering:
  - `build/linux-kernel-cache/linux-7.0/drivers/acpi/sleep.c`
- PS/2 / i8042:
  - `build/linux-kernel-cache/linux-7.0/drivers/input/serio/i8042.c`
- PCI / quirks / MSI:
  - `build/linux-kernel-cache/linux-7.0/drivers/pci/probe.c`
  - `build/linux-kernel-cache/linux-7.0/drivers/pci/quirks.c`
  - `build/linux-kernel-cache/linux-7.0/drivers/pci/msi/msi.c`
- USB / xHCI / hub:
  - `build/linux-kernel-cache/linux-7.0/drivers/usb/host/xhci-pci.c`
  - `build/linux-kernel-cache/linux-7.0/drivers/usb/core/hub.c`
- Storage heuristics:
  - `build/linux-kernel-cache/linux-7.0/drivers/scsi/sd.c`
- IOMMU:
  - `build/linux-kernel-cache/linux-7.0/drivers/iommu/amd/init.c`
  - `build/linux-kernel-cache/linux-7.0/drivers/iommu/intel/iommu.c`
- GPU / DRM:
  - `build/linux-kernel-cache/linux-7.0/drivers/gpu/drm/amd/amdgpu/amdgpu_device.c`
  - `build/linux-kernel-cache/linux-7.0/drivers/gpu/drm/amd/display/amdgpu_dm/amdgpu_dm.c`
- Intel Wi-Fi transport:
  - `build/linux-kernel-cache/linux-7.0/drivers/net/wireless/intel/iwlwifi/pcie/gen1_2/trans.c`

### Linux documentation directly relevant to this plan

- Power / suspend:
  - `build/linux-kernel-cache/linux-7.0/Documentation/power/suspend-and-interrupts.rst`
  - `build/linux-kernel-cache/linux-7.0/Documentation/power/s2ram.rst`
- PCI / interrupts:
  - `build/linux-kernel-cache/linux-7.0/Documentation/PCI/msi-howto.rst`
  - `build/linux-kernel-cache/linux-7.0/Documentation/PCI/boot-interrupts.rst`
- Input / PS/2 context:
  - `build/linux-kernel-cache/linux-7.0/Documentation/input/input-programming.rst`
- ACPI:
  - `build/linux-kernel-cache/linux-7.0/Documentation/driver-api/acpi/acpi-drivers.rst`
  - `build/linux-kernel-cache/linux-7.0/Documentation/driver-api/acpi/scan_handlers.rst`
- USB:
  - `build/linux-kernel-cache/linux-7.0/Documentation/driver-api/usb/writing_usb_driver.rst`
  - `build/linux-kernel-cache/linux-7.0/Documentation/usb/mass-storage.rst`
- GPU / DRM:
  - `build/linux-kernel-cache/linux-7.0/Documentation/gpu/drm-kms.rst`
  - `build/linux-kernel-cache/linux-7.0/Documentation/gpu/drm-uapi.rst`
  - `build/linux-kernel-cache/linux-7.0/Documentation/gpu/drm-internals.rst`
- Wi-Fi:
  - `build/linux-kernel-cache/linux-7.0/Documentation/driver-api/80211/introduction.rst`
  - `build/linux-kernel-cache/linux-7.0/Documentation/driver-api/80211/cfg80211.rst`
  - `build/linux-kernel-cache/linux-7.0/Documentation/driver-api/80211/mac80211.rst`
  - `build/linux-kernel-cache/linux-7.0/Documentation/driver-api/80211/mac80211-advanced.rst`
  - `build/linux-kernel-cache/linux-7.0/Documentation/networking/napi.rst`

### Red Bear current-state and planning sources used

- `docs/04-LINUX-DRIVER-COMPAT.md`
- `local/docs/ACPI-IMPROVEMENT-PLAN.md`
- `local/docs/IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md`
- `local/docs/USB-IMPLEMENTATION-PLAN.md`
- `local/docs/USB-VALIDATION-RUNBOOK.md`
- `local/docs/WIFI-IMPLEMENTATION-PLAN.md`
- `local/docs/QUIRKS-SYSTEM.md`
- `local/docs/IOMMU-SPEC-REFERENCE.md`
- `local/docs/DBUS-INTEGRATION-PLAN.md`
- `local/docs/DRM-MODERNIZATION-EXECUTION-PLAN.md`
- `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md`

---

# Part 1 — Comprehensive assessment

## 1. Red Bear ownership model that must be preserved

### Kernel-owned minimum platform baseline

Grounded in:
- `recipes/core/kernel/source/src/startup/mod.rs`
- `recipes/core/kernel/source/src/startup/memory.rs`
- `recipes/core/kernel/source/src/acpi/mod.rs`
- `recipes/core/kernel/source/src/scheme/serio.rs`

Kernel should keep only:
- boot memory/bootstrap
- early ACPI table discovery
- MADT / HPET / APIC / IRQ baseline
- race-critical `serio` byte queueing

### Userspace runtime/controller ownership

Grounded in:
- `recipes/core/base/source/drivers/hwd/src/main.rs`
- `recipes/core/base/source/drivers/hwd/src/backend/acpi.rs`
- `recipes/core/base/source/drivers/acpid/src/acpi.rs`
- `recipes/core/base/source/drivers/pcid/src/main.rs`
- `recipes/core/base/source/drivers/input/ps2d/src/controller.rs`
- `local/recipes/system/iommu/source/src/main.rs`
- `recipes/core/base/source/drivers/usb/xhcid/src/main.rs`

Userspace owns:
- ACPI runtime and AML policy (`acpid`)
- PCI enumeration and driver-facing interrupt policy (`pcid`)
- IOMMU runtime ownership (`iommu`)
- PS/2 controller state machine (`ps2d`)
- USB controller/runtime policy (`xhcid`, related daemons)
- session-facing power signals (`redbear-sessiond`)
- Wi-Fi control/runtime policy (`redbear-wifictl`, `redbear-netctl`)

### Shared Rust substrate

Grounded in:
- `local/recipes/drivers/redox-driver-sys/source/src/{pci,irq,dma}.rs`
- `local/recipes/drivers/redox-driver-sys/source/src/quirks/*`

Shared Rust should own:
- reusable PCI helpers
- MSI/MSI-X helpers
- DMA helpers
- quirk lookups
- future IVRS/DMAR helper modules

## 2. Actual startup / init ordering

### Strict chain

Grounded in:
- `recipes/core/base/source/drivers/hwd/src/backend/acpi.rs`
- `recipes/core/base/source/drivers/hwd/src/main.rs`
- `recipes/core/base/source/drivers/acpid/src/acpi.rs`
- `recipes/core/base/source/init.initfs.d/40_pcid.service`
- `recipes/core/base/source/init.initfs.d/41_acpid.service`
- `recipes/core/base/source/init.initfs.d/40_hwd.service`
- `recipes/core/base/source/init.initfs.d/40_pcid-spawner-initfs.service`

Strict order:
1. kernel bootstrap / memory / early ACPI / IRQ / serio baseline
2. userspace bootstrap
3. `pcid` starts in initfs (`40_pcid.service`)
4. `acpid` starts in initfs (`41_acpid.service`)
5. `hwd` starts (`40_hwd.service`) and probes only after `pcid` + `acpid`
6. `pcid-spawner` runs (`40_pcid-spawner-initfs.service`)
7. `acpid` waits for PCI registration before AML-symbol readiness

### Shared initfs target membership (not strict serialization)

Grounded in:
- `recipes/core/base/source/init.initfs.d/40_pcid.service`
- `recipes/core/base/source/init.initfs.d/40_hwd.service`
- `recipes/core/base/source/init.initfs.d/41_acpid.service`
- `recipes/core/base/source/init.initfs.d/40_pcid-spawner-initfs.service`
- `recipes/core/base/source/init.initfs.d/40_ps2d.service`
- `recipes/core/base/source/init.initfs.d/40_drivers.target`
- `recipes/core/base/source/init.initfs.d/10_inputd.service`
- `recipes/core/base/source/init.initfs.d/10_lived.service`
- `recipes/core/base/source/init.initfs.d/20_graphics.target`

Important nuance:
- `ps2d`, `pcid`, `acpid`, `hwd`, and `pcid-spawner-initfs` all participate in early initfs driver bring-up.
- They are grouped by `40_drivers.target`, but they are **not** one single strict serial chain.

## 3. What Linux material Red Bear should borrow into Rust

### Subsystem matrix

| Subsystem | Linux donor material | Rewrite into Rust | Keep reference-only | Red Bear owner |
|---|---|---|---|---|
| ACPI / suspend | `drivers/acpi/sleep.c`, `Documentation/power/*`, `Documentation/driver-api/acpi/*` | sleep sequencing helpers, AML/power orchestration helpers, wake-source modeling | Linux PM core, ACPI device-node driver ownership | `acpid`, `redbear-sessiond` |
| PCI | `drivers/pci/probe.c`, `drivers/pci/quirks.c`, `Documentation/PCI/*` | capability walkers, BAR/resource validation, fixup/quirk pass model | Linux PCI core ownership | `pcid`, `redox-driver-sys` |
| IRQ / MSI / MSI-X | `drivers/pci/msi/*`, PCI docs | interrupt mode selection, vector policy, masking/fallback helpers | Linux generic IRQ core | kernel `irq:`, `pcid`, `redox-driver-sys` |
| PS/2 / i8042 | `drivers/input/serio/i8042.c`, input docs | reset/resume policy, aux/mux quirks, recovery deltas only | Linux input core | `serio`, `ps2d` |
| IOMMU | `drivers/iommu/amd/init.c`, `drivers/iommu/intel/iommu.c` | IVRS/DMAR parsers, table encoders, pre-enabled translation handling | Linux iommu-core structure | `iommu`, shared Rust helpers |
| USB / xHCI | `drivers/usb/host/xhci-pci.c`, `drivers/usb/core/hub.c`, USB docs | quirk logic, suspend/resume sequencing, composite/interface correctness | Linux USB core / driver model | `xhcid`, `usbhubd`, `usbscsid` |
| USB storage | `drivers/scsi/sd.c`, `Documentation/usb/mass-storage.rst` | bounded cache/flush/capacity heuristics | broad Linux SCSI midlayer architecture | `usbscsid` |
| Wi-Fi | `iwlwifi` transport, 80211 docs, NAPI docs | selected queue/DMA/IRQ/timeout helper patterns only | cfg80211/mac80211/NAPI architecture | bounded donor transport + native `wifictl`/`netctl` |
| GPU / DRM | `amdgpu_device.c`, `amdgpu_dm.c`, DRM docs, imported AMDGPU tree | orchestration, phase sequencing, quirk policy, selected shared helpers later | full DRM/AMDGPU runtime architecture | `redox-drm`, bounded vendor backends |

## 4. What Linux material must remain reference-only

- Linux PM core
- Linux driver core
- Linux USB core
- Linux input core
- Linux wireless subsystem architecture (`cfg80211`, `mac80211`, NAPI ownership model)
- Linux tasklet/workqueue ownership model
- Full AMDGPU runtime architecture

Reason: all of those conflict with the ownership rules that Red Bear already implements and should keep.

## 5. What Red Bear still materially needs

- ACPI sleep beyond `_S5`
- clean Intel VT-d / DMAR runtime ownership outside `acpid`
- better PCI host bridge / interrupt-link handling
- quirk convergence in `redox-driver-sys`
- USB composite/interface correctness
- hardware validation before deeper GPU/Wi-Fi extraction

---

# Part 2 — How to implement it in Red Bear OS

## 1. Placement rules

### Kernel

Keep only:
- bootstrap
- memory initialization
- early ACPI table discovery
- APIC/HPET/IRQ baseline
- race-critical `serio`

### Userspace daemons

- `acpid`: ACPI runtime policy, AML, sleep orchestration
- `pcid`: PCI enumeration/config/capability export/interrupt mode policy
- `iommu`: AMD-Vi / VT-d runtime ownership
- `ps2d`: PS/2 controller init/reset/resume/data path
- `xhcid`, `usbhubd`, `usbscsid`: controller/hub/storage runtime logic
- `redbear-sessiond`: D-Bus/session-facing sleep/shutdown bridge
- `redbear-wifictl`, `redbear-netctl`: native Wi-Fi control plane

### Shared Rust crates

- `redox-driver-sys`: canonical home for reusable PCI/IRQ/DMA/quirk/IOMMU helpers
- `linux-kpi`: bounded donor bridge for GPU/Wi-Fi only

## 2. Implementation order

For current execution order, priority ranking, and acceptance language:

- use `local/docs/IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md` for PCI / IRQ / low-level
  controller work,
- use `local/docs/ACPI-IMPROVEMENT-PLAN.md` for ACPI ownership/robustness/sleep work,
- use `local/docs/USB-IMPLEMENTATION-PLAN.md` for USB maturity,
- and use the Wi-Fi / DRM plans for those later subsystem-specific phases.

This file should keep the **borrowing and rewrite policy** for those phases, not act as a competing
execution roadmap.

## 3. Work package backlog

### Phase A — PCI / IRQ / quirk substrate

For Phase A execution detail, file targets, acceptance criteria, and validation language, use the
canonical PCI/IRQ plan:

- `local/docs/IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md`

This document keeps only the borrowing-policy summary for Phase A:

- borrow Linux capability/fixup/MSI semantics as donor knowledge,
- reimplement them as typed Rust helpers in `redox-driver-sys` / `pcid`,
- prefer data-driven quirks over daemon-local special cases,
- and do not import Linux generic IRQ-core ownership into Red Bear.

Current implementation progress remains true but is intentionally summarized only:

- shared PCI capability parsing and interrupt-support summarization are already present in
  `redox-driver-sys`,
- and the remaining rollout/convergence work now belongs to the canonical IRQ plan rather than this
  borrowing-policy document.

### Phase B — ACPI / suspend / IOMMU

**Primary targets**
- `recipes/core/base/source/drivers/acpid/src/acpi.rs`
- `recipes/core/base/source/drivers/acpid/src/sleep.rs` (new)
- `local/recipes/system/iommu/source/src/main.rs`
- shared future IVRS/DMAR helper modules in `redox-driver-sys`

**Implement**
- Red Bear-native sleep coordinator
- `_PTS` / `_WAK` / wake-source handling helpers
- IVRS/DMAR parsers and table builders
- move long-term DMAR runtime ownership into `iommu`

**Acceptance**
- `_S5` preserved
- explicit sleep phase machine exists
- IOMMU ownership clarified and moved out of `acpid`

**Current implementation progress (2026-04-18)**
- `acpid` now has an explicit `SleepTarget` / `SleepPhase` model in userspace, naming `S1`, `S3`,
  `S4`, and `S5` as Red Bear sleep targets.
- The real shutdown path now routes through that target model, while non-`S5` targets are
  recognized but still remain groundwork-only rather than implemented suspend/resume support.
- Unit coverage exists for sleep-target parsing, AML sleep-object naming, and the current
  Red Bear-native rule that only `S5` is treated as an implemented soft-off path today.
- This is still groundwork only: there is no claim of full suspend/resume or sleep eventing yet,
  and Linux suspend sequencing remains reference material rather than imported structure.
- The `iommu` daemon now also detects the presence of a kernel ACPI `DMAR` table and reports that
  Intel VT-d runtime ownership should converge there instead of remaining conceptually attached to
  the old transitional `acpid` DMAR code, but that ownership is not yet cleanly closed in the
  current tree.

### Phase C — PS/2 / USB / storage

**Primary targets**
- `recipes/core/base/source/drivers/input/ps2d/src/{controller,state}.rs`
- `recipes/core/base/source/drivers/usb/xhcid/src/xhci/*`
- `recipes/core/base/source/drivers/storage/usbscsid/src/*`

**Implement**
- PS/2 reset/resume hardening
- xHCI quirk and interface-selection corrections
- bounded storage heuristics from Linux SCSI logic

**Acceptance**
- PS/2 proof remains green
- xHCI and USB maturity proofs remain green
- no Linux USB/input-core structure imported

**Current implementation progress (2026-04-18)**
- `xhcid` now tracks active alternate settings per interface and resolves endpoint descriptors using
  that active-alternate map instead of flattening all interface descriptors in a configuration.
- Direct unit coverage exists for default-alternate endpoint selection and alternate-setting-aware
  endpoint remapping, closing the most explicit in-tree USB interface-selection TODO without
  importing Linux USB-core structure.
- `xhcid` now also preserves previously selected alternates on the same configuration and applies a
  requested interface/alternate override before endpoint planning, so alternate-setting
  reconfiguration no longer silently falls back to all-zero defaults.
- `xhcid` endpoint-direction lookup now also follows the active interface/alternate selection state
  instead of reading from the first configuration/interface pair unconditionally.
- `xhcid` driver spawning now also follows the selected configuration and active alternate map
  instead of hardcoding the first configuration and ignoring non-zero alternates.
- `xhcid` now also has a preserve-and-grow event-ring path in the IRQ reactor, so `EventRingFull`
  recovery no longer drops unread event TRBs while resizing the primary event ring.
- `usbhubd` and `xhcid` now propagate USB 2 hub TT Think Time from the parent hub descriptor into
  the xHCI Slot Context TT information bits using a bounded Linux-compatible encoding path.
- `xhcid` endpoint-context calculations are now protocol-speed-aware for SuperSpeedPlus, so
  interval and ESIT-payload selection distinguish SSP paths from generic SuperSpeed using the
  resolved port protocol speed rather than only endpoint companion presence.
- `usbscsid` now has a bounded native `SYNCHRONIZE CACHE(10/16)` heuristic gated by the existing
  `needs_sync_cache` storage quirk, directly reflecting the planned Linux `sd.c` donor usage without
  importing Linux SCSI midlayer structure.
- `ps2d` now performs an explicit controller-output flush during probe and at the key controller
  reinitialization boundaries in `Ps2::init()`, matching the Linux `i8042_flush()` discipline in a
  bounded Red Bear-native way without importing Linux input-core structure.

### Phase D — Wi-Fi and GPU/DRM

**Primary targets**
- `local/recipes/system/redbear-wifictl/source/*`
- `local/recipes/system/redbear-netctl/source/*`
- `local/recipes/gpu/redox-drm/source/*`

**Implement**
- only reusable queue/DMA/IRQ helper extraction from bounded Wi-Fi donor transport
- only orchestration / phase sequencing / quirk policy extraction for DRM

**Acceptance**
- control plane remains native
- DRM display-vs-render boundary remains explicit
- no claim of full AMDGPU rewrite or Linux wireless-architecture import

**Current implementation progress (2026-04-18)**
- `redbear-wifictl` transport probing now uses the shared `redox-driver-sys` PCI parser and the
  shared quirk-aware interrupt-support summary instead of relying only on local raw-config logic.
- This is a bounded helper extraction only: the native Wi-Fi control plane remains authoritative,
  and there is still no import of Linux wireless subsystem structure.
- `redox-drm` now turns shared hotplug and vblank events into a queued scheme-visible
  `EVENT_READ` surface for `card0`, with hotplug also reaching the matching connector handle.
  That makes shared DRM event delivery observable without conflating it with render-fence
  semantics.

## 4. Subsystem-specific code guidelines

### ACPI / suspend

- Use Linux power docs/source for sequencing and debugging principles only.
- Do not port Linux PM callback ownership.
- Keep ACPI policy in `acpid` and session-facing signaling in `redbear-sessiond`.

### PCI / IRQ

- Reimplement Linux capability/fixup logic as typed Rust helpers.
- Prefer data-driven quirks over daemon-local special cases.

### PS/2

- Treat current `serio` + `ps2d` as correct baseline.
- Only import missing deltas from Linux `i8042.c`.

### IOMMU

- Reimplement parsing/table logic in Rust.
- Keep runtime MMIO ownership in userspace `iommu`.

### USB

- Borrow Linux quirks and sequencing; keep controller ownership in Rust daemons.
- Do not recreate Linux USB driver registration models.

### Wi-Fi

- Keep Linux 80211 docs as reference-only behavior material.
- Keep `cfg80211` / `mac80211` / NAPI architecture out of Red Bear.

### GPU / DRM

- Use Linux DRM docs and donor code to inform orchestration and boundary discipline.
- Do not treat imported AMDGPU code as a roadmap for wholesale Rust replacement.

## 5. Validation and evidence rules

Every Linux-derived rewrite should clear these gates in order:

1. **Donor identified** — exact Linux source/doc named
2. **Rust landing point identified** — exact crate/module/file named
3. **Boundary stated** — rewrite target vs reference-only
4. **Build-valid** — compiles cleanly
5. **Runtime-valid** — bounded proof exists
6. **Hardware-valid** — only once target hardware evidence exists

Do not collapse those categories. Build success is not runtime proof, and runtime proof is not hardware support.

## 6. Final policy summary

The correct Red Bear approach is:

- borrow Linux **knowledge**, **algorithms**, **parsers**, **quirk semantics**, and **phase sequencing**
- rewrite those into **Rust helpers** and **Red Bear-native state machines**
- keep the **kernel minimal**
- keep **runtime/controller policy in userspace daemons**
- keep **`linux-kpi` bounded to GPU/Wi‑Fi donor islands only**
- avoid importing Linux subsystem ownership models into Red Bear OS
