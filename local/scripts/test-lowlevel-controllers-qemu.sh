#!/usr/bin/env bash
# Run the bounded low-level controller proof helpers in sequence.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

usage() {
    cat <<'USAGE'
Usage: test-lowlevel-controllers-qemu.sh [config]

Run the bounded low-level controller/runtime proof helpers in sequence.
Defaults to redbear-desktop.

Checks run:
  - xHCI interrupt path
  - IOMMU first-use path
  - PS/2 + serio path
  - monotonic timer path

MSI-X remains a separate proof helper because its current default target is redbear-full.
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

echo ">>> Running IOMMU first-use proof"
bash "$SCRIPT_DIR/test-iommu-qemu.sh" --check "$config"

echo ">>> Running PS/2 + serio proof"
bash "$SCRIPT_DIR/test-ps2-qemu.sh" --check "$config"

echo ">>> Running monotonic timer proof"
bash "$SCRIPT_DIR/test-timer-qemu.sh" --check "$config"

echo "All bounded low-level controller checks passed for $config"
