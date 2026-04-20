#!/usr/bin/env bash
# Run the bounded low-level controller proof helpers in sequence.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

usage() {
    cat <<'USAGE'
Usage: test-lowlevel-controllers-qemu.sh [config]

Run the bounded low-level controller/runtime proof helpers in sequence.
Defaults to redbear-mini (mapped by the individual helpers where needed).

Note: the IOMMU first-use proof still requires a target that actually ships `/usr/bin/iommu`, so
the wrapper automatically upgrades that single leg to `redbear-full` when invoked with
`redbear-mini`.

Checks run:
  - MSI-X path
  - xHCI interrupt path
  - IOMMU first-use path
  - PS/2 + serio path
  - monotonic timer path
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

config="${1:-redbear-mini}"
iommu_config="$config"
if [[ "$config" == "redbear-mini" ]]; then
  iommu_config="redbear-full"
fi

echo ">>> Running MSI-X proof"
bash "$SCRIPT_DIR/test-msix-qemu.sh" "$config"

echo ">>> Running xHCI interrupt proof"
bash "$SCRIPT_DIR/test-xhci-irq-qemu.sh" --check "$config"

echo ">>> Running IOMMU first-use proof"
iommu_image="build/x86_64/${iommu_config}/harddrive.img"
if [[ -f "$iommu_image" ]]; then
  bash "$SCRIPT_DIR/test-iommu-qemu.sh" --check "$iommu_config"
else
  echo "SKIP: IOMMU first-use proof skipped because $iommu_image is missing"
fi

echo ">>> Running PS/2 + serio proof"
bash "$SCRIPT_DIR/test-ps2-qemu.sh" --check "$config"

echo ">>> Running monotonic timer proof"
bash "$SCRIPT_DIR/test-timer-qemu.sh" --check "$config"

echo "All bounded low-level controller checks passed for $config (IOMMU leg used $iommu_config)"
