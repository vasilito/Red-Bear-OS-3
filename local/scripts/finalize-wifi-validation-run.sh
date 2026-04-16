#!/usr/bin/env bash
# Summarize and package Wi-Fi validation artifacts after a real run.

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: finalize-wifi-validation-run.sh <capture.json> [artifact.tar.gz] [additional files...]

Runs the packaged Wi-Fi analyzer on the supplied capture JSON and then packages the provided
artifacts into a tarball.

Defaults:
  artifact tarball: ./wifi-validation-artifacts.tar.gz
EOF
}

if [[ $# -lt 1 || "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  [[ $# -ge 1 ]] && exit 0 || exit 1
fi

capture="$1"
shift
archive="${1:-wifi-validation-artifacts.tar.gz}"
if [[ $# -gt 0 ]]; then
  shift
fi

if [[ ! -f "$capture" ]]; then
  echo "ERROR: missing capture file $capture" >&2
  exit 1
fi

echo "=== Wi-Fi Validation Analysis ==="
if command -v redbear-phase5-wifi-analyze >/dev/null 2>&1; then
  redbear-phase5-wifi-analyze "$capture"
else
  echo "WARN: redbear-phase5-wifi-analyze not installed; skipping analyzer"
fi

echo "=== Packaging Artifacts ==="
files=("$capture")
if [[ $# -gt 0 ]]; then
  for path in "$@"; do
    files+=("$path")
  done
fi
./local/scripts/package-wifi-validation-artifacts.sh "$archive" "${files[@]}"
echo "finalized_archive=$archive"
