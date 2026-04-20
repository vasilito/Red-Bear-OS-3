#!/usr/bin/env bash
# Validate a live MSI-X runtime path in QEMU using the existing virtio-net stack.

set -euo pipefail

find_uefi_firmware() {
    local candidates=(
        "/usr/share/ovmf/x64/OVMF.4m.fd"
        "/usr/share/OVMF/x64/OVMF.4m.fd"
        "/usr/share/ovmf/x64/OVMF_CODE.4m.fd"
        "/usr/share/OVMF/x64/OVMF_CODE.4m.fd"
        "/usr/share/ovmf/OVMF.fd"
        "/usr/share/OVMF/OVMF_CODE.fd"
        "/usr/share/qemu/edk2-x86_64-code.fd"
    )
    local path
    for path in "${candidates[@]}"; do
        if [[ -f "$path" ]]; then
            printf '%s\n' "$path"
            return 0
        fi
    done
    return 1
}

usage() {
    cat <<'USAGE'
Usage: test-msix-qemu.sh [config]

Boot a Red Bear image in QEMU and verify a live MSI-X path via virtio-net.
Defaults to redbear-mini (mapped to the in-tree redbear-minimal image).
USAGE
}

for arg in "$@"; do
    case "$arg" in
        --help|-h|help)
            usage
            exit 0
            ;;
    esac
done

config="${1:-redbear-mini}"
if [[ "$config" == "redbear-mini" ]]; then
  config="redbear-minimal"
fi
arch="${ARCH:-$(uname -m)}"
image="build/$arch/$config/harddrive.img"
extra="build/$arch/$config/extra.img"
log_file="build/$arch/$config/msix-check.log"
firmware="$(find_uefi_firmware)" || {
    echo "ERROR: no usable x86_64 UEFI firmware found" >&2
    exit 1
}

if [[ ! -f "$image" ]]; then
    echo "ERROR: missing image $image" >&2
    echo "Build it first with: ./local/scripts/build-redbear.sh $config" >&2
    exit 1
fi

if [[ ! -f "$extra" ]]; then
    truncate -s 1g "$extra"
fi

pkill -f "qemu-system-x86_64.*$image" 2>/dev/null || true
sleep 1

rm -f "$log_file"
set +e
timeout 90s qemu-system-x86_64 \
  -name "Red Bear OS x86_64" \
  -device qemu-xhci,id=xhci \
  -smp 4 \
  -m 2048 \
  -bios "$firmware" \
  -chardev stdio,id=debug,signal=off,mux=on \
  -serial chardev:debug \
  -mon chardev=debug \
  -machine q35 \
  -device ich9-intel-hda -device hda-output \
  -device virtio-net,netdev=net0 \
  -netdev user,id=net0 \
  -object filter-dump,id=f1,netdev=net0,file="build/$arch/$config/network.pcap" \
  -nographic -vga none \
  -drive file="$image",format=raw,if=none,id=drv0,snapshot=on \
  -device nvme,drive=drv0,serial=NVME_SERIAL \
  -drive file="$extra",format=raw,if=none,id=drv1,snapshot=on \
  -device nvme,drive=drv1,serial=NVME_EXTRA \
  -enable-kvm -cpu host \
  > "$log_file" 2>&1
set -e

if ! grep -q "virtio-net: using MSI-X interrupt delivery" "$log_file"; then
  echo "ERROR: no live MSI-X evidence found in $log_file" >&2
  exit 1
fi

echo "IRQ_DRIVER=virtio-net"
echo "IRQ_MODE=msix"
echo "IRQ_REASON=driver_selected_msix"
echo "IRQ_LOG=$log_file"
echo "MSI-X runtime path detected in $log_file"
