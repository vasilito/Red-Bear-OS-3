#!/usr/bin/env bash

# Build Red Bear OS live ISO
# Usage: ./scripts/build-iso.sh [CONFIG_NAME] [ARCH]
#   CONFIG_NAME  - build config (default: redbear-live)
#   ARCH         - target architecture (default: x86_64)

set -euo pipefail

CONFIG_NAME="${1:-redbear-live}"
ARCH="${2:-x86_64}"

if [ -z "${CI:-}" ] && { [ ! -t 0 ] || [ ! -t 1 ]; }; then
    export CI=1
fi

echo "Building Red Bear OS ISO"
echo "  config: ${CONFIG_NAME}"
echo "  arch:   ${ARCH}"

make live CONFIG_NAME="${CONFIG_NAME}" ARCH="${ARCH}"

echo ""
echo "Done: build/${ARCH}/${CONFIG_NAME}.iso"
