#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ISO_PATH="${1:-$ROOT_DIR/build/x86_64/redbear-live-mini.iso}"
LOG_PATH="${2:-/tmp/redbear-live-mini-uefi.log}"
TIMEOUT_SECS="${TIMEOUT_SECS:-120}"

if [[ ! -f "$ISO_PATH" ]]; then
  echo "error: ISO not found: $ISO_PATH" >&2
  exit 1
fi

CODE_FD="/usr/share/edk2/x64/OVMF_CODE.4m.fd"
VARS_SRC="/usr/share/edk2/x64/OVMF_VARS.4m.fd"
VARS_FD="/tmp/rb-ovmf-vars-live-mini.fd"

if [[ ! -f "$CODE_FD" || ! -f "$VARS_SRC" ]]; then
  echo "error: OVMF files not found under /usr/share/edk2/x64" >&2
  exit 1
fi

cp -f "$VARS_SRC" "$VARS_FD"

ACCEL="tcg"
if [[ -r /dev/kvm && -w /dev/kvm ]]; then
  ACCEL="kvm:tcg"
fi

echo "ISO:    $ISO_PATH"
echo "LOG:    $LOG_PATH"
echo "ACCEL:  $ACCEL"
echo "TIMEOUT:${TIMEOUT_SECS}s"

timeout "${TIMEOUT_SECS}s" qemu-system-x86_64 \
  -machine "q35,accel=${ACCEL}" \
  -cpu max -smp 4 -m 4096 \
  -nographic -serial mon:stdio \
  -drive "if=pflash,format=raw,readonly=on,file=${CODE_FD}" \
  -drive "if=pflash,format=raw,file=${VARS_FD}" \
  -cdrom "$ISO_PATH" \
  >"$LOG_PATH" 2>&1 || true

echo "---- markers ----"
grep -nE "RedBear OS starting|switchroot to /scheme/initfs|switchroot to /usr|pcid-spawner: matched 0000:00:01.0|panic|UNHANDLED EXCEPTION|emergency shell" "$LOG_PATH" | sed -n '1,200p' || true

echo "---- tail ----"
tail -n 80 "$LOG_PATH" | sed -e 's/\x1b\[[0-9;]*m//g'

