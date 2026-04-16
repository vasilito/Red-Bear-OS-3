#!/usr/bin/env bash
# Package Wi-Fi validation artifacts into a single tarball.

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: package-wifi-validation-artifacts.sh [OUTPUT_TARBALL] [FILES...]

Default output tarball:
  ./wifi-validation-artifacts.tar.gz

If no FILES are provided, this script packages the common host-side artifact names referenced by the
Wi-Fi validation runbook when they exist in the current directory.

If a provided FILE argument is a directory, it is included recursively.
EOF
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

output="${1:-wifi-validation-artifacts.tar.gz}"
shift || true

inputs=()
if [[ $# -gt 0 ]]; then
  for path in "$@"; do
    if [[ -e "$path" ]]; then
      inputs+=("$path")
    else
      echo "WARN: skipping missing artifact $path" >&2
    fi
  done
else
  defaults=(
    "wifi-passthrough-capture.json"
    "wifi-passthrough-capture.json.meta.json"
    "wifi-baremetal-capture.json"
    "wifi-baremetal-serial.log"
    "wifi-baremetal-console.log"
  )
  for path in "${defaults[@]}"; do
    if [[ -e "$path" ]]; then
      inputs+=("$path")
    fi
  done
fi

if [[ ${#inputs[@]} -eq 0 ]]; then
  echo "ERROR: no Wi-Fi validation artifacts found to package" >&2
  exit 1
fi

manifest_dir=$(mktemp -d)
manifest_path="$manifest_dir/wifi-validation-artifacts.manifest.txt"
{
  echo "output=$output"
  for path in "${inputs[@]}"; do
    if command -v sha256sum >/dev/null 2>&1 && [[ -f "$path" ]]; then
      checksum=$(sha256sum "$path" | awk '{print $1}')
      printf 'file=%s sha256=%s\n' "$path" "$checksum"
    else
      printf 'path=%s\n' "$path"
    fi
  done
} > "$manifest_path"

tar -czf "$output" "${inputs[@]}" -C "$manifest_dir" "$(basename "$manifest_path")"
rm -rf "$manifest_dir"

echo "packaged_artifacts=$output"
printf 'included=%s\n' "${inputs[*]}"
echo "manifest=wifi-validation-artifacts.manifest.txt"
