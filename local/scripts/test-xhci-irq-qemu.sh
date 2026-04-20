#!/usr/bin/env bash
# Launch or validate xHCI interrupt-mode bring-up in QEMU.

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
Usage: test-xhci-irq-qemu.sh [--check] [config]

Boot or validate xHCI interrupt-mode bring-up on a Red Bear image in QEMU.
Defaults to redbear-mini (mapped to the in-tree redbear-minimal image).
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
    log_file="build/$arch/$config/xhci-irq-check.log"
    rm -f "$log_file"
    set +e
    timeout 180s qemu-system-x86_64 \
      -name "Red Bear OS x86_64" \
      -device qemu-xhci,id=xhci \
      -device usb-kbd,bus=xhci.0 \
      -device usb-tablet,bus=xhci.0 \
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
      > "$log_file" 2>&1
    status=$?
    set -e
    if ! grep -q "xhcid: using MSI/MSI-X interrupt delivery\|xhcid: using legacy INTx interrupt delivery\|XHCI .* IRQ:" "$log_file"; then
        echo "ERROR: xhcid did not report an interrupt-driven mode; see $log_file" >&2
        exit 1
    fi
    if ! grep -q "xhcid: begin attach for port\|xhcid: queueing initial enumeration for port" "$log_file"; then
        echo "ERROR: xhcid interrupt-mode proof never observed attached-device enumeration pressure; see $log_file" >&2
        exit 1
    fi
    mode="unknown"
    reason="unknown"
    if grep -q "xhcid: using MSI/MSI-X interrupt delivery" "$log_file"; then
        mode="msi_or_msix"
        reason="driver_selected_interrupt_delivery"
    elif grep -q "xhcid: using legacy INTx interrupt delivery" "$log_file"; then
        mode="legacy"
        reason="driver_fell_back_to_legacy_intx"
    elif grep -q "xhcid: falling back to polling mode" "$log_file"; then
        mode="polling"
        reason="driver_fell_back_to_polling"
    fi

    echo "IRQ_DRIVER=xhcid"
    echo "IRQ_MODE=$mode"
    echo "IRQ_REASON=$reason"
    echo "IRQ_LOG=$log_file"
    echo "xHCI interrupt mode detected in $log_file"
    exit 0
fi

exec qemu-system-x86_64 \
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
  -vga std \
  -drive file="$image",format=raw,if=none,id=drv0,snapshot=on \
  -device nvme,drive=drv0,serial=NVME_SERIAL \
  -drive file="$extra",format=raw,if=none,id=drv1,snapshot=on \
  -device nvme,drive=drv1,serial=NVME_EXTRA \
  -enable-kvm -cpu host
