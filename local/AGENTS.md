# RED BEAR OS — DERIVATIVE OF REDOX OS

This directory contains ALL custom work on top of mainline Redox. When mainline Redox
updates (`git pull` on the build system repo), this directory is untouched.

## DESIGN PRINCIPLE

Red Bear OS relates to Redox OS in the same way Ubuntu relates to Debian:
  - We track Redox OS as upstream, merging changes regularly
  - We add custom packages, drivers, configs, and branding on top
  - The `local/` directory is our overlay — untouched by upstream updates
  - First-class configs use `redbear-*` naming (not `my-*`, which is gitignored)

Build flow:
```
make all CONFIG_NAME=redbear-desktop
  → mk/config.mk resolves to config/redbear-desktop.toml
  → Config includes desktop.toml (mainline) + Red Bear packages
  → repo cook builds all packages including our custom ones
  → mk/disk.mk creates harddrive.img with Red Bear branding
```

Update flow:
```
./local/scripts/sync-upstream.sh          # Rebase onto upstream Redox + verify symlinks
make all CONFIG_NAME=redbear-full          # Rebuild with latest
```

## TRACKING UPSTREAM (SYNC WITH REDOX OS)

Red Bear OS tracks the Redox OS build system as upstream. The `local/` directory
survives upstream updates untouched.

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
│   ├── redbear-desktop.toml   ← RED BEAR OS configs (first-class, tracked)
│   ├── redbear-minimal.toml
│   └── redbear-live.toml
├── recipes/                   ← mainline package recipes (untouched)
├── mk/                        ← mainline build system (untouched)
├── local/                     ← RED BEAR OS custom work
│   ├── AGENTS.md              ← This file
│   ├── config/                ← Legacy configs (my-*, gitignored)
│   ├── recipes/
│   │   ├── core/              ← ext4d (ext4 filesystem scheme daemon + mkfs tool)
│   │   ├── branding/          ← redbear-release (os-release, hostname, motd)
│   │   ├── drivers/           ← redox-driver-sys, linux-kpi
│   │   ├── gpu/               ← redox-drm (AMD + Intel display drivers), amdgpu (C port)
│   │   ├── system/            ← cub, evdevd, udev-shim, firmware-loader, redbear-hwutils, redbear-info, redbear-netctl, redbear-meta
│   │   ├── wayland/           ← Wayland compositor (Phase 4)
│   │   └── kde/               ← KDE Plasma (Phase 6)
│   ├── patches/
│   │   ├── kernel/            ← Kernel patches (ACPI, x2APIC)
│   │   ├── base/              ← Base patches (acpid fixes, power methods, pcid /config endpoint)
│   │   ├── relibc/            ← relibc patches (POSIX: eventfd, signalfd, timerfd)
│   │   ├── bootloader/        ← Bootloader patches
│   │   └── installer/         ← Installer patches (ext4 filesystem support)
│   ├── Assets/                ← Branding assets (icon, loading background)
│   │   └── images/            ← Red Bear OS icon (1254x1254) + loading bg (1536x1024)
│   ├── firmware/              ← GPU firmware blobs (gitignored, fetched)
│   ├── scripts/
│   │   ├── sync-upstream.sh   ← Sync with upstream Redox OS
│   │   ├── build-redbear.sh   ← Unified Red Bear OS build script
│   │   ├── fetch-firmware.sh  ← Download AMD firmware
│   │   ├── build-amd.sh       ← Legacy AMD-specific build (use build-redbear.sh)
│   │   ├── test-amd-gpu.sh    ← AMD GPU test script
│   │   └── test-baremetal.sh  ← Bare metal test script
│   └── docs/                  ← Integration docs
```

## HOW TO BUILD RED BEAR OS

```bash
# Full desktop with GPU drivers + branding
./local/scripts/build-redbear.sh redbear-desktop

# Minimal server variant
./local/scripts/build-redbear.sh redbear-minimal

# Live ISO
./local/scripts/build-redbear.sh redbear-live && make live CONFIG_NAME=redbear-live

# Or manually:
make all CONFIG_NAME=redbear-desktop

# Single custom recipe:
./target/release/repo cook local/recipes/branding/redbear-release
./target/release/repo cook local/recipes/system/redbear-meta
./target/release/repo cook local/recipes/core/ext4d
```

## TRACKING MAINLINE CHANGES

When mainline updates affect our work:

| Component | What to check | Where |
|-----------|---------------|-------|
| Kernel | ACPI, scheme, memory API changes | `recipes/core/kernel/source/src/` |
| relibc | New POSIX functions added upstream | `recipes/core/relibc/source/src/header/` |
| Base drivers | Driver API changes | `recipes/core/base/source/drivers/` |
| libdrm | DRM API updates | `recipes/wip/x11/libdrm/` or `recipes/libs/` |
| Mesa | OpenGL/Vulkan backend changes | `recipes/libs/mesa/` |
| Build system | Makefile/config changes | `mk/`, `src/` |
| rsext4 | ext4 crate API changes | `local/recipes/core/ext4d/source/` Cargo.toml |
| Installer | ext4 dispatch, filesystem selection | `local/patches/installer/redox.patch` |

## FILESYSTEMS

Red Bear OS supports two filesystems:

| Filesystem | Implementation | Package | Status |
|------------|---------------|---------|--------|
| RedoxFS | Mainline Redox (default) | `recipes/core/redoxfs` | ✅ Stable |
| ext4 | rsext4 0.3 crate + ext4d scheme daemon | `local/recipes/core/ext4d` | ✅ Compiles + Installer wired |

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

### Installer ext4 Integration (`local/patches/installer/redox.patch`)

The mainline installer is patched to support ext4 as an install target filesystem:
- `GeneralConfig.filesystem: Option<String>` — TOML field, accepts `"redoxfs"` (default) or `"ext4"`
- `FilesystemType` enum — dispatch tag used by `install_inner`
- `with_whole_disk_ext4()` — GPT partition layout + ext4 mkfs + file sync (mirrors `with_whole_disk`)
- `Ext4SliceDisk<T>` — adapts `DiskWrapper` to rsext4's `BlockDevice` trait
- `sync_host_dir_to_ext4()` — copies staged sysroot files into ext4 filesystem
- CLI flag: `--filesystem ext4` or `--filesystem redoxfs`

Usage in config TOML:
```toml
[general]
filesystem = "ext4"        # "redoxfs" is default
filesystem_size = 10240    # MB
```

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
| icon.png | About dialog | COSMIC desktop reads from icon theme |
| loading background.png | Boot splash | Convert to framebuffer-compatible format, display before orbital starts |
| loading background.png | Login screen | Set as orblogin/orbital background |

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

```
redbear-live.toml
  └── redbear-desktop.toml
        ├── desktop.toml (mainline)
        │     ├── desktop-minimal.toml
        │     │     └── minimal.toml
        │     │           └── base.toml
        │     └── server.toml
        │           └── minimal.toml
        │                 └── base.toml
        └── [packages] redbear-release, redox-driver-sys, linux-kpi,
                       firmware-loader, redox-drm, cub, redbear-hwutils,
                       redbear-netctl, evdevd, udev-shim, redbear-meta
        NOTE: ext4d is inherited from desktop.toml (mainline package)
        NOTE: cub is included via redbear-desktop.toml and depends on the custom
              recipe symlink (recipes/system/cub → local/recipes/system/cub) being
              created by integrate-redbear.sh or apply-patches.sh before building.
        NOTE: redbear-netctl provides a Redox-native `netctl` command with profiles
              in /etc/netctl and a boot-time `netctl --boot` service.
        NOTE: redbear-info is the canonical runtime integration report. Keep it updated when
              Red Bear adds new tools, schemes, services, or hardware integration paths.

redbear-full.toml
  └── desktop.toml (mainline)
  └── redbear-legacy-base.toml     ← Neutralize broken base legacy init scripts
  └── redbear-legacy-desktop.toml  ← Neutralize broken desktop legacy init scripts
  └── redbear-device-services.toml ← Shared firmware-loader / evdevd / udev service wiring
  └── redbear-netctl.toml          ← Shared Red Bear network profile files + netctl boot service

redbear-kde.toml
  └── desktop.toml (mainline)
  └── redbear-legacy-base.toml     ← Neutralize broken base legacy init scripts
  └── redbear-legacy-desktop.toml  ← Neutralize broken desktop legacy init scripts
  └── redbear-device-services.toml ← Shared firmware-loader / evdevd / udev service wiring
  └── redbear-netctl.toml          ← Shared Red Bear network profile files + netctl boot service

redbear-minimal.toml
  └── minimal.toml (mainline)
        └── base.toml
  └── redbear-legacy-base.toml     ← Neutralize broken base legacy init scripts
  └── redbear-device-services.toml ← Shared firmware-loader / evdevd / udev service wiring
  └── redbear-netctl.toml          ← Shared Red Bear network profile files + netctl boot service
  └── [packages] redbear-release, redbear-hwutils, redbear-netctl,
                 redox-driver-sys, firmware-loader, evdevd, udev-shim
```

Config comparison:
| Config | GPU Stack | Desktop | Branding | ext4d | filesystem_size |
|--------|-----------|---------|----------|-------|-----------------|
| redbear-desktop | Full | COSMIC | Yes | ✅ (via desktop.toml) | 10240 MiB |
| redbear-minimal | None | None | Yes | ❌ | 512 MiB |
| redbear-live | Full | COSMIC | Yes | ✅ (via desktop.toml) | 12288 MiB |
