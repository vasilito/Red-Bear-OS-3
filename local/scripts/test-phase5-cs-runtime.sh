#!/usr/bin/env bash
# Phase 5 GPU command-submission validation harness.
# Validates GEM allocation, PRIME sharing, CS ioctl reachability, and fence waits.
# Real hardware rendering validation is still pending.

set -euo pipefail

PROG="$(basename "$0")"

usage() {
    cat <<'EOF'
Usage: test-phase5-cs-runtime.sh [--guest|--qemu CONFIG]
Modes:
  --guest          Run inside already-booted Red Bear OS
  --qemu CONFIG    Launch QEMU with CONFIG and run checks
Exit: 0 if all pass, 1 otherwise.
EOF
    exit 1
}

MODE=""
CONFIG=""
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
            failures=$((failures + 1))
            return 0
        fi

        echo "  Running $name..."
        if "$cmd" --json >/dev/null 2>&1; then
            echo "  PASS  $name: $desc"
        else
            echo "  FAIL  $name: $desc (exit non-zero)"
            failures=$((failures + 1))
        fi
    }

    echo "=== Phase 5 GPU Command Submission Validation ==="
    echo
    run_check "CS" "redbear-phase5-cs-check" "CS ioctls + GEM + PRIME + fence wait"
    echo
    echo "=== Phase 5 CS Summary ==="
    if [[ $failures -eq 0 ]]; then
        echo "ALL PHASE 5 CS CHECKS PASSED"
    else
        echo "FAILURES: $failures"
        exit 1
    fi
    exit 0
}

require_qemu_path() {
    local value="$1"
    local label="$2"
    if [[ "$value" == *$'\n'* || "$value" == *$'\r'* ]]; then
        echo "$PROG: $label contains a newline or carriage return"
        exit 1
    fi
    if [[ "$value" == *,* ]]; then
        echo "$PROG: $label must not contain commas for QEMU -drive parsing"
        exit 1
    fi
}

run_qemu_checks() {
    local arch="${ARCH:-x86_64}"
    local image="build/${arch}/${CONFIG}/harddrive.img"
    local firmware="${FIRMWARE_PATH:-/usr/share/ovmf/x64/OVMF.fd}"

    require_qemu_path "$image" "image path"
    require_qemu_path "$firmware" "firmware path"

    if [[ ! -f "$image" ]]; then
        echo "$PROG: image not found: $image (build with: make all CONFIG_NAME=$CONFIG)"
        exit 1
    fi

    if [[ ! -f "$firmware" ]]; then
        echo "$PROG: firmware not found: $firmware"
        exit 1
    fi

    env RBOS_PHASE5_CS_IMAGE="$image" RBOS_PHASE5_CS_FIRMWARE="$firmware" expect <<'EXPECT_SCRIPT'
log_user 1; set timeout 300
spawn qemu-system-x86_64 -name {Red Bear OS} -device qemu-xhci -smp 4 -m 2048 -bios $env(RBOS_PHASE5_CS_FIRMWARE) -chardev stdio,id=debug,signal=off,mux=on -serial chardev:debug -mon chardev:debug -machine q35 -device virtio-net,netdev=net0 -netdev user,id=net0 -nographic -vga none -drive file=$env(RBOS_PHASE5_CS_IMAGE),format=raw,if=none,id=drv0 -device nvme,drive=drv0,serial=NVME_SERIAL -enable-kvm -cpu host
expect "login:"; send "root\r"
expect "assword:"; send "password\r"
expect "Type 'help' for available commands."
send "echo __READY__\r"; expect "__READY__"
send "redbear-phase5-cs-check --json >/dev/null 2>&1 && echo __P5CS_OK__ || echo __P5CS_FAIL__\r"
expect { "__P5CS_OK__" { } "__P5CS_FAIL__" { puts "FAIL: Phase 5 CS"; exit 1 } timeout { puts "FAIL: timeout"; exit 1 } eof { puts "FAIL: eof"; exit 1 } }
puts "ALL PHASE 5 CS CHECKS PASSED"
EXPECT_SCRIPT
    exit $?
}

case "$MODE" in
    guest) run_guest_checks ;;
    qemu)
        export FIRMWARE_PATH="${FIRMWARE_PATH:-/usr/share/ovmf/x64/OVMF.fd}"
        run_qemu_checks
        ;;
    *) usage ;;
esac
