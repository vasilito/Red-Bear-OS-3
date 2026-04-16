#!/usr/bin/env bash
# Validate host prerequisites for Intel Wi-Fi VFIO passthrough testing.

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: validate-wifi-vfio-host.sh --host-pci 0000:xx:yy.z [--expect-driver DRIVER]

Check whether the current host appears ready to run the Red Bear Intel Wi-Fi VFIO passthrough
validation path.

Options:
  --host-pci BDF         Host PCI address of the Intel Wi-Fi device to validate (required)
  --expect-driver NAME   Host driver expected before VFIO rebind (optional)
  -h, --help             Show this help text

This command does not modify the host. It only reports readiness signals and common blockers.
EOF
}

host_pci=""
expect_driver=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --host-pci)
      host_pci="$2"
      shift 2
      ;;
    --expect-driver)
      expect_driver="$2"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "ERROR: unknown argument $1" >&2
      usage
      exit 1
      ;;
  esac
done

if [[ -z "$host_pci" ]]; then
  echo "ERROR: --host-pci is required" >&2
  usage
  exit 1
fi

script_root="$(dirname "$0")"
repo_root="$(cd "$script_root/../.." && pwd)"
arch="${ARCH:-$(uname -m)}"
image="$repo_root/build/$arch/redbear-full/harddrive.img"

find_uefi_firmware() {
  local candidates=(
    "/usr/share/ovmf/x64/OVMF.4m.fd"
    "/usr/share/OVMF/x64/OVMF.4m.fd"
    "/usr/share/ovmf/x64/OVMF_CODE.4m.fd"
    "/usr/share/OVMF/x64/OVMF_CODE.4m.fd"
    "/usr/share/ovmf/OVMF.fd"
    "/usr/share/OVMF/OVMF_CODE.fd"
    "/usr/share/qemu/edk2-x86_64-code.fd"
  )
  local path
  for path in "${candidates[@]}"; do
    if [[ -f "$path" ]]; then
      printf '%s\n' "$path"
      return 0
    fi
  done
  return 1
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

read_pci_id() {
  local bdf="$1"
  local vendor device
  vendor=$(<"/sys/bus/pci/devices/$bdf/vendor")
  device=$(<"/sys/bus/pci/devices/$bdf/device")
  printf '%s %s\n' "$vendor" "$device"
}

status=0

echo "=== Red Bear Wi-Fi VFIO Host Validation ==="
echo "host_pci=$host_pci"

if [[ ! -e "/sys/bus/pci/devices/$host_pci" ]]; then
  echo "FAIL: PCI device $host_pci not found in sysfs"
  exit 1
fi

read -r vendor device < <(read_pci_id "$host_pci")
driver=$(current_driver "$host_pci")
echo "vendor=$vendor"
echo "device=$device"
echo "current_driver=$driver"

if [[ -n "$expect_driver" && "$driver" != "$expect_driver" && "$driver" != "vfio-pci" ]]; then
  echo "WARN: expected host driver $expect_driver but found $driver"
  status=1
fi

if ! find_uefi_firmware >/dev/null; then
  echo "FAIL: no supported x86_64 UEFI firmware found for QEMU"
  status=1
else
  echo "uefi_firmware=present"
fi

if [[ ! -f "$image" ]]; then
  echo "FAIL: missing image $image"
  status=1
else
  echo "redbear_image=present"
fi

if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
  echo "FAIL: missing qemu-system-x86_64"
  status=1
else
  echo "qemu=present"
fi

if ! command -v expect >/dev/null 2>&1; then
  echo "FAIL: missing expect"
  status=1
else
  echo "expect=present"
fi

if ! lsmod | grep -q '^vfio_pci'; then
  echo "WARN: vfio_pci module is not currently loaded"
  status=1
else
  echo "vfio_pci=loaded"
fi

if [[ -d /sys/kernel/iommu_groups && -n "$(find /sys/kernel/iommu_groups -mindepth 1 -maxdepth 1 -type d 2>/dev/null)" ]]; then
  echo "iommu_groups=present"
else
  echo "WARN: IOMMU groups not visible under /sys/kernel/iommu_groups"
  status=1
fi

if command -v lspci >/dev/null 2>&1; then
  echo "lspci_summary=$(lspci -nn -s "$host_pci" 2>/dev/null || true)"
fi

if [[ "$driver" == "vfio-pci" ]]; then
  echo "vfio_binding=already-bound"
else
  echo "vfio_binding=not-bound"
fi

if [[ "$status" -eq 0 ]]; then
  echo "PASS: host appears ready for Wi-Fi VFIO validation"
else
  echo "FAIL: host validation found one or more blockers/warnings"
fi

exit "$status"
