#!/usr/bin/env bash
# Fetch bounded firmware blobs from linux-firmware repository.
# AMD support remains GPU-focused; Intel support now includes display, Wi-Fi,
# and Bluetooth subsets for current Red Bear bring-up milestones.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
LINUX_FIRMWARE_REPO="https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git"
TEMP_DIR=$(mktemp -d)
VENDOR="amd"
SUBSET="all"
COPIED_COUNT=0

usage() {
    cat <<EOF
Usage: $(basename "$0") [--vendor amd|intel] [--subset all|rdna|dmc|wifi|bluetooth]

Fetch bounded firmware blobs from linux-firmware.

Options:
  --vendor amd      Fetch AMD GPU firmware (default)
  --vendor intel    Fetch bounded Intel firmware subsets
  --subset all      Fetch the full AMD amdgpu firmware set (default for AMD)
  --subset rdna     Fetch only RDNA2/RDNA3-oriented AMD firmware blobs
  --subset dmc      Fetch bounded Intel DMC display firmware set (default for Intel)
  --subset wifi     Fetch Intel iwlwifi .ucode and .pnvm blobs
  --subset bluetooth Fetch Intel Bluetooth ibt .sfi and .ddc blobs
  -h, --help        Show this help text
EOF
}

set_firmware_dir() {
    case "$VENDOR" in
    amd) FIRMWARE_DIR="$SCRIPT_DIR/../firmware/amdgpu" ;;
        intel)
            case "$SUBSET" in
                wifi) FIRMWARE_DIR="$SCRIPT_DIR/../firmware" ;;
                bluetooth) FIRMWARE_DIR="$SCRIPT_DIR/../firmware/intel" ;;
                *) FIRMWARE_DIR="$SCRIPT_DIR/../firmware/i915" ;;
            esac
            ;;
    esac
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
        --vendor)
            if [ "$#" -lt 2 ]; then
                echo "ERROR: --vendor requires a value"
                usage
                exit 1
            fi
            VENDOR="$2"
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

case "$VENDOR" in
    amd)
        case "$SUBSET" in
            all|rdna) ;;
            *)
                echo "ERROR: Unsupported AMD subset: $SUBSET"
                usage
                exit 1
                ;;
        esac
        ;;
    intel)
        if [ "$SUBSET" = "all" ]; then
            SUBSET="dmc"
        fi
        case "$SUBSET" in
            dmc|wifi|bluetooth) ;;
            *)
                echo "ERROR: Unsupported Intel subset: $SUBSET"
                usage
                exit 1
                ;;
        esac
        ;;
    *)
        echo "ERROR: Unsupported vendor: $VENDOR"
        usage
        exit 1
        ;;
esac

set_firmware_dir

echo "=== Firmware Fetcher ==="
echo "Vendor: $VENDOR"
echo "Target: $FIRMWARE_DIR"
echo "Subset: $SUBSET"

# Clone linux-firmware (shallow)
echo "Cloning linux-firmware repository..."
git clone --depth 1 "$LINUX_FIRMWARE_REPO" "$TEMP_DIR/linux-firmware"

# Create target directory
mkdir -p "$FIRMWARE_DIR"

copy_amd_firmware() {
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
    COPIED_COUNT=${#selected_blobs[@]}
    echo "Copied $COPIED_COUNT firmware blobs"

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
}

copy_intel_dmc_firmware() {
    echo "Copying bounded Intel DMC firmware blobs..."
    if [ ! -d "$TEMP_DIR/linux-firmware/i915" ]; then
        echo "ERROR: i915 firmware directory not found in linux-firmware"
        exit 1
    fi

    local selected_blobs=()
    local candidates=(
        adlp_dmc.bin
        adlp_dmc_ver2_16.bin
        tgl_dmc.bin
        tgl_dmc_ver2_12.bin
        dg2_dmc.bin
        dg2_dmc_ver2_06.bin
        mtl_dmc.bin
    )

    for blob in "${candidates[@]}"; do
        if [ -f "$TEMP_DIR/linux-firmware/i915/$blob" ]; then
            selected_blobs+=("$TEMP_DIR/linux-firmware/i915/$blob")
        fi
    done

    if [ "${#selected_blobs[@]}" -eq 0 ]; then
        echo "ERROR: No Intel DMC firmware blobs were found"
        exit 1
    fi

    rm -f "$FIRMWARE_DIR"/*.bin
    cp -v "${selected_blobs[@]}" "$FIRMWARE_DIR/"
    COPIED_COUNT=${#selected_blobs[@]}
    echo "Copied $COPIED_COUNT Intel DMC firmware blobs"
}

copy_intel_wifi_firmware() {
    echo "Copying Intel Wi-Fi firmware blobs..."

    shopt -s nullglob
    local ucode_blobs=("$TEMP_DIR/linux-firmware/"iwlwifi-*.ucode)
    local pnvm_blobs=("$TEMP_DIR/linux-firmware/"iwlwifi-*.pnvm)
    local selected_blobs=("${ucode_blobs[@]}" "${pnvm_blobs[@]}")

    if [ "${#ucode_blobs[@]}" -eq 0 ]; then
        echo "ERROR: No Intel Wi-Fi .ucode blobs were found"
        exit 1
    fi
    if [ "${#pnvm_blobs[@]}" -eq 0 ]; then
        echo "ERROR: No Intel Wi-Fi .pnvm blobs were found"
        exit 1
    fi

    rm -f "$FIRMWARE_DIR"/iwlwifi-*.ucode "$FIRMWARE_DIR"/iwlwifi-*.pnvm
    cp -v "${selected_blobs[@]}" "$FIRMWARE_DIR/"
    COPIED_COUNT=${#selected_blobs[@]}
    echo "Copied $COPIED_COUNT Intel Wi-Fi firmware blobs"
    shopt -u nullglob
}

copy_intel_bluetooth_firmware() {
    echo "Copying Intel Bluetooth firmware blobs..."

    if [ ! -d "$TEMP_DIR/linux-firmware/intel" ]; then
        echo "ERROR: intel firmware directory not found in linux-firmware"
        exit 1
    fi

    shopt -s nullglob
    local sfi_blobs=("$TEMP_DIR/linux-firmware/intel/"ibt-*.sfi)
    local ddc_blobs=("$TEMP_DIR/linux-firmware/intel/"ibt-*.ddc)
    local selected_blobs=("${sfi_blobs[@]}" "${ddc_blobs[@]}")

    if [ "${#sfi_blobs[@]}" -eq 0 ]; then
        echo "ERROR: No Intel Bluetooth .sfi blobs were found"
        exit 1
    fi
    if [ "${#ddc_blobs[@]}" -eq 0 ]; then
        echo "ERROR: No Intel Bluetooth .ddc blobs were found"
        exit 1
    fi

    rm -f "$FIRMWARE_DIR"/ibt-*.sfi "$FIRMWARE_DIR"/ibt-*.ddc
    cp -v "${selected_blobs[@]}" "$FIRMWARE_DIR/"
    COPIED_COUNT=${#selected_blobs[@]}
    echo "Copied $COPIED_COUNT Intel Bluetooth firmware blobs"
    shopt -u nullglob
}

case "$VENDOR" in
    amd)
        copy_amd_firmware
        ;;
    intel)
        case "$SUBSET" in
            dmc) copy_intel_dmc_firmware ;;
            wifi) copy_intel_wifi_firmware ;;
            bluetooth) copy_intel_bluetooth_firmware ;;
        esac
        ;;
esac

echo "NOTE: MANIFEST.txt is now generated by firmware-loader (--generate-manifest or daemon startup)."

# Summary
echo ""
echo "=== Firmware blobs installed ==="
ls -la "$FIRMWARE_DIR/" | head -20
echo "..."
echo "Copied: $COPIED_COUNT files"
echo ""
echo "WARNING: These firmware blobs are third-party upstream firmware."
echo "They are NOT open source. Verify your license compliance."
