#!/usr/bin/env bash
# test-dbus-qemu.sh — Validate D-Bus system bus and redbear-sessiond inside a QEMU guest
#
# Usage:
#   ./local/scripts/test-dbus-qemu.sh [--check] [--config CONFIG]
#
# Options:
#   --check            Run non-interactively, exit 0 on pass, 1 on fail
#   --config CONFIG    Build config to test (default: redbear-full)
#
# --check mode boots the image, waits for the login prompt, then sends D-Bus
# validation commands via the serial console. Output is captured and parsed.
#
# Checks performed inside the guest:
#   1. dbus-daemon is running (system bus socket exists)
#   2. org.freedesktop.login1 is registered on the system bus
#   3. redbear-sessiond process is running
#   4. login1.Manager.ListSessions returns session c1
#   5. login1.Manager.IdleHint property is false
#   6. login1.Session.Active property is true
#   7. login1.Seat.CanGraphical property is true
#
# Exit codes:
#   0  All checks passed
#   1  One or more checks failed
#   2  Build or QEMU launch failed

set -euo pipefail

CHECK_MODE=0
CONFIG_NAME="redbear-full"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --check)
            CHECK_MODE=1
            shift
            ;;
        --config)
            CONFIG_NAME="$2"
            shift 2
            ;;
        *)
            echo "Usage: $0 [--check] [--config CONFIG]" >&2
            exit 2
            ;;
    esac
done

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
IMAGE="${REPO_ROOT}/build/${CONFIG_NAME}/x86_64/harddrive.img"

if [[ ! -f "$IMAGE" ]]; then
    echo "test-dbus-qemu: image not found at ${IMAGE}" >&2
    echo "  Run: make all CONFIG_NAME=${CONFIG_NAME}" >&2
    exit 2
fi

GUEST_SCRIPT='/tmp/dbus-check.sh'
OUTPUT_FILE='/tmp/dbus-qemu-output.txt'

# Build the guest-side check script as a standalone file.
# Uses org.freedesktop.DBus.Properties.Get for property access (checks 5-7).
cat > "$GUEST_SCRIPT" <<'GUEST_EOF'
#!/bin/sh
echo "=== D-Bus System Bus Validation ==="

# Check 1: dbus-daemon running
if [ -S /run/dbus/system_bus_socket ]; then
    echo "PASS: system bus socket exists at /run/dbus/system_bus_socket"
else
    echo "FAIL: system bus socket not found at /run/dbus/system_bus_socket"
fi

# Check 2: org.freedesktop.login1 registered
if command -v dbus-send >/dev/null 2>&1; then
    RESULT=$(dbus-send --system --dest=org.freedesktop.DBus \
        --type=method_call --print-reply \
        /org/freedesktop/DBus \
        org.freedesktop.DBus.ListNames 2>&1)
    if echo "$RESULT" | grep -q "org.freedesktop.login1"; then
        echo "PASS: org.freedesktop.login1 is registered on the system bus"
    else
        echo "FAIL: org.freedesktop.login1 not found on the system bus"
        echo "  Available names: $(echo "$RESULT" | grep string || echo none)"
    fi
else
    echo "SKIP: dbus-send not available (install dbus package)"
fi

# Check 3: redbear-sessiond process
if ps | grep -q '[r]edbear-sessiond'; then
    echo "PASS: redbear-sessiond process is running"
else
    echo "FAIL: redbear-sessiond process not found"
fi

# Check 4: login1.Manager.ListSessions
if command -v dbus-send >/dev/null 2>&1; then
    SESSIONS=$(dbus-send --system --dest=org.freedesktop.login1 \
        --type=method_call --print-reply \
        /org/freedesktop/login1 \
        org.freedesktop.login1.Manager.ListSessions 2>&1)
    if echo "$SESSIONS" | grep -q "c1"; then
        echo "PASS: ListSessions returns session c1"
    else
        echo "FAIL: ListSessions did not return session c1"
    fi
fi

# Check 5: login1.Manager.IdleHint (property, not method)
if command -v dbus-send >/dev/null 2>&1; then
    IDLE=$(dbus-send --system --dest=org.freedesktop.login1 \
        --type=method_call --print-reply \
        /org/freedesktop/login1 \
        org.freedesktop.DBus.Properties.Get \
        string:'org.freedesktop.login1.Manager' \
        string:'IdleHint' 2>&1)
    if echo "$IDLE" | grep -q "false"; then
        echo "PASS: Manager.IdleHint = false"
    else
        echo "FAIL: Manager.IdleHint not false (got: $IDLE)"
    fi
fi

# Check 6: login1.Session.Active (property, not method)
if command -v dbus-send >/dev/null 2>&1; then
    ACTIVE=$(dbus-send --system --dest=org.freedesktop.login1 \
        --type=method_call --print-reply \
        /org/freedesktop/login1/session/c1 \
        org.freedesktop.DBus.Properties.Get \
        string:'org.freedesktop.login1.Session' \
        string:'Active' 2>&1)
    if echo "$ACTIVE" | grep -q "true"; then
        echo "PASS: Session.Active = true"
    else
        echo "FAIL: Session.Active not true (got: $ACTIVE)"
    fi
fi

# Check 7: login1.Seat.CanGraphical (property, not method)
if command -v dbus-send >/dev/null 2>&1; then
    GRAPH=$(dbus-send --system --dest=org.freedesktop.login1 \
        --type=method_call --print-reply \
        /org/freedesktop/login1/seat/seat0 \
        org.freedesktop.DBus.Properties.Get \
        string:'org.freedesktop.login1.Seat' \
        string:'CanGraphical' 2>&1)
    if echo "$GRAPH" | grep -q "true"; then
        echo "PASS: Seat.CanGraphical = true"
    else
        echo "FAIL: Seat.CanGraphical not true (got: $GRAPH)"
    fi
fi

echo "=== D-Bus Validation Complete ==="
GUEST_EOF
chmod +x "$GUEST_SCRIPT"

if [[ "$CHECK_MODE" -eq 1 ]]; then
    if ! command -v expect >/dev/null 2>&1; then
        echo "test-dbus-qemu: --check mode requires 'expect' (install tcllib/expect)" >&2
        exit 2
    fi

    echo "test-dbus-qemu: launching QEMU with D-Bus checks (non-interactive)"

    # Use expect to boot the guest, wait for login, run checks, capture output
    expect <<'EXPECT_EOF' > "$OUTPUT_FILE" 2>&1
set timeout 120
spawn qemu-system-x86_64 \
    -drive file="$::env(IMAGE)" \
    -m 2G \
    -smp 2 \
    -nographic \
    -no-reboot

expect {
    "login:" { }
    timeout { puts "FAIL: timed out waiting for login prompt"; exit 1 }
}

sleep 2
send "root\r"

expect {
    "#" { }
    timeout { puts "FAIL: timed out waiting for shell prompt"; exit 1 }
}

sleep 1

# Read and send the check script line by line
set fp [open "/tmp/dbus-check.sh" r]
while {[gets $fp line] >= 0} {
    send "$line\r"
    expect {
        "#" { }
        timeout { puts "FAIL: timed out during check execution"; exit 1 }
    }
}
close $fp

sleep 2
send "poweroff\r"

expect {
    "Power down" { }
    timeout { }
}

exit 0
EXPECT_EOF

    FAILED=$(grep -c "FAIL" "$OUTPUT_FILE" 2>/dev/null || true)
    PASSED=$(grep -c "PASS" "$OUTPUT_FILE" 2>/dev/null || true)

    grep -E "PASS|FAIL|=== D-Bus" "$OUTPUT_FILE" || true
    echo ""
    echo "Results: ${PASSED} passed, ${FAILED} failed"

    rm -f "$GUEST_SCRIPT"
    if [[ "$FAILED" -gt 0 ]]; then
        exit 1
    fi
    exit 0
else
    echo "test-dbus-qemu: launching QEMU (interactive mode)"
    echo "  Guest check script written to /tmp/dbus-check.sh"
    echo "  After login, run: sh /tmp/dbus-check.sh"
    echo ""

    qemu-system-x86_64 \
        -drive file="$IMAGE",format=raw \
        -m 2G \
        -smp 2 \
        -serial mon:stdio
fi
