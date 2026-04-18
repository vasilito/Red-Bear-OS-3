#!/usr/bin/env bash
# Validate USB mass-storage autospawn via xHCI in QEMU.

set -euo pipefail

seed_usb_image() {
    local image_path="$1"
    python3 - "$image_path" <<'PY'
import base64
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
payload = (b"REDBEAR-USB-STORAGE-CHECK\0" * 32)[:512]
payload = payload.ljust(512, b'\0')

with path.open("r+b") as fh:
    fh.seek(0)
    fh.write(payload)

print(base64.b64encode(payload).decode("ascii"))
PY
}

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
Usage: test-usb-storage-qemu.sh [config]

Boot a Red Bear image with a USB storage device attached and verify usbscsid autospawn.
Defaults to redbear-desktop.
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

config="${1:-redbear-desktop}"
arch="${ARCH:-$(uname -m)}"
image="build/$arch/$config/harddrive.img"
extra="build/$arch/$config/extra.img"
usb_img="build/$arch/$config/usb-storage.img"
log_file="build/$arch/$config/usb-storage-check.log"
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

if [[ ! -f "$usb_img" ]]; then
    truncate -s 64M "$usb_img"
fi

expected_sector_b64="$(seed_usb_image "$usb_img")"

pkill -f "qemu-system-x86_64.*$image" 2>/dev/null || true
sleep 1

rm -f "$log_file"
set +e
timeout 120s qemu-system-x86_64 \
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
  -drive file="$image",format=raw,if=none,id=drv0 \
  -device nvme,drive=drv0,serial=NVME_SERIAL \
  -drive file="$extra",format=raw,if=none,id=drv1 \
  -device nvme,drive=drv1,serial=NVME_EXTRA \
  -drive file="$usb_img",format=raw,if=none,id=usbdisk \
  -device usb-storage,bus=xhci.0,drive=usbdisk \
  -enable-kvm -cpu host \
  > "$log_file" 2>&1
set -e

if ! grep -q "USB SCSI driver spawned" "$log_file"; then
    echo "ERROR: usbscsid did not autospawn; see $log_file" >&2
    exit 1
fi

if ! grep -Fq "DISK CONTENT: $expected_sector_b64" "$log_file"; then
    echo "ERROR: USB storage sector 0 readback did not match the seeded pattern; see $log_file" >&2
    exit 1
fi

if grep -q "panic\|usbscsid: .*IO ERROR\|usbscsid: startup failed\|usbscsid: event queue error\|usbscsid: scheme tick failed\|bulk .* endpoint stalled" "$log_file"; then
    echo "ERROR: USB storage path hit a crash/error; see $log_file" >&2
    exit 1
fi

echo "USB mass-storage readback verified in $log_file"
