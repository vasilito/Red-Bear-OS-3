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
├── config/          # Build configs (TOML): tracked redbear-* targets plus mainline references
├── mk/              # Makefile fragments: config.mk, repo.mk, prefix.mk, disk.mk, qemu.mk
├── recipes/         # Package recipes (TOML + source). 26 categories. See recipes/AGENTS.md
│   ├── core/        # kernel, bootloader, relibc, base drivers — See recipes/core/AGENTS.md
│   ├── wip/         # Wayland, KDE, driver WIP ports — See recipes/wip/AGENTS.md
│   ├── libs/        # Libraries: mesa, cairo, SDL, zlib, openssl, etc.
│   ├── gui/         # Legacy GUI stack packages
│   └── ...          # 21 other categories (net, dev, games, shells, etc.)
├── src/             # Cookbook Rust tooling (repo binary, cook logic)
├── docs/            # Architecture docs (6 detailed integration guides) — See docs/AGENTS.md
├── local/           # OUR CUSTOM WORK — survives mainline updates — See local/AGENTS.md
│   ├── config/      # Custom configs (my-amd-desktop.toml)
│   ├── recipes/     # Custom recipes (AMD drivers, GPU stack, Wayland)
│   ├── patches/     # Patches against mainline sources (kernel, relibc, base)
│   ├── Assets/      # Branding assets (icon, loading background)
│   ├── firmware/    # AMD GPU firmware blobs (fetched, not committed)
│   ├── scripts/     # Build/deploy scripts (fetch-firmware.sh, build-redbear.sh)
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
| Wayland integration | `recipes/wip/wayland/` + `local/docs/WAYLAND-IMPLEMENTATION-PLAN.md` | 21 WIP recipes + local validation/ownership plan |
| KDE Plasma path | `recipes/wip/kde/` + `docs/05-KDE-PLASMA-ON-REDOX.md` | 9 WIP KDE app recipes |
| **Desktop path plan** | `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` | **Canonical plan: console → HW-accelerated KDE** |
| Linux driver compat | `docs/04-LINUX-DRIVER-COMPAT.md` | linux-kpi + redox-driver-sys architecture (**GPU and Wi-Fi only — not USB**) |
| Build system internals | `src/bin/repo.rs`, `src/lib.rs`, `mk/repo.mk` | Cookbook tool in Rust |
| Cross-toolchain setup | `mk/prefix.mk`, `prefix/x86_64-unknown-redox/` | Downloads Clang/LLVM toolchain |
| Display/session surface | `config/redbear-full.toml` | Active desktop/graphics compile surface; `redbear-kde` references elsewhere are historical/staging and not supported compile targets |
| GPU/graphics stack | `recipes/libs/mesa/` | OSMesa + LLVMpipe (software only) |
| GPU hardware drivers | `local/recipes/gpu/redox-drm/source/` | AMD + Intel DRM/KMS via redox-driver-sys |
| D-Bus integration | `local/docs/DBUS-INTEGRATION-PLAN.md` | Architecture, gap analysis, phased implementation for KDE Plasma D-Bus |
| Boot config | `config/*.toml` | TOML hierarchy, include-based |
| **Hardware quirks** | `local/recipes/drivers/redox-driver-sys/source/src/quirks/` | Data-driven quirk tables: compiled-in + TOML + DMI; see `local/docs/QUIRKS-SYSTEM.md` |

## BUILD COMMANDS

```bash
# Prerequisites (Linux x86_64 host)
#   rustup + nightly-2025-10-03, cargo install just cbedgen, nasm, qemu-system-x86
#   See docs/06-BUILD-SYSTEM-SETUP.md for distro-specific packages

# Configuration
echo 'PODMAN_BUILD?=0' > .config          # Native build (no container)
echo 'PODMAN_BUILD?=1' > .config          # Podman container build

# Build Red Bear OS
# Supported compile targets: redbear-mini, redbear-live-mini, redbear-full, redbear-live-full
# Desktop/graphics are available only on redbear-full and redbear-live-full.
make all CONFIG_NAME=redbear-full         # Desktop/graphics-enabled Red Bear target → harddrive.img
make live CONFIG_NAME=redbear-live-full   # Desktop/graphics live ISO for real bare metal
make all CONFIG_NAME=redbear-mini         # Minimal non-desktop Red Bear target
make live CONFIG_NAME=redbear-live-mini   # Minimal live ISO for real bare metal
CI=1 make all CONFIG_NAME=redbear-mini    # CI mode (disables TUI, for non-interactive)

# Run
make qemu                                 # Boot in QEMU
make qemu QEMUFLAGS="-m 4G"              # With more RAM
make live                                 # Build live ISO for real bare metal → redbear-live.iso

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

When upstream Redox already provides a package, crate, or subsystem for functionality that also
exists in Red Bear local code, prefer the upstream Redox version by default unless the Red Bear
implementation is materially better. Do not grow lower-quality in-house duplicates as a steady
state.

For quirks and driver support specifically:

- prefer improving and using the canonical `redox-driver-sys` path,
- avoid maintaining separate lower-quality quirk engines when the same functionality belongs in
  `redox-driver-sys`,
- if duplication is temporarily unavoidable, treat it as convergence work to remove, not as a
  permanent design.

### Structure

```
local/patches/
├── kernel/redox.patch              # Applied to kernel source during build (symlinked from recipe)
├── kernel/P0-*.patch               # Individual logical patches (for reference/merge)
├── base/redox.patch                # Applied to base source during build (symlinked from recipe)
├── base/P0-*.patch                 # Individual logical patches
├── relibc/P3-*.patch               # POSIX gap patches (eventfd, signalfd, timerfd, etc.)
├── installer/redox.patch           # Installer ext4 + GRUB bootloader support
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

See `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` for the canonical desktop path plan and `local/docs/AMD-FIRST-INTEGRATION.md` for deeper AMD-specific technical detail.

**Target**: AMD64 bare metal, with AMD and Intel machines treated as equal-priority hardware targets.

**amdgpu is 6M+ lines — 18x larger than Intel i915. LinuxKPI compat approach mandatory.**

### Bare Metal Boot Status

| Component | Status | Detail |
|-----------|--------|--------|
| UEFI boot | ✅ | x86_64 bootloader functional |
| AMD CPUs | ✅ | Ryzen Threadripper 128-thread verified |
| ACPI | ✅ Boot-baseline complete | RSDP/SDT checksums, MADT types 0x4/0x5/0x9/0xA, LVT NMI, FADT shutdown/reboot, explicit `RSDP_ADDR` forwarding into `acpid`, x86 BIOS-search AML fallback, and bounded AML-backed power enumeration are present; the explicit AML bootstrap producer contract and broader robustness still remain open — see `local/docs/ACPI-IMPROVEMENT-PLAN.md` |
| ACPI shutdown | 🚧 | PM1a/PM1b S5 via `\_S5` AML exists, but shutdown robustness and bounded validation are still open |
| ACPI reboot | 🚧 | Reset register + keyboard controller fallback exist, but broader reboot correctness and bounded validation are still open |
| ACPI power | 🚧 | `\_PS0`/`\_PS3`/`\_PPC` AML methods are available and the runtime power surface performs bounded AML-backed enumeration, but bootstrap preconditions and validation are still too weak for stronger support claims; see `local/docs/ACPI-IMPROVEMENT-PLAN.md` |
| x2APIC/SMP | ✅ | Multi-core works |
| IOMMU | 🚧 | QEMU first-use proof now passes; real hardware validation still open |
| AMD GPU | 🚧 | MMIO mapped, bounded Red Bear display glue path builds, MSI-X wired; imported Linux AMD DC/TTM/core remain under compile triage; no hardware validation yet |

### Phased Roadmap (historical P0–P6)

> **Note:** The P0–P6 numbering below is the historical hardware-enablement sequence.
> The canonical current desktop path plan uses a new Phase 1–5 structure documented in
> `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` (v2.0, 2026-04-16).

| Phase | Duration | Delivers |
|-------|----------|----------|
| ~~P0: Fix ACPI for AMD~~ | ~~4-6 weeks~~ | ✅ Materially complete — boots on modern AMD bare metal; see `local/docs/ACPI-IMPROVEMENT-PLAN.md` for forward work |
| ~~P1: Driver infrastructure~~ | ~~8-12 weeks~~ | ✅ Complete — redox-driver-sys + linux-kpi + firmware-loader + pcid /config + MSI-X (compiles) |
| ~~P2: AMD GPU display~~ | ~~12-16 weeks~~ | 🚧 Partial — redox-drm + bounded Red Bear AMD display glue build; imported Linux AMD DC/TTM/core remain under compile triage; Intel driver compiles, no HW validation |
| ~~P3: POSIX + input~~ | ~~4-8 weeks~~ | 🚧 Build-side work substantially complete — the active relibc recipe patch chain now carries the bounded fd-event, semaphore, and waitid compatibility surface needed by current downstreams, while broader runtime validation and input-stack maturity remain open |
| P4: Wayland compositor | 4-6 weeks | 🚧 Partial — libwayland/Qt6 Wayland/Mesa EGL+GBM+GLES2/Qt6 OpenGL now build, but compositor/runtime validation is still incomplete |
| ~~P5: DML2 enablement~~ | ~~partial~~ | 🚧 Historical DML2 config work landed, but the current retained AMDGPU build no longer treats imported DML2/TTM as part of the default bounded compile path; libdrm amdgpu ✅, `iommu` daemon now builds; hardware validation still open |
| P6: KDE Plasma | 12-16 weeks | 🚧 In progress — Qt6 ✅, KF6 32/32 ✅, Mesa EGL/GBM/GLES2 ✅, kf6-kcmutils ✅, kf6-kwayland ✅, kdecoration ✅, KWin 🔄 building |

### Canonical Desktop Path (current plan)

The current execution plan uses a three-track model with new Phase 1–5 numbering:
- **Phase 1:** Runtime Substrate Validation (4–6 weeks)
- **Phase 2:** Wayland Compositor Proof (4–6 weeks)
- **Phase 3:** KWin Desktop Session (6–10 weeks)
- **Phase 4:** KDE Plasma Session (8–12 weeks)
- **Phase 5:** Hardware GPU Enablement (12–20 weeks, parallel with 3–4)

See `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` for full detail.

**Total to software-rendered KDE Plasma**: 22–34 weeks (~6–8 months) with 2 developers.
**Total to hardware-accelerated KDE Plasma**: 34–54 weeks (~8–13 months) with 2 developers.

### Critical Path
```
Phase 1 (runtime substrate) → Phase 2 (software compositor) → Phase 3 (KWin session) → Phase 4 (KDE Plasma)
                               Phase 5 (hardware GPU, parallel with Phases 3–4)
```

### Custom Crates (P1/P2)
1. `redox-driver-sys` — `local/recipes/drivers/redox-driver-sys/source/` — Safe Rust wrappers for scheme:memory, scheme:irq, scheme:pci + hardware quirks system (`src/quirks/`)
2. `linux-kpi` — `local/recipes/drivers/linux-kpi/source/` — C headers translating Linux kernel APIs → redox-driver-sys; includes `pci_get_quirk_flags()` C FFI for quirk queries. **GPU and Wi-Fi drivers only — linux-kpi does NOT cover USB.** It provides PCI, DMA, IRQ, DRM, networking (ieee80211/nl80211/mac80211), firmware, and related kernel infrastructure headers, but contains zero USB headers, USB device ID tables, or USB driver implementations.
3. `redox-drm` — `local/recipes/gpu/redox-drm/source/` — DRM scheme daemon (AMD + Intel drivers); consumes quirk flags for MSI/MSI-X fallback and DISABLE_ACCEL
4. `firmware-loader` — `local/recipes/system/firmware-loader/source/` — scheme:firmware for GPU blobs
5. `amdgpu` — `local/recipes/gpu/amdgpu/source/` — AMD DC C port with linux-kpi compat; can query quirks via `pci_has_quirk()` FFI
6. `redbear-sessiond` — `local/recipes/system/redbear-sessiond/source/` — Rust D-Bus session broker exposing `org.freedesktop.login1` subset for KWin (uses `zbus`)
7. `redbear-dbus-services` — `local/recipes/system/redbear-dbus-services/` — D-Bus activation `.service` files and XML policy files for system and session buses

All custom work goes in `local/` — see `local/AGENTS.md` for overlay usage.

## NOTES

- Build requires Linux x86_64 host, 8GB+ RAM, 20GB+ disk
- QEMU used for testing (make qemu). VirtualBox also supported
- The `repo` binary (cookbook CLI) may crash with TUI in non-interactive environments — use `CI=1`
- No git submodules — external repos managed via recipe source URLs and repo manifests
- Historical integration report removed (2026-04-16); see `local/docs/DESKTOP-STACK-CURRENT-STATUS.md` for current state

## SUBSYSTEM PRIORITY AND ORDER

Red Bear OS should treat low-level controllers, USB, Wi-Fi, and Bluetooth as first-class subsystem
targets.

For PCI interrupt plumbing, IRQ delivery quality, MSI/MSI-X follow-up, low-level controller
runtime-proof sequencing, and IOMMU/interrupt-remapping quality, the canonical current plan is:

- `local/docs/IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md`

Use that file as the execution authority and current robustness judgment for PCI/IRQ work. Higher-
level summaries in `README.md`, `docs/README.md`, and this file should stay aligned with its
validation language rather than acting as competing rollout plans.

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
