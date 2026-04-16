#!/usr/bin/env bash
set -euo pipefail

echo "[wifi-runtime] checking wifictl scheme"
if [ ! -d /scheme/wifictl ]; then
  echo "[wifi-runtime] FAIL: /scheme/wifictl is missing"
  exit 1
fi

echo "[wifi-runtime] checking bounded Intel driver package"
if [ ! -x /usr/lib/drivers/redbear-iwlwifi ]; then
  echo "[wifi-runtime] FAIL: /usr/lib/drivers/redbear-iwlwifi is missing"
  exit 1
fi

echo "[wifi-runtime] probing Intel Wi-Fi candidates"
driver_probe=$(/usr/lib/drivers/redbear-iwlwifi --probe 2>/dev/null)
printf '%s\n' "$driver_probe"
case "$driver_probe" in
  *"candidates="*)
    ;;
  *)
    echo "[wifi-runtime] FAIL: redbear-iwlwifi --probe did not report candidates=..."
    exit 1
    ;;
esac

echo "[wifi-runtime] checking backend selection behavior"
probe_output=$(redbear-wifictl --probe 2>/dev/null || true)
printf '%s\n' "$probe_output"
case "$driver_probe" in
  *"candidates=0"*)
    case "$probe_output" in
      *"backend=no-device"*)
        echo "[wifi-runtime] NOTE: no Intel Wi-Fi candidates detected; no-device backend fallback is expected"
        ;;
      *)
        echo "[wifi-runtime] FAIL: expected no-device fallback when no Intel Wi-Fi candidates are detected"
        exit 1
        ;;
    esac
    ;;
  *)
    case "$probe_output" in
      *"backend=intel"*)
        ;;
      *)
        echo "[wifi-runtime] FAIL: redbear-wifictl --probe did not report backend=intel when Intel Wi-Fi candidates are present"
        exit 1
        ;;
    esac
    ;;
esac

echo "[wifi-runtime] checking wifictl interface surface"
if [ ! -d /scheme/wifictl/ifaces ]; then
  echo "[wifi-runtime] FAIL: /scheme/wifictl/ifaces is missing"
  exit 1
fi

echo "[wifi-runtime] checking firmware base path"
if [ ! -d /lib/firmware ]; then
  echo "[wifi-runtime] FAIL: /lib/firmware is missing"
  exit 1
fi

echo "[wifi-runtime] listing interfaces"
ls /scheme/wifictl/ifaces || true

first_iface=$(ls /scheme/wifictl/ifaces 2>/dev/null | head -n 1 || true)
if [ -n "$first_iface" ]; then
  echo "[wifi-runtime] checking firmware-status for $first_iface"
  if [ ! -f "/scheme/wifictl/ifaces/$first_iface/firmware-status" ]; then
    echo "[wifi-runtime] FAIL: missing /scheme/wifictl/ifaces/$first_iface/firmware-status"
    exit 1
  fi
  cat "/scheme/wifictl/ifaces/$first_iface/firmware-status" || true

  echo "[wifi-runtime] checking transport-status for $first_iface"
  if [ ! -f "/scheme/wifictl/ifaces/$first_iface/transport-status" ]; then
    echo "[wifi-runtime] FAIL: missing /scheme/wifictl/ifaces/$first_iface/transport-status"
    exit 1
  fi
  cat "/scheme/wifictl/ifaces/$first_iface/transport-status" || true

  echo "[wifi-runtime] checking prepare control node for $first_iface"
  if [ ! -f "/scheme/wifictl/ifaces/$first_iface/prepare" ]; then
    echo "[wifi-runtime] FAIL: missing /scheme/wifictl/ifaces/$first_iface/prepare"
    exit 1
  fi

  echo "[wifi-runtime] checking scan control and scan-results for $first_iface"
  if [ ! -f "/scheme/wifictl/ifaces/$first_iface/scan" ]; then
    echo "[wifi-runtime] FAIL: missing /scheme/wifictl/ifaces/$first_iface/scan"
    exit 1
  fi
  if [ ! -f "/scheme/wifictl/ifaces/$first_iface/scan-results" ]; then
    echo "[wifi-runtime] FAIL: missing /scheme/wifictl/ifaces/$first_iface/scan-results"
    exit 1
  fi

  echo "[wifi-runtime] checking transport-probe control node for $first_iface"
  if [ ! -f "/scheme/wifictl/ifaces/$first_iface/transport-probe" ]; then
    echo "[wifi-runtime] FAIL: missing /scheme/wifictl/ifaces/$first_iface/transport-probe"
    exit 1
  fi

  echo "[wifi-runtime] checking transport-init nodes for $first_iface"
  if [ ! -f "/scheme/wifictl/ifaces/$first_iface/init-transport" ]; then
    echo "[wifi-runtime] FAIL: missing /scheme/wifictl/ifaces/$first_iface/init-transport"
    exit 1
  fi
  if [ ! -f "/scheme/wifictl/ifaces/$first_iface/transport-init-status" ]; then
    echo "[wifi-runtime] FAIL: missing /scheme/wifictl/ifaces/$first_iface/transport-init-status"
    exit 1
  fi
  cat "/scheme/wifictl/ifaces/$first_iface/transport-init-status" || true

  echo "[wifi-runtime] checking activate-nic and activation-status for $first_iface"
  if [ ! -f "/scheme/wifictl/ifaces/$first_iface/activate-nic" ]; then
    echo "[wifi-runtime] FAIL: missing /scheme/wifictl/ifaces/$first_iface/activate-nic"
    exit 1
  fi
  if [ ! -f "/scheme/wifictl/ifaces/$first_iface/activation-status" ]; then
    echo "[wifi-runtime] FAIL: missing /scheme/wifictl/ifaces/$first_iface/activation-status"
    exit 1
  fi
  cat "/scheme/wifictl/ifaces/$first_iface/activation-status" || true

  echo "[wifi-runtime] checking connect control and profile fields for $first_iface"
  if [ ! -f "/scheme/wifictl/ifaces/$first_iface/connect" ]; then
    echo "[wifi-runtime] FAIL: missing /scheme/wifictl/ifaces/$first_iface/connect"
    exit 1
  fi
  if [ ! -f "/scheme/wifictl/ifaces/$first_iface/ssid" ]; then
    echo "[wifi-runtime] FAIL: missing /scheme/wifictl/ifaces/$first_iface/ssid"
    exit 1
  fi
  if [ ! -f "/scheme/wifictl/ifaces/$first_iface/security" ]; then
    echo "[wifi-runtime] FAIL: missing /scheme/wifictl/ifaces/$first_iface/security"
    exit 1
  fi
  if [ ! -f "/scheme/wifictl/ifaces/$first_iface/key" ]; then
    echo "[wifi-runtime] FAIL: missing /scheme/wifictl/ifaces/$first_iface/key"
    exit 1
  fi
  if [ ! -f "/scheme/wifictl/ifaces/$first_iface/connect-result" ]; then
    echo "[wifi-runtime] FAIL: missing /scheme/wifictl/ifaces/$first_iface/connect-result"
    exit 1
  fi
  if [ ! -f "/scheme/wifictl/ifaces/$first_iface/disconnect-result" ]; then
    echo "[wifi-runtime] FAIL: missing /scheme/wifictl/ifaces/$first_iface/disconnect-result"
    exit 1
  fi

  echo "[wifi-runtime] exercising bounded connect for $first_iface"
  printf 'demo-open\n' > "/scheme/wifictl/ifaces/$first_iface/ssid"
  printf 'open\n' > "/scheme/wifictl/ifaces/$first_iface/security"
  printf '\n' > "/scheme/wifictl/ifaces/$first_iface/key"
  printf '1\n' > "/scheme/wifictl/ifaces/$first_iface/connect"

  connect_status=$(cat "/scheme/wifictl/ifaces/$first_iface/status" 2>/dev/null || true)
  connect_result=$(cat "/scheme/wifictl/ifaces/$first_iface/connect-result" 2>/dev/null || true)
  printf '%s\n' "$connect_status"
  printf '%s\n' "$connect_result"
  case "$connect_status" in
    *"connected"*|*"associated"*)
      ;;
    *)
      echo "[wifi-runtime] FAIL: bounded connect did not update status to a connected/associated state"
      exit 1
      ;;
  esac
  case "$connect_result" in
    *"connect_result="*)
      ;;
    *)
      echo "[wifi-runtime] FAIL: bounded connect did not produce connect-result output"
      exit 1
      ;;
  esac

  echo "[wifi-runtime] exercising bounded disconnect for $first_iface"
  if [ ! -f "/scheme/wifictl/ifaces/$first_iface/disconnect" ]; then
    echo "[wifi-runtime] FAIL: missing /scheme/wifictl/ifaces/$first_iface/disconnect"
    exit 1
  fi
  printf '1\n' > "/scheme/wifictl/ifaces/$first_iface/disconnect"

  disconnect_status=$(cat "/scheme/wifictl/ifaces/$first_iface/status" 2>/dev/null || true)
  disconnect_result=$(cat "/scheme/wifictl/ifaces/$first_iface/disconnect-result" 2>/dev/null || true)
  printf '%s\n' "$disconnect_status"
  printf '%s\n' "$disconnect_result"
  case "$disconnect_status" in
    *"device-detected"*|*"firmware-ready"*)
      ;;
    *)
      echo "[wifi-runtime] FAIL: bounded disconnect did not return the interface to a post-disconnect state"
      exit 1
      ;;
  esac
  case "$disconnect_result" in
    *"disconnect"*)
      ;;
    *)
      echo "[wifi-runtime] FAIL: bounded disconnect did not produce disconnect result output"
      exit 1
      ;;
  esac
else
  case "$driver_probe" in
    *"candidates=0"*)
      echo "[wifi-runtime] NOTE: no wifictl interfaces are expected when no Intel Wi-Fi candidates are present"
      ;;
    *)
      echo "[wifi-runtime] FAIL: Intel Wi-Fi candidates were detected but /scheme/wifictl/ifaces is empty"
      exit 1
      ;;
  esac
fi

echo "[wifi-runtime] checking netctl Wi-Fi examples"
if [ ! -f /etc/netctl/examples/wifi-dhcp ]; then
  echo "[wifi-runtime] FAIL: missing /etc/netctl/examples/wifi-dhcp"
  exit 1
fi

if [ ! -f /etc/netctl/examples/wifi-open-bounded ]; then
  echo "[wifi-runtime] FAIL: missing /etc/netctl/examples/wifi-open-bounded"
  exit 1
fi

echo "[wifi-runtime] exercising netctl profile stop for wifi-open-bounded"
redbear-netctl stop wifi-open-bounded >/tmp/redbear-netctl-stop.out 2>/tmp/redbear-netctl-stop.err || {
  cat /tmp/redbear-netctl-stop.err || true
  echo "[wifi-runtime] FAIL: redbear-netctl stop wifi-open-bounded failed"
  exit 1
}
cat /tmp/redbear-netctl-stop.out || true

echo "[wifi-runtime] checking netcfg interface tree"
if [ ! -d /scheme/netcfg/ifaces ]; then
  echo "[wifi-runtime] FAIL: /scheme/netcfg/ifaces is missing"
  exit 1
fi

echo "[wifi-runtime] netcfg interfaces:"
cat /scheme/netcfg/ifaces || true

echo "[wifi-runtime] PASS: bounded Wi-Fi control-plane surfaces are present"
echo "[wifi-runtime] PASS: experimental runtime selects the Intel backend only when Intel Wi-Fi candidates are actually present"
echo "[wifi-runtime] NOTE: this does not prove real radio association or working Wi-Fi connectivity"
