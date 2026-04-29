#!/usr/bin/env bash
# POSIX/relibc completeness runtime validation harness.
# Runs relibc-phase1-tests C programs and validates POSIX compliance.
# Follows Phase 1-5 pattern: guest + QEMU, exit-code-based.

set -euo pipefail
PROG="$(basename "$0")"

usage() {
    cat <<'EOF'
Usage: test-posix-runtime.sh [--guest|--qemu CONFIG]
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

EXPECTED_TESTS=(
    test_signalfd_wayland
    test_timerfd_qt6
    test_eventfd_qt6
    test_shm_open_qt6
    test_sem_open_qt6
    test_waitid_qt6
)

run_guest_checks() {
    local failures=0
    local posix_dir="/home/user/relibc-phase1-tests"

    echo "=== POSIX/relibc Completeness Validation ==="; echo

    if [[ ! -d "$posix_dir" ]]; then
        echo "  FAIL  POSIX test directory not found at $posix_dir"
        failures=$((failures + 1))
    else
        for test_name in "${EXPECTED_TESTS[@]}"; do
            local test_bin="$posix_dir/$test_name"
            if [[ ! -x "$test_bin" ]]; then
                echo "  FAIL  $test_name: binary missing or not executable"
                failures=$((failures + 1))
                continue
            fi
            echo "  Running $test_name..."
            if "$test_bin" >/dev/null 2>&1; then
                echo "  PASS  $test_name"
            else
                echo "  FAIL  $test_name (exit code non-zero)"
                failures=$((failures + 1))
            fi
        done
    fi

    echo
    echo "=== POSIX Summary ==="
    if [[ $failures -eq 0 ]]; then
        echo "ALL POSIX TESTS PASSED"
    else
        echo "FAILURES: $failures"
        exit 1
    fi
    exit 0
}

run_qemu_checks() {
    local arch="${ARCH:-x86_64}"
    local image="build/${arch}/${CONFIG}/harddrive.img"
    local firmware="${FIRMWARE_PATH:-/usr/share/ovmf/x64/OVMF.fd}"
    if [[ ! -f "$image" ]]; then echo "$PROG: image not found: $image"; exit 1; fi
    if [[ ! -f "$firmware" ]]; then echo "$PROG: firmware not found: $firmware"; exit 1; fi
    expect <<EXPECT_SCRIPT
log_user 1; set timeout 300
spawn qemu-system-x86_64 -name {Red Bear OS} -device qemu-xhci -smp 4 -m 2048 -bios $firmware -chardev stdio,id=debug,signal=off,mux=on -serial chardev:debug -mon chardev:debug -machine q35 -device virtio-net,netdev=net0 -netdev user,id=net0 -nographic -vga none -drive file=$image,format=raw,if=none,id=drv0 -device nvme,drive=drv0,serial=NVME_SERIAL -enable-kvm -cpu host
expect "login:"; send "root\r"
expect "assword:"; send "password\r"
expect "Type 'help' for available commands."
send "echo __READY__\r"; expect "__READY__"
send "cd /home/user/relibc-phase1-tests && POSIX_FAIL=0; for t in test_signalfd_wayland test_timerfd_qt6 test_eventfd_qt6 test_shm_open_qt6 test_sem_open_qt6 test_waitid_qt6; do echo \"POSIX:\\\$t\"; if ./\\\$t >/dev/null 2>&1; then echo \"\\\${t}:PASS\"; else echo \"\\\${t}:FAIL\"; POSIX_FAIL=1; fi; done; echo __POSIX_DONE__\\\$POSIX_FAIL__\r"
expect { "__POSIX_DONE__0__" { } "__POSIX_DONE__1__" { puts "FAIL: POSIX tests"; exit 1 } timeout { puts "FAIL: timeout"; exit 1 } eof { puts "FAIL: eof"; exit 1 } }
puts "ALL POSIX TESTS PASSED"
EXPECT_SCRIPT
    exit $?
}

case "$MODE" in
    guest) run_guest_checks ;;
    qemu) export FIRMWARE_PATH="${FIRMWARE_PATH:-/usr/share/ovmf/x64/OVMF.fd}"; run_qemu_checks ;;
    *) usage ;;
esac
