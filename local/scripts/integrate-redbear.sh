#!/usr/bin/env bash
# integrate-redbear.sh — Prepare Red Bear OS custom work for standard builds.
#
# Usage:
#   ./local/scripts/integrate-redbear.sh
#   REDBEAR_TAG=build/x86_64-unknown-redox/redbear.tag ./local/scripts/integrate-redbear.sh
#
# This script is idempotent and safe to run repeatedly. It ensures the Red Bear OS overlay
# is wired into the main build tree, stages branding assets and firmware into local
# recipe sources, and updates a tag file consumed by the build system.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
REDBEAR_TAG="${REDBEAR_TAG:-build/redbear.tag}"

if [ -t 1 ]; then
    GREEN='\033[1;32m'
    YELLOW='\033[1;33m'
    CYAN='\033[1;36m'
    RESET='\033[0m'
else
    GREEN=''
    YELLOW=''
    CYAN=''
    RESET=''
fi

cd "$PROJECT_ROOT"

status() {
    echo -e "${GREEN}✅${RESET} $1"
}

warn() {
    echo -e "${YELLOW}⚠️${RESET} $1"
}

section() {
    echo -e "${CYAN}==>${RESET} $1"
}

require_repo_relative_path() {
    local path="$1"

    case "$path" in
        /*|../*|*/../*|..)
            warn "Refusing unsafe path outside repo: $path"
            return 1
            ;;
    esac
}

require_real_parent_dirs() {
    local path="$1"
    local parent="$(dirname "$path")"

    require_repo_relative_path "$path"

    while [ "$parent" != "." ] && [ "$parent" != "/" ]; do
        if [ -L "$parent" ]; then
            warn "Refusing path with symlink parent: $path"
            return 1
        fi
        parent="$(dirname "$parent")"
    done
}

symlink() {
    local target="$1"
    local link="$2"
    local current=""

    require_real_parent_dirs "$link"

    mkdir -p "$(dirname "$link")"

    if [ -L "$link" ]; then
        current="$(readlink "$link")"
        if [ "$current" = "$target" ]; then
            return 0
        fi
    fi

    if [ -d "$link" ] && [ ! -L "$link" ]; then
        warn "Refusing to replace directory $link"
        return 1
    fi

    if [ -e "$link" ] || [ -L "$link" ]; then
        rm -f "$link"
        ln -s "$target" "$link"
        status "Refreshed $link -> $target"
    else
        ln -s "$target" "$link"
        status "Linked $link -> $target"
    fi
}

stage_file() {
    local source_path="$1"
    local dest_path="$2"
    local label="$3"

    if [ ! -f "$source_path" ]; then
        warn "$label missing at ${source_path#$PROJECT_ROOT/}; skipping"
        return 0
    fi

    require_real_parent_dirs "$dest_path"

    if [ -L "$dest_path" ]; then
        warn "$label destination is a symlink at ${dest_path#$PROJECT_ROOT/}; refusing to overwrite"
        return 1
    fi

    mkdir -p "$(dirname "$dest_path")"

    if [ -f "$dest_path" ] && cmp -s "$source_path" "$dest_path"; then
        status "$label already staged"
        return 0
    fi

    cp "$source_path" "$dest_path"
    status "Staged $label"
}

echo "========================================"
echo " Red Bear OS Pre-Build Integration"
echo "========================================"
echo "Root: ${PROJECT_ROOT##*/}"
echo "Tag:  $REDBEAR_TAG"
echo ""

section "Ensuring custom recipe symlinks..."
symlink "../../local/recipes/branding/redbear-release" "recipes/branding/redbear-release"
symlink "../../local/recipes/drivers/linux-kpi" "recipes/drivers/linux-kpi"
symlink "../../local/recipes/drivers/redbear-btusb" "recipes/drivers/redbear-btusb"
symlink "../../local/recipes/drivers/redbear-iwlwifi" "recipes/drivers/redbear-iwlwifi"
symlink "../../local/recipes/drivers/redox-driver-sys" "recipes/drivers/redox-driver-sys"
symlink "../../local/recipes/gpu/amdgpu" "recipes/gpu/amdgpu"
symlink "../../local/recipes/gpu/redox-drm" "recipes/gpu/redox-drm"
symlink "../../local/recipes/libs/libqrencode" "recipes/libs/libqrencode"
symlink "../../local/recipes/system/evdevd" "recipes/system/evdevd"
symlink "../../local/recipes/system/redbear-firmware" "recipes/system/redbear-firmware"
symlink "../../local/recipes/system/firmware-loader" "recipes/system/firmware-loader"
symlink "../../local/recipes/system/iommu" "recipes/system/iommu"
symlink "../../local/recipes/system/redbear-btctl" "recipes/system/redbear-btctl"
symlink "../../local/recipes/system/redbear-info" "recipes/system/redbear-info"
symlink "../../local/recipes/system/redbear-hwutils" "recipes/system/redbear-hwutils"
symlink "../../local/recipes/system/redbear-netstat" "recipes/system/redbear-netstat"
symlink "../../local/recipes/system/redbear-netctl" "recipes/system/redbear-netctl"
symlink "../../local/recipes/system/redbear-netctl-console" "recipes/system/redbear-netctl-console"
symlink "../../local/recipes/system/redbear-wifictl" "recipes/system/redbear-wifictl"
symlink "../../local/recipes/system/redbear-traceroute" "recipes/system/redbear-traceroute"
symlink "../../local/recipes/system/redbear-mtr" "recipes/system/redbear-mtr"
symlink "../../local/recipes/system/redbear-nmap" "recipes/system/redbear-nmap"
symlink "../../local/recipes/system/redbear-meta" "recipes/system/redbear-meta"
symlink "../../local/recipes/system/udev-shim" "recipes/system/udev-shim"
symlink "../../local/recipes/core/ext4d" "recipes/core/ext4d"
symlink "../../local/recipes/tui/mc" "recipes/tui/mc"
symlink "../../local/recipes/system/cub" "recipes/system/cub"
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
status "Custom recipe symlinks ready"
echo ""

section "Ensuring recipe patch symlinks..."
symlink "../../../local/patches/kernel/redox.patch" "recipes/core/kernel/redox.patch"
symlink "../../../local/patches/base/redox.patch" "recipes/core/base/redox.patch"
if [ -d "recipes/core/installer" ]; then
    symlink "../../../local/patches/installer/redox.patch" "recipes/core/installer/redox.patch"
else
    warn "Installer recipe directory not present yet; skipping installer patch symlink"
fi
status "Recipe patch symlinks ready"
echo ""

section "Validating Red Bear configs..."
declare -a redbear_configs=(
    "config/redbear-desktop.toml"
    "config/redbear-minimal.toml"
    "config/redbear-bluetooth-experimental.toml"
    "config/redbear-full.toml"
    "config/redbear-wayland.toml"
    "config/redbear-live.toml"
)
declare -a found_configs=()

for config_path in "${redbear_configs[@]}"; do
    if [ -f "$config_path" ] || [ -L "$config_path" ]; then
        found_configs+=("${config_path#config/}")
    fi
done

if [ "${#found_configs[@]}" -gt 0 ]; then
    status "Found Red Bear config(s): ${found_configs[*]}"
else
    warn "No redbear config found in config/. Build may rely on a different config intentionally"
fi
echo ""

section "Validating branding assets..."
icon_path="local/Assets/images/Red Bear OS icon.png"
background_path="local/Assets/images/Red Bear OS loading background.png"
missing_assets=0

if [ ! -f "$icon_path" ]; then
    warn "Missing branding asset: $icon_path"
    missing_assets=1
fi
if [ ! -f "$background_path" ]; then
    warn "Missing branding asset: $background_path"
    missing_assets=1
fi
if [ "$missing_assets" -eq 0 ]; then
    status "Branding assets present"
fi
echo ""

section "Checking AMD firmware blobs..."
shopt -s nullglob
firmware_blobs=(local/firmware/amdgpu/*.bin)
shopt -u nullglob

if [ "${#firmware_blobs[@]}" -gt 0 ]; then
    status "Found ${#firmware_blobs[@]} AMD firmware blob(s)"
else
    warn "No AMD firmware blobs found in local/firmware/amdgpu/"
fi
echo ""

section "Staging branding assets into recipe source..."
stage_file "$icon_path" "local/recipes/branding/redbear-release/source/images/icon.png" "branding icon"
stage_file "$background_path" "local/recipes/branding/redbear-release/source/images/loading-background.png" "branding loading background"
echo ""

section "Staging firmware blobs into firmware-loader recipe source..."
require_real_parent_dirs "local/recipes/system/firmware-loader/source/firmware/amdgpu/.guard"
mkdir -p "local/recipes/system/firmware-loader/source/firmware/amdgpu"
shopt -s nullglob
staged_firmware=(local/recipes/system/firmware-loader/source/firmware/amdgpu/*.bin)
shopt -u nullglob

if [ "${#staged_firmware[@]}" -gt 0 ]; then
    rm -f "${staged_firmware[@]}"
fi

if [ "${#firmware_blobs[@]}" -gt 0 ]; then
    cp "${firmware_blobs[@]}" "local/recipes/system/firmware-loader/source/firmware/amdgpu/"
    status "Staged ${#firmware_blobs[@]} AMD firmware blob(s)"
else
    warn "Skipping firmware staging because no AMD firmware blobs were found"
fi
echo ""

section "Updating build tag..."
require_repo_relative_path "$REDBEAR_TAG"
case "$REDBEAR_TAG" in
    build/*)
        ;;
    *)
        warn "Refusing tag path outside build/: $REDBEAR_TAG"
        exit 1
        ;;
esac
require_real_parent_dirs "$REDBEAR_TAG"
mkdir -p "$(dirname "$REDBEAR_TAG")"
touch "$REDBEAR_TAG"
status "Updated tag file: $REDBEAR_TAG"
echo ""
status "Red Bear integration complete"
exit 0
