#!/usr/bin/env bash
# Reference shell reimplementation of the guest-side Phase 4 Wayland runtime check.
# Run this inside the guest when `redbear-phase4-wayland-check` is unavailable for debugging.

set -euo pipefail

echo "=== Red Bear OS Phase 4 Wayland Runtime Test ==="
echo

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

require_command orbital-wayland "orbital-wayland launcher is installed"
require_command wayland-session "wayland-session launcher is installed"
require_command smallvil "smallvil compositor is installed"
require_command qt6-wayland-smoke "qt6-wayland-smoke is installed"
require_command qt6-bootstrap-check "qt6-bootstrap-check is installed"
require_command qt6-plugin-check "qt6-plugin-check is installed"
require_command redbear-info "redbear-info is installed"

echo
for marker in \
    /home/root/.wayland-session.started \
    /home/root/.qt6-bootstrap-minimal.ok \
    /home/root/.qt6-plugin-minimal.ok \
    /home/root/.qt6-wayland-smoke-minimal.ok \
    /home/root/.qt6-wayland-smoke-offscreen.ok \
    /home/root/.qt6-wayland-smoke-wayland.ok \
    /home/root/.qt6-wayland-smoke.ok
do
    if [[ -f "$marker" ]]; then
        echo "✅ Marker present: $marker"
    else
        echo "❌ Marker missing: $marker"
        exit 1
    fi
done

echo
echo "=== redbear-info --json ==="
redbear-info --json | tee /tmp/redbear-phase4-info.json
if ! grep -q 'virtio_net_present' /tmp/redbear-phase4-info.json; then
    echo "❌ redbear-info --json did not report virtio_net_present"
    exit 1
fi
echo

echo "=== Phase 4 launch surface ==="
echo "orbital-wayland, smallvil, and the Qt6 Phase 4 smoke helpers are present on the wayland profile."
echo "Run this script inside the guest, or use redbear-phase4-wayland-check as the canonical validator."
echo
echo "=== Test Complete ==="
