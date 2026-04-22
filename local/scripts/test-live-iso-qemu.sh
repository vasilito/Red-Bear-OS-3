#!/usr/bin/env bash
# test-live-iso-qemu.sh — bounded QEMU smoke test for Red Bear live ISOs.

set -euo pipefail

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
    cat <<'USAGE'
Usage: test-live-iso-qemu.sh [CONFIG_NAME ...]

Boot one or more Red Bear live ISO targets in QEMU/UEFI and verify that each reaches a text
`login:` prompt on the serial console.

Defaults:
  redbear-live redbear-live-mini redbear-grub-live-mini
USAGE
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" || "${1:-}" == "help" ]]; then
    usage
    exit 0
fi

configs=("$@")
if [[ ${#configs[@]} -eq 0 ]]; then
    configs=(redbear-live redbear-live-mini redbear-grub-live-mini)
fi

for i in "${!configs[@]}"; do
    configs[$i]="$(canonicalize_live_config "${configs[$i]}")"
done

for config in "${configs[@]}"; do
    case "$config" in
        redbear-live|redbear-live-mini|redbear-grub-live-mini)
            ;;
        *)
            echo "ERROR: unsupported live ISO target $config" >&2
            usage >&2
            exit 1
            ;;
    esac
done

arch="${ARCH:-$(uname -m)}"

for config in "${configs[@]}"; do
    image="build/$arch/$config.iso"
    if [[ ! -f "$image" ]]; then
        echo "ERROR: missing ISO $image" >&2
        echo "Build it first with: ./scripts/build-iso.sh $config" >&2
        exit 1
    fi
done

for config in "${configs[@]}"; do
    echo "=== Boot-testing $config ==="
    expect <<EOF
log_user 1
set timeout 420
spawn make qemu CONFIG_NAME=$config live=yes serial=yes gpu=no net=no
expect "login:"
send "root\r"
expect "assword:"
send "password\r"
expect "Type 'help' for available commands."
send "shutdown\r"
expect eof
EOF
done
