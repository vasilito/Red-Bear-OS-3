#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
IMAGE="${1:-$REPO_ROOT/build/x86_64/redbear-mini/harddrive.img}"
[ ! -f "$IMAGE" ] && { echo "Build first: CI=1 make all CONFIG_NAME=redbear-mini"; exit 1; }
OUT=$(mktemp); trap "rm -f $OUT" EXIT
timeout 60 qemu-system-x86_64 -M q35 -m 2048 -enable-kvm -device usb-ehci,id=ehci -device usb-tablet \
  -drive file="$IMAGE",format=raw,if=none,id=disk -device ahci,id=ahci -device ide-hd,drive=disk,bus=ahci.0 \
  -serial file:"$OUT" -display none -no-reboot 2>/dev/null || true
pass=0; total=0
grep -qi "ehci\|EHCI" "$OUT" && { echo "[PASS] EHCI driver loaded"; pass=$((pass+1)); } || echo "[FAIL] EHCI not detected"
total=$((total+1))
grep -q "scheme" "$OUT" && { echo "[PASS] scheme activity"; pass=$((pass+1)); } || echo "[WARN] scheme not confirmed"
total=$((total+1))
echo "EHCI: $pass/$total checks passed"
[ $pass -eq $total ] && exit 0 || exit 1
