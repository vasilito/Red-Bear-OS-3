#!/usr/bin/env python3
"""Extract quirk entries from Linux kernel source and generate Red Bear OS TOML quirk files.

Usage:
    python3 extract-linux-quirks.py /path/to/linux/drivers/pci/quirks.c
    python3 extract-linux-quirks.py /path/to/linux/drivers/usb/core/quirks.c

Outputs TOML quirk entries to stdout that can be appended to files in
/etc/quirks.d/ or local/recipes/system/redbear-quirks/source/quirks.d/.

PCI mode: handler-name → flag mapping is heuristic (substring match).  Output
requires manual review — the script may misinfer flags.  USB table extraction is
direct and does not require review.
"""

import re
import sys


PCI_FLAG_MAP = {
    "PCI_DEV_FLAGS_NO_D3": "no_d3cold",
    "PCI_DEV_FLAGS_NO_ASPM": "no_aspm",
    "PCI_DEV_FLAGS_NO_MSI": "no_msi",
    "PCI_DEV_FLAGS_NO_MSIX": "no_msix",
    "PCI_DEV_FLAGS_ASSIGN_BARS": "disable_bar_sizing",
    "PCI_DEV_FLAGS_BROKEN_PM": "no_pm",
}

USB_FLAG_MAP = {
    "USB_QUIRK_STRING_FETCH": "no_string_fetch",
    "USB_QUIRK_NO_RESET_RESUME": "need_reset",
    "USB_QUIRK_NO_SET_INTF": "no_set_config",
    "USB_QUIRK_NO_LPM": "no_lpm",
    "USB_QUIRK_NO_U1_U2": "no_u1u2",
    "USB_QUIRK_DELAY_INIT": "reset_delay",
    "USB_QUIRK_LINEAR_UFRAME_INTR_BINTERVAL": "bad_descriptor",
    "USB_QUIRK_DISCONNECT_SUSPEND": "no_suspend",
}

PCI_FIXUP_RE = re.compile(
    r'DECLARE_PCI_FIXUP_(?:FINAL|HEADER|EARLY|ENABLE|RESUME|LATE)\s*\(\s*'
    r'(?:0x([0-9a-fA-F]+)|PCI_ANY_ID)\s*,\s*'
    r'(?:0x([0-9a-fA-F]+)|PCI_ANY_ID)\s*,\s*'
    r'(\w+)\s*\)'
)

DMI_MATCH_RE = re.compile(
    r'DMI_MATCH\s*\(\s*DMI_([A-Z_]+)\s*,\s*"([^"]+)"\s*\)'
)

USB_QUIRK_TABLE_RE = re.compile(
    r'\{\s*USB_DEVICE\s*\(\s*(?:0x([0-9a-fA-F]+)|USB_ANY_ID)\s*,\s*'
    r'(?:0x([0-9a-fA-F]+)|USB_ANY_ID)\s*\)\s*,'
    r'([^}]+)\}'
)


def extract_pci_fixups(source):
    entries = []
    for m in PCI_FIXUP_RE.finditer(source):
        vendor = int(m.group(1), 16) if m.group(1) else 0xFFFF
        device = int(m.group(2), 16) if m.group(2) else 0xFFFF
        handler = m.group(3)
        entries.append((vendor, device, handler))
    return entries


def extract_usb_quirks(source):
    entries = []
    for m in USB_QUIRK_TABLE_RE.finditer(source):
        vendor = int(m.group(1), 16) if m.group(1) else 0xFFFF
        product = int(m.group(2), 16) if m.group(2) else 0xFFFF
        flags_raw = m.group(3)
        flags = []
        for flag_name, toml_name in USB_FLAG_MAP.items():
            if flag_name in flags_raw:
                flags.append(toml_name)
        entries.append((vendor, product, flags))
    return entries


def format_pci_toml(entries):
    lines = []
    for vendor, device, flags in entries:
        if not flags:
            continue
        lines.append("[[pci_quirk]]")
        if vendor != 0xFFFF:
            lines.append(f"vendor = 0x{vendor:04X}")
        if device != 0xFFFF:
            lines.append(f"device = 0x{device:04X}")
        lines.append(f'flags = [{", ".join(f"\"{f}\"" for f in flags)}]')
        lines.append("")
    return "\n".join(lines)


def format_usb_toml(entries):
    lines = []
    for vendor, product, flags in entries:
        if not flags:
            continue
        lines.append("[[usb_quirk]]")
        if vendor != 0xFFFF:
            lines.append(f"vendor = 0x{vendor:04X}")
        if product != 0xFFFF:
            lines.append(f"product = 0x{product:04X}")
        lines.append(f'flags = [{", ".join(f"\"{f}\"" for f in flags)}]')
        lines.append("")
    return "\n".join(lines)


def main():
    if len(sys.argv) < 2:
        print(__doc__, file=sys.stderr)
        sys.exit(1)

    path = sys.argv[1]
    with open(path) as f:
        source = f.read()

    if "usb_quirk" in source.lower() or "USB_QUIRK" in source:
        entries = extract_usb_quirks(source)
        print(format_usb_toml(entries))
    else:
        entries = extract_pci_fixups(source)
        flags_map = PCI_FLAG_MAP
        mapped = []
        for vendor, device, handler in entries:
            flags = []
            for flag_name, toml_name in flags_map.items():
                if flag_name.lower() in handler.lower():
                    flags.append(toml_name)
            mapped.append((vendor, device, flags))
        print("# WARNING: PCI handler-name → flag mapping is heuristic.")
        print("# WARNING: Output requires manual review before committing.")
        print("# USB table extraction is direct and does not need review.")
        print(format_pci_toml(mapped))


if __name__ == "__main__":
    main()
