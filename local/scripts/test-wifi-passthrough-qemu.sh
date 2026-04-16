#!/usr/bin/env bash
# Launch Red Bear OS with an Intel Wi-Fi PCI device passed through for runtime validation.

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

capture_output=""

usage() {
    cat <<'EOF'
Usage: test-wifi-passthrough-qemu.sh --host-pci 0000:xx:yy.z [--check] [--capture-output PATH] [extra qemu args...]

Boot Red Bear OS with an Intel Wi-Fi PCI function passed through via VFIO and optionally run the
bounded in-guest Wi-Fi runtime check.

Options:
  --host-pci BDF   Host PCI address of the Intel Wi-Fi device to pass through (required)
  --check          Auto-login and run redbear-phase5-network-check plus the bounded Wi-Fi runtime check
  --capture-output PATH  Save the in-guest Wi-Fi capture bundle to a host-side file during --check
  -h, --help       Show this help text

Notes:
  - The host device must already be detached from the host driver and bound to vfio-pci.
  - This script only provides the launch/check harness. Real success still depends on Red Bear OS
    runtime behavior on the passed-through hardware.
EOF
}

host_pci=""
check_mode=0
filtered_args=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --host-pci)
            if [[ $# -lt 2 ]]; then
                echo "ERROR: --host-pci requires a PCI BDF" >&2
                exit 1
            fi
            host_pci="$2"
            shift 2
            ;;
        --check)
            check_mode=1
            shift
            ;;
        --capture-output)
            if [[ $# -lt 2 ]]; then
                echo "ERROR: --capture-output requires a path" >&2
                exit 1
            fi
            capture_output="$2"
            shift 2
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            filtered_args+=("$1")
            shift
            ;;
    esac
done

if [[ -z "$host_pci" ]]; then
    echo "ERROR: --host-pci is required" >&2
    usage
    exit 1
fi

firmware="$(find_uefi_firmware)" || {
    echo "ERROR: no usable x86_64 UEFI firmware found" >&2
    exit 1
}

arch="${ARCH:-$(uname -m)}"
image="build/$arch/redbear-full/harddrive.img"
extra="build/$arch/redbear-full/extra.img"

if [[ ! -f "$image" ]]; then
    echo "ERROR: missing image $image" >&2
    echo "Build it first with: ./local/scripts/build-redbear.sh redbear-full" >&2
    exit 1
fi

if [[ ! -f "$extra" ]]; then
    truncate -s 1g "$extra"
fi

vfio_arg="vfio-pci,host=${host_pci}"

if [[ "$check_mode" -eq 1 ]]; then
    capture_output="${capture_output:-$(pwd)/wifi-passthrough-capture.json}"
    expect <<EOF
log_user 1
set timeout 420
spawn qemu-system-x86_64 -name {Red Bear OS Wi-Fi Passthrough} -device qemu-xhci -smp 4 -m 4096 -bios $firmware -chardev stdio,id=debug,signal=off,mux=on -serial chardev:debug -mon chardev=debug -machine q35 -device ich9-intel-hda -device hda-output -device vfio-pci,host=$host_pci -vga std -drive file=$image,format=raw,if=none,id=drv0 -device nvme,drive=drv0,serial=NVME_SERIAL -drive file=$extra,format=raw,if=none,id=drv1 -device nvme,drive=drv1,serial=NVME_EXTRA -enable-kvm -cpu host ${filtered_args[*]}
expect "login:"
send "root\r"
expect "assword:"
send "password\r"
expect "Type 'help' for available commands."
send "redbear-phase5-network-check\r"
expect "Red Bear OS Phase 5 Networking Check"
send "redbear-phase5-wifi-run wifi-open-bounded wlan0 /tmp/redbear-phase5-wifi-capture.json\r"
expect "Red Bear OS Phase 5 Wi-Fi Check"
expect "PASS: bounded Intel Wi-Fi runtime path exercised on target"
expect "capture_output=/tmp/redbear-phase5-wifi-capture.json"
expect "root@"
send "wc -c /tmp/redbear-phase5-wifi-capture.json\r"
expect "/tmp/redbear-phase5-wifi-capture.json"
send "printf 'CAPTURE-BEGIN\\n'; cat /tmp/redbear-phase5-wifi-capture.json; printf '\\nCAPTURE-END\\n'\r"
expect "CAPTURE-BEGIN"
expect {
    -re {(\{.*\})\r?\nCAPTURE-END} {
        set capture $expect_out(1,string)
        set fh [open "$capture_output" "w"]
        puts $fh $capture
        close $fh
    }
    timeout {
        send_user "ERROR: timed out while capturing Wi-Fi bundle\n"
        exit 1
    }
}
send "shutdown\r"
expect eof
EOF
    echo "capture_output=$capture_output"
    exit 0
fi

exec qemu-system-x86_64 \
  -name "Red Bear OS Wi-Fi Passthrough" \
  -device qemu-xhci \
  -smp 4 \
  -m 4096 \
  -bios "$firmware" \
  -chardev stdio,id=debug,signal=off,mux=on \
  -serial chardev:debug \
  -mon chardev=debug \
  -machine q35 \
  -device ich9-intel-hda -device hda-output \
  -device "$vfio_arg" \
  -vga std \
  -drive file="$image",format=raw,if=none,id=drv0 \
  -device nvme,drive=drv0,serial=NVME_SERIAL \
  -drive file="$extra",format=raw,if=none,id=drv1 \
  -device nvme,drive=drv1,serial=NVME_EXTRA \
  -enable-kvm -cpu host \
  "${filtered_args[@]}"
