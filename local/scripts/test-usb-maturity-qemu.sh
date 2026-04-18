#!/usr/bin/env bash
# Run the bounded USB maturity proof helpers in sequence.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

usage() {
    cat <<'USAGE'
Usage: test-usb-maturity-qemu.sh [config]

Run the bounded USB maturity proof helpers in sequence.
Defaults to redbear-desktop.

Checks run:
  - xHCI interrupt mode
  - full USB stack proof
  - USB storage bounded readback proof
USAGE
}

for arg in "$@"; do
    case "$arg" in
        --help|-h|help)
            usage
            exit 0
            ;;
    esac
done

config="${1:-redbear-desktop}"

echo ">>> Running xHCI interrupt proof"
bash "$SCRIPT_DIR/test-xhci-irq-qemu.sh" --check "$config"

echo ">>> Running full USB stack proof"
bash "$SCRIPT_DIR/test-usb-qemu.sh" --check "$config"

echo ">>> Running USB storage readback proof"
bash "$SCRIPT_DIR/test-usb-storage-qemu.sh" "$config"

echo "All bounded USB maturity checks passed for $config"
