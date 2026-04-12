#!/usr/bin/env bash
# Test AMD GPU driver on Red Bear OS
# Run this inside RBOS (or via QEMU serial console)
set -euo pipefail

echo "=== AMD GPU Driver Test ==="
echo ""

# Check if scheme:drm exists
if [ -e "/scheme/drm" ]; then
    echo "✅ scheme:drm registered"
else
    echo "❌ scheme:drm NOT found — redox-drm daemon not running?"
    exit 1
fi

# Check card0
if [ -e "/scheme/drm/card0" ]; then
    echo "✅ /scheme/drm/card0 exists"
else
    echo "❌ /scheme/drm/card0 NOT found — AMD GPU not detected?"
    exit 1
fi

# Try to read connector info
echo ""
echo "=== Connector Info ==="
if command -v modetest &>/dev/null; then
    modetest -M amd 2>&1 | head -50
else
    echo "modetest not available — reading raw scheme"
    # Read from scheme directly
    cat /scheme/drm/card0 2>&1 | head -20 || true
fi

echo ""
echo "=== PCI Devices (GPU) ==="
ls /scheme/pci/ 2>/dev/null | while read -r entry; do
    echo "  $entry"
done

echo ""
echo "=== Test Complete ==="
