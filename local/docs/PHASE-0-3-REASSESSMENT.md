# Red Bear OS Phase 0–3 Reassessment

## Purpose

This document reconciles the current public execution plan in `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md`
with the older hardware-oriented roadmap in `local/docs/AMD-FIRST-INTEGRATION.md`.

The goal is to make Phase 0 through Phase 3 readable in terms of **what is built**, **what is
boot/runtime wired**, and **what is actually validated**.

## Validation States

- **builds** — code or profile compiles successfully
- **boots** — image or service path reaches a usable boot/runtime state
- **validated** — behavior has been exercised with real evidence for the claimed scope
- **experimental** — available for bring-up but not support-promised

This repo should not treat “compiles” as equivalent to “validated”.

## Why this reassessment exists

Two active documents describe the early Red Bear roadmap differently:

- `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md` is the canonical public execution plan.
- `local/docs/AMD-FIRST-INTEGRATION.md` is the older AMD-first technical roadmap.

They are both useful, but they number phases differently:

- `docs/07` uses a product-enablement framing (`Phase 1` repository/profile structure, `Phase 2`
  minimal-system baseline, `Phase 3` driver/runtime substrate).
- `AMD-FIRST` uses a hardware-enablement framing (`P0` ACPI boot, `P1` driver infrastructure,
  `P2` AMD display, `P3` input + POSIX).

This document is the bridge for Phase 0–3 discussions.

## Phase 0 — Bare-metal boot and ACPI baseline

### Source of truth

- `local/docs/AMD-FIRST-INTEGRATION.md`
- Root `AGENTS.md` status summary

### Scope

- AMD bare-metal bootability
- ACPI checksums and table handling
- shutdown/reboot/power-method support
- SMP/x2APIC-era platform readiness

### Current status

- **builds** — yes
- **boots** — yes
- **validated** — yes, at the platform/boot level described in the AMD-first notes

### Notes

Phase 0 is not part of the public `docs/07` numbering, but it remains a real prerequisite in the
AMD-first implementation history and should stay visible when discussing early Red Bear progress.

## Phase 1 — Repository discipline and profile reproducibility

### Source of truth

- `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md`
- `local/docs/repo-governance.md`
- `local/docs/PROFILE-MATRIX.md`

### Scope

- tracked profile definitions
- shared config fragments instead of duplicated wiring
- helper scripts aligned with tracked profiles
- support-language and validation-language rules

### Current status

- **builds** — yes
- **boots** — indirectly supported by later profile builds
- **validated** — partially, in the sense that `redbear-minimal` and `redbear-desktop` were used as
  reproducibility targets during the Phase 1 cleanup

### Implemented evidence

- `config/redbear-*.toml` shared fragment refactor
- `local/docs/repo-governance.md`
- `local/docs/PROFILE-MATRIX.md`
- `local/scripts/build-redbear.sh` profile coverage updates

### Remaining caution

Phase 1 is structurally in good shape, but support labels still need to be used consistently in
phase-level docs.

## Phase 2 — Minimal-system baseline

### Source of truth

- `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md`
- `local/docs/NETWORKING-RTL8125-NETCTL.md`
- `local/docs/REDBEAR-INFO-RUNTIME-REPORT.md`

### Scope

- bootable minimal profile
- package-management baseline
- VM networking baseline

### Current status

- **builds** — yes
- **boots** — helper and validation surfaces now exist for the VM path
- **validated** — partially; the repo now has explicit validation helpers, but this still needs
  continued real runtime use to graduate from baseline bring-up to stronger support claims

### Implemented evidence

- `redbear-minimal` enables `wired-dhcp` by default
- `redbear-info` reports VirtIO VM networking visibility
- `local/scripts/validate-vm-network-baseline.sh`
- `local/scripts/test-vm-network-qemu.sh`
- `local/scripts/test-vm-network-runtime.sh`

### Remaining caution

Phase 2 should continue to be described as a **baseline**. It now has build-time, launch-time, and
runtime check paths, but that is still not the same as broad hardware validation.

## Phase 3 — Driver and runtime substrate

### Source of truth

- `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md`
- `local/docs/AMD-FIRST-INTEGRATION.md`

### Correct framing

The public plan's wording is the correct top-level framing:

> **Driver and runtime substrate**

The AMD-first wording remains useful as a lower-level technical breakdown:

> **Input + POSIX**

These are not competing scopes. The second explains the concrete components that fulfill the first.

### Scope

- shared driver substrate already built in-tree
- firmware loading available as runtime infrastructure
- input/runtime prerequisites such as `evdevd` and `udev-shim`
- relibc POSIX surfaces required by downstream consumers

### Current status

- **builds** — yes for the major in-tree Phase 3 components
- **boots** — partially wired via profile/service configuration
- **validated** — not yet at the level needed to call the substrate runtime-proven end to end

### Built evidence already in tree

- `local/recipes/drivers/redox-driver-sys/`
- `local/recipes/drivers/linux-kpi/`
- `local/recipes/system/firmware-loader/`
- `local/recipes/system/evdevd/`
- `local/recipes/system/udev-shim/`
- `local/patches/relibc/P3-*.patch`

### Real remaining work

The main remaining Phase 3 task is not “invent the substrate” — it already exists in-tree. The
real gap is **runtime and downstream-consumer validation**:

- prove the relibc POSIX surfaces against actual consumers
- prove the input path from Redox input sources through `evdevd` and `udev-shim`
- keep Phase 3 distinct from later graphics/Wayland/KDE work

### Current runtime-validation helpers

- `./local/scripts/test-phase3-runtime-substrate.sh` — in-guest runtime check for
  `firmware-loader`, `udev-shim`, `evdevd`, and their scheme surfaces
- `redbear-info --verbose` — passive runtime evidence for installed/active integrations

### Runtime evidence gathered during reassessment

- `redbear-desktop` was booted successfully in QEMU with x86_64 UEFI firmware and reached a real
  login prompt over the serial console.
- `pcid-spawner` successfully spawned `virtio-netd` during the guest boot sequence.
- `firmware-loader` registered `scheme:firmware` without crashing, even with an empty
  `/usr/firmware/` directory.
- `evdevd` registered `scheme:evdev` and `udev-shim` registered `scheme:udev` during the same
  guest boot.
- `redbear-info --json` inside the guest reported `virtio_net_present: true`, a configured
  `eth0` address, and live firmware/udev integration evidence.

## Recommended interpretation going forward

When discussing the roadmap publicly:

- use `docs/07` phase numbering as canonical
- treat `AMD-FIRST` phase numbering as historical hardware-roadmap context
- always attach validation language (`builds`, `boots`, `validated`, `experimental`) to claims

## Summary

Phase 0 is the AMD-first bare-metal boot foundation.

Phase 1 is structurally implemented and largely cleaned up.

Phase 2 now has an actual VM-network baseline with repo, launch, and in-guest validation helpers.

One practical caveat surfaced during reassessment: the QEMU launch helper also depends on usable
x86_64 UEFI firmware on the host. When that firmware is missing, the failure mode is a host-side
SeaBIOS/iPXE fallback rather than a guest-side Red Bear runtime failure, so the helper now checks
for that prerequisite explicitly.

Phase 3 should be understood as **runtime-substrate validation and wiring**, not as a brand-new
infrastructure buildout from zero.

## Quality Assessment

### Planning quality

**Strong points**

- The public plan in `docs/07` is clearer and more execution-oriented than the older roadmap.
- Phase 1 and Phase 2 now have concrete helper scripts and docs instead of relying on implicit
  operator knowledge.
- The profile matrix and governance docs substantially reduce ambiguity about what each tracked
  profile is supposed to represent.

**Weak points**

- Historical phase numbering from `AMD-FIRST-INTEGRATION.md` still differs from the newer public
  plan, which can confuse progress reporting if the bridge document is not consulted.
- Some status language across the repo still tends to overvalue “builds” relative to “validated”.

### Implementation quality

**Strong points**

- Shared Red Bear config fragments reduced duplication in tracked profiles.
- The VM-network baseline now has layered validation surfaces: repo-level, launcher-level, and
  in-guest runtime checks.
- `redbear-info` remains aligned with real integration changes instead of becoming stale.

**Weak points**

- Runtime validation is still thinner than build validation across the early phases.
- Some local operating docs needed follow-up cleanup to reflect the newer scripts and profile set.

### Recommendation

For Phase 0–3 work, prefer closing validation gaps and documentation drift before adding new scope.
The early-phase codebase is in a much better structural state now; the main quality risk is no
longer missing packages, but overstating readiness before runtime evidence exists.

## Phase 4 Handoff Note

Phase 4 should begin from the existing `wayland.toml` profile, not by jumping straight to KWin.
The current repo already contains the `smallvil`, `cosmic-comp`, `qtwayland`, and Mesa software
 rendering pieces; the highest-value next work is validating the `orbital-wayland` → `smallvil`
 runtime path on QEMU/VirtIO and only then widening to heavier compositor/session stacks.
