#!/usr/bin/env bash
set -euo pipefail

vendor=""
card_path="/scheme/drm/card0"
modeset_spec=""

usage() {
    cat <<'USAGE'
Usage: test-drm-display-runtime.sh --vendor amd|intel [--card /scheme/drm/card0] [--modeset CONNECTOR:MODE]

Bounded DRM/KMS display validation harness.

This proves only display-path evidence:
  - scheme:drm registration
  - DRM card reachability
  - connector/mode enumeration
  - optional bounded modeset proof when a specific CONNECTOR:MODE is supplied

This does NOT prove render command submission, fence semantics, or hardware rendering.
USAGE
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --vendor)
            vendor="${2:-}"
            shift 2
            ;;
        --card)
            card_path="${2:-}"
            shift 2
            ;;
        --modeset)
            modeset_spec="${2:-}"
            shift 2
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            echo "ERROR: unknown argument: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

if [[ "$vendor" != "amd" && "$vendor" != "intel" ]]; then
    echo "ERROR: --vendor must be amd or intel" >&2
    exit 1
fi

echo "=== Red Bear DRM Display Runtime Check ==="
echo "DRM_VENDOR=${vendor}"
echo "DRM_CARD=${card_path}"

command=(redbear-drm-display-check --vendor "$vendor" --card "$card_path")
if [[ -n "$modeset_spec" ]]; then
    command+=(--modeset "$modeset_spec")
fi

exec "${command[@]}"
