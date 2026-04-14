#!/usr/bin/env bash
#
# test-vm-network-qemu.sh - Launch the Red Bear OS VM networking baseline in QEMU
#
# This helper boots the selected Red Bear config with a VirtIO NIC so the
# Phase 2 minimal-system networking path can be exercised:
#   pcid-spawner -> virtio-netd -> smolnetd -> dhcpd -> netctl --boot

set -euo pipefail

usage() {
    cat <<'USAGE'
Usage: test-vm-network-qemu.sh [config] [extra qemu args...]

Launch Red Bear OS in QEMU with the VirtIO network baseline enabled.

Arguments:
  config            Optional config name (default: redbear-minimal)
  extra qemu args   Additional arguments appended to QEMUFLAGS

Examples:
  ./local/scripts/test-vm-network-qemu.sh
  ./local/scripts/test-vm-network-qemu.sh redbear-minimal -m 4G
  QEMUFLAGS="-display sdl" ./local/scripts/test-vm-network-qemu.sh redbear-desktop

In-guest validation commands:
  redbear-info --verbose
  redbear-info --json
  netctl status
  /scheme/pci/*/config via lspci
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

CONFIG="redbear-minimal"
if [[ $# -gt 0 ]] && [[ "$1" == redbear-* ]]; then
    CONFIG="$1"
    shift
fi

case "$CONFIG" in
    redbear-minimal|redbear-desktop|redbear-full|redbear-kde|redbear-live)
        ;;
    *)
        echo "ERROR: unsupported config '$CONFIG'" >&2
        exit 1
        ;;
esac

ARCH="${ARCH:-$(uname -m)}"
IMAGE="build/$ARCH/$CONFIG/harddrive.img"

if [[ ! -f "$IMAGE" ]]; then
    echo "ERROR: missing image $IMAGE" >&2
    echo "Build it first with: ./local/scripts/build-redbear.sh $CONFIG" >&2
    exit 1
fi

EXTRA_QEMU_ARGS="$*"
if [[ -n "${QEMUFLAGS:-}" ]]; then
    QEMUFLAGS="${QEMUFLAGS} ${EXTRA_QEMU_ARGS}"
else
    QEMUFLAGS="${EXTRA_QEMU_ARGS}"
fi

echo "=== Red Bear OS VM Network Baseline ==="
echo "Config: $CONFIG"
echo "Image:  $IMAGE"
echo "Net:    virtio"
echo
echo "Suggested in-guest checks:"
echo "  redbear-info --verbose"
echo "  redbear-info --json"
echo "  netctl status"
echo "  lspci"
echo

exec make qemu CONFIG_NAME="$CONFIG" net=virtio QEMUFLAGS="$QEMUFLAGS"
