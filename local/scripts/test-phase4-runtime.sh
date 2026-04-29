#!/usr/bin/env bash
# Phase 4 KDE Plasma preflight — automated runtime validation harness.
# Validates KF6 libraries, plasma binaries, and session entry points.
# Does NOT validate real KDE Plasma session behavior (gated on Qt6Quick/QML + real KWin).

set -euo pipefail

PROG="$(basename "$0")"

usage() {
    cat <<'EOF'
Usage: test-phase4-runtime.sh [--guest|--qemu CONFIG]
Modes:
  --guest          Run inside already-booted Red Bear OS
  --qemu CONFIG    Launch QEMU with CONFIG and run checks
Exit: 0 if all pass, 1 otherwise.
EOF
    exit 1
}

MODE=""; CONFIG=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --guest) MODE="guest"; shift ;;
        --qemu) MODE="qemu"; CONFIG="$2"; shift 2 ;;
        -h|--help) usage ;;
        *) echo "$PROG: unknown: $1"; usage ;;
    esac
done
[[ -z "$MODE" ]] && usage

run_guest_checks() {
    local failures=0
    run_check() {
        local name="$1" cmd="$2" desc="$3"
        if ! command -v "$cmd" >/dev/null 2>&1; then
            echo "  FAIL  $name: $cmd not found ($desc)"
            failures=$((failures + 1)); return 0
        fi
        echo "  Running $name..."
        if "$cmd" --json >/dev/null 2>&1; then
            echo "  PASS  $name: $desc"
        else
            echo "  FAIL  $name: $desc (exit non-zero)"
            failures=$((failures + 1))
        fi
    }

    echo "=== Phase 4 KDE Plasma Preflight ==="; echo
    run_check "KDE plasma" "redbear-phase4-kde-check" "KF6 libs + plasma binaries + session entry"
    echo
    echo "=== Phase 4 Summary ==="
    if [[ $failures -eq 0 ]]; then echo "ALL PHASE 4 CHECKS PASSED"; else echo "FAILURES: $failures"; exit 1; fi
    exit 0
}

run_qemu_checks() {
    local arch="${ARCH:-x86_64}"
    local image="build/${arch}/${CONFIG}/harddrive.img"
    local firmware="${FIRMWARE_PATH:-/usr/share/ovmf/x64/OVMF.fd}"
    if [[ ! -f "$image" ]]; then echo "$PROG: image not found: $image (build with: make all CONFIG_NAME=$CONFIG)"; exit 1; fi
    if [[ ! -f "$firmware" ]]; then echo "$PROG: firmware not found: $firmware"; exit 1; fi
    expect <<EXPECT_SCRIPT
log_user 1; set timeout 300
spawn qemu-system-x86_64 -name {Red Bear OS} -device qemu-xhci -smp 4 -m 2048 -bios $firmware -chardev stdio,id=debug,signal=off,mux=on -serial chardev:debug -mon chardev:debug -machine q35 -device virtio-net,netdev=net0 -netdev user,id=net0 -nographic -vga none -drive file=$image,format=raw,if=none,id=drv0 -device nvme,drive=drv0,serial=NVME_SERIAL -enable-kvm -cpu host
expect "login:"; send "root\r"
expect "assword:"; send "password\r"
expect "Type 'help' for available commands."
send "echo __READY__\r"; expect "__READY__"
send "redbear-phase4-kde-check --json >/dev/null 2>&1 && echo __P4_OK__ || echo __P4_FAIL__\r"
expect { "__P4_OK__" { } "__P4_FAIL__" { puts "FAIL: Phase 4"; exit 1 } timeout { puts "FAIL: timeout"; exit 1 } eof { puts "FAIL: eof"; exit 1 } }
puts "ALL PHASE 4 CHECKS PASSED"
EXPECT_SCRIPT
    exit $?
}

case "$MODE" in
    guest) run_guest_checks ;;
qemu) export FIRMWARE_PATH="${FIRMWARE_PATH:-/usr/share/ovmf/x64/OVMF.fd}"; run_qemu_checks ;; 
    *) usage ;;
esac
