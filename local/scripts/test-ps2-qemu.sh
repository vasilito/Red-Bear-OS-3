#!/usr/bin/env bash
# Launch or validate the PS/2 + serio path in QEMU.

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
Usage: test-ps2-qemu.sh [--check] [config] [extra qemu args...]

Launch or validate the PS/2 + serio path on a Red Bear image in QEMU.
USAGE
}

check_mode=0
filtered_args=()
config="redbear-desktop"
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
spawn qemu-system-x86_64 -name {Red Bear OS x86_64} -device qemu-xhci -smp 4 -m 2048 -bios $firmware -chardev stdio,id=debug,signal=off,mux=on -serial chardev:debug -mon chardev=debug -machine q35 -device ich9-intel-hda -device hda-output -device virtio-net,netdev=net0 -netdev user,id=net0 -object filter-dump,id=f1,netdev=net0,file=build/$arch/$config/network.pcap -nographic -vga none -drive file=$image,format=raw,if=none,id=drv0,snapshot=on -device nvme,drive=drv0,serial=NVME_SERIAL -drive file=$extra,format=raw,if=none,id=drv1,snapshot=on -device nvme,drive=drv1,serial=NVME_EXTRA -enable-kvm -cpu host $extra_qemu_args
expect "login:"
send "root\r"
expect "assword:"
send "password\r"
expect "Type 'help' for available commands."
send "redbear-phase-ps2-check\r"
expect "Red Bear OS PS/2 Runtime Check"
expect "present=/scheme/serio/0"
expect "present=/scheme/serio/1"
expect "phase3_input_check=ok"
send "shutdown\r"
sleep 2
EOF
    pkill -f "qemu-system-x86_64.*$image" 2>/dev/null || true
    echo "PS/2 serio runtime validation completed via guest runtime check"
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
  -object filter-dump,id=f1,netdev=net0,file="build/$arch/$config/network.pcap" \
  -nographic -vga none \
  -drive file="$image",format=raw,if=none,id=drv0,snapshot=on \
  -device nvme,drive=drv0,serial=NVME_SERIAL \
  -drive file="$extra",format=raw,if=none,id=drv1,snapshot=on \
  -device nvme,drive=drv1,serial=NVME_EXTRA \
  -enable-kvm -cpu host \
  $extra_qemu_args
