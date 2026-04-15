# RED BEAR OS BUILD SYSTEM — PROJECT KNOWLEDGE BASE

**Generated:** 2026-04-12 (P1/P2 complete)
**Toolchain:** Rust nightly-2025-10-03 (edition 2024)
**Architecture:** Microkernel OS in Rust, ~38k files, ~294k LoC Rust
**Target Hardware**: AMD64 bare metal, with AMD and Intel machines treated as equal-priority Red Bear OS targets

## OVERVIEW

Red Bear OS build system orchestrator — fetches, builds, and packages ~100+ Git repositories
into a bootable Redox image. Uses a Makefile + Rust "cookbook" tool + TOML configs.
Languages: Rust (core), C (ported packages), TOML (config), Make (build orchestration).

RedBearOS should be treated as an overlay distribution on top of Redox in the same way Ubuntu
relates to Debian:

- Redox is upstream
- Red Bear carries integration, packaging, validation, and subsystem overlays on top
- upstream-owned source trees are refreshable working copies
- durable Red Bear state belongs in `local/patches/`, `local/recipes/`, `local/docs/`, and tracked
  Red Bear configs

If we can fetch refreshed upstream sources, reapply our overlays, and rebuild successfully, the
project is in the right shape for long-term maintenance.

## STRUCTURE

```
redox-master/
├── config/          # Build configs (TOML): desktop, server, wayland, x11, minimal
├── mk/              # Makefile fragments: config.mk, repo.mk, prefix.mk, disk.mk, qemu.mk
├── recipes/         # Package recipes (TOML + source). 26 categories. See recipes/AGENTS.md
│   ├── core/        # kernel, bootloader, relibc, base drivers — See recipes/core/AGENTS.md
│   ├── wip/         # Wayland, KDE, driver WIP ports — See recipes/wip/AGENTS.md
│   ├── libs/        # Libraries: mesa, cairo, SDL, zlib, openssl, etc.
│   ├── gui/         # Orbital, orbterm, orbutils
│   └── ...          # 21 other categories (net, dev, games, shells, etc.)
├── src/             # Cookbook Rust tooling (repo binary, cook logic)
├── docs/            # Architecture docs (6 detailed integration guides) — See docs/AGENTS.md
├── local/           # OUR CUSTOM WORK — survives mainline updates — See local/AGENTS.md
│   ├── config/      # Custom configs (my-amd-desktop.toml)
│   ├── recipes/     # Custom recipes (AMD drivers, GPU stack, Wayland)
│   ├── patches/     # Patches against mainline sources (kernel, relibc, base)
│   ├── Assets/      # Branding assets (icon, loading background)
│   ├── firmware/    # AMD GPU firmware blobs (fetched, not committed)
│   ├── scripts/     # Build/deploy scripts (fetch-firmware.sh, build-amd.sh)
│   └── docs/        # Red Bear integration docs (AMD roadmap, Wi-Fi/Bluetooth plans, status notes)
├── prefix/          # Cross-compiler toolchain (Clang/LLVM for x86_64-unknown-redox)
├── build/           # Build outputs, logs, fstools, per-arch directories
├── repo/            # Package manifests and PKGAR artifacts per architecture
├── bin/             # Cross-tool wrappers (pkg-config, llvm-config per target)
├── scripts/         # Helper scripts (backtrace, category, changelog, etc.)
├── podman/          # Podman container build support
├── .cargo/          # Cargo config: linker per target (aarch64, x86_64, i586, i686, riscv64gc)
├── Makefile         # Root orchestrator (all, live, image, rebuild, clean, qemu, gdb)
├── Cargo.toml       # Cookbook crate: binaries (repo, repo_builder), lib (cookbook)
├── rust-toolchain.toml  # nightly-2025-10-03 + rust-src + rustfmt + clippy
└── .config          # PODMAN_BUILD=0 (set to 1 for container builds)
```

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Add a package | `recipes/<category>/<name>/recipe.toml` | Use `template = "cargo\|cmake\|meson\|custom"` |
| Change build config | `config/<name>.toml` | Include chain: wayland→desktop→desktop-minimal→minimal→base |
| Fix kernel | `recipes/core/kernel/source/` | Kernel is a recipe, not top-level |
| Fix a driver | `recipes/core/base/source/drivers/` | All drivers are userspace daemons |
| Fix relibc (POSIX) | `recipes/core/relibc/source/` | C library written in Rust |
| Wayland integration | `recipes/wip/wayland/` + `docs/03-WAYLAND-ON-REDOX.md` | 21 WIP recipes |
| KDE Plasma path | `recipes/wip/kde/` + `docs/05-KDE-PLASMA-ON-REDOX.md` | 9 WIP KDE app recipes |
| Linux driver compat | `docs/04-LINUX-DRIVER-COMPAT.md` | linux-kpi + redox-driver-sys architecture |
| Build system internals | `src/bin/repo.rs`, `src/lib.rs`, `mk/repo.mk` | Cookbook tool in Rust |
| Cross-toolchain setup | `mk/prefix.mk`, `prefix/x86_64-unknown-redox/` | Downloads Clang/LLVM toolchain |
| Display server | Orbital: `recipes/gui/orbital/` | Userspace scheme-based display server |
| GPU/graphics stack | `recipes/libs/mesa/` | OSMesa + LLVMpipe (software only) |
| GPU hardware drivers | `local/recipes/gpu/redox-drm/source/` | AMD + Intel DRM/KMS via redox-driver-sys |
| Boot config | `config/*.toml` | TOML hierarchy, include-based |

## BUILD COMMANDS

```bash
# Prerequisites (Linux x86_64 host)
#   rustup + nightly-2025-10-03, cargo install just cbedgen, nasm, qemu-system-x86
#   See docs/06-BUILD-SYSTEM-SETUP.md for distro-specific packages

# Configuration
echo 'PODMAN_BUILD?=0' > .config          # Native build (no container)
echo 'PODMAN_BUILD?=1' > .config          # Podman container build

# Build Red Bear OS
make all                                  # Build desktop config → harddrive.img
make all CONFIG_NAME=redbear-full         # Full Red Bear OS desktop + custom drivers
make all CONFIG_NAME=redbear-minimal      # Minimal Red Bear OS server
CI=1 make all CONFIG_NAME=redbear-minimal # CI mode (disables TUI, for non-interactive)

# Run
make qemu                                 # Boot in QEMU
make qemu QEMUFLAGS="-m 4G"              # With more RAM
make live                                 # Build live ISO → redbear-live.iso

# Single recipe
./target/release/repo cook recipes/libs/mesa     # Build one recipe
./target/release/repo fetch recipes/core/kernel   # Fetch source only
make r.mesa                                      # Make shorthand for cook
make cr.mesa                                     # Clean + rebuild

# Clean
make clean                                # Remove build artifacts
make distclean                            # Remove sources + artifacts
```

## BUILD FLOW

```
make all
  → mk/config.mk (ARCH, CONFIG_NAME, FILESYSTEM_CONFIG)
  → mk/depends.mk (check host tools: rustup, cbedgen, nasm, just)
  → mk/prefix.mk (download/setup cross-toolchain if needed)
  → mk/fstools.mk (build cookbook repo binary + fstools)
  → mk/repo.mk (repo cook --filesystem=config/*.toml)
    → For each recipe: fetch source → apply patches → build → stage into sysroot
  → mk/disk.mk (create filesystem.img, harddrive.img, redbear-live.iso)
    → redoxfs-mkfs → redox_installer → bootloader embedding
```

## CONVENTIONS

- **Rust edition 2024**, nightly channel
- **rustfmt.toml**: max_width=100, brace_style=SameLineWhere
- **clippy.toml**: cognitive-complexity-threshold=100, type-complexity-threshold=1000
- **Recipe format**: TOML with `[source]` + `[build]` + optional `[package]`
- **Build templates**: `cargo`, `meson`, `cmake`, `make`, `configure`, `custom`
- **WIP recipes**: Must start with `#TODO` comment explaining what's missing
- **Custom configs**: Name with `my-` prefix (git-ignored by convention)
- **CI**: GitLab CI (`.gitlab-ci.yml`) at root + per-recipe; some have GitHub Actions
- **Syscall ABI**: Unstable intentionally. Stability via `libredox` and `relibc`
- **Drivers**: ALL userspace daemons via scheme system. No kernel-space drivers (except serio)

## ANTI-PATTERNS (THIS PROJECT)

- **DO NOT** suppress errors with `as any` / `@ts-ignore` — use proper `Result` handling
- **DO NOT** use `unwrap()` / `expect()` in library/driver code — pervasive anti-pattern (~14k instances)
- **DO NOT** modify kernel syscall ABI directly — use `libredox` or `relibc`
- **DO NOT** put drivers in kernel space — all drivers are userspace daemons
- **DO NOT** hardcode `/dev/` paths — use scheme paths (`/scheme/drm/card0`)
- **DO NOT** skip patches in WIP recipes — document what's missing with `#TODO`

## PATCH MANAGEMENT

All Red Bear OS modifications to upstream files are kept separately in `local/patches/`.

This is not just a convenience rule; it is a long-term maintenance rule. For fast-moving upstream
areas like relibc, prefer the upstream solution whenever upstream already solves the same problem.
Keep Red Bear patch carriers only for gaps or compatibility work that upstream still does not solve
adequately.

### Structure

```
local/patches/
├── kernel/redox.patch              # Applied to kernel source during build (symlinked from recipe)
├── kernel/P0-*.patch               # Individual logical patches (for reference/merge)
├── base/redox.patch                # Applied to base source during build (symlinked from recipe)
├── base/P0-*.patch                 # Individual logical patches
├── relibc/P3-*.patch               # POSIX gap patches (eventfd, signalfd, timerfd, etc.)
├── installer/redox.patch           # Installer ext4 support
└── build-system/
    ├── 001-rebrand-and-build.patch # Makefile, mk/*, scripts, build.sh rebranding
    ├── 002-cookbook-fixes.patch    # src/ Rust fixes (fetch.rs, staged_pkg.rs, repo.rs, html.rs)
    ├── 003-config.patch            # config/*.toml changes (os-release, hostname, redbear-full)
    └── 004-docs-and-cleanup.patch  # README, CONTRIBUTING, LICENSE, deleted upstream files
```

### Protection Mechanism

1. **Recipe patches** (`kernel/redox.patch`, `base/redox.patch`): Canonical copy lives in
   `local/patches/`. The recipe directory contains a **symlink** to it:
   ```
   recipes/core/kernel/redox.patch → ../../../local/patches/kernel/redox.patch
   recipes/core/base/redox.patch   → ../../../local/patches/base/redox.patch
   ```
   The build system follows symlinks transparently. Patches are never touched by `make clean`
   or `make distclean`. Only `local/` modifications affect them.

2. **Build-system patches**: Generated via `git diff` against the upstream base commit.
   These serve as a backup — the working tree already has patches applied (via git commits).
   If upstream update via rebase fails, these can be applied from scratch.

3. **Custom recipes**: Live entirely in `local/recipes/` with symlinks into `recipes/`:
   ```
   recipes/drivers/linux-kpi       → ../../local/recipes/drivers/linux-kpi
   recipes/gpu/amdgpu              → ../../local/recipes/gpu/amdgpu
   recipes/system/firmware-loader  → ../../local/recipes/system/firmware-loader
   ... etc
   ```

### Scripts

| Script | Purpose |
|--------|---------|
| `local/scripts/apply-patches.sh` | Apply all build-system patches + create recipe symlinks |
| `local/scripts/sync-upstream.sh` | Fetch upstream + rebase Red Bear OS commits + verify symlinks |

### Updating from Upstream

```bash
# Automated (preferred):
./local/scripts/sync-upstream.sh              # Rebase Red Bear OS onto latest upstream
./local/scripts/sync-upstream.sh --dry-run    # Preview conflicts first

# Manual:
git remote add upstream-redox https://github.com/redox-os/redox.git  # once
git fetch upstream-redox master
git rebase upstream-redox/master             # replays Red Bear OS commits on new upstream

# Nuclear option (if rebase fails badly):
git rebase --abort
git reset --hard upstream-redox/master
./local/scripts/apply-patches.sh --force     # apply from scratch via patch files
```

## AMD-FIRST INTEGRATION PATH

See `local/docs/AMD-FIRST-INTEGRATION.md` for the full plan.

**Target**: AMD64 bare metal, with AMD and Intel machines treated as equal-priority hardware targets.

**amdgpu is 6M+ lines — 18x larger than Intel i915. LinuxKPI compat approach mandatory.**

### Bare Metal Boot Status

| Component | Status | Detail |
|-----------|--------|--------|
| UEFI boot | ✅ | x86_64 bootloader functional |
| AMD CPUs | ✅ | Ryzen Threadripper 128-thread verified |
| ACPI | ✅ Complete | RSDP/SDT checksums, MADT types 0x4/0x5/0x9/0xA, LVT NMI, FADT shutdown/reboot |
| ACPI shutdown | ✅ | PM1a/PM1b S5 via `\_S5` AML |
| ACPI reboot | ✅ | Reset register + keyboard controller fallback |
| ACPI power | ✅ | `\_PS0`/`\_PS3`/`\_PPC` AML methods available |
| x2APIC/SMP | ✅ | Multi-core works |
| IOMMU | ❌ | No AMD-Vi support |
| AMD GPU | 🚧 | MMIO mapped, DC port compiles, MSI-X wired, no hardware validation yet |

### Phased Roadmap

| Phase | Duration | Delivers |
|-------|----------|----------|
| ~~P0: Fix ACPI for AMD~~ | ~~4-6 weeks~~ | ✅ Complete — boots on modern AMD bare metal |
| ~~P1: Driver infrastructure~~ | ~~8-12 weeks~~ | ✅ Complete — redox-driver-sys + linux-kpi + firmware-loader + pcid /config + MSI-X (compiles) |
| ~~P2: AMD GPU display~~ | ~~12-16 weeks~~ | ✅ Complete — redox-drm + AMD DC port + Intel driver (compiles, no HW validation) |
| ~~P3: POSIX + input~~ | ~~4-8 weeks~~ | 🚧 Build-side work substantially complete — relibc gaps exported to downstream consumers, evdevd/udev-shim/libevdev/libinput/D-Bus build; runtime validation still open |
| P4: Wayland compositor | 4-6 weeks | 🚧 Partial — libwayland/Qt6 Wayland/Mesa EGL+GBM+GLES2/Qt6 OpenGL now build, but compositor/runtime validation is still incomplete |
| ~~P5: DML2 enablement~~ | ~~partial~~ | 🚧 DML2 config enabled, 63 DML source files in build, TTM compiled, libdrm amdgpu ✅, `iommu` daemon now builds; hardware validation still open |
| P6: KDE Plasma | 12-16 weeks | 🚧 In progress — Qt6 ✅, KF6 32/32 ✅, Mesa EGL/GBM/GLES2 ✅, kf6-kcmutils ✅, kf6-kwayland ✅, kdecoration ✅, KWin 🔄 building |

**Total to KDE Plasma on AMD**: ~48 weeks (~11 months) with 2 developers (P0-P2 complete; P3/P4 build-side substantially advanced, runtime still open).

### Critical Path
```
P0 (ACPI boot) ✅ → P1 (driver infra) ✅ → P2 (AMD display) ✅ → P3 (POSIX+input, build-side) 🚧 → P4 (Wayland runtime) 🚧 → P6 (KDE)
                                                                                    P5 (full amdgpu, parallel)
```

### Custom Crates (P1/P2)
1. `redox-driver-sys` — `local/recipes/drivers/redox-driver-sys/source/` — Safe Rust wrappers for scheme:memory, scheme:irq, scheme:pci
2. `linux-kpi` — `local/recipes/drivers/linux-kpi/source/` — C headers translating Linux kernel APIs → redox-driver-sys
3. `redox-drm` — `local/recipes/gpu/redox-drm/source/` — DRM scheme daemon (AMD + Intel drivers)
4. `firmware-loader` — `local/recipes/system/firmware-loader/source/` — scheme:firmware for GPU blobs
5. `amdgpu` — `local/recipes/gpu/amdgpu/source/` — AMD DC C port with linux-kpi compat

All custom work goes in `local/` — see `local/AGENTS.md` for overlay usage.

## NOTES

- Build requires Linux x86_64 host, 8GB+ RAM, 20GB+ disk
- QEMU used for testing (make qemu). VirtualBox also supported
- The `repo` binary (cookbook CLI) may crash with TUI in non-interactive environments — use `CI=1`
- No git submodules — external repos managed via recipe source URLs and repo manifests
- File `INTEGRATION_REPORT.md` contains detailed integration status from a previous analysis

## SUBSYSTEM PRIORITY AND ORDER

Red Bear OS should treat low-level controllers, USB, Wi-Fi, and Bluetooth as first-class subsystem
targets.

Current execution order:

1. low-level controllers / IRQ quality / runtime-proof
2. USB controller and topology maturity
3. Wi-Fi native control-plane and one bounded driver path
4. Bluetooth host/controller path
5. desktop/session compatibility layers on top of those runtime services

Current blocker emphasis:

- low-level controller quality blocks reliable USB and Wi-Fi validation
- USB maturity blocks the realistic first Bluetooth transport path
- Wi-Fi and Bluetooth should not be treated as optional polish; both remain missing subsystem work
  that must be implemented fully, but in the right order
