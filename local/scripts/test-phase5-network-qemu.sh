#!/usr/bin/env bash
# Launch or validate the Phase 5 desktop/network plumbing path in QEMU.

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
Usage: test-phase5-network-qemu.sh [--check] [extra qemu args...]

Boot or validate the Red Bear OS Phase 5 desktop/network plumbing path on redbear-full.
USAGE
}

check_mode=0
filtered_args=()
for arg in "$@"; do
    case "$arg" in
        --help|-h|help)
            usage
            exit 0
            ;;
        --check)
            check_mode=1
            ;;
        *)
            filtered_args+=("$arg")
            ;;
    esac
done

firmware="$(find_uefi_firmware)" || {
    echo "ERROR: no usable x86_64 UEFI firmware found" >&2
    exit 1
}

arch="${ARCH:-$(uname -m)}"
image="build/$arch/redbear-full/harddrive.img"
extra="build/$arch/redbear-full/extra.img"
extra_qemu_args="${filtered_args[*]:-}"

if [[ ! -f "$image" ]]; then
    echo "ERROR: missing image $image" >&2
    echo "Build it first with: ./local/scripts/build-redbear.sh redbear-full" >&2
    exit 1
fi

if [[ ! -f "$extra" ]]; then
    truncate -s 1g "$extra"
fi

if [[ "$check_mode" -eq 1 ]]; then
    expect <<EOF
log_user 1
set timeout 240
spawn qemu-system-x86_64 -name {Red Bear OS x86_64} -device qemu-xhci -smp 4 -m 2048 -bios $firmware -chardev stdio,id=debug,signal=off,mux=on -serial chardev:debug -mon chardev=debug -machine q35 -device ich9-intel-hda -device hda-output -device virtio-net,netdev=net0 -netdev user,id=net0 -object filter-dump,id=f1,netdev=net0,file=build/$arch/redbear-full/network.pcap -vga std -drive file=$image,format=raw,if=none,id=drv0 -device nvme,drive=drv0,serial=NVME_SERIAL -drive file=$extra,format=raw,if=none,id=drv1 -device nvme,drive=drv1,serial=NVME_EXTRA -enable-kvm -cpu host $extra_qemu_args
expect "login:"
send "root\r"
expect "assword:"
send "password\r"
expect "Type 'help' for available commands."
send "redbear-phase5-network-check\r"
expect "Red Bear OS Phase 5 Networking Check"
expect "dbus-daemon"
expect "virtio_net_present"
expect "wifi_control_state=present"
expect "wifi_connect_result=present"
    expect "WIFICTL_INTERFACES=present"
    expect "WIFICTL_CAPABILITIES=present"
    expect "DBUS_SYSTEM_BUS="
    expect "UPOWER_BUS_NAME=present"
    expect "UPOWER_RUNTIME_ADAPTERS="
    expect "UPOWER_RUNTIME_BATTERIES="
    expect "UPOWER_ENUMERATED_DEVICES="
    expect "UPOWER_NATIVE_PATHS=validated"
    expect "UDISKS_BUS_NAME=present"
    expect "UDISKS_RUNTIME_DRIVE_SURFACES="
    expect "UDISKS_RUNTIME_BLOCK_SURFACES="
    expect "UDISKS_MANAGED_OBJECTS=present"
    expect "UDISKS_BLOCK_OBJECT_PATHS=validated"
expect "PHASE5_WIFI_CHECK=pass"
    send "shutdown\r"
    expect eof
EOF
    exit 0
fi

exec qemu-system-x86_64 \
  -name "Red Bear OS x86_64" \
  -device qemu-xhci \
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
  -object filter-dump,id=f1,netdev=net0,file="build/$arch/redbear-full/network.pcap" \
  -vga std \
  -drive file="$image",format=raw,if=none,id=drv0 \
  -device nvme,drive=drv0,serial=NVME_SERIAL \
  -drive file="$extra",format=raw,if=none,id=drv1 \
  -device nvme,drive=drv1,serial=NVME_EXTRA \
  -enable-kvm -cpu host \
  $extra_qemu_args
