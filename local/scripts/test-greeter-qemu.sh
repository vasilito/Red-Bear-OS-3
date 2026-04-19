#!/usr/bin/env bash
# test-greeter-qemu.sh — bounded QEMU proof for the Red Bear greeter/auth surface.

set -euo pipefail

usage() {
    cat <<'USAGE'
Usage: test-greeter-qemu.sh [--check]

Boot redbear-full in QEMU, log in on the fallback console, and verify the greeter daemon/socket
surface, invalid-login handling, and a bounded successful-login return-to-greeter proof.
USAGE
}

check_mode=0
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
            echo "ERROR: unsupported argument $arg" >&2
            usage >&2
            exit 1
            ;;
    esac
done

firmware=""
for candidate in \
    /usr/share/ovmf/x64/OVMF.4m.fd \
    /usr/share/OVMF/x64/OVMF.4m.fd \
    /usr/share/ovmf/OVMF.fd \
    /usr/share/OVMF/OVMF_CODE.fd \
    /usr/share/qemu/edk2-x86_64-code.fd
do
    if [[ -f "$candidate" ]]; then
        firmware="$candidate"
        break
    fi
done

if [[ -z "$firmware" ]]; then
    echo "ERROR: no usable x86_64 UEFI firmware found" >&2
    exit 1
fi

arch="${ARCH:-$(uname -m)}"
image="build/$arch/redbear-full/harddrive.img"

if [[ ! -f "$image" ]]; then
    echo "ERROR: missing image $image" >&2
    echo "Build it first with: ./local/scripts/build-redbear.sh redbear-full" >&2
    exit 1
fi

if [[ "$check_mode" -eq 0 ]]; then
    exec qemu-system-x86_64 \
        -name "Red Bear Greeter Validation" \
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
        -vga none \
        -device virtio-gpu \
        -drive file="$image",format=raw,if=none,id=drv0 \
        -device nvme,drive=drv0,serial=NVME_SERIAL \
        -enable-kvm -cpu host
fi

expect <<EOF
log_user 1
set timeout 240
spawn qemu-system-x86_64 -name {Red Bear Greeter Validation} -device qemu-xhci -smp 4 -m 2048 -bios $firmware -chardev stdio,id=debug,signal=off,mux=on -serial chardev:debug -mon chardev=debug -machine q35 -device ich9-intel-hda -device hda-output -device virtio-net,netdev=net0 -netdev user,id=net0 -vga none -device virtio-gpu -drive file=$image,format=raw,if=none,id=drv0 -device nvme,drive=drv0,serial=NVME_SERIAL -enable-kvm -cpu host
expect "login:"
send "root\r"
expect "assword:"
send "password\r"
expect "Type 'help' for available commands."
send "redbear-greeter-check\r"
expect "Red Bear Greeter Runtime Check"
expect "GREETER_HELLO=ok"
send "redbear-greeter-check --invalid root wrong\r"
expect "GREETER_INVALID=ok"
send "redbear-greeter-check --valid root password\r"
expect "GREETER_VALID=ok"
send "shutdown\r"
expect eof
EOF
