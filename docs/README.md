# Redox OS Fork — Wayland, KDE & Linux Driver Compatibility

Technical documentation for forking Redox OS to include Wayland protocol support,
KDE Plasma desktop environment, and a Linux driver compatibility layer.

> **Status note (2026-04-14):** several documents below are historical implementation plans whose
> original "missing / not started" language is now stale. The repo already contains substantial
> Red Bear OS work under `local/`; use each document's top-level status notes together with
> `local/docs/AMD-FIRST-INTEGRATION.md` and `local/docs/QT6-PORT-STATUS.md` for current state.

## Documents

| # | Document | Description |
|---|----------|-------------|
| 01 | [Architecture Overview](01-REDOX-ARCHITECTURE.md) | Redox OS internals: microkernel, scheme system, driver model, display stack |
| 02 | [Gap Analysis & Roadmap](02-GAP-ANALYSIS.md) | What's missing between current Redox and our Wayland/KDE/driver-compat goals |
| 03 | [Wayland on Redox](03-WAYLAND-ON-REDOX.md) | Deep-dive into Wayland protocol requirements and current porting status |
| 04 | [Linux Driver Compatibility Layer](04-LINUX-DRIVER-COMPAT.md) | Design for a FreeBSD LinuxKPI-style driver compatibility shim |
| 05 | [KDE Plasma on Redox](05-KDE-PLASMA-ON-REDOX.md) | Feasibility study and implementation plan for KDE Plasma |
| 06 | [Build System Setup](06-BUILD-SYSTEM-SETUP.md) | How to build Redox from this repository |
| 07 | [Red Bear OS Implementation Plan](07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md) | Canonical public implementation plan focused on profiles, packaging, validation, and staged hardware enablement |

## Current State Summary (as of 2026-04-14)

- **Display server**: Orbital (custom, scheme-based) — works
- **Wayland**: libwayland + wayland-protocols built. Smallvil/cosmic-comp remain partial runtime experiments.
- **Qt6**: qtbase 6.11.0 (Core+Gui+Widgets+DBus+Wayland), qtdeclarative, qtsvg, qtwayland ALL BUILT
- **D-Bus**: 1.16.2 built for Redox. Qt6DBus enabled.
- **KF6 Frameworks**: mixed state — many real builds, but some packages are still shimmed or stubbed.
- **Mesa**: software-rendered path is present; full GBM / hardware-validated Wayland path is still incomplete.
- **GPU drivers**: redox-drm scheme daemon and AMD+Intel compile-oriented paths exist; hardware validation is still pending.
- **Input**: evdevd compiled, libevdev built, libinput 1.30.2 built
- **Networking**: native wired stack present (`pcid-spawner` → NIC daemon → `smolnetd`/`dhcpd`/`netcfg`), Red Bear ships a native `netctl` command, and RTL8125 is wired into the existing Realtek autoload path
- **KDE**: `redbear-kde.toml` exists and the recipe tree is populated, but the runtime stack is still incomplete.
- **Linux driver compat**: linux-kpi (31 C headers + 13 Rust FFI), redox-driver-sys, firmware-loader all compile.

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
| Orbital | Display server + WM | https://gitlab.redox-os.org/redox-os/orbital |
| RedoxFS | Default filesystem | https://gitlab.redox-os.org/redox-os/redoxfs |
| libredox | System library | https://gitlab.redox-os.org/redox-os/libredox |
| This repo | Build system | https://gitlab.redox-os.org/redox-os/redox |
