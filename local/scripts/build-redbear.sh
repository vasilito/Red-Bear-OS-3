#!/usr/bin/env bash
# build-redbear.sh — Build Red Bear OS from upstream base + Red Bear overlay
#
# Usage:
#   ./local/scripts/build-redbear.sh                     # Default: redbear-desktop
#   ./local/scripts/build-redbear.sh redbear-minimal     # Minimal validation baseline
#   ./local/scripts/build-redbear.sh redbear-full        # Full Red Bear integration target
#   ./local/scripts/build-redbear.sh redbear-wayland     # Wayland runtime validation profile
#   ./local/scripts/build-redbear.sh redbear-kde         # KDE Plasma bring-up target
#   ./local/scripts/build-redbear.sh redbear-live        # Live ISO variant
#   APPLY_PATCHES=0 ./local/scripts/build-redbear.sh     # Skip patch application
#
# This script assumes the Red Bear overlay model:
# - upstream-owned sources are refreshable working trees
# - Red Bear-owned shipping deltas live in local/patches/ and local/recipes/
# - upstream WIP recipes are not trusted as stable shipping inputs until upstream promotes them
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

CONFIG="${1:-redbear-desktop}"
JOBS="${JOBS:-$(nproc)}"
APPLY_PATCHES="${APPLY_PATCHES:-1}"

case "$CONFIG" in
    redbear-desktop|redbear-minimal|redbear-full|redbear-wayland|redbear-kde|redbear-live)
        ;;
    *)
        echo "ERROR: Unknown config '$CONFIG'"
        echo "Supported: redbear-desktop, redbear-minimal, redbear-full, redbear-wayland, redbear-kde, redbear-live"
        exit 1
        ;;
esac

echo "========================================"
echo "       Red Bear OS Build System"
echo "========================================"
echo "Config:        $CONFIG"
echo "Jobs:          $JOBS"
echo "Apply patches: $APPLY_PATCHES"
echo "Root:          ${PROJECT_ROOT##*/}"
echo "========================================"
echo ""

cd "$PROJECT_ROOT"

stash_nested_repo_if_dirty() {
    local target_dir="$1"
    local label="$2"
    if [ -d "$target_dir/.git" ]; then
        if ! git -C "$target_dir" diff --quiet || ! git -C "$target_dir" diff --cached --quiet || [ -n "$(git -C "$target_dir" ls-files --others --exclude-standard)" ]; then
            echo ">>> Stashing dirty nested $label checkout before build..."
            rm -f "$target_dir/.git/index.lock"
            git -C "$target_dir" stash push --all -m "build-redbear-auto-stash" > /dev/null 2>&1 || true
        fi
    fi
}

stash_nested_repo_if_dirty "$PROJECT_ROOT/recipes/core/relibc/source" "relibc"

# Step 0: Apply local patches
if [ "$APPLY_PATCHES" = "1" ]; then
    echo ">>> Applying local patches..."

    apply_patch_dir() {
        local patch_dir="$1"
        local target_dir="$2"
        local label="$3"

        if [ "$label" = "relibc" ] && [ -d "$target_dir/.git" ]; then
            if ! git -C "$target_dir" diff --quiet || ! git -C "$target_dir" diff --cached --quiet || [ -n "$(git -C "$target_dir" ls-files --others --exclude-standard)" ]; then
                echo "    STASH relibc source (dirty nested checkout)"
                rm -f "$target_dir/.git/index.lock"
                git -C "$target_dir" stash push --all -m "build-redbear-auto-stash" > /dev/null 2>&1 || true
            fi
        fi

        if [ ! -d "$patch_dir" ]; then
            return 0
        fi

        for patch_file in "$patch_dir"/*.patch; do
            [ -f "$patch_file" ] || continue
            patch_name=$(basename "$patch_file")

            if [ "$label" = "base" ] && [ "$patch_name" = "P0-acpid-power-methods.patch" ]; then
                acpid_file="$target_dir/drivers/acpid/src/acpi.rs"
                if [ -f "$acpid_file" ] && grep -q "pub fn evaluate_acpi_method(" "$acpid_file"; then
                    echo "    SKIP $patch_name (ACPI power helper methods already present)"
                    continue
                fi
            fi

            if [ ! -d "$target_dir" ]; then
                echo "    SKIP $patch_name ($label source not fetched yet)"
                continue
            fi
            if patch --dry-run -p1 -d "$target_dir" < "$patch_file" > /dev/null 2>&1; then
                patch -p1 -d "$target_dir" < "$patch_file" > /dev/null 2>&1
                echo "    OK   $patch_name"
            else
                echo "    SKIP $patch_name (already applied or won't apply)"
            fi
        done
    }

    apply_patch_dir "$PROJECT_ROOT/local/patches/kernel"     "$PROJECT_ROOT/recipes/core/kernel/source"     "kernel"
    apply_patch_dir "$PROJECT_ROOT/local/patches/base"       "$PROJECT_ROOT/recipes/core/base/source"        "base"
    apply_patch_dir "$PROJECT_ROOT/local/patches/relibc"     "$PROJECT_ROOT/recipes/core/relibc/source"      "relibc"
    apply_patch_dir "$PROJECT_ROOT/local/patches/bootloader" "$PROJECT_ROOT/recipes/core/bootloader/source"  "bootloader"
    apply_patch_dir "$PROJECT_ROOT/local/patches/installer"  "$PROJECT_ROOT/recipes/core/installer/source"   "installer"

    # repo cook refetches nested sources before building; keep relibc clean after patch application
    stash_nested_repo_if_dirty "$PROJECT_ROOT/recipes/core/relibc/source" "relibc"
    echo ""
fi

# Step 1: Build cookbook binary
if [ ! -f "target/release/repo" ]; then
    echo ">>> Building cookbook binary..."
    cargo build --release
fi

# Step 2: Check firmware
FW_AMD_DIR="$PROJECT_ROOT/local/firmware/amdgpu"
if [ "$CONFIG" != "redbear-minimal" ]; then
    if [ -d "$FW_AMD_DIR" ] && [ -n "$(ls -A "$FW_AMD_DIR" 2>/dev/null)" ]; then
        FW_COUNT=$(ls "$FW_AMD_DIR"/*.bin 2>/dev/null | wc -l)
        echo ">>> Found $FW_COUNT AMD firmware blobs"
    else
        echo ">>> WARNING: No AMD firmware blobs found."
        echo "    Run: ./local/scripts/fetch-firmware.sh"
        echo "    GPU driver will NOT function without firmware."
    fi
    echo ""
fi

# Step 3: Build
echo ">>> Building Red Bear OS with config: $CONFIG"
echo ">>> This may take 30-60 minutes on first build..."
CI=1 make all "CONFIG_NAME=$CONFIG" "JOBS=$JOBS"

# Step 4: Report
ARCH="${ARCH:-$(uname -m)}"
echo ""
echo "========================================"
echo "         Build Complete!"
echo "========================================"
echo "Image: build/$ARCH/$CONFIG/harddrive.img"
echo ""
echo "To run in QEMU:"
echo "  make qemu QEMUFLAGS=\"-m 4G\""
if [ "$CONFIG" = "redbear-minimal" ] || [ "$CONFIG" = "redbear-desktop" ]; then
    echo ""
    echo "To validate the Phase 2 VM network baseline:"
    echo "  ./local/scripts/validate-vm-network-baseline.sh"
    echo "  ./local/scripts/test-vm-network-qemu.sh $CONFIG"
fi
if [ "$CONFIG" = "redbear-wayland" ]; then
    echo ""
    echo "To validate the Phase 4 Wayland runtime path:"
    echo "  ./local/scripts/test-phase4-wayland-qemu.sh"
fi
echo ""
echo "To build live ISO:"
echo "  make live CONFIG_NAME=$CONFIG"
echo ""
echo "To burn to USB (verify device first!):"
echo "  dd if=build/$ARCH/$CONFIG/harddrive.img of=/dev/sdX bs=4M status=progress"
