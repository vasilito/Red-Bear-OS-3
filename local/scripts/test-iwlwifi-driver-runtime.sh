#!/usr/bin/env bash
set -euo pipefail

echo "[iwlwifi-runtime] checking driver binary"
if [ ! -x /usr/lib/drivers/redbear-iwlwifi ]; then
  echo "[iwlwifi-runtime] FAIL: /usr/lib/drivers/redbear-iwlwifi is missing"
  exit 1
fi

echo "[iwlwifi-runtime] checking PCI surface"
if [ ! -d /scheme/pci ]; then
  echo "[iwlwifi-runtime] FAIL: /scheme/pci is missing"
  exit 1
fi

echo "[iwlwifi-runtime] running bounded probe"
/usr/lib/drivers/redbear-iwlwifi --probe || {
  echo "[iwlwifi-runtime] FAIL: driver probe failed"
  exit 1
}

echo "[iwlwifi-runtime] running bounded status"
/usr/lib/drivers/redbear-iwlwifi --status || {
  echo "[iwlwifi-runtime] FAIL: driver status failed"
  exit 1
}

echo "[iwlwifi-runtime] running bounded prepare"
/usr/lib/drivers/redbear-iwlwifi --prepare || {
  echo "[iwlwifi-runtime] FAIL: driver prepare failed"
  exit 1
}

echo "[iwlwifi-runtime] running bounded transport init"
/usr/lib/drivers/redbear-iwlwifi --init-transport || {
  echo "[iwlwifi-runtime] FAIL: driver init-transport failed"
  exit 1
}

echo "[iwlwifi-runtime] running bounded activate-nic"
/usr/lib/drivers/redbear-iwlwifi --activate-nic || {
  echo "[iwlwifi-runtime] FAIL: driver activate-nic failed"
  exit 1
}

echo "[iwlwifi-runtime] running bounded scan"
/usr/lib/drivers/redbear-iwlwifi --scan || {
  echo "[iwlwifi-runtime] FAIL: driver scan failed"
  exit 1
}

echo "[iwlwifi-runtime] running bounded connect"
/usr/lib/drivers/redbear-iwlwifi --connect demo wpa2-psk secret || {
  echo "[iwlwifi-runtime] FAIL: driver connect failed"
  exit 1
}

echo "[iwlwifi-runtime] running bounded disconnect"
/usr/lib/drivers/redbear-iwlwifi --disconnect || {
  echo "[iwlwifi-runtime] FAIL: driver disconnect failed"
  exit 1
}

echo "[iwlwifi-runtime] running bounded retry"
/usr/lib/drivers/redbear-iwlwifi --retry || {
  echo "[iwlwifi-runtime] FAIL: driver retry failed"
  exit 1
}

echo "[iwlwifi-runtime] PASS: bounded Intel Wi-Fi driver-side action set executed"
echo "[iwlwifi-runtime] NOTE: this still does not prove real scan, real association, or network connectivity"
