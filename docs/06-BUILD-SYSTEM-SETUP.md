# 06 — Build System Setup Guide

> **Status note (2026-04-15):** This file explains the mechanics of building the repository, but it
> is not the canonical source for repository ownership policy or current execution order. For the
> current repository model, use `README.md`, `AGENTS.md`, and
> `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md`. For Red Bear-owned subsystem planning, use the
> current documents under `local/docs/`.

## Repository Model Reminder

Build this repository using the Red Bear overlay model:

- upstream-owned source trees are refreshable working copies,
- durable Red Bear state lives in `local/patches/`, `local/recipes/`, `local/docs/`, and tracked
  Red Bear configs,
- upstream WIP recipes are useful inputs, but should not automatically be treated as the durable
  shipping source of truth for Red Bear.

## Prerequisites

### System Requirements

- **OS**: Linux (Arch/Manjaro, Debian/Ubuntu, Fedora, Gentoo)
- **Architecture**: x86_64 (primary), also supports aarch64, i586, riscv64gc
- **RAM**: 4GB minimum, 8GB+ recommended
- **Disk**: 20GB+ free space (full build with all recipes)
- **Network**: Required for downloading sources and toolchain

### Install Build Dependencies

#### Arch / Manjaro

```bash
sudo pacman -S --needed --noconfirm \
  autoconf automake bison cmake curl doxygen expat file flex fuse3 \
  gdb git gmp libjpeg-turbo libpng libtool m4 make meson nasm \
  ninja openssl patch patchelf perl pkgconf po4a protobuf python \
  python-mako rsync scons sdl12-compat syslinux texinfo unzip \
  wget xdg-utils zip zstd qemu-system-x86 qemu-system-arm qemu-system-riscv
```

#### Debian / Ubuntu

```bash
sudo apt-get update
sudo apt-get install --assume-yes \
  ant autoconf automake bison build-essential cmake curl doxygen \
  expect file flex fuse3 g++ gdb-multiarch git libc6-dev-i386 \
  libfuse3-dev libgdk-pixbuf2.0-bin libglib2.0-dev-bin libgmp-dev \
  libhtml-parser-perl libjpeg-dev libmpfr-dev libsdl1.2-dev \
  libsdl2-ttf-dev llvm m4 make meson nasm ninja-build patch \
  patchelf perl pkg-config po4a protobuf-compiler python3 \
  python3-dev python3-mako rsync ruby scons texinfo unzip wget \
  xdg-utils xxd zip zstd qemu-system-x86 qemu-kvm
```

#### Fedora

```bash
sudo dnf install --assumeyes \
  @development-tools autoconf automake bison cmake curl doxygen \
  expat-devel file flex fuse3-devel gcc gcc-c++ gdb genisoimage \
  gettext-devel glibc-devel.i686 gmp-devel libjpeg-turbo-devel \
  libpng-devel libtool m4 make meson nasm ninja-build openssl \
  patch patchelf perl po4a protobuf-compiler python3-mako \
  SDL2_ttf-devel sdl12-compat-devel syslinux texinfo unzip vim \
  zip zstd qemu-system-x86 qemu-kvm
```

### Install Rust and Cargo Tools

```bash
# Install Rust via rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

# Install required cargo tools
cargo install just cbindgen
```

## Configuration

### Native Build (No Container)

```bash
# In the redox-master directory:
echo 'PODMAN_BUILD?=0' > .config
```

### Podman Build (Containerized, Default)

```bash
# Default uses Podman — no configuration needed
# Ensure Podman is installed:
# Arch: sudo pacman -S podman
# Debian: sudo apt-get install podman
```

### Select Build Configuration

Mainline configs still exist, but tracked Red Bear work should normally be built and validated
through the four supported `redbear-*` compile targets. For desktop work specifically,
`redbear-full` is the tracked desktop-capable target.

Available configs (in `config/`):

| Config | Description |
|---|---|
| `redbear-mini` | Minimal tracked Red Bear image |
| `redbear-live-mini` | Live/recovery variant of the minimal target |
| `redbear-full` | Desktop-capable tracked Red Bear image |
| `redbear-live-full` | Live/recovery variant of the desktop-capable target |

## Building

### Full Build (Desktop)

```bash
make all
```

This produces the image for the selected target, such as `build/x86_64/redbear-full/harddrive.img`.

### Export External Toolchain

After `make prefix`, you can export a relocatable external cross toolchain that provides
`x86_64-unknown-redox-gcc` and the related host-side wrappers in one directory:

```bash
make export-toolchain TARGET=x86_64-unknown-redox
source build/toolchain-export/x86_64-unknown-redox/activate.sh
x86_64-unknown-redox-gcc --version
```

To export somewhere else:

```bash
make export-toolchain TARGET=x86_64-unknown-redox \
  TOOLCHAIN_EXPORT_DIR=/opt/redbear/toolchains/x86_64-unknown-redox
```

For the full layout and rationale, see `local/docs/EXTERNAL-TOOLCHAIN.md`.

### Build with Specific Config

```bash
# Preferred Red Bear wrapper:
./local/scripts/build-redbear.sh redbear-mini
./local/scripts/build-redbear.sh redbear-live-mini
./local/scripts/build-redbear.sh redbear-full
./local/scripts/build-redbear.sh redbear-live-full

# Direct make is still valid when needed:
make all CONFIG_NAME=redbear-full
```

For tracked Red Bear work, prefer these four compile targets over older historical names.

### Build a Live ISO

```bash
make live CONFIG_NAME=redbear-live-full
# Produces: build/x86_64/redbear-live-full/redox-live.iso
```

### Rebuild After Changes

```bash
make rebuild    # Clean rebuild of filesystem image
```

## Running

### QEMU (Recommended)

```bash
# Default desktop-capable tracked target:
make qemu

# Explicit desktop-capable tracked target:
make qemu CONFIG_NAME=redbear-full

# With more RAM:
make qemu QEMUFLAGS="-m 4G"

# Without GUI (serial console):
make qemu QEMUFLAGS="-nographic"

# With network (port forwarding):
make qemu QEMUFLAGS="-net nic -net user,hostfwd=tcp::8080-:80"
```

### VirtualBox

```bash
make virtualbox
```

### Live USB

```bash
# Write image to USB device (replace sdX with your device):
sudo dd if=build/x86_64/redbear-kde/harddrive.img of=/dev/sdX bs=4M status=progress
```

## Building Specific Packages (Recipes)

### Build a Single Recipe

```bash
# Using the repo tool:
./target/release/repo cook recipes/libs/mesa
./target/release/repo cook recipes/wip/kde/kwin
```

Under the Red Bear overlay model, remember:

- `recipes/*/source/` is a refreshable working tree,
- Red Bear-owned shipping deltas should be preserved under `local/patches/` and `local/recipes/`,
- if a recipe is still upstream WIP, Red Bear may still choose to ship from `local/recipes/` instead.

### Understanding Recipe Format

Each recipe is in `recipes/<category>/<name>/recipe.toml`:

```toml
[source]
git = "https://example.com/repo.git"    # Git source
# tar = "https://example.com/source.tar.gz"  # Or tar source
# branch = "main"                        # Git branch
# rev = "abc123"                         # Or specific commit
# patches = ["redox.patch"]              # Patches to apply

[build]
template = "cargo"   # Build template: cargo, meson, cmake, make, custom
dependencies = [
    "dep1",           # Other recipe names
    "dep2",
]

# For custom builds:
script = """
DYNAMIC_INIT
cookbook_cargo --release
mkdir -p ${COOKBOOK_STAGE}/usr/bin
cp target/release/myapp ${COOKBOOK_STAGE}/usr/bin/
"""
```

### Build Templates

| Template | Description |
|---|---|
| `cargo` | Rust project (cargo build) |
| `meson` | Meson build system |
| `cmake` | CMake build system |
| `make` | GNU Make |
| `custom` | Custom script in `script` field |

## Key Build Variables

| Variable | Default | Description |
|---|---|---|
| `ARCH` | Host arch | Target architecture (x86_64, aarch64, i586, riscv64gc) |
| `CONFIG_NAME` | `redbear-kde` | Build config name |
| `PODMAN_BUILD` | `1` | Use Podman container |
| `PREFIX_BINARY` | `1` | Use prebuilt toolchain (faster) |
| `REPO_BINARY` | `0` | Use prebuilt packages (faster, no compilation) |
| `REPO_NONSTOP` | `0` | Continue on build errors |
| `REPO_OFFLINE` | `0` | Don't update source repos |

### Environment Variables for Recipes

Inside recipe scripts, these are available:

| Variable | Description |
|---|---|
| `COOKBOOK_SOURCE` | Path to extracted source |
| `COOKBOOK_STAGE` | Path to staging directory (install target) |
| `COOKBOOK_SYSROOT` | Path to sysroot with deps |
| `COOKBOOK_TARGET` | Target triple (e.g., x86_64-unknown-redox) |
| `COOKBOOK_CARGO` | Cargo command with correct target |
| `COOKBOOK_MAKE` | Make command with correct flags |

## Troubleshooting

### Toolchain Download Fails

```bash
# Clean and retry:
rm -rf prefix/
make prefix  # Re-download toolchain
```

### Build Errors in Specific Recipes

```bash
# Rebuild a specific recipe:
./target/release/repo cook recipes/<category>/<name>

# Skip failing recipes:
make all REPO_NONSTOP=1
```

### SELinux Issues (Fedora/RHEL)

```bash
make all USE_SELINUX=0
```

### Out of Disk Space

```bash
# Clean everything:
make clean

# Clean only fetched sources:
make distclean
```

## Directory Layout After Build

```
redox-master/
├── build/
│   └── x86_64/
│       └── redbear-kde/
│           ├── harddrive.img      # Bootable disk image
│           ├── redox-live.iso     # Live CD ISO
│           ├── filesystem/        # Mounted filesystem (during build)
│           └── repo.tag           # Build completion marker
├── prefix/
│   └── x86_64-unknown-redox/
│       └── clang-install/         # Cross-compilation toolchain
├── repo/
│   └── *.pkgar                    # Built packages
├── source/
│   └── <recipe-name>/             # Extracted recipe sources
└── target/
    └── release/
        └── repo                   # Build system binary
```
