# Drivers

- [Libraries](#libraries)
- [Services](#services)
- [Hardware Interfaces](#hardware-interfaces)
- [Devices](#devices)
  - [CPU](#cpu)
  - [Controllers](#controllers)
  - [Storage](#storage)
  - [Graphics](#graphics)
  - [Input](#input)
  - [Sound](#sound)
  - [Networking](#networking)
  - [Virtualization](#virtualization)
- [System Interfaces](#system-interfaces)
- [System Calls](#system-calls)
- [Schemes](#schemes)
- [Contribution Details](#contribution-details)

## Libraries

- amlserde - Library to provide serialization/deserialization of the AML symbol table from ACPI
- common - Library with shared driver code
- executor - Library to run Rust futures and integrate the executor in an interrupt+queue model without a separated reactor thread
- [graphics/console-draw](graphics/console-draw/) - Library with shared terminal drawing code
- [graphics/driver-graphics](graphics/driver-graphics/) - Library with shared graphics code
- [graphics/graphics-ipc](graphics/graphics-ipc/) - Library with graphics IPC shared code
- [net/driver-network](net/driver-network/) - Library with shared networking code
- [storage/partitionlib](storage/partitionlib/) - Library with MBR and GPT code
- [storage/driver-block](storage/driver-block/) - Library with shared storage code
- virtio-core - VirtIO driver library

## Services

- [graphics/fbbootlogd](graphics/fbbootlogd/) - Daemon for boot log drawing
- [graphics/fbcond](graphics/fbcond/) - Terminal daemon
- hwd - Daemon that handle the ACPI and DeviceTree booting
- inputd - Multiplexes input from multiple input drivers and provides that to Orbital
- pcid-spawner - Daemon for PCI-based device driver spawn
- [storage/lived](storage/lived/) - Daemon for live disk
- redoxerd - Daemon that send/receive terminal text between the host system and QEMU

## Hardware Interfaces

- acpid - ACPI interface driver
- pcid - PCI and PCI Express driver

## Devices

### CPU

- rtcd - x86 Real Time Clock driver

### Controllers

- [usb/xhcid](usb/xhcid/) - xHCI USB controller driver

### Storage

- [storage/ahcid](storage/ahcid/) - AHCI (SATA) driver
- [storage/bcm2835-sdhcid](storage/bcm2835-sdhcid/) - BCM2835 storage driver
- [storage/ided](storage/ided/) - PATA (IDE) driver
- [storage/nvmed](storage/nvmed/) - NVMe driver
- [storage/virtio-blkd](storage/virtio-blkd/) - VirtIO block device driver
- [storage/usbscsid](storage/usbscsid/) - USB SCSI driver

### Graphics

- [graphics/ihdgd](graphics/ihdgd/) - Intel graphics driver
- [graphics/vesad](graphics/vesad/) - VESA video driver
- [graphics/virtio-gpud](graphics/virtio-gpud/) - VirtIO-GPU device driver

### Input

- [input/ps2d](input/ps2d/) - PS/2 interface driver
- [input/usbhidd](input/usbhidd/) - USB HID driver
- [usb/usbhubd](usb/usbhubd/) - USB Hub driver
- [usb/usbctl](usb/usbctl/) - TODO

### Sound

- [audio/ac97d](audio/ac97d/) - AC'97 codec driver
- [audio/ihdad](audio/ihdad/) - Intel HD Audio chipset driver
- [audio/sb16d](audio/sb16d/) - Sound Blaster sound card driver

### Networking

- [net/e1000d](net/e1000d/) - Intel Gigabit ethernet driver
- [net/ixgbed](net/ixgbed/) - Intel 10 Gigabit ethernet driver
- [net/rtl8139d](net/rtl8139d/), [net/rtl8168d](net/rtl8168d/) - Realtek ethernet drivers
- [net/virtio-netd](net/virtio-netd/) - VirtIO network device driver

### Virtualization

- vboxd - VirtualBox driver

Some drivers are work-in-progress and incomplete, read [this](https://gitlab.redox-os.org/redox-os/base/-/issues/56) tracking issue to verify.

## System Interfaces

This section explain the system interfaces used by drivers.

### System Calls

- `iopl` : system call that sets the I/O privilege level. x86 has four privilege rings (0/1/2/3), of which the kernel runs in ring 0 and userspace in ring 3. IOPL can only be changed by the kernel, for obvious security reasons, and therefore the Redox kernel needs root to set it. It is unique for each process. Processes with IOPL=3 can access I/O ports, and the kernel can access them as well.

### Schemes

- `/scheme/memory/physical` : Allows mapping physical memory frames to driver-accessible virtual memory pages, with various available memory types:
    - `/scheme/memory/physical` : Default memory type (currently writeback)
    - `/scheme/memory/physical@wb` Writeback cached memory
    - `/scheme/memory/physical@uc` : Uncacheable memory
    - `/scheme/memory/physical@wc` : Write-combining memory
- `/scheme/irq` : Allows getting events from interrupts. It is used primarily by listening for its file descriptors using the `/scheme/event` scheme.

## Contribution Details

### Driver Design

A device driver on Redox is an user-space daemon that use system calls and schemes to work, while operating systems with monolithic kernels drivers use internal kernel APIs instead of common program APIs.

If you want to port a driver from a monolithic operating system to Redox you will need to rewrite the driver with reverse enginnering of the code logic, because the logic is adapted to internal kernel APIs (it's a hard task if the device is complex, datasheets are much more easy).

### Write a Driver

Datasheets are preferable (much more easy depending on device complexity), when they are freely available. Be aware that datasheets are often provided under a [Non-Disclosure Agreement](https://en.wikipedia.org/wiki/Non-disclosure_agreement) from hardware vendors, which can affect the ability to create an MIT-licensed driver.

If datasheets aren't available you need to do reverse-engineering of BSD or Linux drivers (if you want use a Linux driver as reference for your Redox driver please ask in the [Chat](https://doc.redox-os.org/book/chat.html) before the implementation to know/satisfy the license requirements and not waste your time, also if you use a BSD driver not licensed as BSD as reference).

### Libraries

You should use the [redox-scheme](https://crates.io/crates/redox-scheme) and [redox_event](https://crates.io/crates/redox_event) libraries to create your drivers, you can also read the [example driver](https://gitlab.redox-os.org/redox-os/exampled) or read the code of other drivers with the same type of your device.

Before testing your changes be aware of [this](https://doc.redox-os.org/book/coding-and-building.html#how-to-update-initfs).

### References

If you want to reverse enginner the existing drivers, you can access the BSD code using these links:

- [FreeBSD drivers](https://github.com/freebsd/freebsd-src/tree/main/sys/dev)
- [NetBSD drivers](https://github.com/NetBSD/src/tree/trunk/sys/dev)
- [OpenBSD drivers](https://github.com/openbsd/src/tree/master/sys/dev)

## How To Contribute

To learn how to contribute to this system component you need to read the following document:

- [CONTRIBUTING.md](https://gitlab.redox-os.org/redox-os/redox/-/blob/master/CONTRIBUTING.md)

## Development

To learn how to do development with this system component inside the Redox build system you need to read the [Build System](https://doc.redox-os.org/book/build-system-reference.html) and [Coding and Building](https://doc.redox-os.org/book/coding-and-building.html) pages.

### How To Build

To build this system component you need to download the Redox build system, you can learn how to do it on the [Building Redox](https://doc.redox-os.org/book/podman-build.html) page.

This is necessary because they only work with cross-compilation to a Redox virtual machine or real hardware, but you can do some testing from Linux.

[Back to top](#drivers)
