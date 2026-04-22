#!/usr/bin/env bash
# build-redbear.sh — Build Red Bear OS from upstream base + Red Bear overlay
#
# Usage:
#   ./local/scripts/build-redbear.sh                                # Default: redbear-full
#   ./local/scripts/build-redbear.sh redbear-mini                   # Minimal validation baseline
#   ./local/scripts/build-redbear.sh redbear-full                   # Full Red Bear desktop/session target
#   ./local/scripts/build-redbear.sh redbear-live                   # Canonical full live profile config
#   ./local/scripts/build-redbear.sh redbear-live-mini              # Text-only mini live profile config
#   ./local/scripts/build-redbear.sh redbear-grub-live-mini         # Text-only GRUB mini live profile config
#   ./local/scripts/build-redbear.sh --upstream redbear-full        # Allow Redox/upstream recipe refresh
#   APPLY_PATCHES=0 ./local/scripts/build-redbear.sh                # Skip patch application
#
# This script assumes the Red Bear overlay model:
# - upstream-owned sources are refreshable working trees
# - Red Bear-owned shipping deltas live in local/patches/ and local/recipes/
# - upstream WIP recipes are not trusted as stable shipping inputs until upstream promotes them
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

CONFIG="redbear-full"
JOBS="${JOBS:-$(nproc)}"
APPLY_PATCHES="${APPLY_PATCHES:-1}"
ALLOW_UPSTREAM=0

canonicalize_config() {
    case "$1" in
        redbear-mini)
            printf '%s\n' "redbear-minimal"
            ;;
        redbear-live-full)
            printf '%s\n' "redbear-live"
            ;;
        redbear-live-mini-grub)
            printf '%s\n' "redbear-grub-live-mini"
            ;;
        *)
            printf '%s\n' "$1"
            ;;
    esac
}

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS] [CONFIG]

Build a tracked Red Bear OS profile.

Options:
  --upstream          Allow Redox/upstream recipe source refresh during build
  -h, --help          Show this help

Configs:
  redbear-mini, redbear-full, redbear-live, redbear-live-mini, redbear-grub-live-mini
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

CONFIG="$(canonicalize_config "$CONFIG")"

case "$CONFIG" in
    redbear-minimal)
        ;;
    redbear-live)
        ;;
    redbear-live-mini)
        ;;
    redbear-grub-live-mini)
        ;;
    redbear-full)
        ;;
    *)
        echo "ERROR: Unknown config '$CONFIG'"
        echo "Supported: redbear-mini, redbear-full, redbear-live, redbear-live-mini, redbear-grub-live-mini"
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

if [ -x "$PROJECT_ROOT/local/scripts/verify-overlay-integrity.sh" ]; then
    echo ">>> Verifying overlay integrity (auto-repair)..."
    "$PROJECT_ROOT/local/scripts/verify-overlay-integrity.sh" --repair
    echo ""
fi

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

ensure_relibc_desktop_surface() {
    local relibc_target="$PROJECT_ROOT/recipes/core/relibc/target/x86_64-unknown-redox"
    local relibc_stage_include="$relibc_target/stage/usr/include"
    local relibc_stage_lib="$relibc_target/stage/usr/lib/libc.so"

    if [ ! -f "$relibc_stage_include/sys/signalfd.h" ] || \
       [ ! -f "$relibc_stage_include/sys/timerfd.h" ] || \
       [ ! -f "$relibc_stage_include/sys/eventfd.h" ] || \
       [ ! -f "$relibc_stage_lib" ] || \
       ! readelf -Ws "$relibc_stage_lib" | grep -q '_Z7strtoldPKcPPc'; then
        echo ">>> Refreshing relibc staged surface for full desktop target..."
        rm -rf \
            "$relibc_target/build" \
            "$relibc_target/stage" \
            "$relibc_target/stage.tmp" \
            "$relibc_target/sysroot"
        rm -f \
            "$relibc_target/auto_deps.toml" \
            "$relibc_target/stage.pkgar" \
            "$relibc_target/stage.toml"
        REPO_OFFLINE=1 COOKBOOK_OFFLINE=true CI=1 ./target/release/repo cook relibc
        echo ""
    fi
}

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

if [ -x "$PROJECT_ROOT/local/scripts/verify-overlay-integrity.sh" ]; then
    echo ">>> Verifying overlay integrity (strict)..."
    "$PROJECT_ROOT/local/scripts/verify-overlay-integrity.sh"
    echo ""
fi

if [ ! -f "target/release/repo" ]; then
    echo ">>> Building cookbook binary..."
    cargo build --release
fi

if [ "$CONFIG" = "redbear-full" ] || [ "$CONFIG" = "redbear-live" ]; then
    ensure_relibc_desktop_surface
fi

FW_AMD_DIR="$PROJECT_ROOT/local/firmware/amdgpu"
if [ "$CONFIG" != "redbear-minimal" ] && [ "$CONFIG" != "redbear-live-mini" ] && [ "$CONFIG" != "redbear-grub-live-mini" ]; then
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

echo ">>> Building Red Bear OS with config: $CONFIG"
echo ">>> This may take 30-60 minutes on first build..."
if [ "$ALLOW_UPSTREAM" -eq 1 ]; then
    echo ">>> Upstream recipe refresh enabled"
    REPO_OFFLINE=0 COOKBOOK_OFFLINE=false CI=1 make all "CONFIG_NAME=$CONFIG" "JOBS=$JOBS"
else
    echo ">>> Upstream recipe refresh disabled (pass --upstream to enable)"
    REPO_OFFLINE=1 COOKBOOK_OFFLINE=true CI=1 make all "CONFIG_NAME=$CONFIG" "JOBS=$JOBS"
fi

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
echo "  # live .iso outputs are for real bare metal, not VM/QEMU use"
echo ""
echo "To write a real bare-metal image to USB (verify device first!):"
echo "  dd if=build/$ARCH/$CONFIG/harddrive.img of=/dev/sdX bs=4M status=progress"
