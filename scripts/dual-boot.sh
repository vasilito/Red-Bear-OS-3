#!/usr/bin/env bash

# This script install Red Bear OS in the free space of your storage device
# and add a boot entry (if you are using the systemd-boot boot loader)

set -e

if [ -n "$1" ]
then
    DISK="$1"
else
    DISK=/dev/disk/by-partlabel/Red Bear OS_INSTALL
fi

if [ ! -b "${DISK}" ]
then
    echo "$0: '${DISK}' is not a block device" >&2
    exit 1
fi

eval $(make setenv)

IMAGE="${BUILD}/filesystem.img"
set -x
rm -f "${IMAGE}"
make "${IMAGE}"
sudo popsicle "${IMAGE}" "${DISK}"
set +x

ESP="$(bootctl --print-esp-path)"
if [ -z "${ESP}" ]
then
    echo "$0: no ESP found" >&2
    exit 1
fi

BOOTLOADER="recipes/core/bootloader/target/${ARCH}-unknown-redox/stage/usr/lib/boot/bootloader.efi"
set -x
sudo mkdir -pv "${ESP}/EFI" "${ESP}/loader/entries"
sudo cp -v "${BOOTLOADER}" "${ESP}/EFI/redbear.efi"
sudo tee "${ESP}/loader/entries/redbear.conf" <<EOF
title Red Bear OS
efi /EFI/redbear.efi
EOF
set +x

sync

echo "Finished installing Red Bear OS dual boot"
echo ""
echo "To mount the Red Bear OS filesystem partition, run:"
echo "  ./scripts/mount-redoxfs.sh ${DISK}"
