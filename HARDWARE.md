# Hardware Compatibility

> Hardware compatibility inherited from upstream Redox OS. See https://doc.redox-os.org/book/hardware-support.html for the latest upstream data.
>
> **Status note (2026-04-15):** This file is a hardware-reporting/support-tracking surface, not the
> canonical source for profile support language or project execution order. For current Red Bear
> support framing, also use `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md` and
> `local/docs/PROFILE-MATRIX.md`.

This document tracks the current hardware compatibility of Red Bear OS.

Red Bear OS should now treat AMD and Intel machines as equal-priority hardware targets. Older
AMD-first wording in subsystem roadmap documents should be read as historical sequencing rather
than current platform policy.

- [Why are hardware reports needed?](#why-are-hardware-reports-needed)
- [What if my computer is customized?](#what-if-my-computer-is-customized)
- [Status](#status)
- [General](#general)
- [Contribute to this document](#contribute-to-this-document)
    - [Template](#template)
    - [Table row ordering](#table-row-ordering)
- [Recommended](#recommended)
- [Booting](#booting)
- [Broken](#broken)

## Why are hardware reports needed?

Each computer model has different hardware interfaces, firmware implementations, and devices, which can cause the following problems:

- Boot bugs
- Lack of device support
- Performance degradation

These reports helps us to fix the problems above, your report may help to fix many computers affected by the same bugs or missing drivers.

## What if my computer is customized?

If your desktop is customized (common) you should use the "Custom" word on the "Vendor" category and insert the motherboard and CPU vendor/model in the "Model" category.

A customized laptop should only be reported if you replaced the original CPU, report the CPU vendor and model in the "Model" category.

We also recommend to add your `pciutils` log as a comment on [this](https://gitlab.redox-os.org/redox-os/redox/-/issues/1797) tracking issue to help us with probable device porting.

## Status

- **Recommended:** The operating system boots with video, sound, PS/2 or USB input, Ethernet, terminal, and the graphical session working.
- **Booting:** The operating system boots with some issues or lacking hardware support (write the issues and what supported hardware is not working in the "Report" section).
- **Broken:** The boot loader don't work or can't bootstrap the operating system.

## General

This section contain limitations that apply to any status.

- ACPI bring-up is **materially complete for boot baseline**; implemented: kernel
  RSDP/RSDT/XSDT/MADT/FADT coverage, AML mutexes with real tracked state, EC widened accesses via
  byte transactions, kstop-based shutdown eventing, explicit `RSDP_ADDR` forwarding into `acpid`,
  x86 BIOS-search AML fallback, and bounded AML-backed power enumeration. The explicit boot-path
  producer contract for AML bootstrap is still underdocumented, `acpid` startup hardening remains
  open, shutdown/power reporting are still provisional, sleep
  state transitions beyond `\_S5`, DMAR ownership cleanup, and broader platform validation all
  remain open — see `local/docs/ACPI-IMPROVEMENT-PLAN.md`
- Wi-Fi broad support is not available yet; bounded Intel Wi-Fi scaffolding and validation paths now
  cover probe/status/prepare/init/activate plus bounded scan/connect/disconnect/retry surfaces, but
  validated real wireless connectivity support remains incomplete
- Bluetooth broad support is not available yet; one bounded in-tree BLE-first experimental slice
  exists, but broad controller or desktop parity remains incomplete
- Broad hardware-validated GPU acceleration is not supported yet; the default proven path remains
  BIOS VESA and UEFI GOP, even though Red Bear now carries compile-oriented AMD/Intel DRM work in
  the local overlay
- I2C devices aren't supported yet (PS/2 or USB devices should be used)
- USB support still varies by machine and device class, but Red Bear now has QEMU-proven xHCI
  interrupt-mode and USB mass-storage autospawn paths
- Automatic operating system discovery is not implemented in the boot loader yet (remember this before installing Red Bear OS)

## Contribute to this document

To contribute to this document, learn how to create your GitLab account, follow the project-wide contribution guidelines and suggestions, please refer to the [CONTRIBUTING.md](./CONTRIBUTING.md) document.

### Template

You will use this template to insert your computer on the table.

```
|  |  |  |  |  |  |  |  |
```

The Redox image date should use the [ISO format](https://en.wikipedia.org/wiki/ISO_8601)

### Table row ordering

New reports should use an independent alphabetical order in the "Vendor" and "Model" table rows, for example:

```
| ASUS | ROG g55vw |
| ASUS | X554L |
| System76 | Galago Pro (galp5) |
| System76 | Lemur Pro (lemp9) |
```

A comes before S, R comes before X, G comes before L

Each "Vendor" has its own alphabetical order in "Model", independent from models from other vendor.

## Recommended

| **Vendor** | **Model** | **Red Bear OS Version** | **Image Date** | **Variant** | **CPU Architecture** | **Motherboard Firmware** | **Report** |
|------------|-----------|-------------------|----------------|-------------|----------------------|--------------------------|------------|
| Lenovo | IdeaPad Y510P | 0.8.0 | 2022-11-11 | desktop | x86-64 | BIOS, UEFI | Boots to graphical session |
| System76 | Galago Pro (galp5) | 0.8.0 | 2022-11-11 | desktop | x86-64 | UEFI | Boots to graphical session |
| System76 | Lemur Pro (lemp9) | 0.8.0 | 2022-11-11 | desktop | x86-64 | UEFI | Boots to graphical session |

## Booting

| **Vendor** | **Model** | **Red Bear OS Version** | **Image Date** | **Variant** | **CPU Architecture** | **Motherboard Firmware** | **Report** |
|------------|-----------|-------------------|----------------|-------------|----------------------|--------------------------|------------|
| ASUS | Eee PC 900 | 0.8.0 | 2022-11-11 | desktop | i686 | BIOS | Boots to graphical session, No ethernet driver, Correct video mode not offered (firmware issue) |
| ASUS | PRIME B350M-E (custom) | 0.9.0 | 2024-09-20 | desktop | x86-64 | UEFI | Partial support for the PS/2 keyboard, PS/2 mouse is broken |
| ASUS | ROG g55vw | 0.8.0 | 2023-11-11 | desktop | x86-64 | BIOS | Boots to graphical session, UEFI panic in SETUP |
| ASUS | X554L | 0.8.0 | 2022-11-11 | desktop | x86-64 | BIOS | Boots to graphical session, No audio, HDA driver cannot find output pins |
| ASUS | Vivobook 15 OLED (M1503Q) | 0.9.0 | 2025-08-04 | desktop | x86-64 | UEFI | Boots to graphical session, touchpad and usb do not work, cannot connect to the internet, right maximum display resolution 2880x1620 |
| Dell | XPS 13 (9350) | 0.8.0 | 2022-11-11 | desktop | i686 | BIOS | Boots to graphical session, NVMe driver livelocks |
| Dell | XPS 13 (9350) | 0.8.0 | 2022-11-11 | desktop | x86-64 | BIOS, UEFI | Boots to graphical session, NVMe driver livelocks |
| Framework | Laptop 16 (AMD Ryzen 7040 Series) | 0.9.0 | 2026-3-29 | desktop, demo | x86-64 | UEFI | Historical ACPI boot-baseline fixes applied (RSDP/SDT checksums, MADT NMI types, FADT parse); moved from Broken table; broader bounded validation still needed |
| HP | Dev One | 0.8.0 | 2022-11-11 | desktop | x86-64 | UEFI | Boots to graphical session, No touchpad support, requires I2C HID |
| HP | EliteBook Folio 9480M | 0.9.0 | 2025-11-04 | desktop | x86-64 | UEFI | Boots to graphical session, touchpad and usb work, cannot connect to the Internet, install failed, right maximum display resolution 1600x900
| HP | Compaq nc6120 | 0.9.0 | 2024-11-08 | desktop, server | i686 | BIOS | xAPIC fix applied; **hardware validation still needed**; moved from Broken table |
| Lenovo | ThinkPad Yoga 260 Laptop - Type 20FE | 0.9.0 | 2024-09-07 | demo | x86-64 | UEFI | Boots to graphical session, No audio |
| Lenovo | Yoga S730-13IWL | 0.9.0 | 2024-11-09 | desktop | x86-64 | UEFI | Boots to graphical session, No trackpad or USB mouse input support |
| Raspberry Pi | 3 Model B+ | 0.8.0 | Unknown | server | ARM64 | U-Boot | Boots to UART serial console (pl011) |
| Samsung | Series 3 (NP350V5C) | 0.9.0 | 2025-08-04 | desktop | x86-64 | UEFI | Boots to graphical session, touchpad works, USB does not work, can connect to the Internet through LAN. Wrong maximum display resolution 1024x768 |
| System76 | Oryx Pro (oryp10) | 0.8.0 | 2022-11-11 | desktop | x86-64 | UEFI | Boots to graphical session, No touchpad support, though it should be working |
| System76 | Pangolin (pang12) | 0.8.0 | 2022-11-11 | desktop | x86-64 | UEFI | Boots to graphical session, No touchpad support, requires I2C HID |
| Toshiba | Satellite L500 | 0.8.0 | 2022-11-11 | desktop | x86-64 | BIOS | Boots to graphical session, No Ethernet driver, Correct video mode not offered (firmware issue) |

## Broken

| **Vendor** | **Model** | **Red Bear OS Version** | **Image Date** | **Variant** | **CPU Architecture** | **Motherboard Firmware** | **Report** |
|------------|-----------|-------------------|----------------|-------------|----------------------|--------------------------|------------|
| ASUS | PN41 | 0.8.0 | 2024-05-30 | server | x86-64 | Unknown | Aborts after panic in xhcid |
| BEELINK | U59 | 0.8.0 | 2024-05-30 | server | x86-64 | Unknown | Aborts after panic in xhcid |
| HP | EliteBook 2570p | 0.8.0 | 2022-11-23 | demo | x86-64 | BIOS (CSM mode?) | Gets to resolution selection, Fails assert in `src/os/bios/mod.rs:77` after selecting resolution |
| Lenovo | G570 | 0.8.0 | 2022-11-11 | desktop | x86-64 | BIOS | Bootloader panics in `alloc_zeroed_page_aligned`, Correct video mode not offered (firmware issue) |
| Lenovo | IdeaPad Y510P | 0.8.0 | 2022-11-11 | desktop | i686 | BIOS | Panics on `phys_to_virt overflow`, probably having invalid mappings for 32-bit |
| Lenovo | ThinkCentre M83 | 0.9.0 | 2025-11-09 | desktop | x86_64 | UEFI | Presents user with a set of display resolution options. After user selects an option, it takes a long time for the "live" thing to load all the way to 647MiB. Once it does reach 647MiB, however, it dumps a bunch of logs onto the screen. Those logs also happen to be offset so that the leftmost portion of all text "exists" past the leftmost part of the screen, resulting in the logs being only partially visible. The logs appear to include (among other things) 1. "thread 'main' (1) panicked at acpid/src/acpi.rs:256:68: Called `Result::unwrap()` on an `Err` value: Aml(NoCurrentOp)"; 2. "thread 'main' (1) panicked at acpid/src/main.rs:147:39:acpid: failed to daemonize: Error `I/O error` 5"; 3. "... [@hwd:40 ERROR] failed to probe with error No such device (os error 19)..."; etc. |
| Panasonic | Toughbook CF-18 | 0.8.0 | 2022-11-11 | desktop | i686 | BIOS | Hangs after PIT initialization |
| Toshiba | Satellite L500 | 0.8.0 | 2022-11-11 | desktop | i686 | BIOS | Correct video mode not offered (firmware issue), Panics on `phys_to_virt overflow`, probably having invalid mappings for 32-bit |
| XMG (Schenker) | Apex 17 (M21) | 0.9.0 | 2024-09-30 | demo, server | x86-64 | UEFI | After selecting resolution, (release) repeats `...::interrupt::irq::ERROR -- Local apic internal error: ESR=0x40` a few times before it freezes; (daily) really slowly prints statements from `...::rmm::INFO` before it abruptly aborts |
