#!/usr/bin/env bash
# Bind or unbind an Intel Wi-Fi PCI function to vfio-pci for Red Bear validation.

set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  prepare-wifi-vfio.sh bind <PCI_BDF>
  prepare-wifi-vfio.sh unbind <PCI_BDF> <HOST_DRIVER>

Examples:
  sudo ./local/scripts/prepare-wifi-vfio.sh bind 0000:00:14.3
  sudo ./local/scripts/prepare-wifi-vfio.sh unbind 0000:00:14.3 iwlwifi

Notes:
  - This helper only prepares host PCI binding for VFIO-backed validation.
  - It does NOT itself prove Wi-Fi works inside Red Bear OS.
  - Use with care on a machine where the selected device is safe to detach.
EOF
}

require_root() {
  if [[ ${EUID:-$(id -u)} -ne 0 ]]; then
    echo "ERROR: this script must run as root" >&2
    exit 1
  fi
}

require_sysfs_path() {
  local path="$1"
  if [[ ! -e "$path" ]]; then
    echo "ERROR: missing sysfs path $path" >&2
    exit 1
  fi
}

read_pci_id() {
  local bdf="$1"
  local vendor device
  vendor=$(<"/sys/bus/pci/devices/$bdf/vendor")
  device=$(<"/sys/bus/pci/devices/$bdf/device")
  printf '%s %s\n' "$vendor" "$device"
}

current_driver() {
  local bdf="$1"
  local link="/sys/bus/pci/devices/$bdf/driver"
  if [[ -L "$link" ]]; then
    basename "$(readlink -f "$link")"
  else
    printf 'none\n'
  fi
}

bind_vfio() {
  local bdf="$1"
  require_sysfs_path "/sys/bus/pci/devices/$bdf"

  local driver vendor device
  driver=$(current_driver "$bdf")
  read -r vendor device < <(read_pci_id "$bdf")

  echo "Preparing $bdf for vfio-pci"
  echo "vendor=$vendor device=$device current_driver=$driver"

  modprobe vfio-pci

  if [[ "$driver" != "none" && "$driver" != "vfio-pci" ]]; then
    echo "$bdf" > "/sys/bus/pci/devices/$bdf/driver/unbind"
  fi

  printf '%s %s\n' "$vendor" "$device" > /sys/bus/pci/drivers/vfio-pci/new_id || true
  echo "$bdf" > /sys/bus/pci/drivers/vfio-pci/bind

  echo "vfio_driver=$(current_driver "$bdf")"
}

unbind_vfio() {
  local bdf="$1"
  local host_driver="$2"
  require_sysfs_path "/sys/bus/pci/devices/$bdf"
  require_sysfs_path "/sys/bus/pci/drivers/$host_driver"

  local driver
  driver=$(current_driver "$bdf")
  echo "Restoring $bdf from $driver to $host_driver"

  if [[ "$driver" == "vfio-pci" ]]; then
    echo "$bdf" > /sys/bus/pci/drivers/vfio-pci/unbind
  elif [[ "$driver" != "none" && "$driver" != "$host_driver" ]]; then
    echo "$bdf" > "/sys/bus/pci/devices/$bdf/driver/unbind"
  fi

  echo "$bdf" > "/sys/bus/pci/drivers/$host_driver/bind"
  echo "restored_driver=$(current_driver "$bdf")"
}

main() {
  if [[ $# -lt 1 ]]; then
    usage
    exit 1
  fi

  case "$1" in
    --help|-h|help)
      usage
      exit 0
      ;;
  esac

  require_root

  case "$1" in
    bind)
      if [[ $# -ne 2 ]]; then
        usage
        exit 1
      fi
      bind_vfio "$2"
      ;;
    unbind)
      if [[ $# -ne 3 ]]; then
        usage
        exit 1
      fi
      unbind_vfio "$2" "$3"
      ;;
    *)
      usage
      exit 1
      ;;
  esac
}

main "$@"
