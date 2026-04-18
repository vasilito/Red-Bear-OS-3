#!/usr/bin/env bash
# build-redbear.sh — Build Red Bear OS from upstream base + Red Bear overlay
#
# Usage:
#   ./local/scripts/build-redbear.sh                                # Default: redbear-kde
#   ./local/scripts/build-redbear.sh redbear-minimal                # Minimal validation baseline
#   ./local/scripts/build-redbear.sh redbear-bluetooth-experimental # First bounded Bluetooth slice
#   ./local/scripts/build-redbear.sh redbear-full                   # Full Red Bear integration target
#   ./local/scripts/build-redbear.sh redbear-wayland                # Bounded Wayland runtime validation profile
#   ./local/scripts/build-redbear.sh redbear-kde                    # Tracked KWin Wayland desktop target
#   ./local/scripts/build-redbear.sh redbear-live                   # Live ISO variant
#   ./local/scripts/build-redbear.sh --upstream redbear-kde         # Allow Redox/upstream recipe refresh
#   APPLY_PATCHES=0 ./local/scripts/build-redbear.sh                # Skip patch application
#
# This script assumes the Red Bear overlay model:
# - upstream-owned sources are refreshable working trees
# - Red Bear-owned shipping deltas live in local/patches/ and local/recipes/
# - upstream WIP recipes are not trusted as stable shipping inputs until upstream promotes them
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

CONFIG="redbear-kde"
JOBS="${JOBS:-$(nproc)}"
APPLY_PATCHES="${APPLY_PATCHES:-1}"
ALLOW_UPSTREAM=0

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS] [CONFIG]

Build a tracked Red Bear OS profile.

Options:
  --upstream          Allow Redox/upstream recipe source refresh during build
  -h, --help          Show this help

Configs:
  redbear-desktop, redbear-minimal, redbear-bluetooth-experimental,
  redbear-full, redbear-wayland, redbear-kde, redbear-live
EOF
}

POSITIONAL=()
while [ $# -gt 0 ]; do
    case "$1" in
        --upstream)
            ALLOW_UPSTREAM=1
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        -*)
            echo "Unknown option: $1" >&2
            usage >&2
            exit 1
            ;;
        *)
            POSITIONAL+=("$1")
            ;;
    esac
    shift
done

if [ ${#POSITIONAL[@]} -gt 1 ]; then
    echo "ERROR: Too many positional arguments" >&2
    usage >&2
    exit 1
fi

[ ${#POSITIONAL[@]} -eq 1 ] && CONFIG="${POSITIONAL[0]}"

case "$CONFIG" in
    redbear-desktop|redbear-minimal|redbear-bluetooth-experimental|redbear-full|redbear-wayland|redbear-kde|redbear-live)
        ;;
    *)
        echo "ERROR: Unknown config '$CONFIG'"
        echo "Supported: redbear-desktop, redbear-minimal, redbear-bluetooth-experimental, redbear-full, redbear-wayland, redbear-kde, redbear-live"
        exit 1
        ;;
esac

echo "========================================"
echo "       Red Bear OS Build System"
echo "========================================"
echo "Config:        $CONFIG"
echo "Jobs:          $JOBS"
echo "Apply patches: $APPLY_PATCHES"
echo "Upstream:      $ALLOW_UPSTREAM"
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

    # repo cook can refetch nested sources when --upstream is enabled; keep relibc clean after patch application
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
if [ "$ALLOW_UPSTREAM" -eq 1 ]; then
    echo ">>> Upstream recipe refresh enabled"
    REPO_OFFLINE=0 COOKBOOK_OFFLINE=false CI=1 make all "CONFIG_NAME=$CONFIG" "JOBS=$JOBS"
else
    echo ">>> Upstream recipe refresh disabled (pass --upstream to enable)"
    REPO_OFFLINE=1 COOKBOOK_OFFLINE=true CI=1 make all "CONFIG_NAME=$CONFIG" "JOBS=$JOBS"
fi

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
if [ "$CONFIG" = "redbear-desktop" ] || [ "$CONFIG" = "redbear-full" ] || [ "$CONFIG" = "redbear-wayland" ] || [ "$CONFIG" = "redbear-kde" ]; then
    echo ""
    echo "To validate bounded low-level controller proofs:"
    echo "  ./local/scripts/test-lowlevel-controllers-qemu.sh $CONFIG"
    echo "  # or run individual checks: test-xhci-irq-qemu.sh, test-iommu-qemu.sh, test-ps2-qemu.sh, test-timer-qemu.sh"
    echo ""
    echo "To validate bounded USB maturity proofs:"
    echo "  ./local/scripts/test-usb-maturity-qemu.sh $CONFIG"
    echo "  # or run individual checks: test-usb-qemu.sh --check, test-usb-storage-qemu.sh"
fi
if [ "$CONFIG" = "redbear-wayland" ]; then
    echo ""
    echo "To validate the bounded Phase 4 Wayland runtime harness:"
    echo "  ./local/scripts/test-phase4-wayland-qemu.sh"
    echo "  # in guest: redbear-drm-display-check --vendor amd|intel"
fi
if [ "$CONFIG" = "redbear-kde" ]; then
    echo ""
    echo "To validate the primary KWin Wayland desktop path:"
    echo "  ./local/scripts/test-phase6-kde-qemu.sh --check"
    echo "  # in guest: redbear-drm-display-check --vendor amd|intel"
fi
echo ""
echo "To build live ISO:"
echo "  make live CONFIG_NAME=$CONFIG"
echo ""
echo "To burn to USB (verify device first!):"
echo "  dd if=build/$ARCH/$CONFIG/harddrive.img of=/dev/sdX bs=4M status=progress"
