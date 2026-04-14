#!/usr/bin/env bash
#
# test-iommu-qemu.sh - Launch QEMU with AMD IOMMU device for hardware testing
#
# This wrapper adds the AMD IOMMU device to QEMU for testing IOMMU/vIOMMU
# functionality on AMD hardware. It forwards any additional QEMU flags to the
# make qemu invocation.
#
# Usage:
#   ./local/scripts/test-iommu-qemu.sh [--help]
#   ./local/scripts/test-iommu-qemu.sh [extra QEMU flags...]
#
# Examples:
#   ./local/scripts/test-iommu-qemu.sh                    # Basic IOMMU test
#   ./local/scripts/test-iommu-qemu.sh -display sdl        # With SDL display
#   ./local/scripts/test-iommu-qemu.sh -m 4G               # With 4GB RAM

set -e

# Print usage information
usage() {
    cat << USAGE
Usage: $(basename "$0") [options]

Launch QEMU with AMD IOMMU device for hardware testing.

Options:
  --help      Show this help message

Any additional arguments are passed as extra QEMU flags.

Environment:
  QEMUFLAGS   Additional flags (prepended to device amd-iommu)

Examples:
  $(basename "$0")
  $(basename "$0") -display sdl -m 4G
  QEMUFLAGS="-smp 8" $(basename "$0")

USAGE
    exit 0
}

# Parse --help before anything else
for arg in "$@"; do
    case "$arg" in
        --help|-h|help)
            usage
            ;;
    esac
done

# Trap to handle Ctrl+C gracefully
# Kill any background QEMU process if interrupted
cleanup() {
    echo "Interrupted, cleaning up..."
    # The make qemu process will be killed by the signal
    exit 130
}
trap cleanup SIGINT SIGTERM

# Build QEMUFLAGS with AMD IOMMU device
# Prepend user QEMUFLAGS if set, then add the amd-iommu device
IOMMU_FLAGS="-device amd-iommu"
if [[ -n "${QEMUFLAGS:-}" ]]; then
    QEMUFLAGS="${QEMUFLAGS} ${IOMMU_FLAGS} $@"
else
    QEMUFLAGS="${IOMMU_FLAGS} $@"
fi

# Launch QEMU via make
exec make qemu QEMUFLAGS="$QEMUFLAGS"
