#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
JOBS="${JOBS:-$(nproc)}"
ALLOW_UPSTREAM="${ALLOW_UPSTREAM:-0}"
BUILD_LOG_DIR="${PROJECT_ROOT}/build/logs"

targets=(redbear-full redbear-mini redbear-grub)

mkdir -p "$BUILD_LOG_DIR"

echo "========================================"
echo "   Red Bear OS — Build All ISOs"
echo "========================================"
echo "Targets: ${targets[*]}"
echo "Jobs:    $JOBS"
echo "Upstream refresh: $ALLOW_UPSTREAM"
echo "Log dir: $BUILD_LOG_DIR"
echo "========================================"
echo ""

cd "$PROJECT_ROOT"

# Ensure .config has PODMAN_BUILD=0
if ! grep -q '^PODMAN_BUILD?=0' .config 2>/dev/null; then
    echo "PODMAN_BUILD?=0" > .config
    echo ">>> Set PODMAN_BUILD=0 in .config"
fi

# Build or ensure cookbook binary exists
if [ ! -f "target/release/repo" ]; then
    echo ">>> Building cookbook binary..."
    cargo build --release 2>&1 | tee "$BUILD_LOG_DIR/cookbook-build.log"
fi

# Determine offline flags
if [ "$ALLOW_UPSTREAM" -eq 1 ]; then
    OFFLINE_FLAGS="REPO_OFFLINE=0 COOKBOOK_OFFLINE=false"
else
    OFFLINE_FLAGS="REPO_OFFLINE=1 COOKBOOK_OFFLINE=true"
fi

failed=()

for target in "${targets[@]}"; do
    logfile="$BUILD_LOG_DIR/${target}-$(date +%Y%m%d-%H%M%S).log"
    echo ""
    echo "========================================"
    echo "  BUILDING: $target"
    echo "  Log: $logfile"
    echo "========================================"

    # Run clean + live build for this target
    if $OFFLINE_FLAGS CI=1 make clean live "CONFIG_NAME=$target" "JOBS=$JOBS" 2>&1 | tee "$logfile"; then
        echo ""
        echo "  OK: $target built successfully"
    else
        echo ""
        echo "  FAILED: $target build failed"
        failed+=("$target")
    fi
	done

echo ""
echo "========================================"
echo "   Build Summary"
echo "========================================"

for target in "${targets[@]}"; do
    iso="build/x86_64/${target}.iso"
    if [ -f "$iso" ]; then
        size=$(du -h "$iso" | cut -f1)
        echo "  OK   $target  ($size)  →  $iso"
    else
        echo "  MISSING $target ISO"
        failed+=("$target")
    fi
done

if [ ${#failed[@]} -gt 0 ]; then
    echo ""
    echo "FAILED targets: ${failed[*]}"
    echo "Check logs in: $BUILD_LOG_DIR"
    exit 1
fi

echo ""
echo "All ISOs built successfully."
echo "Logs: $BUILD_LOG_DIR"
