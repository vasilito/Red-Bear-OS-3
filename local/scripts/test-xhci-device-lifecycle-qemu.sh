#!/usr/bin/env bash
# Validate bounded xHCI device attach/detach lifecycle behavior in QEMU.

set -euo pipefail

seed_usb_image() {
    local image_path="$1"
    python3 - "$image_path" <<'PY'
import base64
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
payload = (b"REDBEAR-XHCID-LIFECYCLE-CHECK\0" * 32)[:512]
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
Usage: test-xhci-device-lifecycle-qemu.sh [--check] [config]

Boot a Red Bear image and exercise bounded xHCI attach/detach behavior via
QEMU monitor hotplug events. Defaults to redbear-mini (mapped to the in-tree redbear-minimal image).
USAGE
}

config="redbear-mini"
for arg in "$@"; do
    case "$arg" in
        --help|-h|help)
            usage
            exit 0
            ;;
        --check)
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
usb_img="build/$arch/$config/usb-lifecycle-storage.img"
log_file="build/$arch/$config/xhci-device-lifecycle.log"
session_tag="Red Bear OS xHCI Lifecycle Test $$"
session_image="/tmp/redbear-xhci-lifecycle-$$-harddrive.img"
session_extra="/tmp/redbear-xhci-lifecycle-$$-extra.img"
session_usb_img="/tmp/redbear-xhci-lifecycle-$$-usb.img"
image="$(realpath "$image")"
extra="$(realpath "$extra")"
usb_img="$(realpath "$usb_img")"
log_file="$(realpath -m "$log_file")"

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
seed_usb_image "$usb_img" >/dev/null

ln -sf "$image" "$session_image"
ln -sf "$extra" "$session_extra"
ln -sf "$usb_img" "$session_usb_img"

pkill -f "qemu-system-x86_64.*$session_tag" 2>/dev/null || true
sleep 1

rm -f "$log_file"

expect <<EOF
log_user 1
log_file -noappend $log_file
set timeout 1800
set send_slow {1 0.0}
spawn qemu-system-x86_64 -name {$session_tag} -device qemu-xhci,id=xhci -smp 4 -m 2048 -bios $firmware -chardev stdio,id=debug,signal=off,mux=on -serial chardev:debug -mon chardev=debug -machine q35 -device ich9-intel-hda -device hda-output -device virtio-net,netdev=net0 -netdev user,id=net0 -nographic -vga none -drive file=$session_image,format=raw,if=none,id=drv0,snapshot=on -device nvme,drive=drv0,serial=NVME_SERIAL -drive file=$session_extra,format=raw,if=none,id=drv1,snapshot=on -device nvme,drive=drv1,serial=NVME_EXTRA -drive file=$session_usb_img,format=raw,if=none,id=usbdisk0,snapshot=on -drive file=$session_usb_img,format=raw,if=none,id=usbdisk1,snapshot=on -enable-kvm -cpu host
expect -re {xhcid: using MSI/MSI-X interrupt delivery|xhcid: using legacy INTx interrupt delivery}
expect "login:"
send "root\r"
expect "assword:"
send "password\r"
expect -re {Type 'help' for available commands\.}
expect -re {# }

send "rm -f /tmp/xhcid-test-hook\r"
after 500

send "\001c"
expect "(qemu)"
send "device_add usb-kbd,bus=xhci.0,id=usbkbd0\r"
expect "(qemu)"
send "\001c"
expect -re {xhcid: begin attach for port [0-9\.]+}
set hid_port \$expect_out(0,string)
regexp {port ([0-9\.]+)} \$hid_port -> hid_port
expect -re {xhcid: finished attach for port [0-9\.]+}
expect -re {Device on port [0-9\.]+ was attached}
send "\r"
expect -re {# }
set hid_scheme "usb.pci-0000-00-01.0_xhci"
set H "/tmp/xhcid-test-hook"
set P "/scheme/\$hid_scheme/port\$hid_port"

send "echo x > \$P/suspend\r"
expect -re {xhcid: suspended port [0-9\.]+}
expect -re {# }
after 500
send "echo x > \$P/configure\r"
expect -re {xhcid: port [0-9\.]+ rejected routable operation while suspended}
expect -re {# }
after 500
send "echo x > \$P/resume\r"
expect -re {xhcid: resumed port [0-9\.]+}
expect -re {# }

send "\001c"
expect "(qemu)"
send "device_del usbkbd0\r"
expect "(qemu)"
send "\001c"
expect -re {Device on port [0-9\.]+ was detached}

send "echo fail_after_configure_endpoint > \$H\r"
after 500

send "\001c"
expect "(qemu)"
send "device_add usb-kbd,bus=xhci.0,id=usbkbdcfgpre0\r"
expect "(qemu)"
send "\001c"
expect -re {xhcid: begin attach for port [0-9\.]+}
expect -re {xhcid: finished attach for port [0-9\.]+}
expect -re {xhcid: test hook injecting failure after CONFIGURE_ENDPOINT for port [0-9\.]+}

send "\001c"
expect "(qemu)"
send "device_del usbkbdcfgpre0\r"
expect "(qemu)"
send "\001c"
expect -re {Device on port [0-9\.]+ was detached}

send "\001c"
expect "(qemu)"
send "device_add usb-kbd,bus=xhci.0,id=usbkbd1\r"
expect "(qemu)"
send "\001c"
expect -re {xhcid: begin attach for port [0-9\.]+}
expect -re {xhcid: finished attach for port [0-9\.]+}
expect -re {Device on port [0-9\.]+ was attached}

send "\001c"
expect "(qemu)"
send "device_del usbkbd1\r"
expect "(qemu)"
send "\001c"
expect -re {Device on port [0-9\.]+ was detached}

send "echo fail_after_set_configuration > \$H\r"
after 500

send "\001c"
expect "(qemu)"
send "device_add usb-kbd,bus=xhci.0,id=usbkbdcfg0\r"
expect "(qemu)"
send "\001c"
expect -re {xhcid: begin attach for port [0-9\.]+}
expect -re {xhcid: finished attach for port [0-9\.]+}
expect -re {xhcid: test hook injecting failure after SET_CONFIGURATION for port [0-9\.]+}

send "\001c"
expect "(qemu)"
send "device_del usbkbdcfg0\r"
expect "(qemu)"
send "\001c"
expect -re {Device on port [0-9\.]+ was detached}

send "echo delay_before_attach_commit_ms=15000 > \$H\r"
after 500

send "\001c"
expect "(qemu)"
send "device_add usb-storage,bus=xhci.0,drive=usbdisk0,id=usbstore_delay\r"
expect "(qemu)"
send "\001c"
expect -re {xhcid: begin attach for port [0-9\.]+}
expect -re {xhcid: test hook delaying attach commit for port [0-9\.]+ by 15000 ms}

send "\001c"
expect "(qemu)"
send "device_del usbstore_delay\r"
expect "(qemu)"
send "\001c"
expect -re {attach for port [0-9\.]+ completed after detach already started; skipping publication}
expect -re {Device on port [0-9\.]+ was detached}

send "\001c"
expect "(qemu)"
send "device_add usb-storage,bus=xhci.0,drive=usbdisk1,id=usbstore0\r"
expect "(qemu)"
send "\001c"
expect -re {xhcid: begin attach for port [0-9\.]+}
expect -re {xhcid: finished attach for port [0-9\.]+}
expect -re {Device on port [0-9\.]+ was attached}
after 3000

send "\001c"
expect "(qemu)"
send "device_del usbstore0\r"
expect "(qemu)"
send "\001c"
expect -re {Device on port [0-9\.]+ was detached}

send "shutdown\r"
sleep 2
EOF

pkill -f "qemu-system-x86_64.*$session_tag" 2>/dev/null || true
rm -f "$session_image" "$session_extra" "$session_usb_img"

failures=0

echo "--- xHCI Device Lifecycle Validation: $config ---"

if grep -aq "xhcid: using MSI/MSI-X interrupt delivery\|xhcid: using legacy INTx interrupt delivery" "$log_file"; then
    echo "  [PASS] xHCI interrupt-driven mode detected"
else
    echo "  [FAIL] xHCI did not report interrupt-driven mode" >&2
    failures=$((failures + 1))
fi

if [[ "$(grep -Eac 'xhcid: begin attach for port [0-9.]+' "$log_file")" -ge 5 ]]; then
    echo "  [PASS] xHCI attach path observed across repeated hotplug cycles"
else
    echo "  [FAIL] Missing repeated xHCI attach evidence" >&2
    failures=$((failures + 1))
fi

if [[ "$(grep -Eac 'xhcid: finished attach for port [0-9.]+' "$log_file")" -ge 4 ]]; then
    echo "  [PASS] xHCI attach completion observed for full attach cycles"
else
    echo "  [FAIL] Missing xHCI attach completion evidence" >&2
    failures=$((failures + 1))
fi

if [[ "$(grep -Eac 'Device on port [0-9.]+ was detached' "$log_file")" -ge 4 ]]; then
    echo "  [PASS] xHCI detach path observed for repeated hot-unplugged devices"
else
    echo "  [FAIL] Missing xHCI detach evidence" >&2
    failures=$((failures + 1))
fi

if [[ "$(grep -Eac 'USB HID driver spawned' "$log_file")" -ge 2 ]]; then
    echo "  [PASS] USB HID spawn observed across repeated hotplug cycles"
else
    echo "  [FAIL] USB HID driver did not spawn after hotplug" >&2
    failures=$((failures + 1))
fi

if [[ "$(grep -Eac 'USB SCSI driver spawned' "$log_file")" -ge 1 ]]; then
    echo "  [PASS] USB SCSI spawn observed during hotplug lifecycle proof"
else
    echo "  [FAIL] USB SCSI driver did not spawn after hotplug" >&2
    failures=$((failures + 1))
fi

if grep -aq 'xhcid: test hook injecting failure after SET_CONFIGURATION for port ' "$log_file"; then
    echo "  [PASS] Configure-failure injection hook fired during lifecycle proof"
else
    echo "  [FAIL] Missing configure-failure injection evidence" >&2
    failures=$((failures + 1))
fi

if grep -aq 'xhcid: test hook injecting failure after CONFIGURE_ENDPOINT for port ' "$log_file"; then
    echo "  [PASS] Configure-endpoint injection hook fired during lifecycle proof"
else
    echo "  [FAIL] Missing configure-endpoint injection evidence" >&2
    failures=$((failures + 1))
fi

if grep -aq 'xhcid: test hook delaying attach commit for port ' "$log_file"; then
    echo "  [PASS] Attach-commit timing hook fired during lifecycle proof"
else
    echo "  [FAIL] Missing attach-commit timing hook evidence" >&2
    failures=$((failures + 1))
fi

if grep -Eaq 'attach for port [0-9.]+ completed after detach already started; skipping publication' "$log_file"; then
    echo "  [PASS] Delayed attach stayed unpublished once detach won the race"
else
    echo "  [FAIL] Missing delayed attach race outcome evidence" >&2
    failures=$((failures + 1))
fi

if grep -aq 'xhcid: suspended port ' "$log_file" && grep -aq 'xhcid: resumed port ' "$log_file" && grep -aq 'xhcid: port .* rejected routable operation while suspended' "$log_file"; then
    echo "  [PASS] Suspend/resume admission checks blocked configure while suspended"
else
    echo "  [FAIL] Missing suspend/resume admission evidence" >&2
    failures=$((failures + 1))
fi

if grep -aqi "Failed to setup protocol\|failed to disable port slot" "$log_file"; then
    echo "  [FAIL] Lifecycle path hit crash-class or teardown errors" >&2
    failures=$((failures + 1))
else
    echo "  [PASS] No crash-class lifecycle errors detected"
fi

echo "--- Results: $failures failure(s), log: $log_file ---"

if [[ "$failures" -gt 0 ]]; then
    exit 1
fi

exit 0
