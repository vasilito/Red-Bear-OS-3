#!/usr/bin/env bash
# Validate the bounded Intel Wi-Fi runtime path on a real Red Bear OS target.

set -euo pipefail

PROFILE="${1:-wifi-open-bounded}"
IFACE="${2:-wlan0}"

usage() {
  cat <<'EOF'
Usage: test-wifi-baremetal-runtime.sh [PROFILE] [INTERFACE]

Run the strongest in-OS validation currently available for the bounded Intel Wi-Fi path.

Defaults:
  PROFILE   wifi-open-bounded
  INTERFACE wlan0

This script validates runtime surfaces and bounded lifecycle behavior on a real Red Bear OS target.
It does NOT prove real AP association, packet flow, or end-to-end Wi-Fi connectivity by itself.
EOF
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

echo "=== Red Bear OS Wi-Fi Bare-Metal Runtime Check ==="
echo "profile=$PROFILE"
echo "interface=$IFACE"

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "FAIL: missing command $1"
    exit 1
  fi
}

need_cmd redbear-iwlwifi
need_cmd redbear-wifictl
need_cmd redbear-netctl
need_cmd redbear-info
need_cmd redbear-phase5-wifi-capture
need_cmd redbear-phase5-wifi-check
need_cmd redbear-phase5-wifi-run

echo "--- packaged wifi check ---"
redbear-phase5-wifi-run "$PROFILE" "$IFACE" "/tmp/redbear-phase5-wifi-capture.json"

echo "--- driver probe ---"
driver_probe=$(redbear-iwlwifi --probe)
printf '%s\n' "$driver_probe"
case "$driver_probe" in
  *"candidates="*) ;;
  *)
    echo "FAIL: redbear-iwlwifi --probe did not report candidates"
    exit 1
    ;;
esac

echo "--- control probe ---"
wifictl_probe=$(redbear-wifictl --probe)
printf '%s\n' "$wifictl_probe"
case "$wifictl_probe" in
  *"interfaces="*"$IFACE"*) ;;
  *)
    echo "FAIL: redbear-wifictl --probe did not report interface $IFACE"
    exit 1
    ;;
esac

echo "--- bounded connect ---"
connect_output=$(redbear-wifictl --connect "$IFACE" demo open)
printf '%s\n' "$connect_output"
case "$connect_output" in
  *"status=connected"*|*"status=associated"*|*"status=associating"*) ;;
  *)
    echo "FAIL: bounded connect did not report bounded connect state"
    exit 1
    ;;
esac
case "$connect_output" in
  *"connect_result="*) ;;
  *)
    echo "FAIL: bounded connect did not report connect_result"
    exit 1
    ;;
esac

echo "--- bounded disconnect ---"
disconnect_output=$(redbear-wifictl --disconnect "$IFACE")
printf '%s\n' "$disconnect_output"
case "$disconnect_output" in
  *"status=device-detected"*|*"status=firmware-ready"*) ;;
  *)
    echo "FAIL: bounded disconnect did not return interface to post-disconnect state"
    exit 1
    ;;
esac
case "$disconnect_output" in
  *"disconnect_result="*) ;;
  *)
    echo "FAIL: bounded disconnect did not report disconnect_result"
    exit 1
    ;;
esac

echo "--- profile manager start/stop ---"
start_output=$(redbear-netctl start "$PROFILE")
printf '%s\n' "$start_output"
status_output=$(redbear-netctl status "$PROFILE")
printf '%s\n' "$status_output"
case "$status_output" in
  *"interface=$IFACE"*) ;;
  *)
    echo "FAIL: netctl status did not report interface $IFACE"
    exit 1
    ;;
esac
case "$status_output" in
  *"connect_result="*) ;;
  *)
    echo "FAIL: netctl status did not report connect_result"
    exit 1
    ;;
esac

stop_output=$(redbear-netctl stop "$PROFILE")
printf '%s\n' "$stop_output"

echo "--- runtime report ---"
info_output=$(redbear-info --json)
printf '%s\n' "$info_output"
case "$info_output" in
  *"wifi_control_state"*"wifi_connect_result"*"wifi_disconnect_result"*) ;;
  *)
    echo "FAIL: redbear-info --json did not include Wi-Fi lifecycle reporting fields"
    exit 1
    ;;
esac

capture_path="/tmp/redbear-phase5-wifi-capture.json"

echo "PASS: bounded Intel Wi-Fi runtime path exercised on target"
echo "CAPTURE: $capture_path"
echo "NOTE: this still does not prove real AP scan/auth/association, packet flow, DHCP success over Wi-Fi, or validated end-to-end connectivity"
