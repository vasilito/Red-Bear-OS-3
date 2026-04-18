#!/usr/bin/env bash

# Build Red Bear OS live ISO
# Usage: ./scripts/build-iso.sh [--upstream] [CONFIG_NAME] [ARCH]
#   CONFIG_NAME  - build config (default: redbear-live)
#   ARCH         - target architecture (default: x86_64)

set -euo pipefail

CONFIG_NAME="redbear-live"
ARCH="x86_64"
ALLOW_UPSTREAM=0

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS] [CONFIG_NAME] [ARCH]

Build a Red Bear OS live ISO.

Options:
  --upstream          Allow Redox/upstream recipe source refresh during build
  -h, --help          Show this help

Defaults:
  CONFIG_NAME=redbear-live
  ARCH=x86_64
EOF
}

POSITIONAL=()
while [ $# -gt 0 ]; do
    case "$1" in
        --upstream)
            ALLOW_UPSTREAM=1
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        --)
            shift
            POSITIONAL+=("$@")
            break
            ;;
        -*)
            echo "Unknown option: $1" >&2
            usage >&2
            exit 1
            ;;
        *)
            POSITIONAL+=("$1")
            ;;
    esac
    shift
done

if [ ${#POSITIONAL[@]} -gt 2 ]; then
    echo "ERROR: Too many positional arguments" >&2
    usage >&2
    exit 1
fi

[ ${#POSITIONAL[@]} -ge 1 ] && CONFIG_NAME="${POSITIONAL[0]}"
[ ${#POSITIONAL[@]} -ge 2 ] && ARCH="${POSITIONAL[1]}"

if [ -z "${CI:-}" ] && { [ ! -t 0 ] || [ ! -t 1 ]; }; then
    export CI=1
fi

echo "Building Red Bear OS ISO"
echo "  config: ${CONFIG_NAME}"
echo "  arch:   ${ARCH}"
if [ "$ALLOW_UPSTREAM" -eq 1 ]; then
    echo "  upstream recipe refresh: enabled"
    REPO_OFFLINE=0 COOKBOOK_OFFLINE=false make live CONFIG_NAME="${CONFIG_NAME}" ARCH="${ARCH}"
else
    echo "  upstream recipe refresh: disabled (pass --upstream to enable)"
    REPO_OFFLINE=1 COOKBOOK_OFFLINE=true make live CONFIG_NAME="${CONFIG_NAME}" ARCH="${ARCH}"
fi

echo ""
echo "Done: build/${ARCH}/${CONFIG_NAME}.iso"
