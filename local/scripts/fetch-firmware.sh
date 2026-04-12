#!/usr/bin/env bash
# Fetch AMD GPU firmware blobs from linux-firmware repository
# These are required for amdgpu driver to function

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
FIRMWARE_DIR="$SCRIPT_DIR/../firmware/amdgpu"
LINUX_FIRMWARE_REPO="https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git"
TEMP_DIR=$(mktemp -d)
SUBSET="all"

usage() {
    cat <<EOF
Usage: $(basename "$0") [--subset all|rdna]

Fetch AMD GPU firmware blobs from linux-firmware.

Options:
  --subset all      Fetch the full amdgpu firmware set (default)
  --subset rdna     Fetch only RDNA2/RDNA3-oriented firmware blobs
  -h, --help        Show this help text
EOF
}

cleanup() {
    rm -rf "$TEMP_DIR"
}

trap cleanup EXIT

while [ "$#" -gt 0 ]; do
    case "$1" in
        --subset)
            if [ "$#" -lt 2 ]; then
                echo "ERROR: --subset requires a value"
                usage
                exit 1
            fi
            SUBSET="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "ERROR: Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

case "$SUBSET" in
    all|rdna)
        ;;
    *)
        echo "ERROR: Unsupported subset: $SUBSET"
        usage
        exit 1
        ;;
esac

echo "=== AMD GPU Firmware Fetcher ==="
echo "Target: $FIRMWARE_DIR"
echo "Subset: $SUBSET"

# Clone linux-firmware (shallow)
echo "Cloning linux-firmware repository..."
git clone --depth 1 "$LINUX_FIRMWARE_REPO" "$TEMP_DIR/linux-firmware"

# Create target directory
mkdir -p "$FIRMWARE_DIR"

# Copy AMD GPU firmware
echo "Copying AMD GPU firmware blobs..."
if [ -d "$TEMP_DIR/linux-firmware/amdgpu" ]; then
    shopt -s nullglob
    source_blobs=("$TEMP_DIR/linux-firmware/amdgpu/"*.bin)

    if [ "$SUBSET" = "rdna" ]; then
        selected_blobs=()
        for blob in "${source_blobs[@]}"; do
            base="$(basename "$blob")"
            case "$base" in
                psp_13_*|gc_10_3_*|gc_11_0_*|sdma_5_*|sdma_6_*|dcn_3_*|dcn_3_1_*|mes_2_*|smu_13_*|vcn_4_*|gc_11_5_*)
                    selected_blobs+=("$blob")
                    ;;
            esac
        done
    else
        selected_blobs=("${source_blobs[@]}")
    fi

    if [ "${#selected_blobs[@]}" -eq 0 ]; then
        echo "ERROR: No firmware blobs matched subset: $SUBSET"
        exit 1
    fi

    rm -f "$FIRMWARE_DIR"/*.bin
    cp -v "${selected_blobs[@]}" "$FIRMWARE_DIR/"
    echo "Copied $(ls "$FIRMWARE_DIR/"*.bin 2>/dev/null | wc -l) firmware blobs"

    echo "=== Verifying firmware selection ==="
    if [ "$SUBSET" = "rdna" ]; then
        if ls "$FIRMWARE_DIR"/gc_10_3_*.bin "$FIRMWARE_DIR"/gc_11_0_*.bin >/dev/null 2>&1; then
            echo "Verified RDNA graphics firmware families (gfx10.3/gfx11) are present"
        else
            echo "ERROR: Missing RDNA2/RDNA3 graphics firmware blobs"
            exit 1
        fi

        if ls "$FIRMWARE_DIR"/psp_13_*_sos.bin >/dev/null 2>&1; then
            echo "Verified PSP SOS firmware is present"
        else
            echo "ERROR: Missing PSP SOS firmware blobs"
            exit 1
        fi

        non_rdna_count=0
        for blob in "$FIRMWARE_DIR"/*.bin; do
            base="$(basename "$blob")"
            case "$base" in
                psp_13_*|gc_10_3_*|gc_11_0_*|sdma_5_*|sdma_6_*|dcn_3_*|mes_2_*|smu_13_*|vcn_4_*|gc_11_5_*) ;;
                *) non_rdna_count=$((non_rdna_count + 1)) ;;
            esac
        done
        if [ "$non_rdna_count" -gt 0 ]; then
            echo "ERROR: Non-RDNA firmware blob detected in rdna subset"
            exit 1
        fi
        echo "Verified subset contains only RDNA-oriented firmware families"
    else
        if ls "$FIRMWARE_DIR"/*.bin >/dev/null 2>&1; then
            echo "Verified full AMD firmware set copied successfully"
        else
            echo "ERROR: No firmware blobs were copied"
            exit 1
        fi
    fi

    shopt -u nullglob
else
    echo "ERROR: amdgpu firmware directory not found in linux-firmware"
    exit 1
fi

# Also create a listing of which firmware blobs map to which ASICs
echo "=== Creating firmware manifest ==="
cat > "$FIRMWARE_DIR/MANIFEST.txt" << 'MANIFEST'
# AMD GPU Firmware for Red Bear OS
# Source: linux-firmware (https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git)
# License: Various — see linux-firmware WHENCE file for details
#
# Required for: RDNA2 (gfx10.3), RDNA3 (gfx11)
# Minimum set for basic display output:
#   - PSP SOS + TA (security processor)
#   - GC ME/PFP/CE/MEC (graphics/compute)
#   - SDMA (DMA engine)
#   - DMCUB (Display Microcontroller)
#
# Key files for RDNA2 (Navi 21/22/23/24, gfx10.3):
#   psp_13_0_*_sos.bin, gc_10_3_*.bin, sdma_5_*.bin, dcn_3_*.bin
#
# Key files for RDNA3 (Navi 31/32/33, gfx11):
#   psp_13_*_sos.bin, gc_11_0_*.bin, sdma_6_*.bin, dcn_3_1_*.bin
MANIFEST

echo "$FIRMWARE_DIR/MANIFEST.txt created"

# Summary
echo ""
echo "=== Firmware blobs installed ==="
ls -la "$FIRMWARE_DIR/" | head -20
echo "..."
echo "Total: $(ls "$FIRMWARE_DIR/"*.bin 2>/dev/null | wc -l) blobs"
echo ""
echo "WARNING: These are proprietary firmware blobs from AMD."
echo "They are NOT open source. Verify your license compliance."
