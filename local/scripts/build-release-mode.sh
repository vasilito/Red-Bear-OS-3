#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

RELEASE=""
CONFIG="redbear-full"
EXTRA_PACKAGES=()

usage() {
    cat <<EOF
Usage: $(basename "$0") --release=<ver> [--config=<name>] [--extra-package=<pkg> ...]
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --release=*) RELEASE="${1#*=}" ;;
        --config=*) CONFIG="${1#*=}" ;;
        --extra-package=*) EXTRA_PACKAGES+=("${1#*=}") ;;
        -h|--help) usage; exit 0 ;;
        *) echo "Unknown: $1" >&2; usage >&2; exit 1 ;;
    esac
    shift
done

if [ -z "$RELEASE" ]; then
    echo "ERROR: --release is required" >&2
    exit 1
fi

cd "$PROJECT_ROOT"

echo ">>> Release mode: $RELEASE"
bash "$SCRIPT_DIR/verify-sources-archived.sh" --release="$RELEASE"

if [ -f "$SCRIPT_DIR/ensure-release-sources.sh" ]; then
    echo ">>> Ensuring release source trees for $CONFIG..."
    args=("$SCRIPT_DIR/ensure-release-sources.sh" "--release=$RELEASE" "--config=$CONFIG")
    for package_name in "${EXTRA_PACKAGES[@]}"; do
        args+=("--extra-package=$package_name")
    done
    bash "${args[@]}"
fi
