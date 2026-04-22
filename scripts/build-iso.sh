#!/usr/bin/env bash

set -euo pipefail

CONFIG_NAME="redbear-live"
ARCH="x86_64"
ALLOW_UPSTREAM=0

canonicalize_live_config() {
    case "$1" in
        redbear-live-full)
            printf '%s\n' "redbear-live"
            ;;
        redbear-live-mini-grub)
            printf '%s\n' "redbear-grub-live-mini"
            ;;
        *)
            printf '%s\n' "$1"
            ;;
    esac
}

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS] [CONFIG_NAME] [ARCH]

Build a Red Bear OS live ISO for real bare metal.

Important:
  Live .iso outputs are for bare-metal boot/install/recovery workflows.
  They are not the virtual/QEMU target surface; use harddrive.img + make qemu for virtualization.

Options:
  --upstream          Allow Redox/upstream recipe source refresh during build
  -h, --help          Show this help

Supported live ISO targets:
  redbear-live           Full live ISO
  redbear-live-mini      Text-only mini live ISO
  redbear-grub-live-mini Text-only mini live ISO with GRUB bootloader

Legacy compatibility aliases:
  redbear-live-full
  redbear-live-mini-grub

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

CONFIG_NAME="$(canonicalize_live_config "$CONFIG_NAME")"

case "$CONFIG_NAME" in
    redbear-live|redbear-live-mini|redbear-grub-live-mini)
        ;;
    *)
        echo "ERROR: Unsupported live ISO target '$CONFIG_NAME'" >&2
        usage >&2
        exit 1
        ;;
esac

if [ -z "${CI:-}" ] && { [ ! -t 0 ] || [ ! -t 1 ]; }; then
    export CI=1
fi

echo "Building Red Bear OS ISO for real bare metal"
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
echo "Note: live .iso outputs are for real bare metal, not VM/QEMU use."
