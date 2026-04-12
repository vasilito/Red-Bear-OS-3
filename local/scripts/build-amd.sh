#!/usr/bin/env bash
# Build Red Bear OS with AMD GPU support (Phase P2)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

CONFIG="${1:-my-amd-desktop}"
JOBS="${JOBS:-$(nproc)}"
APPLY_PATCHES="${APPLY_PATCHES:-1}"

echo "=== Red Bear OS AMD GPU Build ==="
echo "Config:        $CONFIG"
echo "Jobs:          $JOBS"
echo "Apply patches: $APPLY_PATCHES"
echo "Root:          $PROJECT_ROOT"
echo ""

cd "$PROJECT_ROOT"

# Step 0: Apply local patches
if [ "$APPLY_PATCHES" = "1" ]; then
    echo ">>> Applying local patches..."

    apply_patch_dir() {
        local patch_dir="$1"
        local target_dir="$2"
        local label="$3"

        if [ ! -d "$patch_dir" ]; then
            return 0
        fi

        for patch_file in $(ls "$patch_dir"/*.patch 2>/dev/null | sort); do
            patch_name=$(basename "$patch_file")
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

    apply_patch_dir "$PROJECT_ROOT/local/patches/kernel"  "$PROJECT_ROOT/recipes/core/kernel/source"  "kernel"
    apply_patch_dir "$PROJECT_ROOT/local/patches/base"    "$PROJECT_ROOT/recipes/core/base/source"     "base"
    apply_patch_dir "$PROJECT_ROOT/local/patches/relibc"  "$PROJECT_ROOT/recipes/core/relibc/source"   "relibc"
    apply_patch_dir "$PROJECT_ROOT/local/patches/bootloader" "$PROJECT_ROOT/recipes/core/bootloader/source" "bootloader"
    apply_patch_dir "$PROJECT_ROOT/local/patches/installer"  "$PROJECT_ROOT/recipes/core/installer/source" "installer"
    echo ""
fi

# Step 1: Build cookbook binary if needed
if [ ! -f "target/release/repo" ]; then
    echo ">>> Building cookbook binary..."
    cargo build --release
fi

# Step 2: Fetch AMD firmware blobs if missing
FW_DIR="$PROJECT_ROOT/local/firmware/amdgpu"
if [ -z "$(ls -A "$FW_DIR" 2>/dev/null)" ]; then
    echo ">>> AMD firmware blobs not found. Run local/scripts/fetch-firmware.sh first."
    echo "    Skipping firmware fetch. Driver will NOT function without firmware."
else
    FW_COUNT=$(ls "$FW_DIR"/*.bin 2>/dev/null | wc -l)
    echo ">>> Found $FW_COUNT AMD firmware blobs"
fi

# Step 3: Build
echo ">>> Building Red Bear OS with config: $CONFIG"
echo ">>> This may take 30-60 minutes on first build..."
CI=1 make all "CONFIG_NAME=$CONFIG" "JOBS=$JOBS"

echo ""
echo "=== Build Complete ==="
echo "Image: build/x86_64/harddrive.img"
echo ""
echo "To run in QEMU:"
echo "  make qemu QEMUFLAGS=\"-m 4G\""
echo ""
echo "To test on bare metal:"
echo "  dd if=build/x86_64/harddrive.img of=/dev/sdX bs=4M status=progress"
