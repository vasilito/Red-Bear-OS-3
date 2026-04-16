#!/usr/bin/env bash
# Summarize packaged Wi-Fi validation artifacts for quick review.

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: summarize-wifi-validation-artifacts.sh <capture.json|artifact.tar.gz>

Print a compact summary of the packaged Wi-Fi validation evidence so the next debugging loop can
see the most important runtime signals quickly.
EOF
}

if [[ $# -ne 1 || "$1" == "--help" || "$1" == "-h" ]]; then
  usage
  [[ $# -eq 1 ]] && exit 0 || exit 1
fi

input="$1"
tmpdir=""
cleanup() {
  if [[ -n "$tmpdir" && -d "$tmpdir" ]]; then
    rm -rf "$tmpdir"
  fi
}
trap cleanup EXIT

json_path=""
if [[ "$input" == *.tar.gz ]]; then
  tmpdir=$(mktemp -d)
  tar -xzf "$input" -C "$tmpdir"
  json_path=$(find "$tmpdir" -maxdepth 2 -type f \( -name '*wifi*capture*.json' -o -name 'redbear-phase5-wifi-capture.json' \) | head -n 1 || true)
  if [[ -z "$json_path" ]]; then
    echo "ERROR: no Wi-Fi capture JSON found inside $input" >&2
    exit 1
  fi
else
  json_path="$input"
fi

python - <<'PY' "$json_path"
import json, sys
path = sys.argv[1]
with open(path, 'r', encoding='utf-8') as f:
    data = json.load(f)

def cmd_stdout(name):
    return str(data.get('commands', {}).get(name, {}).get('stdout', '')).strip()

def scheme_value(name):
    value = data.get('scheme', {}).get(name, {})
    if isinstance(value, dict):
        return value.get('value', '').strip()
    return ''

print("=== Red Bear Wi-Fi Validation Summary ===")
print(f"capture={path}")
print(f"captured_at_unix={data.get('captured_at_unix', 'unknown')}")
print(f"profile={data.get('profile', 'unknown')}")
print(f"interface={data.get('interface', 'unknown')}")

installed = data.get('installed', {})
print(f"driver_installed={installed.get('driver')}")
print(f"wifictl_installed={installed.get('wifictl')}")
print(f"netctl_installed={installed.get('netctl')}")

for key in [
    'driver_probe',
    'driver_status',
    'wifictl_probe',
    'wifictl_status',
    'netctl_status',
    'phase5_network_check',
    'phase5_wifi_check',
]:
    out = cmd_stdout(key)
    first = out.splitlines()[0] if out else ''
    print(f"{key}_first_line={first}")

for key in [
    'status',
    'link_state',
    'firmware_status',
    'transport_status',
    'transport_init_status',
    'activation_status',
    'connect_result',
    'disconnect_result',
    'last_error',
]:
    print(f"scheme_{key}={scheme_value(key)}")

scan = scheme_value('scan_results')
print(f"scheme_scan_results={scan}")
PY
