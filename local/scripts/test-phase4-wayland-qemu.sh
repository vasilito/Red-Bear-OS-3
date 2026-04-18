#!/usr/bin/env bash
# Launch the Phase 4 Wayland validation path in QEMU using the repo's Wayland profile.
# This script validates the current bounded software-path Wayland runtime slice.
# It is a bounded validation harness, not the production desktop path.
# It does NOT currently prove a hardware-accelerated desktop path in QEMU.

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
Usage: test-phase4-wayland-qemu.sh [--check] [extra qemu args...]

Boot the repo's Wayland profile in QEMU with a VirtIO NIC using UEFI firmware.

Examples:
  ./local/scripts/test-phase4-wayland-qemu.sh
  ./local/scripts/test-phase4-wayland-qemu.sh --check
  ./local/scripts/test-phase4-wayland-qemu.sh -m 4G

Expected validation path:
  display session -> validation launcher -> compositor -> wayland-session

Important:
  the current harness uses '-vga std' and today still surfaces llvmpipe in-guest.
  Treat this as a Phase 4 software-path/runtime smoke check and regression harness.
  Hardware-accelerated desktop proof is a separate bare-metal/runtime-driver milestone.
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

extra_qemu_args="${filtered_args[*]:-}"
if [[ -n "${QEMUFLAGS:-}" ]]; then
    QEMUFLAGS="${QEMUFLAGS} ${extra_qemu_args}"
else
    QEMUFLAGS="${extra_qemu_args}"
fi

arch="${ARCH:-$(uname -m)}"
image="build/$arch/redbear-wayland/harddrive.img"
extra="build/$arch/redbear-wayland/extra.img"

if [[ ! -f "$image" ]]; then
    echo "ERROR: missing image $image" >&2
    echo "Build it first with: ./local/scripts/build-redbear.sh redbear-wayland" >&2
    exit 1
fi

if [[ ! -f "$extra" ]]; then
    truncate -s 1g "$extra"
fi

echo "=== Red Bear OS Phase 4 Wayland QEMU Launch ==="
echo "Config: redbear-wayland"
echo "Image:  $image"
echo "UEFI:   $firmware"
echo
echo "Suggested in-guest checks:"
echo "  redbear-info --json"
echo "  netctl status"
echo "  redbear-phase4-wayland-check"
echo "  the validation compositor should own the bounded runtime path"
echo "  qt6-wayland-smoke should leave a success marker via wayland-session"
echo "  production desktop direction is redbear-kde -> kwin_wayland"
echo

if [[ "$check_mode" -eq 1 ]]; then
    expect <<EOF
log_user 1
set timeout 240
spawn qemu-system-x86_64 -name {Red Bear OS x86_64} -device qemu-xhci -smp 4 -m 2048 -bios $firmware -chardev stdio,id=debug,signal=off,mux=on -serial chardev:debug -mon chardev=debug -machine q35 -device ich9-intel-hda -device hda-output -device virtio-net,netdev=net0 -netdev user,id=net0 -object filter-dump,id=f1,netdev=net0,file=build/$arch/redbear-wayland/network.pcap -vga std -drive file=$image,format=raw,if=none,id=drv0 -device nvme,drive=drv0,serial=NVME_SERIAL -drive file=$extra,format=raw,if=none,id=drv1 -device nvme,drive=drv1,serial=NVME_EXTRA -enable-kvm -cpu host $QEMUFLAGS
expect "login:"
send "root\r"
expect "assword:"
send "password\r"
expect "Type 'help' for available commands."
send "redbear-phase4-wayland-check\r"
expect "Red Bear OS Phase 4 Wayland Runtime Check"
expect "redbear-validation-session"
expect "wayland-session"
expect "/home/root/.qt6-bootstrap-minimal.ok"
expect "/home/root/.qt6-plugin-minimal.ok"
expect "/home/root/.qt6-wayland-smoke-minimal.ok"
expect "/home/root/.qt6-wayland-smoke-offscreen.ok"
expect "/home/root/.qt6-wayland-smoke-wayland.ok"
expect "/home/root/.qt6-wayland-smoke.ok"
expect "qt6-wayland-smoke"
expect "virtio_net_present"
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
  -object filter-dump,id=f1,netdev=net0,file="build/$arch/redbear-wayland/network.pcap" \
  -vga std \
  -drive file="$image",format=raw,if=none,id=drv0 \
  -device nvme,drive=drv0,serial=NVME_SERIAL \
  -drive file="$extra",format=raw,if=none,id=drv1 \
  -device nvme,drive=drv1,serial=NVME_EXTRA \
  -enable-kvm -cpu host \
  $QEMUFLAGS
