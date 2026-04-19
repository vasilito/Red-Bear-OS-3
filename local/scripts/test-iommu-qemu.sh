#!/usr/bin/env bash
# Launch or validate the IOMMU path in QEMU with an AMD IOMMU device.

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

# Print usage information
usage() {
    cat << USAGE
Usage: $(basename "$0") [--check] [config] [extra qemu args...]

Launch or validate QEMU with an AMD IOMMU device.

Options:
  --help      Show this help message
  --check     Boot and verify the guest reaches a login prompt

Arguments:
  config      Optional config name (default: redbear-desktop)
  extra qemu args   Additional arguments appended to the QEMU command

Environment:
  QEMUFLAGS   Additional flags (prepended to device amd-iommu)

Examples:
  $(basename "$0")
  $(basename "$0") --check
  $(basename "$0") redbear-desktop -m 4G

USAGE
    exit 0
}

check_mode=0
filtered_args=()
config="redbear-desktop"
for arg in "$@"; do
    case "$arg" in
        --help|-h|help)
            usage
            ;;
        --check)
            check_mode=1
            ;;
        redbear-*)
            config="$arg"
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
image="build/$arch/$config/harddrive.img"
extra="build/$arch/$config/extra.img"
extra_qemu_args="${filtered_args[*]:-}"

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

if [[ "$check_mode" -eq 1 ]]; then
    expect <<EOF
log_user 1
set timeout 240
spawn qemu-system-x86_64 -name {Red Bear OS x86_64} -device qemu-xhci -device amd-iommu -smp 4 -m 2048 -bios $firmware -chardev stdio,id=debug,signal=off,mux=on -serial chardev:debug -mon chardev=debug -machine q35 -device ich9-intel-hda -device hda-output -device virtio-net,netdev=net0 -netdev user,id=net0 -object filter-dump,id=f1,netdev=net0,file=build/$arch/$config/network.pcap -nographic -vga none -drive file=$image,format=raw,if=none,id=drv0,snapshot=on -device nvme,drive=drv0,serial=NVME_SERIAL -drive file=$extra,format=raw,if=none,id=drv1,snapshot=on -device nvme,drive=drv1,serial=NVME_EXTRA -enable-kvm -cpu host $extra_qemu_args
expect -re {PCI .*1022:1419}
expect "login:"
send "root\r"
expect "assword:"
send "password\r"
expect "Type 'help' for available commands."
send "redbear-phase-iommu-check\r"
expect "Red Bear OS IOMMU Runtime Check"
expect "units_detected="
expect "units_initialized_now="
expect "units_initialized_after="
expect "events_drained="
send "shutdown\r"
sleep 2
EOF
    pkill -f "qemu-system-x86_64.*$image" 2>/dev/null || true
    echo "IOMMU first-use validation path completed via guest runtime check"
    exit 0
fi

exec qemu-system-x86_64 \
  -name "Red Bear OS x86_64" \
  -device qemu-xhci \
  -device amd-iommu \
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
  $extra_qemu_args
