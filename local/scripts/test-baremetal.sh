#!/usr/bin/env bash
# Build and burn a Red Bear OS hard drive image for bare-metal AMD testing
# Requires explicit target device selection and write permissions

set -euo pipefail

REDOX_ROOT="$(dirname "$0")/../.."
REDOX_ROOT="$(cd "$REDOX_ROOT" && pwd)"
IMAGE_PATH="$REDOX_ROOT/build/harddrive.img"

# Auto-disable TUI when stdout is not a terminal (prevents repo cook panic)
if [ -z "${CI:-}" ] && { [ ! -t 0 ] || [ ! -t 1 ]; }; then
    export CI=1
fi

CONFIG="my-amd-desktop"
DEVICE=""
DRY_RUN=0
SKIP_BUILD=0
VERIFY_BURN=0

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS] [CONFIG_NAME]

Build and burn a Red Bear OS bare-metal test image to a block device.

Arguments:
  CONFIG_NAME           Red Bear OS config to build (default: my-amd-desktop)

Options:
  --device PATH         Target block device to overwrite (required)
  --skip-build          Skip 'make all CONFIG_NAME=...'
  --verify              Verify the written image with cmp after dd
  --dry-run             Show actions without building or writing
  -h, --help            Show this help text

Notes:
  - This script never auto-detects a target device.
  - Run it with permissions that can write to the selected block device.
  - Expected image path: build/harddrive.img
EOF
}

run_cmd() {
    if [ "$DRY_RUN" -eq 1 ]; then
        printf '[dry-run]'
        printf ' %q' "$@"
        printf '\n'
    else
        "$@"
    fi
}

show_available_devices() {
    echo "=== Available block devices ==="
    lsblk -e7 -o NAME,PATH,SIZE,MODEL,TRAN,TYPE,MOUNTPOINTS
    echo ""
}

warn_if_system_disk() {
    local target_path="$1"
    local target_name parent_name mount_info root_source root_parent

    target_name="$(basename "$target_path")"
    parent_name="$(lsblk -no PKNAME "$target_path" 2>/dev/null | head -n 1 || true)"
    mount_info="$(lsblk -nr -o PATH,MOUNTPOINTS "$target_path" 2>/dev/null || true)"
    root_source="$(findmnt -n -o SOURCE / 2>/dev/null || true)"
    root_parent=""

    if [ -n "$root_source" ] && [ -b "$root_source" ]; then
        root_parent="$(lsblk -no PKNAME "$root_source" 2>/dev/null | head -n 1 || true)"
    fi

    if printf '%s\n' "$mount_info" | grep -Eq '(/|/boot|/home|\[SWAP\])'; then
        echo "WARNING: $target_path or one of its partitions appears to be mounted."
        echo "$mount_info"
    fi

    if [ -n "$root_source" ]; then
        if [ "$root_source" = "$target_path" ] || [ "/dev/$parent_name" = "$target_path" ] || [ "$target_name" = "$root_parent" ]; then
            echo "WARNING: $target_path appears related to the current root device ($root_source)."
        fi
    fi
}

refuse_unsafe_device() {
    local target_path="$1"
    local target_name

    target_name="$(basename "$target_path")"

    case "$target_name" in
        sda|hda|vda|xvda|mmcblk0|nvme0|nvme0n1)
            echo "ERROR: Refusing to write to likely system disk $target_path"
            exit 1
            ;;
    esac
}

confirm_write() {
    local prompt="$1"
    local reply

    if [ "$DRY_RUN" -eq 1 ]; then
        echo "[dry-run] Confirmation skipped: $prompt"
        return
    fi

    read -r -p "$prompt [y/N]: " reply
    case "$reply" in
        y|Y|yes|YES)
            ;;
        *)
            echo "Aborted."
            exit 1
            ;;
    esac
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --device)
            if [ "$#" -lt 2 ]; then
                echo "ERROR: --device requires a path"
                usage
                exit 1
            fi
            DEVICE="$2"
            shift 2
            ;;
        --skip-build)
            SKIP_BUILD=1
            shift
            ;;
        --verify)
            VERIFY_BURN=1
            shift
            ;;
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        --*)
            echo "ERROR: Unknown option: $1"
            usage
            exit 1
            ;;
        *)
            CONFIG="$1"
            shift
            ;;
    esac
done

echo "=== Red Bear OS Bare-Metal AMD Test Image Burner ==="
echo "Config: $CONFIG"
echo "Image:  $IMAGE_PATH"
echo "Device: ${DEVICE:-<not set>}"
echo ""

if [ -z "$DEVICE" ]; then
    echo "ERROR: You must specify a target block device with --device"
    echo ""
    usage
    exit 1
fi

show_available_devices

if [ ! -e "$DEVICE" ]; then
    echo "ERROR: Target device does not exist: $DEVICE"
    exit 1
fi

if [ ! -b "$DEVICE" ]; then
    echo "ERROR: Target path is not a block device: $DEVICE"
    exit 1
fi

if [ "$(lsblk -dn -o TYPE "$DEVICE")" != "disk" ]; then
    echo "ERROR: Target must be a whole-disk block device, not a partition: $DEVICE"
    exit 1
fi

refuse_unsafe_device "$DEVICE"
warn_if_system_disk "$DEVICE"

if [ "$SKIP_BUILD" -eq 0 ]; then
    echo "=== Building Red Bear OS image ==="
    run_cmd make -C "$REDOX_ROOT" all CONFIG_NAME="$CONFIG" CI=1
else
    echo "=== Skipping build step ==="
fi

echo "=== Checking image ==="
if [ ! -f "$IMAGE_PATH" ]; then
    echo "ERROR: Red Bear OS image not found: $IMAGE_PATH"
    exit 1
fi

IMAGE_SIZE_BYTES="$(stat -c %s "$IMAGE_PATH")"
echo "Image size: $IMAGE_SIZE_BYTES bytes"
echo ""

echo "About to write $IMAGE_PATH to $DEVICE"
echo "This will overwrite all data on the target device."
confirm_write "Continue with dd write?"

echo "=== Writing image to device ==="
run_cmd dd if="$IMAGE_PATH" of="$DEVICE" bs=4M conv=fsync status=progress

echo "=== Synchronizing device ==="
run_cmd sync

if [ "$VERIFY_BURN" -eq 1 ]; then
    echo "=== Verifying written image ==="
    run_cmd cmp -n "$IMAGE_SIZE_BYTES" "$IMAGE_PATH" "$DEVICE"
    echo "Verification completed successfully."
fi

echo ""
echo "=== Next steps ==="
echo "1. Safely eject or unplug the target device if your host requires it."
echo "2. Insert the device into the AMD test machine and boot from it in UEFI mode."
echo "3. Capture serial output during boot if available to diagnose early failures."
echo "4. Check ACPI, SMP, framebuffer, and storage initialization on real hardware."
echo "5. If validating the Intel Wi-Fi path, run: redbear-phase5-wifi-check"
echo "6. For the strongest bounded Wi-Fi runtime path, also run: test-wifi-baremetal-runtime.sh"
echo "7. Preserve /tmp/redbear-phase5-wifi-capture.json from the target after the run."
echo ""
echo "If you need serial logs, connect your serial console before powering on the target system."
