# Red Bear OS Documentation Index

Technical documentation for Red Bear OS as an overlay distribution on top of Redox OS.

This index is the entry point for the documentation set. Its main job is to make the
current/canonical versus historical/reference split obvious.

> **Status note (2026-04-16):** The canonical desktop path document is now
> `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` (v2.0, three-track Phase 1–5 model). It consolidates
> the Wayland, KDE, and GPU roadmap into one honest implementation plan with testable exit criteria.
> The historical docs below (01–05) remain useful for architecture reference and implementation
> rationale, but they should be read together with the new plan and the current local subsystem docs.

> **Status note (2026-04-14):** several documents below are historical implementation plans whose
> original "missing / not started" language is now stale. The repo already contains substantial
> Red Bear OS work under `local/`; use each document's top-level status notes together with
> `local/docs/AMD-FIRST-INTEGRATION.md` and `local/docs/QT6-PORT-STATUS.md` for current state.

> **Red Bear note:** newer subsystem plans can also live under `local/docs/` when they are Red Bear-
> specific rather than general Redox architecture material. In particular, see
> `local/docs/WIFI-IMPLEMENTATION-PLAN.md` for the current Wi-Fi direction,
> `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` for the canonical desktop path,
> and `local/docs/AMD-FIRST-INTEGRATION.md` for AMD-specific technical detail only.

> **Repository model:** RedBearOS relates to Redox in the same way Ubuntu relates to Debian.
> Upstream Redox remains the base platform; Red Bear carries packaging, patch, validation, and
> subsystem overlays on top. For long-term stability, upstream-owned source trees should be treated
> as refreshable working copies, while durable Red Bear state belongs in `local/patches/`,
> `local/recipes/`, `local/docs/`, and tracked Red Bear configs.
>
> **WIP policy:** if an upstream recipe or subsystem is still marked WIP, Red Bear treats it as a
> local project until upstream promotes it to first-class status. We may refresh from upstream WIP,
> but we should fix and ship from the Red Bear overlay until upstream support is real enough to
> replace the local copy.

## Document Status Matrix

| Document set | Role |
|---|---|
| `README.md`, `AGENTS.md`, `docs/README.md`, `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md` | canonical repository-level policy and current execution model |
| `local/docs/*IMPLEMENTATION-PLAN*.md`, `local/docs/*STATUS*.md`, `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md`, `local/docs/DRM-MODERNIZATION-EXECUTION-PLAN.md` | canonical current Red Bear subsystem plans and status |
| `docs/01-REDOX-ARCHITECTURE.md` | architecture reference |
| `docs/04-LINUX-DRIVER-COMPAT.md`, `docs/05-KDE-PLASMA-ON-REDOX.md` | valuable but partly historical roadmap/design material |

When a current-state local document conflicts with an older historical public roadmap, prefer the
current local subsystem plan.

## Documents

| # | Document | Description |
|---|----------|-------------|
| 01 | [Architecture Overview](01-REDOX-ARCHITECTURE.md) | Architecture reference for Redox internals: microkernel, scheme system, driver model, display stack |
| 04 | [Linux Driver Compatibility Layer](04-LINUX-DRIVER-COMPAT.md) | Historical/current hybrid design reference for the LinuxKPI-style driver compatibility model |
| 05 | [KDE Plasma on Redox](05-KDE-PLASMA-ON-REDOX.md) | Historical KDE implementation path plus deeper KDE-specific rationale |
| 06 | [Build System Setup](06-BUILD-SYSTEM-SETUP.md) | How to build Redox from this repository |
| 07 | [Red Bear OS Implementation Plan](07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md) | Canonical public implementation plan focused on profiles, packaging, validation, and staged hardware enablement |
| 08 | [Firmware in Red Bear OS](firmware.md) | Canonical firmware packaging, licensing, and runtime-loading policy |

## Related Red Bear-local current-state plans

- `../local/docs/USB-IMPLEMENTATION-PLAN.md` — current USB completeness and rollout plan
- `../local/docs/WIFI-IMPLEMENTATION-PLAN.md` — current Wi-Fi architecture and rollout plan
- `../local/docs/WIFI-VALIDATION-RUNBOOK.md` — canonical operator path for bare-metal/VFIO Wi-Fi validation and evidence capture
- `../local/docs/WIFICTL-SCHEME-REFERENCE.md` — bounded `/scheme/wifictl` interface reference for the current Wi-Fi control surface
- `../local/recipes/system/redbear-netctl-console/source/` — Redox-native ncurses terminal client for the bounded Wi-Fi profile flow
- `../local/docs/SCRIPT-BEHAVIOR-MATRIX.md` — guarantees and non-guarantees for the main Wi-Fi and Bluetooth validation helpers plus core repo scripts
- `../local/docs/BLUETOOTH-IMPLEMENTATION-PLAN.md` — current Bluetooth architecture and rollout plan
- `../local/docs/BLUETOOTH-VALIDATION-RUNBOOK.md` — canonical operator path for the bounded Bluetooth Battery Level QEMU validation slice
- `../local/docs/ACPI-IMPROVEMENT-PLAN.md` — current ACPI ownership, robustness, and validation plan
- `../local/docs/ACPI-IMPROVEMENT-PLAN.md` — current ACPI ownership, robustness, and validation plan
- `../local/docs/IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md` — canonical current plan for PCI/IRQ quality, low-level controller robustness, MSI/MSI-X follow-up, and controller runtime-proof sequencing
- `../local/docs/DRM-MODERNIZATION-EXECUTION-PLAN.md` — current DRM-focused execution plan beneath the canonical desktop path, with equal Intel/AMD evidence bars
- `../local/docs/WAYLAND-IMPLEMENTATION-PLAN.md` — canonical Wayland subsystem plan beneath the desktop path
- `../local/docs/RELIBC-COMPLETENESS-AND-ENHANCEMENT-PLAN.md` — canonical relibc quality/completeness/robustness assessment and improvement plan
- `../local/docs/RELIBC-IPC-ASSESSMENT-AND-IMPROVEMENT-PLAN.md` — IPC-focused companion plan for the active relibc compatibility surface
- `../local/docs/GREETER-LOGIN-IMPLEMENTATION-PLAN.md` — canonical greeter/login plan for the Red Bear-native login boundary on `redbear-full`
- PCI vendor/device names in Red Bear runtime tools now come from the shipped `pciids` database; PCI quirk policy still lives in `../local/docs/QUIRKS-SYSTEM.md`
- `../local/docs/AMD-FIRST-INTEGRATION.md` — AMD-focused technical roadmap and hardware detail only, not the canonical desktop plan
- `../local/docs/DESKTOP-STACK-CURRENT-STATUS.md` — canonical current build/runtime truth summary for the desktop stack

These local Red Bear plans should be treated as first-class subsystem references for desktop/session,
USB, Wi-Fi, Bluetooth, and low-level controller work. They carry blocker detail that the public docs
summarize at a higher level.

For PCI/IRQ language specifically, prefer the local IRQ plan’s distinction between:

- compile-visible infrastructure,
- bounded QEMU/runtime proof,
- and broader hardware validation.

Do not flatten those into one “supported” claim in public summaries.

## Related Red Bear-local governance docs

- `../local/docs/WIP-MIGRATION-LEDGER.md` — current WIP ownership and upstream-vs-local migration ledger
- `../local/docs/SCRIPT-BEHAVIOR-MATRIX.md` — what the main sync/fetch/apply/build scripts do and do not guarantee
- `../local/docs/EXTERNAL-TOOLCHAIN.md` — how to export a relocatable external `x86_64-unknown-redox-gcc` toolchain from the built prefix

## Current State Summary (as of 2026-04-18)

This summary is only a quick orientation layer. For canonical current-state detail, prefer:

- `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md` for repository-wide execution order,
- `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` for the canonical desktop path from console to KDE Plasma on Wayland,
- `local/docs/DESKTOP-STACK-CURRENT-STATUS.md` for desktop build/runtime truth,
- `local/docs/PROFILE-MATRIX.md` for support-language by tracked profile,
- and the active subsystem plans under `local/docs/` for detailed current workstreams.

- **Compile targets**: the supported compile targets are `redbear-mini`, `redbear-full`, and `redbear-grub`
- **Live ISO policy**: live `.iso` outputs (`make live`) are for real bare-metal boot/install/recovery workflows, not the VM/QEMU execution surface.
- **Wayland**: libwayland + wayland-protocols built. A bounded greeter/compositor-backed login proof now passes, but broader compositor/runtime stability remains incomplete.
- **Qt6**: qtbase 6.11.0 (Core+Gui+Widgets+DBus+Wayland), qtdeclarative, qtsvg, qtwayland ALL BUILT
- **D-Bus**: 1.16.2 built for Redox. Qt6DBus enabled.
- **KF6 Frameworks**: all 32/32 built. Some packages remain shimmed or stubbed (kirigami stub-only, kf6-kio heavy shim).
- **Mesa**: software-rendered path is present; full GBM / hardware-validated Wayland path is still incomplete.
- **GPU drivers**: redox-drm scheme daemon exists; Intel build-oriented path exists; AMD currently has a bounded retained compile path (`redox-drm` + Red Bear glue) while the imported Linux AMD DC/TTM/core trees remain under compile triage. Hardware validation is still pending.
- **Input**: evdevd compiled, libevdev built, libinput 1.30.2 built
- **Networking**: native wired stack present (`pcid-spawner` → NIC daemon → `smolnetd`/`dhcpd`/`netcfg`), Red Bear ships a native `netctl` command, RTL8125 is wired into the existing Realtek autoload path, and the bounded Intel Wi‑Fi path now has host-tested profile start/stop plus interface-specific DHCP handoff without claiming real wireless connectivity.
- **PCI / IRQ quality**: architecturally strong substrate exists, with bounded MSI-X, IOMMU, xHCI IRQ, and low-level-controller proof surfaces; broader hardware robustness is still intentionally tracked as open work in `../local/docs/IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md`
- **Wi-Fi profile target**: `config/redbear-wifi-experimental.toml` is the first explicit tracked image slice for bounded Intel Wi‑Fi validation, instead of spreading that claim across the generic desktop profiles.
- **Bluetooth**: one bounded in-tree BLE-first experimental slice exists, and the Battery Level read-only workload now has a packaged in-guest checker plus a host QEMU harness; QEMU validation is still in progress, so broad desktop Bluetooth parity is still incomplete
- **Desktop direction**: `redbear-full` carries the desktop-capable target surface; the bounded greeter/login slice now passes, while the wider desktop runtime stack is still incomplete.
- **ACPI**: materially complete for the historical boot baseline, not release-grade complete; implemented: AML mutex real state, EC widened accesses via byte transactions, kstop-based shutdown eventing, explicit `RSDP_ADDR` forwarding into `acpid`, x86 BIOS-search AML fallback, and real-but-provisional AML-backed power enumeration. **Known gaps**: the explicit boot-path producer contract for AML bootstrap is still underdocumented, `acpid` startup hardening remains open, shutdown/power reporting are still provisional, sleep state transitions and sleep eventing remain incomplete, DMAR ownership is still transitional, and bare-metal validation is still bounded. See `local/docs/ACPI-IMPROVEMENT-PLAN.md`.
- **Linux driver compat**: linux-kpi now includes early wireless-subsystem compatibility scaffolding in addition to the earlier helper layer, redox-driver-sys and firmware-loader compile, and the bounded Intel Wi-Fi path now has host-tested scan/connect/disconnect/profile/reporting flows without claiming real hardware Wi-Fi connectivity.
- **Wi-Fi validation tooling**: `redbear-phase5-wifi-check` and `redbear-phase5-wifi-capture` are now packaged in-guest helpers for bounded Intel Wi-Fi runtime validation and evidence capture on bare metal or VFIO-backed guests.
- **Phase 5 naming note**: the bounded `redbear-phase5-network-check` / `test-phase5-network-qemu.sh` path proves desktop/network plumbing on `redbear-full` in QEMU; it does **not** mean the Wi-Fi implementation plan's later Phase W5 real-hardware reporting/recovery milestone is complete.

## Quick Start

```bash
# 1. Install dependencies (Arch/Manjaro)
sudo pacman -S --needed --noconfirm gdb meson nasm patchelf python-mako \
  doxygen expat file fuse3 gmp libjpeg-turbo libpng po4a scons \
  sdl12-compat syslinux texinfo xdg-utils zstd

# 2. Install Rust + tools
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
cargo install just cbindgen

# 3. Configure for native build (no Podman)
echo 'PODMAN_BUILD?=0' > .config

# 4. Build (downloads cross-toolchain, then compiles)
make all

# 5. Run in QEMU
make qemu
```

## Key Repositories

| Repo | Purpose | URL |
|------|---------|-----|
| Kernel | Microkernel | https://gitlab.redox-os.org/redox-os/kernel |
| Base | Drivers + system components | https://gitlab.redox-os.org/redox-os/base |
| relibc | C library (Rust) | https://gitlab.redox-os.org/redox-os/relibc |
| RedoxFS | Default filesystem | https://gitlab.redox-os.org/redox-os/redoxfs |
| libredox | System library | https://gitlab.redox-os.org/redox-os/libredox |
| This repo | Build system | https://gitlab.redox-os.org/redox-os/redox |
