#!/usr/bin/env bash
# IRQ and low-level controller runtime validation — automated QEMU test harness.
#
# Boots a Red Bear OS image in QEMU, logs in, and runs all IRQ runtime check
# binaries to validate that each low-level controller surface is present and
# actually functional at runtime, not just installed.
#
# Modes:
#   --guest            Run inside a Red Bear OS guest
#   --qemu [CONFIG]    Boot CONFIG in QEMU and run the same checks automatically
#
# Exit codes:
#   0 — all checks passed
#   1 — one or more checks failed
#   2 — QEMU boot or login failure

set -euo pipefail

find_uefi_firmware() {
    local candidates=(
        "/usr/share/ovmf/x64/OVMF.4m.fd"
        "/usr/share/OVMF/x64/OVMF.4m.fd"
        "/usr/share/ovmf/x64/OVMF_CODE.4m.fd"
        "/usr/share/OVMF/x64/OVMF_CODE.4m.fd"
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
    echo "=== Red Bear OS IRQ Runtime Validation ==="
    echo

    local failures=0

    run_check() {
        local name="$1"
        local cmd="$2"
        local description="$3"

        if ! command -v "$cmd" >/dev/null 2>&1; then
            echo "  FAIL  $name: $cmd not found — $description"
            failures=$((failures + 1))
            return 0
        fi

        echo "  Running $name..."
        if "$cmd" >/dev/null 2>&1; then
            echo "  PASS  $name: $description"
        else
            echo "  FAIL  $name: $description (exit code non-zero)"
            failures=$((failures + 1))
        fi
    }

    echo "--- PCI IRQ ---"
    run_check "pci-irq" "redbear-phase-pci-irq-check" "/scheme/irq, MSI/MSI-X capability, affinity, and spurious IRQ routing quality"
    echo

    echo "--- IOMMU ---"
    run_check "iommu" "redbear-phase-iommu-check" "/scheme/iommu, AMD-Vi/Intel VT-d detection, event log, and interrupt remap setup"
    echo

    echo "--- DMA ---"
    run_check "dma" "redbear-phase-dma-check" "DMA buffer allocation and write/readback"
    echo

    echo "--- PS/2 + serio ---"
    run_check "ps2" "redbear-phase-ps2-check" "/scheme/input/ps2 or serio runtime path"
    echo

    echo "--- monotonic timer ---"
    run_check "timer" "redbear-phase-timer-check" "/scheme/time/CLOCK_MONOTONIC monotonic progress"
    echo

    echo "=== IRQ Runtime Validation Summary ==="
    echo "  Failure count: $failures"
    if [ "$failures" -gt 0 ]; then
        echo "  Result: FAIL"
        return 1
    fi

    echo "  Result: PASS"
    return 0
}

run_qemu_checks() {
    local config="${1:-redbear-full}"
    local firmware
    firmware="$(find_uefi_firmware)" || {
        echo "ERROR: no usable x86_64 UEFI firmware found" >&2
        exit 2
    }

    local arch image extra
    arch="${ARCH:-$(uname -m)}"
    image="build/$arch/$config/harddrive.img"
    extra="build/$arch/$config/extra.img"

    if [[ ! -f "$image" ]]; then
        echo "ERROR: missing image $image" >&2
        echo "Build it first with: ./local/scripts/build-redbear.sh $config" >&2
        exit 2
    fi

    if [[ ! -f "$extra" ]]; then
        truncate -s 1g "$extra"
    fi

    expect <<EXPECT_SCRIPT
log_user 1
set timeout 300

proc run_check {name cmd description ok_marker fail_marker missing_marker} {
    global failures

    puts "--- \$name ---"
    send "if command -v \$cmd >/dev/null 2>&1; then if \$cmd >/dev/null 2>&1; then echo \$ok_marker; else echo \$fail_marker; fi; else echo \$missing_marker; fi\r"
    expect {
        \$ok_marker {
            puts "  PASS  \$name: \$description"
        }
        \$fail_marker {
            puts "  FAIL  \$name: \$description (exit code non-zero)"
            incr failures
        }
        \$missing_marker {
            puts "  FAIL  \$name: \$cmd not found — \$description"
            incr failures
        }
        timeout {
            puts "  FAIL  \$name: timed out"
            incr failures
        }
        eof {
            puts "  FAIL  \$name: guest exited before check completion"
            exit 1
        }
    }
    puts ""
}

set failures 0
spawn qemu-system-x86_64 -name {Red Bear OS x86_64} -device qemu-xhci -smp 4 -m 2048 -bios $firmware -chardev stdio,id=debug,signal=off,mux=on -serial chardev:debug -mon chardev=debug -machine q35 -device ich9-intel-hda -device hda-output -device virtio-net,netdev=net0 -netdev user,id=net0 -nographic -vga none -drive file=$image,format=raw,if=none,id=drv0 -device nvme,drive=drv0,serial=NVME_SERIAL -drive file=$extra,format=raw,if=none,id=drv1 -device nvme,drive=drv1,serial=NVME_EXTRA -enable-kvm -cpu host
expect "login:"
send "root\r"
expect "assword:"
send "password\r"
expect "Type 'help' for available commands."
send "echo __READY__\r"
expect "__READY__"

puts "=== Red Bear OS IRQ Runtime Validation ==="
puts ""

run_check "PCI IRQ" "redbear-phase-pci-irq-check" "/scheme/irq, MSI/MSI-X capability, affinity, and spurious IRQ routing quality" "__PCI_IRQ_OK__" "__PCI_IRQ_FAIL__" "__PCI_IRQ_MISSING__"
run_check "IOMMU" "redbear-phase-iommu-check" "/scheme/iommu, AMD-Vi/Intel VT-d detection, event log, and interrupt remap setup" "__IOMMU_OK__" "__IOMMU_FAIL__" "__IOMMU_MISSING__"
run_check "DMA" "redbear-phase-dma-check" "DMA buffer allocation and write/readback" "__DMA_OK__" "__DMA_FAIL__" "__DMA_MISSING__"
run_check "PS/2 + serio" "redbear-phase-ps2-check" "/scheme/input/ps2 or serio runtime path" "__PS2_OK__" "__PS2_FAIL__" "__PS2_MISSING__"
run_check "monotonic timer" "redbear-phase-timer-check" "/scheme/time/CLOCK_MONOTONIC monotonic progress" "__TIMER_OK__" "__TIMER_FAIL__" "__TIMER_MISSING__"

puts "=== IRQ Runtime Validation Summary ==="
puts "  Failure count: \$failures"
if {\$failures == 0} {
    puts "  Result: PASS"
} else {
    puts "  Result: FAIL"
}

send "echo __IRQ_RUNTIME_DONE__\$failures__\r"
expect "__IRQ_RUNTIME_DONE__\$failures__"
if {\$failures != 0} {
    exit 1
}

send "shutdown\r"
expect eof
EXPECT_SCRIPT
}

usage() {
    cat <<'USAGE'
Usage:
  ./local/scripts/test-irq-runtime.sh --guest
  ./local/scripts/test-irq-runtime.sh --qemu [redbear-full]

This script validates the IRQ and low-level controller runtime substrate by
running the guest-side check binaries and using their exit codes as the
authoritative pass/fail signal.

Guest mode runs inside a Red Bear OS instance.
QEMU mode boots an image and runs checks automatically.

Required binaries (must be in PATH inside the guest):
  redbear-phase-pci-irq-check  — PCI IRQ runtime reports, MSI/MSI-X capability, affinity, spurious IRQs
  redbear-phase-iommu-check    — IOMMU runtime self-test + scheme control probes
  redbear-phase-dma-check      — DMA buffer allocation/runtime proof
  redbear-phase-ps2-check      — PS/2 + serio runtime proof
  redbear-phase-timer-check    — monotonic timer runtime proof
USAGE
}

case "${1:-}" in
    --guest)
        run_guest_checks
        ;;
    --qemu)
        run_qemu_checks "${2:-redbear-full}"
        ;;
    *)
        usage
        exit 1
        ;;
esac
