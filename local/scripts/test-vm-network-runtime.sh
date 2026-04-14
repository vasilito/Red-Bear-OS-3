#!/usr/bin/env bash
# Test the Red Bear OS VM networking baseline from inside the guest
# Run this inside Red Bear OS (or via QEMU serial console)

set -euo pipefail

echo "=== Red Bear OS VM Network Runtime Test ==="
echo

if command -v redbear-info >/dev/null 2>&1; then
    echo "=== redbear-info --verbose ==="
    redbear-info --verbose || true
    echo
else
    echo "❌ redbear-info not found"
    exit 1
fi

if [ -e "/scheme/pci" ]; then
    echo "✅ /scheme/pci registered"
else
    echo "❌ /scheme/pci not found — pcid-spawner path is not live"
    exit 1
fi

if [ -e "/scheme/netcfg" ]; then
    echo "✅ /scheme/netcfg registered"
else
    echo "❌ /scheme/netcfg not found — smolnetd path is not live"
    exit 1
fi

if [ -e "/etc/netctl/active" ]; then
    ACTIVE_PROFILE="$(tr -d '\r\n' < /etc/netctl/active)"
    echo "✅ active netctl profile: ${ACTIVE_PROFILE:-<empty>}"
else
    echo "❌ /etc/netctl/active not found"
    exit 1
fi

echo
echo "=== netctl status ==="
if command -v netctl >/dev/null 2>&1; then
    netctl status || true
else
    echo "❌ netctl not found"
    exit 1
fi

echo
echo "=== network schemes ==="
ls /scheme 2>/dev/null | grep '^network\.' || echo "(no network.* schemes visible)"

echo
echo "=== netcfg address ==="
if [ -r "/scheme/netcfg/ifaces/eth0/addr/list" ]; then
    cat /scheme/netcfg/ifaces/eth0/addr/list
else
    echo "❌ /scheme/netcfg/ifaces/eth0/addr/list not readable"
    exit 1
fi

echo
echo "=== Test Complete ==="
