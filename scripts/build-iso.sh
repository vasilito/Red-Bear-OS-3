#!/usr/bin/env bash

# Build Red Bear OS live ISO
# Usage: ./scripts/build-iso.sh [CONFIG_NAME] [ARCH]
#   CONFIG_NAME  - build config (default: redbear-full)
#   ARCH         - target architecture (default: x86_64)

set -euo pipefail

CONFIG_NAME="${1:-redbear-full}"
ARCH="${2:-x86_64}"

echo "Building Red Bear OS ISO"
echo "  config: ${CONFIG_NAME}"
echo "  arch:   ${ARCH}"

make live CONFIG_NAME="${CONFIG_NAME}" ARCH="${ARCH}"

echo ""
echo "Done: redbear-live.iso"
