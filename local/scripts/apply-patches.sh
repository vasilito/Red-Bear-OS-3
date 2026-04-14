#!/usr/bin/env bash
# apply-patches.sh — Apply all Red Bear OS patches on top of upstream Redox build system.
#
# Usage: ./local/scripts/apply-patches.sh [--force]
#
# This script:
#   1. Applies build-system patches (rebranding, cookbook fixes, config, docs)
#   2. Ensures recipe patches are symlinked from local/patches/
#   3. Ensures custom recipe symlinks exist in recipes/
#
# With --force: reapplies even if patches appear already applied.
#
# SAFE: does not touch local/ directory. Only modifies upstream files.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PATCHES_DIR="$REPO_ROOT/local/patches"
FORCE="${1:-}"

cd "$REPO_ROOT"

# ── Helper ──────────────────────────────────────────────────────────
symlink() {
    local target="$1" link="$2"
    if [ -L "$link" ]; then
        current="$(readlink "$link")"
        if [ "$current" = "$target" ]; then
            return 0  # already correct
        fi
    fi
    rm -f "$link"
    ln -s "$target" "$link"
    echo "  linked $link -> $target"
}

# ── 1. Build-system patches ─────────────────────────────────────────
echo "==> Applying build-system patches..."
for patch_file in "$PATCHES_DIR"/build-system/[0-9]*.patch; do
    [ -f "$patch_file" ] || continue
    patch_name="$(basename "$patch_file")"

    # Check if already applied (skip unless --force)
    if [ "$FORCE" != "--force" ]; then
        if git apply --check "$patch_file" 2>/dev/null; then
            : # patch applies cleanly, apply it
        else
            echo "  SKIP $patch_name (already applied or conflicts)"
            echo "       Use --force to attempt re-application"
            continue
        fi
    fi

    if git apply --whitespace=nowarn "$patch_file"; then
        echo "  OK   $patch_name"
    else
        echo "  FAIL $patch_name — resolve conflicts manually"
        echo "       Patch file: $patch_file"
        exit 1
    fi
done

# ── 2. Recipe patches (kernel, base) ───────────────────────────────
echo "==> Linking recipe patches from local/patches/..."
symlink "../../../local/patches/kernel/redox.patch" "recipes/core/kernel/redox.patch"
symlink "../../../local/patches/base/redox.patch"   "recipes/core/base/redox.patch"

# ── 3. Custom recipe symlinks ──────────────────────────────────────
echo "==> Linking custom recipes from local/recipes/..."

# Branding
mkdir -p recipes/branding
symlink "../../local/recipes/branding/redbear-release" "recipes/branding/redbear-release"

# Drivers
mkdir -p recipes/drivers
symlink "../../local/recipes/drivers/linux-kpi"       "recipes/drivers/linux-kpi"
symlink "../../local/recipes/drivers/redox-driver-sys" "recipes/drivers/redox-driver-sys"

# GPU
mkdir -p recipes/gpu
symlink "../../local/recipes/gpu/amdgpu"    "recipes/gpu/amdgpu"
symlink "../../local/recipes/gpu/redox-drm" "recipes/gpu/redox-drm"

# Library stubs / custom libs
mkdir -p recipes/libs
symlink "../../local/recipes/libs/libepoxy-stub"       "recipes/libs/libepoxy-stub"
symlink "../../local/recipes/libs/libudev-stub"        "recipes/libs/libudev-stub"
symlink "../../local/recipes/libs/lcms2-stub"          "recipes/libs/lcms2-stub"
symlink "../../local/recipes/libs/libdisplay-info-stub" "recipes/libs/libdisplay-info-stub"
symlink "../../local/recipes/libs/libxcvt-stub"        "recipes/libs/libxcvt-stub"

# System
mkdir -p recipes/system
symlink "../../local/recipes/system/cub"              "recipes/system/cub"
symlink "../../local/recipes/system/evdevd"           "recipes/system/evdevd"
symlink "../../local/recipes/system/firmware-loader"  "recipes/system/firmware-loader"
symlink "../../local/recipes/system/iommu"            "recipes/system/iommu"
symlink "../../local/recipes/system/redbear-hwutils"  "recipes/system/redbear-hwutils"
symlink "../../local/recipes/system/redbear-netctl"   "recipes/system/redbear-netctl"
symlink "../../local/recipes/system/redbear-meta"     "recipes/system/redbear-meta"
symlink "../../local/recipes/system/udev-shim"        "recipes/system/udev-shim"

# Core additions
mkdir -p recipes/core
symlink "../../local/recipes/core/ext4d" "recipes/core/ext4d"

# ── 4. New files not in upstream ────────────────────────────────────
echo "==> Ensuring Red Bear OS-specific files exist..."

# redbear.ipxe (network boot)
if [ ! -f redbear.ipxe ] && [ ! -L redbear.ipxe ]; then
    cat > redbear.ipxe <<'IPXE'
#!ipxe

kernel bootloader-live.efi
initrd http://${next-server}:8080/redbear-live.iso
boot
IPXE
    echo "  created redbear.ipxe"
fi

# redbear-full config (not in upstream)
if [ ! -f config/redbear-full.toml ] && [ ! -L config/redbear-full.toml ]; then
    cat > config/redbear-full.toml <<'TOML'
# Red Bear OS Full Configuration
# Complete desktop + all Red Bear OS custom drivers and tools
#
# Build: make all CONFIG_NAME=redbear-full
# Live:  make live CONFIG_NAME=redbear-full

include = ["desktop.toml"]

[general]
# 2GB filesystem — plenty for full desktop + drivers
# (desktop.toml sets 650MB, but we want headroom for our custom packages)
filesystem_size = 2048

[packages]
# Red Bear OS branding (os-release, hostname, motd)
redbear-release = {}

# ext4 filesystem support (our custom port)
ext4d = {}

# Red Bear OS driver infrastructure
redox-driver-sys = {}
linux-kpi = {}
firmware-loader = {}

# Input layer
evdevd = {}
udev-shim = {}

# GPU driver (AMD — modesetting display core)
redox-drm = {}
amdgpu = {}

# Red Bear OS meta-package (dependencies, default config)
redbear-meta = {}
TOML
    echo "  created config/redbear-full.toml"
fi

echo ""
echo "==> All Red Bear OS patches applied. Ready to build."
echo "    make all CONFIG_NAME=redbear-full"
