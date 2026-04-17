#!/bin/bash
# install-grub.sh — Install GRUB bootloader into a Red Bear OS disk image
#
# Boot sequence after installation:
#   UEFI firmware → GRUB (menu) → chainload Redox bootloader → kernel → Red Bear OS
#
# Usage:
#   ./local/scripts/install-grub.sh <harddrive.img>
#   ./local/scripts/install-grub.sh build/x86_64/harddrive.img
#
# Prerequisites:
#   - GRUB recipe built: make r.grub
#   - ESP partition >= 8 MiB (set efi_partition_size = 16 in config)
#   - Python 3 (for fat_tool.py — no mtools needed)

set -euo pipefail

IMAGE="${1:?Usage: $0 <harddrive.img>}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

ESP_LBA=2048
ESP_SECTOR_SIZE=512
ESP_OFFSET=$((ESP_LBA * ESP_SECTOR_SIZE))

FAT_TOOL="${SCRIPT_DIR}/fat_tool.py"
ESP="${IMAGE}@${ESP_OFFSET}"

if [ ! -f "${IMAGE}" ]; then
    echo "ERROR: Image file not found: ${IMAGE}" >&2
    exit 1
fi

if ! command -v python3 &>/dev/null; then
    echo "ERROR: python3 not found" >&2
    exit 1
fi

GRUB_EFI=""
GRUB_CFG=""

# Search in recipe stage directory (cookbook stores output here)
for f in $(find "${REPO_ROOT}/local/recipes/core/grub/target" -path "*/stage/usr/lib/boot/grub.efi" 2>/dev/null); do
    GRUB_EFI="${f}"
    break
done

for f in $(find "${REPO_ROOT}/local/recipes/core/grub/target" -path "*/stage/usr/lib/boot/grub.cfg" 2>/dev/null); do
    GRUB_CFG="${f}"
    break
done

# Fallback: search repo extracted packages
if [ -z "${GRUB_EFI}" ]; then
    for f in $(find "${REPO_ROOT}/repo" -path "*/grub/*/usr/lib/boot/grub.efi" 2>/dev/null); do
        GRUB_EFI="${f}"
        break
    done
fi

if [ -z "${GRUB_CFG}" ]; then
    for f in $(find "${REPO_ROOT}/repo" -path "*/grub/*/usr/lib/boot/grub.cfg" 2>/dev/null); do
        GRUB_CFG="${f}"
        break
    done
fi

if [ -z "${GRUB_CFG}" ] && [ -f "${REPO_ROOT}/local/recipes/core/grub/grub.cfg" ]; then
    GRUB_CFG="${REPO_ROOT}/local/recipes/core/grub/grub.cfg"
fi

if [ -z "${GRUB_EFI}" ]; then
    echo "ERROR: Cannot find grub.efi in recipe output." >&2
    echo "Build GRUB first: make r.grub" >&2
    exit 1
fi

if [ -z "${GRUB_CFG}" ]; then
    echo "ERROR: Cannot find grub.cfg" >&2
    exit 1
fi

echo "GRUB Installation"
echo "  Image:    ${IMAGE}"
echo "  ESP:      offset ${ESP_OFFSET} bytes (LBA ${ESP_LBA})"
echo "  GRUB EFI: ${GRUB_EFI}"
echo "  GRUB CFG: ${GRUB_CFG}"
echo ""

echo "Current ESP contents:"
python3 "${FAT_TOOL}" ls "${IMAGE}" "${ESP_OFFSET}" /
echo ""

REDBEAR_EFI=""
for search_path in \
    "${REPO_ROOT}/recipes/core/bootloader/target" \
    "${REPO_ROOT}/local/recipes/core/bootloader/target"; do
    for f in $(find "${search_path}" -path "*/stage/usr/lib/boot/bootloader.efi" 2>/dev/null); do
        REDBEAR_EFI="${f}"
        break 2
    done
done
if [ -z "${REDBEAR_EFI}" ]; then
    for f in $(find "${REPO_ROOT}/repo" -path "*/bootloader/*/usr/lib/boot/bootloader.efi" 2>/dev/null); do
        REDBEAR_EFI="${f}"
        break
    done
fi

if [ -z "${REDBEAR_EFI}" ]; then
    echo "ERROR: Cannot find Redox bootloader (bootloader.efi) in cookbook output." >&2
    echo "Build the bootloader first: make r.bootloader" >&2
    exit 1
fi

echo "Sourcing Redox bootloader from ${REDBEAR_EFI}"
REDBEAR_SIZE=$(stat -c%s "${REDBEAR_EFI}")
echo "  Redox bootloader: ${REDBEAR_SIZE} bytes"

echo "Creating EFI/REDBEAR directory..."
python3 "${FAT_TOOL}" mkdir "${IMAGE}" "${ESP_OFFSET}" "EFI/REDBEAR"

echo "Installing Redox bootloader to EFI/REDBEAR/redbear.efi..."
python3 "${FAT_TOOL}" cp-in "${IMAGE}" "${ESP_OFFSET}" "${REDBEAR_EFI}" "EFI/REDBEAR/redbear.efi"

GRUB_SIZE=$(stat -c%s "${GRUB_EFI}")
echo "Installing GRUB (${GRUB_SIZE} bytes) as EFI/BOOT/BOOTX64.EFI..."
python3 "${FAT_TOOL}" cp-in "${IMAGE}" "${ESP_OFFSET}" "${GRUB_EFI}" "EFI/BOOT/BOOTX64.EFI"

echo "Installing grub.cfg to EFI/BOOT/grub.cfg..."
python3 "${FAT_TOOL}" cp-in "${IMAGE}" "${ESP_OFFSET}" "${GRUB_CFG}" "EFI/BOOT/grub.cfg"

echo ""
echo "Final ESP contents:"
python3 "${FAT_TOOL}" ls "${IMAGE}" "${ESP_OFFSET}" /
echo ""
echo "Installation complete. Boot sequence: UEFI -> GRUB -> Redox bootloader -> kernel"
echo "Test with: make qemu"
echo "Revert:    make all CONFIG_NAME=<your-config>  (rebuild without GRUB)"
