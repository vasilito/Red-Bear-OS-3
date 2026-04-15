#!/usr/bin/env bash
# Reference host-side copy of the guest-side Phase 4 Wayland runtime check.
# The actual in-guest command installed by the profile is `redbear-phase4-wayland-check`.

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
require_command redbear-info "redbear-info is installed"

echo
echo "=== redbear-info --json ==="
redbear-info --json
echo

echo "=== Phase 4 launch surface ==="
echo "orbital-wayland and smallvil are present on the wayland profile."
echo "Run 'orbital-wayland' from a graphical VT to start the compositor path."
echo
echo "=== Test Complete ==="
