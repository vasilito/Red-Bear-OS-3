#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

CONFIG_NAME="redbear-full"
ARCH="$(uname -m)"
BUILD=0
ALLOW_UPSTREAM=0
QEMU_EXTRA_ARGS=()

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Red Bear OS — build and run in QEMU.

Options:
  -b, --build         Build full OS before running
  -c, --config NAME   Config name (default: redbear-full)
  -a, --arch ARCH     Target architecture (default: host arch)
  --upstream          Allow Redox/upstream recipe source refresh during build
  -- ARGS             Pass remaining args to make qemu (e.g. -- QEMUFLAGS="-m 8G")
  -h, --help          Show this help

Examples:
  $(basename "$0")                                    # Run existing image
  $(basename "$0") --build                            # Build + run
  $(basename "$0") --build --upstream                 # Build + run with upstream source refresh enabled
  $(basename "$0") -b -c redbear-minimal              # Build minimal + run
  $(basename "$0") -- QEMUFLAGS="-m 8G"               # Run with 8G RAM
  $(basename "$0") -b -- serial=yes                   # Build + run with serial console
  $(basename "$0") -b -- gpu=virtio kvm=no            # Build + run with virtio GPU, no KVM
EOF
    exit 0
}

while [ $# -gt 0 ]; do
    case "$1" in
        -b|--build)     BUILD=1 ;;
        -c|--config)    CONFIG_NAME="$2"; shift ;;
        -a|--arch)      ARCH="$2"; shift ;;
        --upstream)     ALLOW_UPSTREAM=1 ;;
        -h|--help)      usage ;;
        --)             shift; QEMU_EXTRA_ARGS=("$@"); break ;;
        *)              echo "Unknown option: $1"; exit 1 ;;
    esac
    shift
done

cd "$PROJECT_ROOT"

if [ "$BUILD" -eq 1 ]; then
    echo "==> Ensuring .config is set for native build..."
    if ! grep -q 'PODMAN_BUILD?=0' .config 2>/dev/null; then
        echo 'PODMAN_BUILD?=0' > .config
    fi

    echo "==> Applying Red Bear OS patches..."
    if [ -f local/scripts/apply-patches.sh ]; then
        bash local/scripts/apply-patches.sh
    fi

    echo "==> Building cookbook..."
    cargo build --release

    echo "==> Building Red Bear OS ($CONFIG_NAME, $ARCH)..."
    if [ "$ALLOW_UPSTREAM" -eq 1 ]; then
        echo "==> Upstream recipe refresh: enabled"
        REPO_OFFLINE=0 COOKBOOK_OFFLINE=false CI=1 make all "CONFIG_NAME=$CONFIG_NAME" ARCH="$ARCH"
    else
        echo "==> Upstream recipe refresh: disabled (pass --upstream to enable)"
        REPO_OFFLINE=1 COOKBOOK_OFFLINE=true CI=1 make all "CONFIG_NAME=$CONFIG_NAME" ARCH="$ARCH"
    fi
    echo "==> Build complete."
fi

BUILD_DIR="build/$ARCH/$CONFIG_NAME"
if [ ! -f "$BUILD_DIR/harddrive.img" ]; then
    echo "ERROR: $BUILD_DIR/harddrive.img not found. Run with --build first."
    exit 1
fi

echo "==> Launching Red Bear OS in QEMU ($CONFIG_NAME, $ARCH)..."
echo ""

exec make qemu "CONFIG_NAME=$CONFIG_NAME" ARCH="$ARCH" "${QEMU_EXTRA_ARGS[@]}"
