#!/usr/bin/env python3
"""Extract quirk entries from Linux kernel source and generate Red Bear OS TOML quirk files.

Usage:
    python3 extract-linux-quirks.py /path/to/linux/drivers/pci/quirks.c
    python3 extract-linux-quirks.py /path/to/linux/drivers/usb/core/quirks.c
    python3 extract-linux-quirks.py /path/to/linux/drivers/usb/storage/unusual_devs.h

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
    "USB_QUIRK_STRING_FETCH_255": "no_string_fetch",
    "USB_QUIRK_RESET_RESUME": "need_reset",
    "USB_QUIRK_NO_SET_INTF": "no_set_intf",
    "USB_QUIRK_CONFIG_INTF_STRINGS": "config_intf_strings",
    "USB_QUIRK_RESET": "no_reset",
    "USB_QUIRK_HONOR_BNUMINTERFACES": "honor_bnuminterfaces",
    "USB_QUIRK_DELAY_INIT": "reset_delay",
    "USB_QUIRK_LINEAR_UFRAME_INTR_BINTERVAL": "bad_descriptor",
    "USB_QUIRK_DEVICE_QUALIFIER": "device_qualifier",
    "USB_QUIRK_IGNORE_REMOTE_WAKEUP": "ignore_remote_wakeup",
    "USB_QUIRK_NO_LPM": "no_lpm",
    "USB_QUIRK_LINEAR_FRAME_INTR_BINTERVAL": "linear_frame_binterval",
    "USB_QUIRK_DISCONNECT_SUSPEND": "no_suspend",
    "USB_QUIRK_DELAY_CTRL_MSG": "delay_ctrl_msg",
    "USB_QUIRK_HUB_SLOW_RESET": "hub_slow_reset",
    "USB_QUIRK_ENDPOINT_IGNORE": "endpoint_ignore",
    "USB_QUIRK_SHORT_SET_ADDRESS_REQ_TIMEOUT": "short_set_addr_timeout",
    "USB_QUIRK_NO_BOS": "no_bos",
    "USB_QUIRK_FORCE_ONE_CONFIG": "force_one_config",
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
            pattern = re.escape(flag_name) + r'(?:\s|$|\||\))'
            if re.search(pattern, flags_raw):
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


STORAGE_FLAG_MAP = {
    "US_FL_IGNORE_RESIDUE": "ignore_residue",
    "US_FL_FIX_CAPACITY": "fix_capacity",
    "US_FL_SINGLE_LUN": "single_lun",
    "US_FL_MAX_SECTORS_64": "max_sectors_64",
    "US_FL_FIX_INQUIRY": "fix_inquiry",
    "US_FL_GO_SLOW": "go_slow",
    "US_FL_SANE_SENSE": "sane_sense",
    "US_FL_BAD_SENSE": "bad_sense",
    "US_FL_NOT_LOCKABLE": "not_lockable",
    "US_FL_NO_WP_DETECT": "no_wp_detect",
    "US_FL_IGNORE_DEVICE": "ignore_device",
    "US_FL_IGNORE_UAS": "ignore_uas",
    "US_FL_CAPACITY_HEURISTICS": "capacity_heuristics",
    "US_FL_CAPACITY_OK": "capacity_ok",
    "US_FL_BROKEN_FUA": "broken_fua",
    "US_FL_BULK_IGNORE_TAG": "bulk_ignore_tag",
    "US_FL_BULK32": "bulk32",
    "US_FL_NEED_OVERRIDE": "need_override",
    "US_FL_NO_READ_CAPACITY_16": "no_read_cap16",
    "US_FL_NO_REPORT_OPCODES": "no_report_opcodes",
    "US_FL_NO_READ_DISC_INFO": "no_read_disc_info",
    "US_FL_INITIAL_READ10": "initial_read10",
    "US_FL_WRITE_CACHE": "write_cache",
    "US_FL_SCM_MULT_TARG": "scm_mult_targ",
    "US_FL_ALWAYS_SYNC": "always_sync",
    "US_FL_SENSE_AFTER_SYNC": "sense_after_sync",
    "US_FL_NO_ATA_1X": "no_ata_1x",
    "US_FL_NEEDS_CAP16": "needs_cap16",
    "US_FL_MAX_SECTORS_MIN": "max_sectors_min",
}

UNUSUAL_DEV_RE = re.compile(
    r'UNUSUAL_DEV\s*\(\s*'
    r'0x([0-9a-fA-F]+)\s*,\s*'
    r'0x([0-9a-fA-F]+)\s*,\s*'
    r'0x([0-9a-fA-F]+)\s*,\s*'
    r'0x([0-9a-fA-F]+)\s*,\s*'
    r'"([^"]*)"\s*,\s*'
    r'"([^"]*)"\s*,\s*'
    r'[^,]+,\s*[^,]+,\s*[^,]*,\s*'
    r'([^)]+)\)',
    re.MULTILINE | re.DOTALL
)


def extract_storage_quirks(source):
    entries = []
    for m in UNUSUAL_DEV_RE.finditer(source):
        vendor = int(m.group(1), 16)
        product = int(m.group(2), 16)
        rev_lo = m.group(3)
        rev_hi = m.group(4)
        mfr = m.group(5)
        product_name = m.group(6)
        flags_raw = m.group(7)
        flags = []
        for flag_name, toml_name in STORAGE_FLAG_MAP.items():
            if flag_name in flags_raw:
                flags.append(toml_name)
        entries.append((vendor, product, rev_lo, rev_hi, mfr, product_name, flags))
    return entries


def format_storage_toml(entries):
    lines = []
    lines.append("# USB mass storage device quirks from Linux unusual_devs.h.")
    lines.append("# Type: [[usb_storage_quirk]] with vendor, product, flags, and metadata fields.")
    lines.append("")
    for vendor, product, rev_lo, rev_hi, mfr, product_name, flags in entries:
        if not flags:
            continue
        lines.append("[[usb_storage_quirk]]")
        lines.append(f"vendor = 0x{vendor:04X}")
        lines.append(f"product = 0x{product:04X}")
        lines.append(f'revision = "{rev_lo}-{rev_hi}"')
        if mfr:
            lines.append(f'manufacturer = "{mfr}"')
        if product_name:
            lines.append(f'description = "{product_name}"')
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

    if "UNUSUAL_DEV" in source:
        entries = extract_storage_quirks(source)
        total = len(entries)
        with_flags = sum(1 for e in entries if e[6])
        print(f"# Extracted {with_flags} entries with flags out of {total} total from unusual_devs.h")
        print(format_storage_toml(entries))
    elif "usb_quirk" in source.lower() or "USB_QUIRK" in source:
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
