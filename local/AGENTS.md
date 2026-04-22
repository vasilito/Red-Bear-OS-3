# RED BEAR OS — DERIVATIVE OF REDOX OS

This directory contains ALL custom work on top of mainline Redox. When mainline Redox
updates (`git pull` on the build system repo), this directory is untouched.

## DESIGN PRINCIPLE

Red Bear OS relates to Redox OS in the same way Ubuntu relates to Debian:
  - We track Redox OS as upstream, merging changes regularly
  - We add custom packages, drivers, configs, and branding on top
  - The `local/` directory is our overlay — untouched by upstream updates
  - First-class configs use `redbear-*` naming (not `my-*`, which is gitignored)

## FREE/LIBRE SOFTWARE POLICY

Red Bear OS must remain a free/libre project.

- Prefer components that are open-source, freely available to all users, or built in-tree by Red Bear.
- Do not introduce proprietary, source-unavailable, paywalled, or redistributability-restricted dependencies into the tracked system surface.
- When a dependency is dual-licensed under multiple free/open licenses, choose and document the option that is compatible with the Red Bear project surface.
- For the greeter/login stack specifically, the current SHA-crypt verifier path is the pure-Rust `sha-crypt` crate, licensed `MIT OR Apache-2.0`; Red Bear treats it under the MIT option for compatibility with the project's free-software policy.

Build flow:
```
make all CONFIG_NAME=redbear-full
  → mk/config.mk resolves to the active desktop/graphics compile target
  → Desktop/graphics are available only on redbear-full and redbear-live-full
  → repo cook builds all packages including our custom ones
  → mk/disk.mk creates harddrive.img with Red Bear branding
```

Update flow:
```
./local/scripts/sync-upstream.sh          # Rebase onto upstream Redox + verify symlinks
make all CONFIG_NAME=redbear-full         # Rebuild the active desktop/graphics target
```

## ACTIVE COMPILE TARGETS

The supported compile targets are exactly:

- `redbear-mini`
- `redbear-live-mini`
- `redbear-full`
- `redbear-live-full`

Desktop/graphics are available only on `redbear-full` and `redbear-live-full`.

Names such as `redbear-kde`, `redbear-wayland`, and `redbear-minimal` may still appear in older
docs, legacy validation notes, or in-repo staging configs, but they should not be treated as the
current supported compile targets.

## TRACKING UPSTREAM (SYNC WITH REDOX OS)

Red Bear OS tracks the Redox OS build system as upstream. The `local/` directory
survives upstream updates untouched.

## SOURCE-OF-TRUTH RULE (VERY IMPORTANT)

Treat the repository as two different layers with different durability guarantees:

### 1. Upstream-owned layer — disposable, refreshable every day

These paths are expected to be replaced, refetched, or regenerated when upstream changes:

- `recipes/*/source/`
- most of `recipes/` outside our symlinked `local/recipes/*` overlays
- `config/desktop.toml`, `config/minimal.toml`, and other mainline configs
- generated build outputs under `target/`, `build/`, `repo/`, and recipe-local `target/*`

For relibc specifically, **`recipes/core/relibc/source/` is upstream-owned working source**, not
Red Bear’s durable storage location. We may build and validate there, but we must not rely on that
tree alone to preserve Red Bear work.

### 2. Red Bear-owned layer — durable, must survive upstream refresh

These paths are our actual long-term source of truth:

- `local/patches/` — all durable changes to upstream-owned source trees
- `local/recipes/` — Red Bear recipe overlays and new packages
- `local/docs/` — Red Bear planning, validation, and integration documentation
- tracked Red Bear configs such as `config/redbear-*.toml`

If we can fetch fresh upstream sources tomorrow, reapply `local/patches/*`, relink
`local/recipes/*`, and rebuild successfully, then the work is in the right place.

If a change exists only inside an upstream-owned `recipes/*/source/` tree, then it is **not yet
preserved**, even if the current build happens to pass.

### Upstream-first rule for fast-moving components

Some components, especially relibc, are actively evolving upstream. For those areas, Red Bear must
prefer the upstream solution whenever upstream already solves the same problem.

That means:

- if our local patch solves a gap that upstream still has, keep the patch carrier
- if upstream lands an equivalent or better solution, prefer upstream and shrink or drop our local patch
- do not keep a Red Bear patch just because it existed first; keep it only while it still provides unique value

For relibc specifically, patch carriers should be treated as **temporary compatibility overlays**,
not a permanent fork strategy.

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

### Daily-upstream-safe workflow

For any change to upstream-owned source:

1. make the minimal working change in the live source tree if needed for validation
2. prove it builds/tests against the real recipe
3. mirror that delta into `local/patches/<component>/...`
4. update `local/docs/...` so the rebuild/reapply story is explicit
5. assume the live upstream source tree may be thrown away and recreated at any time

The success criterion is therefore:

> We can pull renewed upstream sources every day, reapply Red Bear’s local overlays, and still
> build the project successfully.

### Local recipe priority vs upstream WIP

When Red Bear maintains a local recipe and upstream contains a package with the same name under
`recipes/wip/*`, Red Bear must prefer the local recipe unconditionally.

- Use the local overlay symlink in `recipes/*/<name> -> ../../local/recipes/...`
- Do not switch back to upstream WIP for active Red Bear builds
- Re-evaluate only when upstream package exits WIP and becomes a normal maintained package

```bash
# Automated sync (preferred):
./local/scripts/sync-upstream.sh              # Fetch + rebase + check patches
./local/scripts/sync-upstream.sh --dry-run    # Preview conflicts before rebasing
./local/scripts/sync-upstream.sh --no-merge   # Only check for patch conflicts

# Manual sync:
git remote add upstream-redox https://github.com/redox-os/redox.git  # First time only
git fetch upstream-redox master
git rebase upstream-redox/master

# If rebase fails (nuclear option):
git rebase --abort
git reset --hard upstream-redox/master
./local/scripts/apply-patches.sh --force     # Rebuild Red Bear OS changes from patch files

# After sync:
cargo build --release                         # Rebuild cookbook
make all CONFIG_NAME=redbear-full             # Rebuild OS
```

## STRUCTURE

```
redox-master/                  ← git pull updates mainline Redox
├── config/
│   ├── desktop.toml           ← mainline configs (untouched)
│   ├── minimal.toml
│   ├── redbear-full.toml      ← Active desktop/graphics target
│   ├── redbear-live-full.toml ← Live desktop/graphics target
│   ├── redbear-mini*.toml     ← Minimal target surface (legacy/staging naming may still vary in-tree)
│   └── redbear-greeter-services.toml ← Greeter/auth/session-launch wiring fragment
├── recipes/                   ← mainline package recipes (untouched)
├── mk/                        ← mainline build system (untouched)
├── local/                     ← RED BEAR OS custom work
│   ├── AGENTS.md              ← This file
│   ├── config/                ← Legacy configs (my-*, gitignored)
│   ├── recipes/
│   │   ├── core/              ← ext4d (ext4 filesystem scheme daemon + mkfs tool), grub (GRUB 2.12 UEFI bootloader)
│   │   ├── branding/          ← redbear-release (os-release, hostname, motd)
│   │   ├── drivers/           ← redox-driver-sys, linux-kpi (GPU/Wi-Fi compat only — NOT USB)
│   │   ├── gpu/               ← redox-drm (AMD + Intel display drivers), amdgpu (C port)
│   │   ├── system/            ← cub, evdevd, udev-shim, redbear-firmware, firmware-loader, redbear-hwutils, redbear-info, redbear-netctl, redbear-quirks, redbear-meta
│   │   │   ├── redbear-sessiond       ← org.freedesktop.login1 D-Bus session broker (zbus-based Rust daemon)
│   │   │   ├── redbear-authd          ← local-user authentication daemon (`/etc/passwd` + `/etc/shadow` + `/etc/group`)
│   │   │   ├── redbear-session-launch ← session bootstrap helper (uid/gid/env/runtime-dir handoff)
│   │   │   ├── redbear-greeter        ← greeter orchestrator package (`redbear-greeterd`, UI, compositor wrapper, staged assets)
│   │   │   ├── redbear-dbus-services  ← D-Bus .service activation files + XML policies
│   │   ├── wayland/           ← Wayland compositor (v2.0 Phase 2)
│   │   └── kde/               ← KDE Plasma (v2.0 Phases 3–4)
│   ├── patches/
│   │   ├── kernel/            ← Kernel patches (ACPI, x2APIC)
│   │   ├── base/              ← Base patches (acpid fixes, power methods, pcid /config endpoint)
│   │   ├── relibc/            ← relibc compatibility overlays still needed beyond upstream (eventfd, signalfd, timerfd, waitid, SysV IPC)
│   │   ├── bootloader/        ← Bootloader patches
│   │   └── installer/         ← Installer patches (ext4 filesystem support + GRUB bootloader)
│   ├── Assets/                ← Branding assets (icon, loading background)
│   │   └── images/            ← Red Bear OS icon (1254x1254) + loading bg (1536x1024)
│   ├── firmware/              ← GPU firmware blobs (gitignored, fetched)
│   ├── scripts/
│   │   ├── sync-upstream.sh   ← Sync with upstream Redox OS
│   │   ├── build-redbear.sh   ← Unified Red Bear OS build script
│   │   ├── fetch-firmware.sh  ← Download bounded AMD or Intel firmware subsets from linux-firmware
│   │   ├── test-drm-display-runtime.sh ← Shared bounded DRM/KMS display validation harness
│   │   ├── test-amd-gpu.sh    ← AMD wrapper for the DRM display validation harness
│   │   ├── test-intel-gpu.sh  ← Intel wrapper for the DRM display validation harness
│   │   ├── test-baremetal.sh  ← Bare metal test script
│   │   ├── build-redbear-wifictl-redox.sh ← Build redbear-wifictl for the Redox target with the repo toolchain
│   │   ├── test-iwlwifi-driver-runtime.sh ← Bounded Intel driver lifecycle check inside a target runtime
│   │   ├── test-wifi-control-runtime.sh ← Bounded Wi-Fi control/profile runtime check inside a target runtime
│   │   ├── test-wifi-baremetal-runtime.sh ← Strongest in-repo Wi-Fi runtime check on a real Red Bear target
│   │   ├── validate-wifi-vfio-host.sh ← Host-side VFIO passthrough readiness check for Intel Wi-Fi validation
│   │   ├── prepare-wifi-vfio.sh ← Bind/unbind Intel Wi-Fi PCI function for VFIO validation
│   │   ├── test-wifi-passthrough-qemu.sh ← QEMU/VFIO Wi-Fi validation harness with in-guest checks
│   │   ├── run-wifi-passthrough-validation.sh ← One-shot host wrapper for the full Wi-Fi passthrough validation flow
│   │   ├── package-wifi-validation-artifacts.sh ← Package Wi-Fi validation artifacts into one host-side tarball
│   │   ├── summarize-wifi-validation-artifacts.sh ← Summarize captured Wi-Fi validation artifacts for quick triage
│   │   ├── finalize-wifi-validation-run.sh ← Analyze a Wi-Fi capture bundle and package the final evidence set
│   │   ├── validate-vm-network-baseline.sh ← Static repo-level VM networking baseline check
│   │   ├── test-vm-network-qemu.sh ← QEMU launcher for the VirtIO VM networking baseline
│   │   ├── test-vm-network-runtime.sh ← In-guest runtime check for the VM networking baseline
│   │   ├── test-ps2-qemu.sh ← QEMU launcher for the bounded PS/2 + serio runtime proof
│   │   ├── test-timer-qemu.sh ← QEMU launcher for the bounded monotonic timer runtime proof
│   │   ├── test-lowlevel-controllers-qemu.sh ← Sequential wrapper for bounded low-level controller proofs
│   │   ├── test-usb-maturity-qemu.sh ← Sequential wrapper for bounded USB maturity proofs
│   │   └── test-greeter-qemu.sh ← Bounded QEMU proof for the Red Bear greeter/auth/session surface
│   └── docs/                  ← Integration docs
```

## HOW TO BUILD RED BEAR OS

```bash
# Active desktop/graphics target
./local/scripts/build-redbear.sh redbear-full

# Minimal non-desktop target
./local/scripts/build-redbear.sh redbear-mini

# Live images
./local/scripts/build-redbear.sh redbear-live-full && make live CONFIG_NAME=redbear-live-full
./local/scripts/build-redbear.sh redbear-live-mini && make live CONFIG_NAME=redbear-live-mini

# VM-network baseline validation helpers
./local/scripts/validate-vm-network-baseline.sh
./local/scripts/test-vm-network-qemu.sh redbear-mini
# Then run inside the guest:
#   ./local/scripts/test-vm-network-runtime.sh

# Phase 1 desktop-substrate validation (v2.0 plan: relibc headers, evdevd, udev-shim,
# firmware-loader, DRM/KMS, health-check — covers 6 acceptance areas)
./local/scripts/test-phase1-desktop-substrate.sh --qemu redbear-wayland

# Legacy Phase 3 runtime-substrate validation (historical P0-P6 numbering; script still works)
# Use the active desktop target when adapting historical validation flows.
./local/scripts/test-phase3-runtime-substrate.sh --qemu redbear-full

# Low-level controller validation
./local/scripts/test-xhci-irq-qemu.sh --check
./local/scripts/test-msix-qemu.sh
./local/scripts/test-iommu-qemu.sh
./local/scripts/test-ps2-qemu.sh --check
./local/scripts/test-timer-qemu.sh --check
./local/scripts/test-lowlevel-controllers-qemu.sh
./local/scripts/test-usb-storage-qemu.sh
./local/scripts/test-usb-qemu.sh --check
./local/scripts/test-usb-maturity-qemu.sh

# The current xHCI proof checks for an interrupt-driven mode in boot logs.
# The current MSI-X proof uses the live virtio-net path in QEMU.
# The current IOMMU proof runs a guest-driven first-use self-test and checks that discovered
# AMD-Vi units initialize and drain events successfully in QEMU.
# The current PS/2 proof checks serio node visibility and then hands off to the existing Phase 3
# input-path checker inside the guest.
# The current timer proof checks that /scheme/time/CLOCK_MONOTONIC advances across two guest reads.
# The aggregate low-level wrapper runs xHCI, IOMMU, PS/2, and timer proofs in sequence.
# The USB storage proof now verifies usbscsid autospawn plus bounded sector-0 readback against a
# host-seeded pattern, while guest-side write verification is still open.
# The aggregate USB wrapper runs xHCI mode, full USB stack, and USB storage readback proofs in sequence.

# Legacy Phase 4 Wayland runtime validation (historical P0-P6 numbering; script still works)
./local/scripts/build-redbear.sh redbear-wayland
./local/scripts/test-phase4-wayland-qemu.sh
# Then run inside the guest:
#   redbear-phase4-wayland-check

# Legacy Phase 5 desktop/network plumbing validation (historical P0-P6 numbering; script still works)
./local/scripts/build-redbear.sh redbear-full
./local/scripts/test-phase5-network-qemu.sh --check
# Then run inside the guest:
#   redbear-phase5-network-check

# Experimental Red Bear greeter/login validation
./local/scripts/build-redbear.sh redbear-full
./local/scripts/test-greeter-qemu.sh --check
# Then run inside the guest:
#   redbear-greeter-check
#   redbear-greeter-check --invalid root wrong

# Bounded Intel Wi-Fi runtime validation (real target or passthrough guest)
# Host preparation for VFIO-backed guests:
#   sudo ./local/scripts/validate-wifi-vfio-host.sh --host-pci 0000:xx:yy.z --expect-driver iwlwifi
#   sudo ./local/scripts/prepare-wifi-vfio.sh bind 0000:xx:yy.z
# Guest/target packaged checks:
#   redbear-phase5-wifi-check
#   redbear-phase5-wifi-link-check
#   redbear-phase5-wifi-run wifi-open-bounded wlan0 /tmp/redbear-phase5-wifi-capture.json
#   redbear-phase5-wifi-capture wifi-open-bounded wlan0 /tmp/redbear-phase5-wifi-capture.json
#   redbear-phase5-wifi-analyze /tmp/redbear-phase5-wifi-capture.json
# Helper scripts:
#   ./local/scripts/test-wifi-baremetal-runtime.sh
#   ./local/scripts/test-wifi-passthrough-qemu.sh --host-pci 0000:xx:yy.z --check --capture-output ./wifi-passthrough-capture.json
#   ./local/scripts/finalize-wifi-validation-run.sh ./wifi-passthrough-capture.json ./wifi-passthrough-artifacts.tar.gz

# Legacy Phase 6 KDE session-surface validation (historical P0-P6 numbering; script still works)
./local/scripts/build-redbear.sh redbear-full
./local/scripts/test-phase6-kde-qemu.sh --check
# Then run inside the guest:
#   redbear-phase6-kde-check

# redbear-netctl user-facing alias
redbear-netctl --help

# Or manually:
make all CONFIG_NAME=redbear-full

# Single custom recipe:
./target/release/repo cook local/recipes/branding/redbear-release
./target/release/repo cook local/recipes/system/redbear-meta
./target/release/repo cook local/recipes/core/ext4d
./target/release/repo cook local/recipes/core/grub  # GRUB bootloader (host build, produces EFI binary)

# GRUB boot manager (installer-native, Phase 2):
make r.grub                                                   # Build GRUB recipe
make all CONFIG_NAME=redbear-full-grub                        # Build with GRUB chainload
# Linux-compatible CLI (add local/scripts to PATH):
grub-install --target=x86_64-efi --disk-image=build/x86_64/harddrive.img
grub-mkconfig -o local/recipes/core/grub/grub.cfg
# Or legacy post-build script:
./local/scripts/install-grub.sh build/x86_64/harddrive.img    # Modify existing image
```

## TRACKING MAINLINE CHANGES

When mainline updates affect our work:

| Component | What to check | Where |
|-----------|---------------|-------|
| Kernel | ACPI, scheme, memory API changes | `recipes/core/kernel/source/src/` |
| relibc | New POSIX functions added upstream | `recipes/core/relibc/source/src/header/` |
| Base drivers | Driver API changes | `recipes/core/base/source/drivers/` |
| libdrm | DRM API updates | `recipes/libs/libdrm/` or the current in-tree libdrm location |
| Mesa | OpenGL/Vulkan backend changes | `recipes/libs/mesa/` |
| Build system | Makefile/config changes | `mk/`, `src/` |
| rsext4 | ext4 crate API changes | `local/recipes/core/ext4d/source/` Cargo.toml |
| Installer | ext4 dispatch, filesystem selection, GRUB bootloader | `local/patches/installer/redox.patch` |
| Quirks | New Linux quirk entries to port | `local/recipes/drivers/redox-driver-sys/source/src/quirks/` |

## PLANNING NOTES

- `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md` is the canonical public execution plan.
- `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` (v2.0) is the canonical desktop path plan from console to
  hardware-accelerated KDE Plasma on Wayland, using a three-track Phase 1–5 model.
- `local/docs/WAYLAND-IMPLEMENTATION-PLAN.md` is the canonical Wayland subsystem plan beneath the
  desktop path. Use it for Wayland-specific stability, completeness, ownership, and runtime-proof
  sequencing.
- `local/docs/DRM-MODERNIZATION-EXECUTION-PLAN.md` is the current DRM-focused execution plan beneath
  the canonical desktop path. It keeps Intel and AMD at the same evidence bar while separating
  display/KMS maturity from render/3D maturity.
- Older GPU-specific docs such as `local/docs/AMD-FIRST-INTEGRATION.md`,
  `local/docs/HARDWARE-3D-ASSESSMENT.md`, and `local/docs/DMA-BUF-IMPROVEMENT-PLAN.md` remain
  useful reference material, but they are not the planning authority when sequencing or acceptance
  criteria differ.
- `local/docs/AMD-FIRST-INTEGRATION.md` remains the deeper AMD-specific technical roadmap, but AMD
  and Intel machines are now equal-priority Red Bear OS targets.
- The earlier Phase 0–3 reassessment bridge has been retired. Its reconciliation role is now
  covered by `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md`,
  `local/docs/DESKTOP-STACK-CURRENT-STATUS.md`, and `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md`.
- `local/docs/WIFI-IMPLEMENTATION-PLAN.md` is the current Wi-Fi architecture and rollout plan,
  including the bounded role of `linux-kpi` and the native wireless control-plane direction.
- `local/docs/USB-IMPLEMENTATION-PLAN.md` and `local/docs/BLUETOOTH-IMPLEMENTATION-PLAN.md` should
  also be treated as first-class subsystem plans, not as side notes.
- `local/docs/WIFI-VALIDATION-RUNBOOK.md` is the canonical operator runbook for bare-metal and
  VFIO-backed Intel Wi-Fi validation, packaged checkers, and capture artifacts.
- `local/docs/IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md` is the current umbrella plan for
  IRQ delivery, MSI/MSI-X quality, IOMMU validation, and other low-level controller completeness work.
- `local/docs/QUIRKS-SYSTEM.md` documents the hardware quirks infrastructure: compiled-in tables,
  TOML runtime files, DMI matching, driver integration, and the linux-kpi C FFI bridge.
- `local/docs/QUIRKS-IMPROVEMENT-PLAN.md` is the current follow-up plan for removing quirks drift,
  integrating quirks into real drivers, and converging on one source of truth.
- `local/docs/DBUS-INTEGRATION-PLAN.md` is the canonical D-Bus architecture and implementation plan for KDE Plasma 6 on Wayland. It defines the phased approach to D-Bus service integration, the `redbear-sessiond` login1-compatible session broker, and the gap analysis for desktop-facing D-Bus services.
- `local/docs/GREETER-LOGIN-IMPLEMENTATION-PLAN.md` is the canonical Red Bear-native greeter/login design and current implementation plan for the `redbear-full` desktop path. It defines the `redbear-authd` / `redbear-session-launch` / `redbear-greeter` split, service wiring, validation surface, and the current boundary between the active greeter path and the older `redbear-validation-session` helper flows.

The current execution order for these subsystem plans is:

1. IRQ / low-level controller quality
2. USB maturity
3. Wi-Fi native control plane and first driver family
4. Bluetooth controller + host path
5. desktop/session compatibility on top of those runtime services

Do not present USB, Wi-Fi, Bluetooth, or low-level controller work as optional or secondary.

## FILESYSTEMS

Red Bear OS supports three filesystems:

| Filesystem | Implementation | Package | Status |
|------------|---------------|---------|--------|
| RedoxFS | Mainline Redox (default) | `recipes/core/redoxfs` | ✅ Stable |
| ext4 | rsext4 0.3 crate + ext4d scheme daemon | `local/recipes/core/ext4d` | ✅ Compiles + Installer wired |
| FAT (VFAT) | fatfs 0.3.6 crate + fatd scheme daemon | `local/recipes/core/fatd` | ✅ Compiles + Tools tested + label write verified |

### ext4 Workspace (`local/recipes/core/ext4d/source/`)

```
ext4d/source/
├── Cargo.toml              ← Workspace: ext4-blockdev, ext4d, ext4-mkfs
├── ext4-blockdev/           ← BlockDevice trait impls for rsext4
│   ├── Cargo.toml           ← Features: default=["redox"], redox=[libredox,syscall]
│   └── src/
│       ├── lib.rs           ← Re-exports: FileDisk, RedoxDisk, Ext4Error, Ext4Result
│       ├── file_disk.rs     ← FileDisk: std::fs backed, builds on host Linux + Redox
│       └── redox_disk.rs    ← RedoxDisk: syscall/libredox backed, Redox-only (feature-gated)
├── ext4d/                   ← ext4 filesystem scheme daemon (Redox userspace)
│   ├── Cargo.toml           ← Features: default=["redox"], redox deps
│   └── src/
│       ├── main.rs          ← Daemon: fork, SIGTERM, scheme registration
│       ├── mount.rs         ← Scheme event loop (redox_scheme::SchemeSync)
│       ├── scheme.rs        ← Full ext4 FSScheme: open, read, write, mkdir, unlink, stat...
│       └── handle.rs        ← FileHandle, DirectoryHandle, Handle types
└── ext4-mkfs/               ← ext4 mkfs tool (host-side utility)
    ├── Cargo.toml
    └── src/main.rs          ← Creates ext4 images via FileDisk + rsext4::mkfs
```

**Architecture**:
- `ext4d` is a Redox scheme daemon — it serves ext4 filesystems via `scheme:ext4d`
- Uses `rsext4` crate (pure Rust ext4 implementation) for all filesystem operations
- `FileDisk` allows building/testing on the Linux host machine
- `RedoxDisk` uses `libredox` + `redox_syscall` for actual Redox bare-metal I/O
- Both impls are behind the `redox` feature flag — `--no-default-features` gives Linux-only

**Recipe**: Symlinked into mainline search path:
```
recipes/core/ext4d → local/recipes/core/ext4d
```

**Config**: ext4d is included in `config/desktop.toml` (mainline), which `redbear-desktop.toml` inherits.

**Dependencies** (from workspace Cargo.toml):
- `rsext4 = "0.3"` — Pure Rust ext4 filesystem implementation
- `redox_syscall = "0.7.3"` — Redox syscall wrappers (scheme, data types, flags)
- `redox-scheme = "0.11.0"` — Scheme server framework
- `libredox = "0.1.13"` — High-level Redox syscalls (open, read, write, fstat)
- `redox-path = "0.3.0"` — Redox path utilities

### Installer ext4 + GRUB Integration (`local/patches/installer/redox.patch`)

The mainline installer is patched to support ext4 as an install target filesystem and
GRUB as an alternative boot manager:
- `GeneralConfig.filesystem: Option<String>` — TOML field, accepts `"redoxfs"` (default) or `"ext4"`
- `GeneralConfig.bootloader: Option<String>` — TOML field, accepts `"redox"` (default) or `"grub"`
- `FilesystemType` enum — dispatch tag used by `install_inner`
- `with_whole_disk_ext4()` — GPT partition layout + ext4 mkfs + file sync (mirrors `with_whole_disk`)
- `Ext4SliceDisk<T>` — adapts `DiskWrapper` to rsext4's `BlockDevice` trait
- `sync_host_dir_to_ext4()` — copies staged sysroot files into ext4 filesystem
- GRUB chainload: when `bootloader = "grub"`, writes GRUB EFI + grub.cfg to ESP alongside Redox bootloader
- CLI flags: `--filesystem ext4` / `--bootloader grub`

Usage in config TOML:
```toml
[general]
filesystem = "ext4"        # "redoxfs" is default
bootloader = "grub"        # "redox" is default
efi_partition_size = 16    # Required for GRUB (default 1 MiB is too small)
filesystem_size = 10240    # MB
```

See `local/docs/GRUB-INTEGRATION-PLAN.md` for the full GRUB architecture and usage guide.

### FAT (VFAT) Workspace (`local/recipes/core/fatd/source/`)

```
fatd/source/
├── Cargo.toml              ← Workspace: fat-blockdev, fatd, fat-mkfs, fat-label, fat-check
├── fat-blockdev/            ← Block device adapter for fatfs crate
│   ├── src/lib.rs           ← Re-exports: FileDisk (always), RedoxDisk (feature-gated)
│   ├── src/file_disk.rs     ← FileDisk: std::fs::File → Read+Write+Seek
│   └── src/redox_disk.rs    ← RedoxDisk: libredox → Read+Write+Seek (redox feature)
├── fatd/                    ← FAT filesystem scheme daemon (Redox userspace)
│   ├── src/main.rs          ← Daemon: fork, SIGTERM, dispatch to FileDisk/RedoxDisk
│   ├── src/mount.rs         ← Scheme event loop (redox_scheme::SchemeSync)
│   ├── src/scheme.rs        ← FatScheme: full FSScheme (open/read/write/mkdir/unlink/stat...)
│   └── src/handle.rs        ← FileHandle, DirectoryHandle, Handle types
├── fat-mkfs/                ← mkfs.fat equivalent (create FAT12/16/32 filesystems)
│   └── src/main.rs
├── fat-label/               ← fatlabel equivalent (read + write volume labels via BPB)
│   └── src/main.rs          ← `-s "LABEL"` writes label at BPB offset 43/71; verifies round-trip
└── fat-check/               ← fsck.fat equivalent (verify BPB, FAT chains, directory tree + safe repair)
    └── src/main.rs          ← `--repair` clears dirty flag, fixes FSInfo, reclaims lost clusters
```

**Architecture**: `fatd` is a Redox scheme daemon using `fatfs` v0.3.6 (MIT, no_std capable).
FAT is for data volumes and ESP only — NOT for root filesystem.
`fscommon::BufStream` wraps block device for mandatory caching.

**Recipe**: Symlinked into mainline search path:
```
recipes/core/fatd → ../../local/recipes/core/fatd
```

**Config**: Packages included via `config/redbear-device-services.toml` (inherited by
`redbear-desktop.toml` and `redbear-full.toml`). Init service at
`/usr/lib/init.d/15_fatd.service`.

**Dependencies**: fatfs 0.3.6, fscommon 0.1.1, redox_syscall, redox-scheme, libredox, libc

**Tool verification status** (2026-04-17):
- `fat-mkfs`: ✅ Creates FAT12/16/32, labels, auto-detection, cluster size option (`-c`), tested up to 1GB
- `fat-label`: ✅ Reads labels; writes BPB + creates/updates root-directory volume-label entry; verifies round-trip on all FAT types (including previously unlabeled volumes)
- `fat-check`: ✅ BPB validation, boot signature check, directory tree walk, cluster stats; ✅ safe repair (dirty flag including FAT12, FSInfo, lost clusters, orphaned LFN). Handles 0xFFFFFFFF FSInfo sentinel on fresh images.
- `fatd`: ✅ Compiles (links on Redox target only — expected). ✅ `frename` + rmdir non-empty check implemented. NOT runtime-tested (requires QEMU/bare metal).
- Phase 4 (runtime auto-mount): Deferred to runtime validation. Static init service exists.
- Known limitation: fatfs v0.3.6 strictly requires `total_sectors_16 == 0` for FAT32, rejecting some Linux `mkfs.fat` images
- `cargo test`: 60 unit tests (25 scheme + 7 label + 28 check) + 13+ integration edge cases

## BRANDING ASSETS

Red Bear OS visual identity files live in `local/Assets/`.

```
local/Assets/
└── images/
    ├── Red Bear OS icon.png              ← App icon / logo (1254x1254px)
    │                                        Red bear head, dark background, red border
    │                                        Use: desktop icon, bootloader logo, about dialog
    └── Red Bear OS loading background.png ← Boot / loading screen (1536x1024px)
                                             Cinematic red bear with forest silhouette
                                             Use: bootloader splash, login screen background
```

**Integration points** (future):
| Asset | Target | How |
|-------|--------|-----|
| icon.png | Bootloader logo | Convert to BMP, embed via bootloader config |
| icon.png | Desktop icon | Install to `/usr/share/icons/hicolor/` via redbear-release recipe |
| icon.png | About dialog | Install through the active icon/theme surface |
| loading background.png | Boot splash | Convert to framebuffer-compatible format, display during startup |
| loading background.png | Login screen | Set as the display-session background |

**Current status**: Assets are committed to git. Not yet integrated into the build — requires bootloader and display server integration (P2 hardware validation).

## ANTI-PATTERNS

- **DO NOT** edit files under mainline `recipes/` directly — put patches in `local/patches/`
- **DO NOT** commit firmware blobs to git — use `local/scripts/fetch-firmware.sh`
- **DO NOT** modify `mk/` or `src/` directly — extend via `local/scripts/`
- **DO NOT** assume mainline recipe names won't conflict — prefix custom ones (e.g., `redox-`)
- **DO NOT** use `my-*` naming for configs that should be tracked in git — use `redbear-*` instead
- **DO NOT** edit config/base.toml directly — our configs include it and override via TOML merge
- **DO NOT** forget to run sync-upstream.sh before major builds — stale upstream causes build failures

## RED BEAR OS CONFIG HIERARCHY

Active compile targets:

- `redbear-mini`
- `redbear-live-mini`
- `redbear-full`
- `redbear-live-full`

Desktop/graphics are available only on the `full` targets. Older names such as `redbear-kde`,
`redbear-wayland`, `redbear-minimal`, and `redbear-live-minimal` may still exist in the tree as
legacy or staging artifacts, but they are not the supported compile-target surface.

```
redbear-live-full.toml
  └── redbear-full.toml
        ├── desktop.toml (mainline)
        ├── redbear-legacy-base.toml     ← Neutralize broken base legacy init scripts
        ├── redbear-legacy-desktop.toml  ← Neutralize broken desktop legacy init scripts
        ├── redbear-device-services.toml ← Shared firmware-loader / evdevd / udev service wiring
        ├── redbear-netctl.toml          ← Shared Red Bear network profile files + netctl boot service
        ├── redbear-greeter-services.toml ← Greeter/auth/session-launch wiring for desktop targets
        └── [packages] redbear-release, redbear-hwutils, redbear-netctl,
                       firmware-loader, evdevd, udev-shim, redbear-info,
                       redbear-sessiond, redbear-authd, redbear-session-launch,
                       redbear-greeter, redbear-meta, cub
        NOTE: Desktop/graphics are available only on redbear-full and redbear-live-full.
        NOTE: ext4d is inherited from desktop.toml (mainline package).
        NOTE: redbear-meta is explicitly included in redbear-full.toml; keep broader inclusion deliberate.
        NOTE: redbear-live-full inherits from redbear-full.toml.

redbear-full.toml
  └── desktop.toml (mainline)
  └── redbear-legacy-base.toml     ← Neutralize broken base legacy init scripts
  └── redbear-legacy-desktop.toml  ← Neutralize broken desktop legacy init scripts
  └── redbear-device-services.toml ← Shared firmware-loader / evdevd / udev service wiring
  └── redbear-netctl.toml          ← Shared Red Bear network profile files + netctl boot service
  └── redbear-greeter-services.toml ← Greeter/auth/session-launch wiring

redbear-live-mini.toml
  └── minimal non-desktop live target
  └── desktop/graphics intentionally absent

redbear-mini
  └── legacy/staging config files in-tree still use the older `redbear-minimal*` names
      in some places; do not treat those names as the supported compile-target surface

redbear-minimal.toml (legacy/staging naming still present in tree)
  └── minimal.toml (mainline)
        └── base.toml
  └── redbear-legacy-base.toml     ← Neutralize broken base legacy init scripts
  └── redbear-device-services.toml ← Shared firmware-loader / evdevd / udev service wiring
  └── redbear-netctl.toml          ← Shared Red Bear network profile files + netctl boot service
  └── [packages] redbear-release, redbear-hwutils, redbear-netctl,
                 firmware-loader, evdevd, udev-shim, redbear-info
```

Config comparison:
| Config | GPU Stack | Desktop | Branding | ext4d | filesystem_size |
|--------|-----------|---------|----------|-------|-----------------|
| redbear-full | Full | Yes | Yes | ✅ (via desktop.toml) | 4096 MiB |
| redbear-live-full | Full | Yes | Yes | ✅ (via redbear-full.toml) | 4096 MiB |
| redbear-mini | None | None | Yes | legacy/staging naming in tree still maps through `redbear-minimal*` files | legacy/staging |
| redbear-live-mini | None | None | Yes | legacy/staging naming in tree still maps through `redbear-live-minimal*` files | legacy/staging |

## ANTI-PATTERNS (COMMIT POLICY)

- **DO NOT** include AI attribution in commit messages — no "Ultraworked with [Sisyphus]", "Co-authored-by: Sisyphus", or similar AI agent footers. Commits belong to the human author only.
