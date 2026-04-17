# GRUB Integration Plan — Red Bear OS

**Date:** 2026-04-17
**Status:** Fully implemented (build-tested, not yet runtime boot-tested). ESP formatted as FAT32
per UEFI spec. Both Phase 1 (post-build script) and Phase 2 (installer-native) are wired.
**Remaining:** Runtime UEFI boot validation in QEMU (`make all CONFIG_NAME=redbear-full-grub && make qemu`).
**Prerequisite:** The `grub` package is included in `redbear-full-grub.toml` for clean-tree builds.
**Approach:** Option A — GRUB as boot manager, chainloading Redox bootloader

## Overview

Add GNU GRUB as an optional boot manager for Red Bear OS. GRUB presents a menu
at boot and chainloads the existing Redox bootloader, which then boots the
kernel normally. This gives users:

- Multi-boot capability alongside Linux, Windows, or other OSes
- Boot menu with timeout and manual selection
- Familiar GRUB rescue shell for debugging
- No changes to the Redox kernel, RedoxFS, or existing boot flow

## Architecture

```
UEFI firmware
  → EFI/BOOT/BOOTX64.EFI (GRUB standalone image)
    → grub.cfg: default entry chainloads Redox bootloader
      → EFI/REDBEAR/redbear.efi (Redox bootloader)
        → Reads RedoxFS partition
          → Loads kernel
            → Boots Red Bear OS
```

### ESP Layout (GRUB mode)

```
EFI/
├── BOOT/
│   ├── BOOTX64.EFI      ← GRUB (primary, loaded by UEFI firmware)
│   └── grub.cfg          ← GRUB configuration
└── REDBEAR/
    └── redbear.efi       ← Redox bootloader (chainload target)
```

### ESP Layout (default, no GRUB)

```
EFI/
└── BOOT/
    └── BOOTX64.EFI       ← Redox bootloader (unchanged)
```

## Why GRUB?

1. **GRUB does not support RedoxFS.** Writing a GRUB filesystem module for
   RedoxFS is high-risk, GPL-licensing-sensitive work. Chainloading avoids it.
2. **The Redox bootloader works.** It reads RedoxFS directly and boots the
   kernel. No need to replicate that logic in GRUB.
3. **GRUB is universally understood.** System administrators know GRUB. A
   `grub.cfg` is easier to customize than a custom bootloader.
4. **Multi-boot.** GRUB can boot Linux, Windows, and other OSes alongside
   Red Bear OS without any changes to those systems.

## GRUB Module Set

The standalone EFI image includes these modules:

| Module | Purpose |
|--------|---------|
| `part_gpt` | GPT partition table support |
| `part_msdos` | MBR partition table support |
| `fat` | FAT32 filesystem (ESP) |
| `ext2` | ext2/3/4 filesystem |
| `normal` | Normal mode (menu, scripting) |
| `configfile` | Load configuration files |
| `search` | Search for files/volumes |
| `search_fs_uuid` | Search by filesystem UUID |
| `search_label` | Search by volume label |
| `echo` | Print messages |
| `test` | Conditional expressions |
| `ls` | List files and devices |
| `cat` | Display file contents |
| `halt` | Shut down |
| `reboot` | Reboot |

Note: `chainloader` is a built-in command in GRUB 2.12 (no separate module needed).

No RedoxFS module is needed — GRUB chainloads the Redox bootloader instead.

## GRUB Configuration

The default `grub.cfg`:

```cfg
# Red Bear OS GRUB Configuration
set default=0
set timeout=5

menuentry "Red Bear OS" {
    chainloader /EFI/REDBEAR/redbear.efi
    boot
}

menuentry "Reboot" {
    reboot
}

menuentry "Shutdown" {
    halt
}
```

Users can customize `grub.cfg` to add entries for other operating systems,
change the timeout, or add additional Red Bear OS entries (e.g., recovery
mode with different kernel parameters, once supported).

## ESP Size Requirements

| Component | Typical Size |
|-----------|--------------|
| GRUB EFI binary (with modules) | ~500 KiB (varies with module list) |
| Redox bootloader | 100–200 KiB |
| grub.cfg | < 1 KiB |
| **Total** | **~1 MiB** |

The default ESP is 1 MiB (too small for GRUB). Configs using GRUB must set:

```toml
[general]
efi_partition_size = 16   # 16 MiB, enough for GRUB + Redox bootloader + margin
```

## Linux-Compatible CLI

Red Bear OS provides `grub-install` and `grub-mkconfig` wrappers that match GNU GRUB
command-line conventions. Users migrating from Linux can use familiar switches.

| Linux Command | Red Bear OS Location |
|---------------|---------------------|
| `grub-install` | `local/scripts/grub-install` |
| `grub-mkconfig` | `local/scripts/grub-mkconfig` |

Add to PATH for convenience:
```bash
export PATH="$PWD/local/scripts:$PATH"
```

### grub-install

```bash
# Install GRUB into a disk image
grub-install --target=x86_64-efi --disk-image=build/x86_64/harddrive.img

# Verbose mode
grub-install --target=x86_64-efi --disk-image=build/x86_64/harddrive.img --verbose

# Show help
grub-install --help
```

Supported options: `--target=`, `--efi-directory=`, `--bootloader-id=`, `--removable`,
`--disk-image=`, `--modules=`, `--no-nvram`, `--verbose`, `--help`, `--version`.

Unsupported Linux options are accepted and ignored silently for script compatibility.

### grub-mkconfig

```bash
# Preview generated config
grub-mkconfig

# Write to file
grub-mkconfig -o local/recipes/core/grub/grub.cfg

# Custom timeout
grub-mkconfig --timeout=10 -o /boot/grub/grub.cfg
```

Supported options: `-o`/`--output=`, `--timeout=`, `--set-default=`, `--help`, `--version`.

## Implementation — Phase 1: Post-Build Script

Phase 1 uses a post-build script to modify the ESP in an existing disk image.
This approach requires **no changes to the installer** and works immediately.

### Files

| File | Purpose |
|------|---------|
| `local/recipes/core/grub/recipe.toml` | Build GRUB from source, produce `grub.efi` |
| `local/recipes/core/grub/grub.cfg` | Default GRUB configuration |
| `local/scripts/install-grub.sh` | Post-build ESP modification script |
| `local/scripts/fat_tool.py` | Python FAT32 tool (no mtools dependency) |
| `recipes/core/grub → local/recipes/core/grub` | Symlink for recipe discovery |

### Workflow

```bash
# 1. Build GRUB recipe
make r.grub

# 2. Build Red Bear OS (with larger ESP)
make all CONFIG_NAME=redbear-full   # Must have efi_partition_size = 16

# 3. Install GRUB into disk image
./local/scripts/install-grub.sh build/x86_64/harddrive.img

# 4. Test
make qemu
```

### Requirements

- Python 3 (for `fat_tool.py` — no mtools dependency)
- GRUB build dependencies: `gcc`, `make`, `bison`, `flex`, `autoconf`, `automake`
- ESP must be ≥ 8 MiB (set `efi_partition_size = 16` in config)

## Implementation — Phase 2: Installer-Native Support

Phase 2 adds GRUB awareness directly to the Redox installer, eliminating the
post-build script step. The installer reads `bootloader = "grub"` from config,
fetches the GRUB package alongside the bootloader, and writes the chainload
ESP layout automatically.

### Changes Made

1. **`GeneralConfig`** (`config/general.rs`): Added `bootloader: Option<String>`
   field (`"redox"` default, `"grub"` for GRUB), with merge support.

2. **`DiskOption`** (`installer.rs`): Added `grub_efi: Option<&[u8]>` and
   `grub_config: Option<&[u8]>` fields for optional GRUB data.

3. **`fetch_bootloaders`**: When `bootloader = "grub"`, installs the `grub`
   package alongside `bootloader` and returns `grub.efi` + `grub.cfg` data.
   Return type extended to `(bios, efi, grub_efi, grub_cfg)`.

4. **`with_whole_disk` / `with_whole_disk_ext4`**: When `grub_efi` and
   `grub_config` are both present, writes the GRUB chainload layout:
   - `EFI/BOOT/BOOTX64.EFI` ← GRUB
   - `EFI/BOOT/grub.cfg` ← GRUB configuration
   - `EFI/REDBEAR/redbear.efi` ← Redox bootloader (chainload target)

5. **`install_inner`**: Passes GRUB data from `fetch_bootloaders` through
   `DiskOption`.

6. **CLI** (`bin/installer.rs`): Added `--bootloader grub` flag that sets
   `config.general.bootloader`.

7. **TUI** (`bin/installer_tui.rs`): Updated `DiskOption` construction with
   `grub_efi: None, grub_config: None`.

### Config Usage

```toml
# config/redbear-full-grub.toml
include = ["redbear-full.toml"]

[general]
bootloader = "grub"
efi_partition_size = 16
```

Or via CLI (note: INSTALLER_OPTS replaces defaults, so --cookbook=. must be included):
```bash
./target/release/repo cook installer
make all CONFIG_NAME=redbear-full INSTALLER_OPTS="--cookbook=. --bootloader grub"
```

**Note:** The config file approach (`redbear-full-grub.toml`) is preferred over the CLI flag
because INSTALLER_OPTS completely replaces the default value (`--cookbook=.`) rather than
appending to it. Omitting `--cookbook=.` breaks local package resolution for GRUB.

## GRUB Recipe Design

The GRUB recipe uses `template = "custom"` because GRUB must be built for the
**host machine** (it's a build tool that produces EFI binaries), not for the
Redox target. The cookbook's `configure` template cross-compiles for Redox,
which is wrong for GRUB.

Key build steps:
1. Configure with `--target=x86_64 --with-platform=efi` (produces x86_64 EFI)
2. Disable unnecessary components (themes, mkfont, mount, device-mapper)
3. Run `grub-mkimage` to create standalone EFI binary with curated modules
4. Stage `grub.efi` and `grub.cfg` to `/usr/lib/boot/`

### Build Notes

The recipe uses `template = "custom"` because the cookbook's default `configure`
template sets `--host="${GNU_TARGET}"` for Redox cross-compilation, which is wrong
for GRUB (a host build tool producing EFI binaries).

Two issues required workarounds:

1. **Cross-compiler override.** The cookbook sets `CC`, `CXX`, `CFLAGS`, etc. to
   the Redox cross-toolchain. GRUB must be built with the host compiler. Fix:
   `unset CC CXX CPP LD AR NM RANLIB OBJCOPY STRIP PKG_CONFIG` and
   `unset CFLAGS CXXFLAGS CPPFLAGS LDFLAGS` at the top of the script.

2. **Missing `extra_deps.lst`.** GRUB 2.12 release tarballs omit
   `grub-core/extra_deps.lst` (normally generated by `autogen.sh` from git).
   Fix: `touch "${COOKBOOK_SOURCE}/grub-core/extra_deps.lst"` before configure.

3. **grub.cfg location.** The config file lives in the recipe directory
   (`${COOKBOOK_RECIPE}/grub.cfg`), not in the extracted source tarball
   (`${COOKBOOK_SOURCE}/`). The copy step uses `COOKBOOK_RECIPE`.

## Security Considerations

- GRUB configuration is on the ESP (FAT32), which is readable/writable by any OS
- Secure Boot: GRUB standalone images are not signed. Users needing Secure Boot
  must sign `BOOTX64.EFI` with their own key or use `shim`
- The chainload target (`EFI/REDBEAR/redbear.efi`) is also on the ESP
- No credentials or secrets are stored in the GRUB configuration

## Limitations

- GRUB cannot read RedoxFS (no module exists)
- Cannot pass kernel parameters directly (chainloading bypasses this)
- BIOS boot is not supported (only UEFI)
- ESP must be sized to ≥ 8 MiB in config (16 MiB recommended)
- GRUB bootloader is incompatible with `skip_partitions = true` (requires GPT layout with ESP)
- TUI installer does not support GRUB mode (intentional — TUI is for live disk reinstall)
- Runtime UEFI boot test has not been performed yet (requires full `make all` build, ~hours)

## Testing

### Phase 1: Post-build script (standalone)

```bash
# Build GRUB recipe
make r.grub

# Build image (any config with efi_partition_size >= 16)
make all CONFIG_NAME=redbear-full

# Install GRUB into disk image (uses fat_tool.py, no mtools needed)
./local/scripts/install-grub.sh build/x86_64/harddrive.img

# Verify ESP contents
python3 local/scripts/fat_tool.py ls build/x86_64/harddrive.img 1048576 /

# Boot in QEMU
make qemu
# Expected: GRUB menu appears, "Red Bear OS" entry boots successfully
```

### Phase 2: Installer-native (automatic)

```bash
# Build GRUB recipe (must be built before installer runs)
make r.grub

# Build image with GRUB config (installer fetches GRUB automatically)
make all CONFIG_NAME=redbear-full-grub

# Or via CLI flag
make all CONFIG_NAME=redbear-full INSTALLER_OPTS="--bootloader grub --cookbook=."

# Verify ESP contents
python3 local/scripts/fat_tool.py ls build/x86_64/harddrive.img 1048576 /

# Boot in QEMU
make qemu
# Expected: GRUB menu appears, "Red Bear OS" entry boots successfully
```

### Unit tests (no full build required)

```bash
# Verify GRUB recipe builds
CI=1 ./target/release/repo cook grub

# Verify host-side installer accepts --bootloader flag
build/fstools/bin/redox_installer --bootloader=grub --config=config/redbear-full-grub.toml --list-packages

# Verify fat_tool.py operations
python3 local/scripts/fat_tool.py --help
```

## References

- GNU GRUB Manual: https://www.gnu.org/software/grub/manual/grub/grub.html
- GRUB EFI standalone image: `grub-mkimage -O x86_64-efi ...`
- UEFI boot specification: `EFI/BOOT/BOOTX64.EFI` is the fallback boot path
- Redox bootloader source: `recipes/core/bootloader/source/`
- Installer GPT layout: `recipes/core/installer/source/src/installer.rs`
