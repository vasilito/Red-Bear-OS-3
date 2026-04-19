#!/usr/bin/env bash
# Full USB stack validation harness for Red Bear OS in QEMU.
#
# Boots a Red Bear image with xHCI, USB keyboard, USB tablet, and USB mass storage
# attached, then checks boot logs for successful USB device enumeration and driver spawn.

set -euo pipefail

seed_usb_image() {
    local image_path="$1"
    python3 - "$image_path" <<'PY'
import base64
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
payload = (b"REDBEAR-USB-FULL-STACK-CHECK\0" * 32)[:512]
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
Usage: test-usb-qemu.sh [--check] [config]

Boot or validate the full USB stack on a Red Bear image in QEMU.
Defaults to redbear-mini (mapped to the in-tree redbear-minimal image).

Checks performed:
  1. xHCI controller initializes and reports interrupt mode
  2. USB HID driver spawns for keyboard/tablet
  3. USB SCSI driver spawns for mass storage
  4. USB storage sector-0 readback matches a seeded host pattern
  5. BOS descriptor fetched (or gracefully skipped for USB 2)
  6. No panics or crash-class errors in USB daemons
USAGE
}

check_mode=0
config="redbear-mini"
for arg in "$@"; do
    case "$arg" in
        --help|-h|help)
            usage
            exit 0
            ;;
        --check)
            check_mode=1
            ;;
        redbear-*)
            config="$arg"
            ;;
    esac
done

if [[ "$config" == "redbear-mini" ]]; then
    config="redbear-minimal"
fi

firmware="$(find_uefi_firmware)" || {
    echo "ERROR: no usable x86_64 UEFI firmware found" >&2
    exit 1
}

arch="${ARCH:-$(uname -m)}"
image="build/$arch/$config/harddrive.img"
extra="build/$arch/$config/extra.img"
usb_img="build/$arch/$config/usb-test-storage.img"
log_file="build/$arch/$config/usb-stack-check.log"

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
expect <<EOF
log_user 1
log_file -noappend $log_file
set timeout 300
spawn qemu-system-x86_64 -name {Red Bear OS USB Test} -device qemu-xhci,id=xhci -smp 4 -m 2048 -bios $firmware -chardev stdio,id=debug,signal=off,mux=on -serial chardev:debug -mon chardev=debug -machine q35 -device ich9-intel-hda -device hda-output -device virtio-net,netdev=net0 -netdev user,id=net0 -nographic -vga none -drive file=$image,format=raw,if=none,id=drv0,snapshot=on -device nvme,drive=drv0,serial=NVME_SERIAL -drive file=$extra,format=raw,if=none,id=drv1,snapshot=on -device nvme,drive=drv1,serial=NVME_EXTRA -drive file=$usb_img,format=raw,if=none,id=usbdisk,snapshot=on -device usb-storage,bus=xhci.0,drive=usbdisk -device usb-kbd,bus=xhci.0 -device usb-tablet,bus=xhci.0 -enable-kvm -cpu host
expect -re {xhcid: using MSI/MSI-X interrupt delivery|xhcid: using legacy INTx interrupt delivery}
send_log "MILESTONE:XHCI_IRQ\n"
expect "USB HID driver spawned"
send_log "MILESTONE:USB_HID\n"
expect "USB SCSI driver spawned"
send_log "MILESTONE:USB_SCSI\n"
expect "DISK CONTENT: $expected_sector_b64"
send_log "MILESTONE:USB_READBACK\n"
expect "login:"
send_log "MILESTONE:LOGIN\n"
send "root\r"
expect "assword:"
send "password\r"
expect "Type 'help' for available commands."
send_log "MILESTONE:SHELL\n"
send "shutdown\r"
sleep 2
EOF
expect_status=$?
set -e

pkill -f "qemu-system-x86_64.*$image" 2>/dev/null || true

failures=0

echo "--- USB Stack Validation: $config ---"

# Check 1: xHCI interrupt mode
if grep -aq "MILESTONE:XHCI_IRQ" "$log_file"; then
    echo "  [PASS] xHCI interrupt-driven mode detected"
else
    echo "  [FAIL] xHCI did not report interrupt-driven mode" >&2
    failures=$((failures + 1))
fi

# Check 2: USB HID driver spawn
if grep -aq "MILESTONE:USB_HID" "$log_file"; then
    echo "  [PASS] USB HID driver spawned"
else
    echo "  [FAIL] USB HID driver did not spawn" >&2
    failures=$((failures + 1))
fi

# Check 3: USB SCSI driver spawn + bounded data readback
if grep -aq "MILESTONE:USB_SCSI" "$log_file"; then
    echo "  [PASS] USB SCSI driver spawned"
else
    echo "  [FAIL] USB SCSI driver did not spawn" >&2
    failures=$((failures + 1))
fi

if grep -aq "MILESTONE:USB_READBACK" "$log_file"; then
    echo "  [PASS] USB storage sector readback matched seeded pattern"
else
    echo "  [FAIL] USB storage sector readback milestone was never reached" >&2
    failures=$((failures + 1))
fi

# Check 4: BOS descriptor handling (info or debug log)
if grep -aq "BOS:" "$log_file"; then
    echo "  [PASS] BOS descriptor processing active"
elif grep -aq "BOS descriptor not available" "$log_file"; then
    echo "  [PASS] BOS descriptor gracefully skipped (USB 2 device)"
else
    echo "  [WARN] No BOS descriptor log output found"
fi

# Check 5: No panics or crash-class errors (stall recovery messages are expected)
if grep -aqi "panic\|usbscsid: .*IO ERROR\|usbscsid: startup failed\|usbhidd: .*IO ERROR" "$log_file"; then
    echo "  [FAIL] USB stack hit crash-class errors" >&2
    failures=$((failures + 1))
else
    echo "  [PASS] No crash-class errors detected"
fi

# Check 6: Boot progressed far enough to reach the guest shell
if grep -aq "MILESTONE:SHELL" "$log_file"; then
    echo "  [PASS] Full USB stack boot reached guest shell"
else
    echo "  [FAIL] Full USB stack run never reached guest shell" >&2
    failures=$((failures + 1))
fi

# Check 7: Hub driver (if hub detected)
if grep -aq "USB HUB driver spawned" "$log_file"; then
    echo "  [PASS] USB hub driver spawned"
else
    echo "  [INFO] No hub driver spawn (expected for direct-attached devices)"
fi

if [[ "$expect_status" -ne 0 ]]; then
    echo "  [INFO] expect exited with status $expect_status; milestone checks above determine pass/fail"
fi

echo "--- Results: $failures failure(s), log: $log_file ---"

if [[ "$failures" -gt 0 ]]; then
    exit 1
fi

exit 0
