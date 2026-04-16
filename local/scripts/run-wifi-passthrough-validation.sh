#!/usr/bin/env bash
# Prepare a host Intel Wi-Fi PCI function for VFIO, run the Red Bear Wi-Fi passthrough
# validation harness, and restore the host driver afterwards.

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: run-wifi-passthrough-validation.sh --host-pci 0000:xx:yy.z --host-driver DRIVER [--artifact-dir DIR] [--capture-output PATH] [--metadata-output PATH] [-- extra qemu args...]

Examples:
  sudo ./local/scripts/run-wifi-passthrough-validation.sh \
    --host-pci 0000:00:14.3 \
    --host-driver iwlwifi \
    --capture-output ./wifi-passthrough-capture.json

This wrapper:
  1. binds the selected PCI function to vfio-pci,
  2. launches test-wifi-passthrough-qemu.sh --check,
  3. restores the selected host driver on exit.

It does NOT itself prove end-to-end Wi-Fi connectivity; it automates the strongest in-repo
passthrough validation path.
EOF
}

host_pci=""
host_driver=""
capture_output=""
artifact_dir=""
extra_args=()
run_started="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
metadata_output=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --host-pci)
      host_pci="$2"
      shift 2
      ;;
    --host-driver)
      host_driver="$2"
      shift 2
      ;;
    --capture-output)
      capture_output="$2"
      shift 2
      ;;
    --artifact-dir)
      artifact_dir="$2"
      shift 2
      ;;
    --metadata-output)
      metadata_output="$2"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    --)
      shift
      extra_args+=("$@")
      break
      ;;
    *)
      extra_args+=("$1")
      shift
      ;;
  esac
done

if [[ -z "$host_pci" || -z "$host_driver" ]]; then
  echo "ERROR: --host-pci and --host-driver are required" >&2
  usage
  exit 1
fi

script_root="$(dirname "$0")"
prepare_script="$script_root/prepare-wifi-vfio.sh"
passthrough_script="$script_root/test-wifi-passthrough-qemu.sh"

cleanup() {
  if [[ -n "$host_pci" && -n "$host_driver" ]]; then
    echo "=== Restoring host driver ==="
    "$prepare_script" unbind "$host_pci" "$host_driver" || true
  fi
}
trap cleanup EXIT

echo "=== Binding Intel Wi-Fi function to vfio-pci ==="
"$prepare_script" bind "$host_pci"

echo "=== Running Wi-Fi passthrough validation ==="
if [[ -n "$artifact_dir" ]]; then
  mkdir -p "$artifact_dir"
  if [[ -z "$capture_output" ]]; then
    capture_output="$artifact_dir/wifi-passthrough-capture.json"
  fi
  if [[ -z "$metadata_output" ]]; then
    metadata_output="$artifact_dir/wifi-passthrough-capture.meta.json"
  fi
fi
if [[ -z "$capture_output" ]]; then
  capture_output="$(pwd)/wifi-passthrough-capture.json"
fi
cmd=("$passthrough_script" --host-pci "$host_pci" --check)
cmd+=(--capture-output "$capture_output")
cmd+=("${extra_args[@]}")
"${cmd[@]}"
run_finished="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

if [[ -z "$metadata_output" ]]; then
  metadata_output="${capture_output}.meta.json"
fi

if [[ -n "$metadata_output" ]]; then
  python - <<'PY' "$metadata_output" "$host_pci" "$host_driver" "$capture_output" "$run_started" "$run_finished"
import json, sys
path, host_pci, host_driver, capture_output, run_started, run_finished = sys.argv[1:7]
payload = {
    "host_pci": host_pci,
    "host_driver": host_driver,
    "capture_output": capture_output or None,
    "run_started": run_started,
    "run_finished": run_finished,
}
with open(path, "w", encoding="utf-8") as f:
    json.dump(payload, f, indent=2, sort_keys=True)
    f.write("\n")
PY
fi

echo "=== Validation complete ==="
if [[ -n "$capture_output" ]]; then
  echo "capture_output=$capture_output"
fi
if [[ -n "$metadata_output" ]]; then
  echo "metadata_output=$metadata_output"
fi
