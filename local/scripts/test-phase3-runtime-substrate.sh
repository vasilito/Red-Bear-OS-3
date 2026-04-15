#!/usr/bin/env bash
# Validate the Red Bear OS Phase 3 runtime substrate.
#
# Modes:
#   --guest            Run inside a Red Bear OS guest
#   --qemu [CONFIG]    Boot CONFIG in QEMU and run the same checks automatically

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

run_guest_checks() {
    echo "=== Red Bear OS Phase 3 Runtime Substrate Test ==="
    echo

    require_path() {
        local path="$1"
        local message="$2"
        if [ -e "$path" ]; then
            echo "✅ $message"
        else
            echo "❌ $message"
            exit 1
        fi
    }

    require_command() {
        local cmd="$1"
        local message="$2"
        if command -v "$cmd" >/dev/null 2>&1; then
            echo "✅ $message"
        else
            echo "❌ $message"
            exit 1
        fi
    }

    echo "=== Runtime commands ==="
    require_command redbear-info "redbear-info is installed"
    require_command udev-shim "udev-shim command is installed"
    require_command evdevd "evdevd command is installed"
    require_command redbear-evtest "evdev consumer test tool is installed"
    require_command redbear-input-inject "input injector test tool is installed"
    require_command redbear-phase3-input-check "phase 3 guest input check script is installed"
    echo

    echo "=== Scheme surfaces ==="
    require_path /scheme/pci "PCI scheme is available"
    echo

    echo "=== redbear-info --json ==="
    local report
    report="$(redbear-info --json)"
    printf '%s\n' "$report"
    case "$report" in
        *'scheme firmware is registered in /scheme'*) echo "✅ firmware scheme reported" ;;
        *) echo "❌ firmware scheme not reported"; exit 1 ;;
    esac
    case "$report" in
        *'scheme udev is registered in /scheme'*) echo "✅ udev scheme reported" ;;
        *) echo "❌ udev scheme not reported"; exit 1 ;;
    esac
    case "$report" in
        *'"name": "evdevd"'*'"state": "active"'*) echo "✅ evdevd reported active" ;;
        *) echo "❌ evdevd not reported active"; exit 1 ;;
    esac

    echo
    echo "=== Phase 3 input validation ==="
    echo "Run 'redbear-phase3-input-check' to prove the evdev consumer path."
    echo
    echo "=== Test Complete ==="
}

run_qemu_checks() {
    local config="$1"
    local firmware
    firmware="$(find_uefi_firmware)" || {
        echo "ERROR: no usable x86_64 UEFI firmware found" >&2
        exit 1
    }

    local arch image extra
    arch="${ARCH:-$(uname -m)}"
    image="build/$arch/$config/harddrive.img"
    extra="build/$arch/$config/extra.img"

    if [[ ! -f "$image" ]]; then
        echo "ERROR: missing image $image" >&2
        echo "Build it first with: ./local/scripts/build-redbear.sh $config" >&2
        exit 1
    fi

    if [[ ! -f "$extra" ]]; then
        truncate -s 1g "$extra"
    fi

    expect <<EOF
log_user 1
set timeout 300
spawn qemu-system-x86_64 -name {Red Bear OS x86_64} -device qemu-xhci -smp 4 -m 2048 -bios $firmware -chardev stdio,id=debug,signal=off,mux=on -serial chardev:debug -mon chardev=debug -machine q35 -device ich9-intel-hda -device hda-output -device virtio-net,netdev=net0 -netdev user,id=net0 -object filter-dump,id=f1,netdev=net0,file=build/$arch/$config/network.pcap -nographic -vga none -drive file=$image,format=raw,if=none,id=drv0 -device nvme,drive=drv0,serial=NVME_SERIAL -drive file=$extra,format=raw,if=none,id=drv1 -device nvme,drive=drv1,serial=NVME_EXTRA -enable-kvm -cpu host
expect "login:"
send "root\r"
expect "assword:"
send "password\r"
expect "Type 'help' for available commands."
send "echo __READY__\r"
expect "__READY__"
send "redbear-phase3-input-check; echo __EVTEST_DONE__\r"
expect "Injected synthetic key event: A"
send "redbear-info --json\r"
expect "\"virtio_net_present\": true"
expect "scheme firmware is registered"
expect "scheme udev is registered"
expect "\"name\": \"evdevd\""
expect "\"state\": \"active\""
expect "EV_KEY code="
expect "__EVTEST_DONE__"
send "shutdown\r"
expect eof
EOF
}

usage() {
    cat <<'USAGE'
Usage:
  ./local/scripts/test-phase3-runtime-substrate.sh --guest
  ./local/scripts/test-phase3-runtime-substrate.sh --qemu [redbear-desktop]
USAGE
}

case "${1:-}" in
    --guest)
        run_guest_checks
        ;;
    --qemu)
        run_qemu_checks "${2:-redbear-desktop}"
        ;;
    *)
        usage
        exit 1
        ;;
esac
