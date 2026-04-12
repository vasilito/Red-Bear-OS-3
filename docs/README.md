# Redox OS Fork — Wayland, KDE & Linux Driver Compatibility

Technical documentation for forking Redox OS to include Wayland protocol support,
KDE Plasma desktop environment, and a Linux driver compatibility layer.

## Documents

| # | Document | Description |
|---|----------|-------------|
| 01 | [Architecture Overview](01-REDOX-ARCHITECTURE.md) | Redox OS internals: microkernel, scheme system, driver model, display stack |
| 02 | [Gap Analysis & Roadmap](02-GAP-ANALYSIS.md) | What's missing between current Redox and our Wayland/KDE/driver-compat goals |
| 03 | [Wayland on Redox](03-WAYLAND-ON-REDOX.md) | Deep-dive into Wayland protocol requirements and current porting status |
| 04 | [Linux Driver Compatibility Layer](04-LINUX-DRIVER-COMPAT.md) | Design for a FreeBSD LinuxKPI-style driver compatibility shim |
| 05 | [KDE Plasma on Redox](05-KDE-PLASMA-ON-REDOX.md) | Feasibility study and implementation plan for KDE Plasma |
| 06 | [Build System Setup](06-BUILD-SYSTEM-SETUP.md) | How to build Redox from this repository |

## Current State Summary (as of Redox 0.9.0)

- **Display server**: Orbital (custom, scheme-based) — works
- **Wayland**: Experimental, WIP. Smallvil (Smithay) and cosmic-comp partially working.
  libwayland patched with shimmed-out `signalfd`, `timerfd`, `eventfd`.
- **X11**: Working via X.org dummy driver inside Orbital.
- **Mesa**: Software-rendered only (LLVMpipe/OSMesa). No GPU acceleration.
- **GPU drivers**: VESA framebuffer + VirtIO GPU only. Experimental Intel modesetting.
- **KDE**: 19 app recipes in WIP, no KDE Plasma infrastructure.
- **Linux driver compat**: None. Redox explicitly chose source-level porting over binary compat.

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
