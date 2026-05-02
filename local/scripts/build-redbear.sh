#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
source "$SCRIPT_DIR/lib/relibc-surface.sh"

# Source .config for release mode settings (REDBEAR_RELEASE, etc.)
if [ -f "$PROJECT_ROOT/.config" ]; then
    while IFS= read -r line; do
        line="${line%%#*}"
        line=$(echo "$line" | xargs)
        [ -z "$line" ] && continue
        if [[ "$line" == *"?="* ]]; then
            key="${line%%\?=*}"
            value="${line#*\?=}"
        elif [[ "$line" == *"="* ]]; then
            key="${line%%=*}"
            value="${line#*=}"
        else
            continue
        fi
        key=$(echo "$key" | xargs)
        value=$(echo "$value" | xargs)
        [ -z "$key" ] && continue
        # Only set if not already set in environment
        [ -n "${!key:-}" ] || export "$key=$value"
    done < "$PROJECT_ROOT/.config"
fi

CONFIG="redbear-full"
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
  redbear-full        Desktop/graphics target (default)
  redbear-mini        Text-only console/recovery target
  redbear-grub        Text-only with GRUB boot manager
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
    redbear-full|redbear-mini|redbear-grub)
        ;;
    *)
        echo "ERROR: Unknown config '$CONFIG'" >&2
        echo "Supported: redbear-full, redbear-mini, redbear-grub" >&2
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

if [ -x "$PROJECT_ROOT/local/scripts/verify-overlay-integrity.sh" ] && [ -z "${REDBEAR_RELEASE:-}" ]; then
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

if [ "$APPLY_PATCHES" = "1" ] && [ -z "${REDBEAR_RELEASE:-}" ]; then
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

    stash_nested_repo_if_dirty "$PROJECT_ROOT/recipes/core/relibc/source" "relibc"
    echo ""
elif [ -n "${REDBEAR_RELEASE:-}" ]; then
    echo ">>> Release mode: skipping patch application (patches pre-applied in archived sources)"
fi

if [ ! -f "target/release/repo" ]; then
    echo ">>> Building cookbook binary..."
    cargo build --release
fi

if [ "$CONFIG" = "redbear-full" ]; then
    redbear_ensure_relibc_desktop_surface
fi

FW_AMD_DIR="$PROJECT_ROOT/local/firmware/amdgpu"
if [ "$CONFIG" = "redbear-full" ]; then
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

if [ -n "${REDBEAR_RELEASE:-}" ]; then
    bash "$PROJECT_ROOT/local/scripts/build-release-mode.sh" --release="$REDBEAR_RELEASE" --config="$CONFIG" --extra-package=relibc
fi

bash "$PROJECT_ROOT/local/scripts/build-preflight.sh" --config="$CONFIG" ${REDBEAR_RELEASE:+--release="$REDBEAR_RELEASE"} --extra-package=relibc

if [ "${REDBEAR_ALLOW_UPSTREAM:-0}" = "1" ]; then
    echo ">>> WARNING: Upstream fetch ENABLED (REDBEAR_ALLOW_UPSTREAM=1)"
    REPO_OFFLINE=0 COOKBOOK_OFFLINE=false CI=1 make all "CONFIG_NAME=$CONFIG" "JOBS=$JOBS"
elif [ -n "${REDBEAR_RELEASE:-}" ]; then
    echo ">>> Release mode: building from immutable archives (offline)"
    REPO_OFFLINE=1 COOKBOOK_OFFLINE=true CI=1 make all "CONFIG_NAME=$CONFIG" "JOBS=$JOBS"
elif [ "$ALLOW_UPSTREAM" -eq 1 ]; then
    echo ">>> Upstream recipe refresh enabled"
    REPO_OFFLINE=0 COOKBOOK_OFFLINE=false CI=1 make all "CONFIG_NAME=$CONFIG" "JOBS=$JOBS"
else
    echo ">>> Upstream recipe refresh disabled (default: offline)"
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
echo ""
echo "To build live ISO:"
echo "  scripts/build-iso.sh $CONFIG"
echo ""
echo "To write a real bare-metal image to USB (verify device first!):"
echo "  dd if=build/$ARCH/$CONFIG/harddrive.img of=/dev/sdX bs=4M status=progress"
