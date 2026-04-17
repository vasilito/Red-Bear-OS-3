#!/usr/bin/env bash
# apply-patches.sh — Apply all Red Bear OS overlays on top of upstream Redox build system.
#
# Usage: ./local/scripts/apply-patches.sh [--force]
#
# This script:
#   1. Applies build-system patches (rebranding, cookbook fixes, config, docs)
#   2. Ensures recipe patches are symlinked from local/patches/
#   3. Ensures custom recipe symlinks exist in recipes/
#
# WIP policy note:
#   If upstream work is still under recipes/wip/, Red Bear may still ship from local/recipes/
#   instead. This script therefore treats the local overlay as the durable source of truth.
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
symlink "../../local/recipes/drivers/redbear-btusb"   "recipes/drivers/redbear-btusb"
symlink "../../local/recipes/drivers/redbear-iwlwifi" "recipes/drivers/redbear-iwlwifi"
symlink "../../local/recipes/drivers/redox-driver-sys" "recipes/drivers/redox-driver-sys"

# GPU
mkdir -p recipes/gpu
symlink "../../local/recipes/gpu/amdgpu"    "recipes/gpu/amdgpu"
symlink "../../local/recipes/gpu/redox-drm" "recipes/gpu/redox-drm"

# Library stubs / custom libs
mkdir -p recipes/libs
symlink "../../local/recipes/libs/libqrencode"         "recipes/libs/libqrencode"
symlink "../../local/recipes/libs/libepoxy-stub"       "recipes/libs/libepoxy-stub"
symlink "../../local/recipes/libs/libudev-stub"        "recipes/libs/libudev-stub"
symlink "../../local/recipes/libs/lcms2-stub"          "recipes/libs/lcms2-stub"
symlink "../../local/recipes/libs/libdisplay-info-stub" "recipes/libs/libdisplay-info-stub"
symlink "../../local/recipes/libs/libxcvt-stub"        "recipes/libs/libxcvt-stub"
symlink "../../local/recipes/libs/zbus"                "recipes/libs/zbus"

# System
mkdir -p recipes/system
symlink "../../local/recipes/system/cub"              "recipes/system/cub"
symlink "../../local/recipes/system/evdevd"           "recipes/system/evdevd"
symlink "../../local/recipes/system/redbear-firmware" "recipes/system/redbear-firmware"
symlink "../../local/recipes/system/firmware-loader"  "recipes/system/firmware-loader"
symlink "../../local/recipes/system/iommu"            "recipes/system/iommu"
symlink "../../local/recipes/system/redbear-hwutils"  "recipes/system/redbear-hwutils"
symlink "../../local/recipes/system/redbear-info"     "recipes/system/redbear-info"
symlink "../../local/recipes/system/redbear-btctl"    "recipes/system/redbear-btctl"
symlink "../../local/recipes/system/redbear-netstat"  "recipes/system/redbear-netstat"
symlink "../../local/recipes/system/redbear-netctl"   "recipes/system/redbear-netctl"
symlink "../../local/recipes/system/redbear-netctl-console" "recipes/system/redbear-netctl-console"
symlink "../../local/recipes/system/redbear-wifictl"  "recipes/system/redbear-wifictl"
symlink "../../local/recipes/system/redbear-traceroute" "recipes/system/redbear-traceroute"
symlink "../../local/recipes/system/redbear-mtr"      "recipes/system/redbear-mtr"
symlink "../../local/recipes/system/redbear-nmap"     "recipes/system/redbear-nmap"
symlink "../../local/recipes/system/redbear-meta"     "recipes/system/redbear-meta"
symlink "../../local/recipes/system/udev-shim"        "recipes/system/udev-shim"
symlink "../../local/recipes/system/redbear-sessiond"  "recipes/system/redbear-sessiond"
symlink "../../local/recipes/system/redbear-dbus-services" "recipes/system/redbear-dbus-services"
symlink "../../local/recipes/system/redbear-notifications" "recipes/system/redbear-notifications"
symlink "../../local/recipes/system/redbear-upower"    "recipes/system/redbear-upower"
symlink "../../local/recipes/system/redbear-udisks"    "recipes/system/redbear-udisks"
symlink "../../local/recipes/system/redbear-polkit"    "recipes/system/redbear-polkit"

# Core additions
mkdir -p recipes/core
symlink "../../local/recipes/core/ext4d" "recipes/core/ext4d"
symlink "../../local/recipes/core/grub"  "recipes/core/grub"

# Resolve WIP conflict: recipes/wip/services/grub also exists,
# so redirect the entire directory to our local overlay to ensure
# COOKBOOK_RECIPE resolves to a directory that contains grub.cfg
if [ -d "recipes/wip/services/grub" ] && [ ! -L "recipes/wip/services/grub" ]; then
    rm -rf "recipes/wip/services/grub"
fi
if [ ! -e "recipes/wip/services/grub" ]; then
    symlink "../../../../local/recipes/core/grub" "recipes/wip/services/grub"
fi

# Wayland additions
mkdir -p recipes/wip/wayland
symlink "../../../local/recipes/wayland/qt6-wayland-smoke" "recipes/wip/wayland/qt6-wayland-smoke"

# KDE / Phase 6 recipes
mkdir -p recipes/kde
symlink "../../local/recipes/kde/plasma-desktop" "recipes/kde/plasma-desktop"
symlink "../../local/recipes/kde/plasma-workspace" "recipes/kde/plasma-workspace"
symlink "../../local/recipes/kde/plasma-framework" "recipes/kde/plasma-framework"
symlink "../../local/recipes/kde/plasma-wayland-protocols" "recipes/kde/plasma-wayland-protocols"
symlink "../../local/recipes/kde/kwin" "recipes/kde/kwin"
symlink "../../local/recipes/kde/kirigami" "recipes/kde/kirigami"
symlink "../../local/recipes/kde/kirigami" "recipes/kde/kf6-kirigami"
symlink "../../local/recipes/kde/kdecoration" "recipes/kde/kdecoration"
symlink "../../local/recipes/kde/kf6-extra-cmake-modules" "recipes/kde/kf6-extra-cmake-modules"
symlink "../../local/recipes/kde/kf6-kcoreaddons" "recipes/kde/kf6-kcoreaddons"
symlink "../../local/recipes/kde/kf6-kwidgetsaddons" "recipes/kde/kf6-kwidgetsaddons"
symlink "../../local/recipes/kde/kf6-kconfig" "recipes/kde/kf6-kconfig"
symlink "../../local/recipes/kde/kf6-ki18n" "recipes/kde/kf6-ki18n"
symlink "../../local/recipes/kde/kf6-kcodecs" "recipes/kde/kf6-kcodecs"
symlink "../../local/recipes/kde/kf6-kguiaddons" "recipes/kde/kf6-kguiaddons"
symlink "../../local/recipes/kde/kf6-kcolorscheme" "recipes/kde/kf6-kcolorscheme"
symlink "../../local/recipes/kde/kf6-kauth" "recipes/kde/kf6-kauth"
symlink "../../local/recipes/kde/kf6-kitemmodels" "recipes/kde/kf6-kitemmodels"
symlink "../../local/recipes/kde/kf6-kitemviews" "recipes/kde/kf6-kitemviews"
symlink "../../local/recipes/kde/kf6-karchive" "recipes/kde/kf6-karchive"
symlink "../../local/recipes/kde/kf6-kwindowsystem" "recipes/kde/kf6-kwindowsystem"
symlink "../../local/recipes/kde/kf6-knotifications" "recipes/kde/kf6-knotifications"
symlink "../../local/recipes/kde/kf6-kjobwidgets" "recipes/kde/kf6-kjobwidgets"
symlink "../../local/recipes/kde/kf6-kconfigwidgets" "recipes/kde/kf6-kconfigwidgets"
symlink "../../local/recipes/kde/kf6-kcrash" "recipes/kde/kf6-kcrash"
symlink "../../local/recipes/kde/kf6-kdbusaddons" "recipes/kde/kf6-kdbusaddons"
symlink "../../local/recipes/kde/kf6-kglobalaccel" "recipes/kde/kf6-kglobalaccel"
symlink "../../local/recipes/kde/kf6-kservice" "recipes/kde/kf6-kservice"
symlink "../../local/recipes/kde/kf6-kpackage" "recipes/kde/kf6-kpackage"
symlink "../../local/recipes/kde/kf6-kiconthemes" "recipes/kde/kf6-kiconthemes"
symlink "../../local/recipes/kde/kf6-kxmlgui" "recipes/kde/kf6-kxmlgui"
symlink "../../local/recipes/kde/kf6-ktextwidgets" "recipes/kde/kf6-ktextwidgets"
symlink "../../local/recipes/kde/kf6-solid" "recipes/kde/kf6-solid"
symlink "../../local/recipes/kde/kf6-sonnet" "recipes/kde/kf6-sonnet"
symlink "../../local/recipes/kde/kf6-kio" "recipes/kde/kf6-kio"
symlink "../../local/recipes/kde/kf6-kbookmarks" "recipes/kde/kf6-kbookmarks"
symlink "../../local/recipes/kde/kf6-kcompletion" "recipes/kde/kf6-kcompletion"
symlink "../../local/recipes/kde/kf6-kdeclarative" "recipes/kde/kf6-kdeclarative"
symlink "../../local/recipes/kde/kf6-kcmutils" "recipes/kde/kf6-kcmutils"
symlink "../../local/recipes/kde/kf6-kidletime" "recipes/kde/kf6-kidletime"
symlink "../../local/recipes/kde/kf6-kwayland" "recipes/kde/kf6-kwayland"
symlink "../../local/recipes/kde/kf6-knewstuff" "recipes/kde/kf6-knewstuff"
symlink "../../local/recipes/kde/kf6-kwallet" "recipes/kde/kf6-kwallet"
symlink "../../local/recipes/kde/kf6-prison" "recipes/kde/kf6-prison"

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
