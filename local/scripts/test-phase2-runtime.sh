#!/usr/bin/env bash
# Phase 2 Wayland compositor proof — automated runtime validation harness.
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
    echo "=== Red Bear OS Phase 2 Wayland Runtime Validation ==="
    echo

    local failures=0
    local expected_bins=(
        "redbear-phase2-wayland-check"
    )

    local bin
    for bin in "${expected_bins[@]}"; do
        if ! command -v "$bin" >/dev/null 2>&1; then
            echo "  FAIL  $bin: required Phase 2 check binary is not installed"
            failures=$((failures + 1))
        fi
    done

    if [[ "$failures" -eq 0 ]]; then
        echo "  Running redbear-phase2-wayland-check..."
        if redbear-phase2-wayland-check --json >/dev/null 2>&1; then
            echo "  PASS  redbear-phase2-wayland-check: Wayland compositor proof checks passed"
        else
            echo "  FAIL  redbear-phase2-wayland-check: Wayland compositor proof checks failed"
            failures=$((failures + 1))
        fi
    fi

    echo
    echo "=== Phase 2 Wayland Runtime Validation Complete ==="
    if [[ "$failures" -gt 0 ]]; then
        echo "  $failures check(s) FAILED"
        return 1
    fi
    echo "  All checks PASSED"
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
spawn qemu-system-x86_64 -name {Red Bear OS x86_64} -device qemu-xhci -smp 4 -m 2048 -bios $firmware -chardev stdio,id=debug,signal=off,mux=on -serial chardev:debug -mon chardev=debug -machine q35 -device ich9-intel-hda -device hda-output -device virtio-net,netdev=net0 -netdev user,id=net0 -nographic -vga none -drive file=$image,format=raw,if=none,id=drv0 -device nvme,drive=drv0,serial=NVME_SERIAL -drive file=$extra,format=raw,if=none,id=drv1 -device nvme,drive=drv1,serial=NVME_EXTRA -enable-kvm -cpu host
expect "login:"
send "root\r"
expect "assword:"
send "password\r"
expect "Type 'help' for available commands."
send "echo __READY__\r"
expect "__READY__"

send "command -v redbear-phase2-wayland-check >/dev/null 2>&1 && echo __PHASE2_BIN_OK__ || echo __PHASE2_BIN_FAIL__\r"
expect {
    "__PHASE2_BIN_OK__" { }
    "__PHASE2_BIN_FAIL__" { puts "FAIL: redbear-phase2-wayland-check is missing"; exit 1 }
    timeout { puts "FAIL: timed out while checking for redbear-phase2-wayland-check"; exit 1 }
    eof { puts "FAIL: guest exited before Phase 2 binary check completed"; exit 1 }
}

send "redbear-phase2-wayland-check --json >/dev/null 2>&1 && echo __PHASE2_OK__ || echo __PHASE2_FAIL__\r"
expect {
    "__PHASE2_OK__" { }
    "__PHASE2_FAIL__" { puts "FAIL: redbear-phase2-wayland-check reported failures"; exit 1 }
    timeout { puts "FAIL: timed out while running redbear-phase2-wayland-check"; exit 1 }
    eof { puts "FAIL: guest exited before Phase 2 check completed"; exit 1 }
}

send "echo __PHASE2_RUNTIME_DONE__\r"
expect "__PHASE2_RUNTIME_DONE__"
send "shutdown\r"
expect eof
EXPECT_SCRIPT
}

usage() {
    cat <<'USAGE'
Usage:
  ./local/scripts/test-phase2-runtime.sh --guest
  ./local/scripts/test-phase2-runtime.sh --qemu [redbear-full]

This script validates the Phase 2 Wayland compositor proof by running the
canonical Phase 2 check binary and treating its exit code as authoritative.

Required binary (must be in PATH inside the guest):
  redbear-phase2-wayland-check — Wayland compositor proof validation
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
