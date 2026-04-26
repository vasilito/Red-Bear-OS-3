#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

ALLOW_UPSTREAM=0

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Build ALL Red Bear OS live ISOs for real bare metal.

Targets built in order:
  1. redbear-full   — Full desktop ISO (Wayland + KDE + GPU drivers)
  2. redbear-mini   — Text-only ISO
  3. redbear-grub   — Text-only ISO with GRUB boot manager

Options:
  --upstream          Allow Redox/upstream recipe source refresh during build
  -h, --help          Show this help

Environment:
  CI=1                Force non-interactive mode (no TUI)
  MAKEFLAGS           Passed through to make
EOF
    exit 0
}

while [ $# -gt 0 ]; do
    case "$1" in
        --upstream)
            ALLOW_UPSTREAM=1
            ;;
        -h|--help)
            usage
            ;;
        -*)
            echo "Unknown option: $1" >&2
            usage >&2
            exit 1
            ;;
        *)
            echo "Unknown argument: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
    shift
done

cd "$PROJECT_ROOT"

# Auto-disable TUI when stdout is not a terminal (prevents repo cook panic)
if [ -z "${CI:-}" ] && { [ ! -t 0 ] || [ ! -t 1 ]; }; then
    export CI=1
fi

TARGETS=("redbear-full" "redbear-mini" "redbear-grub")
ARCH="x86_64"
FAILED=()

for CONFIG_NAME in "${TARGETS[@]}"; do
    echo ""
    echo "======================================================================"
    echo "  Building ISO: $CONFIG_NAME"
    echo "======================================================================"
    echo ""

    if [ "$ALLOW_UPSTREAM" -eq 1 ]; then
        if ! bash "$SCRIPT_DIR/build-iso.sh" --upstream "$CONFIG_NAME" "$ARCH"; then
            FAILED+=("$CONFIG_NAME")
            echo ""
            echo "WARNING: Build failed for $CONFIG_NAME — continuing with next target..."
            echo ""
        fi
    else
        if ! bash "$SCRIPT_DIR/build-iso.sh" "$CONFIG_NAME" "$ARCH"; then
            FAILED+=("$CONFIG_NAME")
            echo ""
            echo "WARNING: Build failed for $CONFIG_NAME — continuing with next target..."
            echo ""
        fi
    fi
done

echo ""
echo "======================================================================"
echo "  Build Summary"
echo "======================================================================"
echo ""

for CONFIG_NAME in "${TARGETS[@]}"; do
    ISO_PATH="build/$ARCH/$CONFIG_NAME.iso"
    if [ -f "$ISO_PATH" ]; then
        echo "  [OK]   $CONFIG_NAME  →  $ISO_PATH"
    else
        echo "  [MISSING] $CONFIG_NAME"
    fi
done

if [ ${#FAILED[@]} -gt 0 ]; then
    echo ""
    echo "FAILED targets: ${FAILED[*]}"
    exit 1
fi

echo ""
echo "All ISOs built successfully."
