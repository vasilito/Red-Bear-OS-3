#!/usr/bin/env bash
# Phase 1 Runtime Substrate Validation — automated QEMU test harness.
#
# Boots a Red Bear OS image in QEMU, logs in, and runs all Phase 1 runtime
# check binaries plus redbear-info --probe to validate that each substrate
# service is present at runtime, not just installed.
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
    echo "=== Red Bear OS Phase 1 Runtime Substrate Validation ==="
    echo

    local failures=0

    # Run a check binary by exit code only. --json is for machine output;
    # the exit code (0=pass, 1=fail) is the authoritative result.
    run_check() {
        local name="$1"
        local cmd="$2"
        local description="$3"

        if ! command -v "$cmd" >/dev/null 2>&1; then
            echo "  FAIL  $name: $cmd not found — Phase 1 check binaries must be installed ($description)"
            failures=$((failures + 1))
            return 0
        fi

        echo "  Running $name..."
        if "$cmd" --json >/dev/null 2>&1; then
            echo "  PASS  $name: $description"
        else
            echo "  FAIL  $name: $description (exit code non-zero)"
            failures=$((failures + 1))
        fi
    }

    echo "--- relibc POSIX API surface ---"
    local posix_tests_dir="/home/user/relibc-phase1-tests"
    local expected_bins=(
        "test_signalfd_wayland"
        "test_timerfd_qt6"
        "test_eventfd_qt6"
        "test_shm_open_qt6"
        "test_sem_open_qt6"
        "test_waitid_qt6"
    )
    if [[ -d "$posix_tests_dir" ]]; then
        for test_name in "${expected_bins[@]}"; do
            local test_bin="$posix_tests_dir/$test_name"
            if [[ -x "$test_bin" ]]; then
                echo "  Running $test_name..."
                if "$test_bin" >/dev/null 2>&1; then
                    echo "  PASS  $test_name"
                else
                    echo "  FAIL  $test_name (exit code non-zero)"
                    failures=$((failures + 1))
                fi
            else
                echo "  FAIL  $test_name (binary missing or not executable)"
                failures=$((failures + 1))
            fi
        done
    else
        echo "  FAIL  relibc POSIX tests directory not found at $posix_tests_dir"
        failures=$((failures + 1))
    fi
    echo

    echo "--- evdevd input path ---"
    run_check "evdevd" "redbear-phase1-evdev-check" "evdevd input event delivery"
    echo

    echo "--- udev-shim device enumeration ---"
    run_check "udev-shim" "redbear-phase1-udev-check" "udev-shim device enumeration"
    echo

    echo "--- firmware-loader ---"
    run_check "firmware-loader" "redbear-phase1-firmware-check" "firmware blob loading"
    echo

    echo "--- DRM/KMS ---"
    run_check "redox-drm" "redbear-phase1-drm-check" "DRM scheme + KMS queries"
    echo

    echo "--- redbear-info --probe ---"
    if ! command -v redbear-info >/dev/null 2>&1; then
        echo "  FAIL  redbear-info not found — must be installed"
        failures=$((failures + 1))
    else
        echo "  Running redbear-info --probe..."
        if redbear-info --probe >/dev/null 2>&1; then
            echo "  PASS  redbear-info --probe reports all services present"
        else
            echo "  FAIL  redbear-info --probe reports gaps (exit non-zero)"
            failures=$((failures + 1))
        fi
    fi
    echo

    echo "=== Phase 1 Runtime Substrate Validation Complete ==="
    if [ "$failures" -gt 0 ]; then
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

    # All Phase 1 check binaries use exit code 0 for pass, 1 for fail.
    # redbear-info --probe exits 0 if all services present, non-zero otherwise.
    # relibc POSIX tests use exit code 0/1.
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

# Relibc POSIX tests — FAIL markers cause overall failure
send "cd /home/user/relibc-phase1-tests && POSIX_FAIL=0; for t in test_signalfd_wayland test_timerfd_qt6 test_eventfd_qt6 test_shm_open_qt6 test_sem_open_qt6 test_waitid_qt6; do echo \"POSIX:\\\$t\"; if ./\\\$t >/dev/null 2>&1; then echo \"\\\${t}:PASS\"; else echo \"\\\${t}:FAIL\"; POSIX_FAIL=1; fi; done; echo __POSIX_DONE__\\\$POSIX_FAIL__\r"
expect {
    "__POSIX_DONE__0__" { }
    "__POSIX_DONE__1__" { puts "FAIL: one or more relibc POSIX tests failed"; exit 1 }
    timeout { puts "FAIL: timed out before POSIX test completion"; exit 1 }
    eof { puts "FAIL: guest exited before POSIX test completion"; exit 1 }
}

# Phase 1 check binaries — exit code is authoritative
send "redbear-phase1-evdev-check --json >/dev/null 2>&1 && echo __EVDV_OK__ || echo __EVDV_FAIL__\r"
expect {
    "__EVDV_OK__" { }
    "__EVDV_FAIL__" { puts "FAIL: evdevd check failed"; exit 1 }
}

send "redbear-phase1-udev-check --json >/dev/null 2>&1 && echo __UDEV_OK__ || echo __UDEV_FAIL__\r"
expect {
    "__UDEV_OK__" { }
    "__UDEV_FAIL__" { puts "FAIL: udev-shim check failed"; exit 1 }
}

send "redbear-phase1-firmware-check --json >/dev/null 2>&1 && echo __FW_OK__ || echo __FW_FAIL__\r"
expect {
    "__FW_OK__" { }
    "__FW_FAIL__" { puts "FAIL: firmware-loader check failed"; exit 1 }
}

send "redbear-phase1-drm-check --json >/dev/null 2>&1 && echo __DRM_OK__ || echo __DRM_FAIL__\r"
expect {
    "__DRM_OK__" { }
    "__DRM_FAIL__" { puts "FAIL: DRM check failed"; exit 1 }
}

send "redbear-info --probe >/dev/null 2>&1 && echo __PROBE_OK__ || echo __PROBE_FAIL__\r"
expect {
    "__PROBE_OK__" { }
    "__PROBE_FAIL__" { puts "FAIL: redbear-info --probe reported gaps"; exit 1 }
}

send "echo __PHASE1_RUNTIME_DONE__\r"
expect "__PHASE1_RUNTIME_DONE__"
send "shutdown\r"
expect eof
EXPECT_SCRIPT
}

usage() {
    cat <<'USAGE'
Usage:
  ./local/scripts/test-phase1-runtime.sh --guest
  ./local/scripts/test-phase1-runtime.sh --qemu [redbear-full]

This script validates the Phase 1 runtime substrate by running probes
against each service, checking exit codes for authoritative pass/fail.

Guest mode runs inside a Red Bear OS instance.
QEMU mode boots an image and runs checks automatically.

Required binaries (must be in PATH inside the guest):
  redbear-phase1-evdev-check    — evdevd input event validation
  redbear-phase1-udev-check     — udev-shim device enumeration validation
  redbear-phase1-firmware-check — firmware-loader blob loading validation
  redbear-phase1-drm-check      — DRM/KMS scheme query validation
  redbear-info --probe          — Phase 1 service presence probe

Required test programs (in /home/user/relibc-phase1-tests/):
  test_signalfd_wayland  — signalfd POSIX API
  test_timerfd_qt6       — timerfd POSIX API
  test_eventfd_qt6       — eventfd POSIX API
  test_shm_open_qt6     — shm_open POSIX API
  test_sem_open_qt6     — sem_open POSIX API
  test_waitid_qt6        — waitid POSIX API
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