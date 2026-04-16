# Red Bear OS Hardware Quirks System

## Overview

Red Bear OS implements a data-driven hardware quirks system inspired by Linux's
PCI/USB/DMI quirk infrastructure, adapted for Redox's microkernel/userspace-driver
architecture.

Quirks handle known hardware defects that cannot be fixed by correct driver code
alone. They override default driver behavior for specific devices, revisions, or
entire system models.

For the current follow-up cleanup and integration roadmap, see
`local/docs/QUIRKS-IMPROVEMENT-PLAN.md`.

## Architecture

```
Driver probes device
  └─ PciDeviceInfo::quirks()
       ├─ Layer 1: Compiled-in table (pci_table.rs, usb_table.rs)
       ├─ Layer 2: TOML files from /etc/quirks.d/*.toml
       └─ Layer 3: DMI-based system rules
       └─ Returns: PciQuirkFlags (bitwise OR of all matching entries)
```

All matching entries accumulate via bitwise OR, so broad rules (e.g., "all AMD GPUs
need firmware") and narrow rules (e.g., "this specific revision has broken MSI-X")
compose naturally.

## Quirk Sources

### 1. Compiled-in Tables

Location: `local/recipes/drivers/redox-driver-sys/source/src/quirks/`

Critical quirks that must be available before the root filesystem is mounted.
Defined as `const` arrays in Rust:

- `pci_table.rs` — `PCI_QUIRK_TABLE: &[PciQuirkEntry]`
- `usb_table.rs` — `USB_QUIRK_TABLE: &[UsbQuirkEntry]`

Each entry specifies:
- Vendor/device/subsystem match fields (0xFFFF = wildcard)
- Revision range (lo..hi inclusive)
- Class code mask and match value
- `PciQuirkFlags` bitmask

### 2. TOML Quirk Files

Location: `/etc/quirks.d/*.toml` (shipped by the `redbear-quirks` package)

Extensible at runtime without recompiling drivers. Format:

```toml
[[pci_quirk]]
vendor = 0x1002
device = 0x73BF
flags = ["need_firmware", "no_d3cold"]

[[pci_quirk]]
vendor = 0x10EC
device = 0x8125
flags = ["no_aspm"]

[[usb_quirk]]
vendor = 0x0A12
flags = ["bad_descriptor", "no_set_config"]
```

Files are loaded alphabetically from `/etc/quirks.d/`. Recommended naming:
`00-core.toml`, `10-gpu.toml`, `20-usb.toml`, `30-net.toml`, `40-storage.toml`,
`50-system.toml`.

Runtime TOML loading now also supports `[[dmi_system_quirk]]` entries. Those
entries are applied when `acpid` is running and serving live DMI data from
`/scheme/acpi/dmi`.

### 3. DMI-Based System Quirks

Match by SMBIOS fields (sys_vendor, board_name, product_name) to apply
system-wide quirk overrides. Eight compiled-in rules exist for known systems,
and `/etc/quirks.d/*.toml` can now add `[[dmi_system_quirk]]` rules with
`match.*` keys plus optional `pci_vendor` / `pci_device` selectors. Runtime use
now reads live SMBIOS strings from `acpid` via `/scheme/acpi/dmi`.

## Available Quirk Flags

### PCI Quirks (PciQuirkFlags)

| Flag | Meaning |
|------|---------|
| `NO_MSI` | MSI capability broken; use MSI-X or legacy |
| `NO_MSIX` | MSI-X capability broken; use MSI or legacy |
| `FORCE_LEGACY_IRQ` | Must use INTx interrupts |
| `NO_PM` | Disable all power management |
| `NO_D3COLD` | Cannot recover from D3cold power state |
| `NO_ASPM` | Active State Power Management broken |
| `NEED_IOMMU` | Requires IOMMU isolation |
| `NO_IOMMU` | Must NOT be behind IOMMU |
| `DMA_32BIT_ONLY` | Only supports 32-bit DMA |
| `RESIZE_BAR` | BAR sizing reports incorrectly |
| `DISABLE_BAR_SIZING` | Use firmware BAR values as-is |
| `NEED_FIRMWARE` | Requires firmware files to initialize |
| `DISABLE_ACCEL` | Disable hardware acceleration |
| `FORCE_VRAM_ONLY` | No GTT/system memory fallback |
| `NO_USB3` | Force USB 2.0 mode |
| `RESET_DELAY_MS` | Needs extra post-reset delay |
| `NO_STRING_FETCH` | Do not fetch string descriptors |
| `BAD_EEPROM` | EEPROM unreliable; use hardcoded values |
| `BUS_MASTER_DELAY` | Needs delay after bus-master enable |
| `WRONG_CLASS` | Reports incorrect class code |
| `BROKEN_BRIDGE` | PCI bridge forwarding bug |
| `NO_RESOURCE_RELOC` | Do not relocate PCI resources |

### USB Quirks (UsbQuirkFlags)

| Flag | Meaning |
|------|---------|
| `NO_STRING_FETCH` | Do not fetch string descriptors |
| `RESET_DELAY` | Needs extra reset delay |
| `NO_USB3` | Disable USB 3.x |
| `NO_SET_CONFIG` | Cannot handle SetConfiguration |
| `NO_SUSPEND` | Broken suspend/resume |
| `NEED_RESET` | Needs reset after probe |
| `BAD_DESCRIPTOR` | Wrong descriptor sizes |
| `NO_LPM` | Disable Link Power Management |
| `NO_U1U2` | Disable U1/U2 link transitions |

## Driver Integration

### For Rust Drivers (using redox-driver-sys)

```rust
use redox_driver_sys::quirks::PciQuirkFlags;

fn probe(info: &PciDeviceInfo) {
    let quirks = info.quirks();

    if quirks.contains(PciQuirkFlags::NO_MSIX) {
        // Skip MSI-X, try MSI or legacy
    }

    if quirks.contains(PciQuirkFlags::NEED_FIRMWARE) {
        // Load firmware before initializing device
    }

    if quirks.contains(PciQuirkFlags::DISABLE_ACCEL) {
        // Skip hardware probe, let software renderer take over
        return Err(DriverError::QuirkDisabled);
    }
}
```

### For C Drivers (using linux-kpi)

The `linux-kpi` crate exposes two FFI functions for C drivers to query quirks:

```c
#include <linux/pci.h>

// After pci_enable_device() in your probe callback:
static int my_probe(struct pci_dev *dev, const struct pci_device_id *id)
{
    u64 quirks = pci_get_quirk_flags(dev);

    if (quirks & PCI_QUIRK_NO_MSIX) {
        // Skip MSI-X, fall back to MSI or legacy IRQ
    }

    if (pci_has_quirk(dev, PCI_QUIRK_NEED_FIRMWARE)) {
        // Load firmware before initializing hardware
    }
}
```

The amdgpu Redox glue/runtime path is now the first in-tree production C consumer
of this interface: it queries `pci_get_quirk_flags()` during AMD DC init, logs the
resulting IRQ expectations, and treats `PCI_QUIRK_NEED_FIRMWARE` as a hard failure
instead of a warn-and-continue path when that quirk is active.

Available C quirk flag macros (defined in `linux/pci.h`):

| Macro | Bit | Meaning |
|-------|-----|---------|
| `PCI_QUIRK_NO_MSI` | 0 | MSI interrupts broken |
| `PCI_QUIRK_NO_MSIX` | 1 | MSI-X interrupts broken |
| `PCI_QUIRK_FORCE_LEGACY` | 2 | Must use legacy INTx |
| `PCI_QUIRK_NO_PM` | 3 | Power management broken |
| `PCI_QUIRK_NO_D3COLD` | 4 | D3cold state broken |
| `PCI_QUIRK_NO_ASPM` | 5 | ASPM broken |
| `PCI_QUIRK_NEED_IOMMU` | 6 | Requires IOMMU |
| `PCI_QUIRK_DMA_32BIT_ONLY` | 8 | DMA limited to 32-bit |
| `PCI_QUIRK_NEED_FIRMWARE` | 11 | Requires firmware load |
| `PCI_QUIRK_DISABLE_ACCEL` | 12 | Disable hardware acceleration |

## Adding New Quirks

### To the compiled-in table

Edit `local/recipes/drivers/redox-driver-sys/source/src/quirks/pci_table.rs`:

```rust
const F_MY_FLAGS: PciQuirkFlags = PciQuirkFlags::from_bits_truncate(
    PciQuirkFlags::NEED_FIRMWARE.bits() | PciQuirkFlags::NO_ASPM.bits(),
);

PciQuirkEntry {
    vendor: 0xVENDOR,
    device: 0xDEVICE,
    flags: F_MY_FLAGS,
    ..PciQuirkEntry::WILDCARD
},
```

### To a TOML file

Create or edit a file in `local/recipes/system/redbear-quirks/source/quirks.d/`:

```toml
[[pci_quirk]]
vendor = 0xVENDOR
device = 0xDEVICE
flags = ["need_firmware", "no_aspm"]

[[dmi_system_quirk]]
pci_vendor = 0xVENDOR
flags = ["disable_accel"]
match.sys_vendor = "Example Vendor"
match.product_name = "Example Model"

[[acpi_table_quirk]]
signature = "DMAR"
match.sys_vendor = "Example Vendor"
match.product_name = "Example Model"
```

### Choosing where to add

- **Compiled-in**: Boot-critical quirks, anything needed before root mount
- **TOML**: Everything else — easier to update, no recompilation needed
- **DMI rule**: System-specific workarounds that apply to specific laptop models

## File Layout

```
local/recipes/drivers/redox-driver-sys/source/src/quirks/
├── mod.rs           # Public API: lookup_pci_quirks(), PciQuirkFlags, PciQuirkEntry
├── pci_table.rs     # Compiled-in PCI quirk table
├── usb_table.rs     # Compiled-in USB quirk table
├── dmi.rs           # DMI/SMBIOS matching and system-level quirk rules
└── toml_loader.rs   # /etc/quirks.d/*.toml parser

local/recipes/system/redbear-quirks/
├── recipe.toml      # Custom build: copies TOML files to /etc/quirks.d/
└── source/quirks.d/
    ├── 00-core.toml
    ├── 10-gpu.toml
    ├── 20-usb.toml
    ├── 30-net.toml
    ├── 40-storage.toml
    └── 50-system.toml
```

## Relationship to Linux Quirks

| Linux Pattern | Red Bear Equivalent |
|---------------|-------------------|
| `DECLARE_PCI_FIXUP_HEADER(v, d, fn)` | `PciQuirkEntry { vendor: v, device: d, flags: ... }` |
| `pci_dev->dev_flags \|= PCI_DEV_FLAGS_NO_BUS_RESET` | No direct equivalent — future flag candidate |
| `USB_QUIRK_STRING_FETCH` | `UsbQuirkFlags::NO_STRING_FETCH` |
| `DMI_MATCH(DMI_SYS_VENDOR, "Lenovo")` | `DmiMatchRule { sys_vendor: Some("Lenovo") }` |
| `acpi_black_listed()` | `[[acpi_table_quirk]] signature = "...."` with skip semantics in `acpid` |

## Testing

Run quirks unit tests:

```bash
cd local/recipes/drivers/redox-driver-sys/source
cargo test
```

## Implementation Status

| Phase | Component | Status |
|-------|-----------|--------|
| Q1 | Core types (PciQuirkFlags, PciQuirkEntry, UsbQuirkFlags) | ✅ Done |
| Q1 | Compiled-in PCI/USB quirk tables | ✅ Done |
| Q1 | Lookup API (quirks(), has_quirk()) | ✅ Done |
| Q1 | Subsystem (subvendor/subdevice) fields | ✅ Done — compiled and TOML PCI matching both apply subsystem selectors |
| Q2 | TOML loader for /etc/quirks.d/ | ✅ Done |
| Q2 | redbear-quirks data package | ✅ Done |
| Q3 | redox-drm integration (MSI-X/MSI/legacy + DISABLE_ACCEL) | ✅ Done |
| Q3 | xhcid PCI controller quirks (interrupt + reset delay) | ✅ Done |
| Q3 | xhcid USB device quirks (descriptor/configuration/BOS handling) | ✅ Done |
| Q3 | pcid-spawner quirk passthrough | ✅ Done |
| Q3 | linux-kpi quirk flag bridge | ✅ Done |
| Q3 | amdgpu linux-kpi quirk consumption | ✅ Done |
| Q3 | redbear-info --quirks display | ✅ Done |
| Q4 | DMI/SMBIOS compiled-in rules | ✅ Done — 8 system rules (const table) |
| Q4 | DMI/SMBIOS TOML runtime loading | ✅ Done — `dmi_system_quirk` uses live `/scheme/acpi/dmi` data from `acpid` |
| Q4 | ACPI table blacklist/override | ✅ Done — `acpid` applies `[[acpi_table_quirk]]` skip rules during table load |
| Q5 | lspci quirk display | ✅ Done — shows active quirks per device |
| Q5 | lsusb quirk display | ✅ Done — shows active quirks per device |
| Q5 | Linux quirk extraction tool | ✅ Script exists — PCI mode uses heuristic name matching, USB mode works for table entries |

Quirk flags span data definition, infrastructure wiring, and driver consumption.
Most flags are defined but not yet consumed at runtime — the tables below show
the honest breakdown.

**Flags consumed by drivers (runtime checks in production code):**
- redox-drm: `NO_MSIX`, `NO_MSI`, `FORCE_LEGACY_IRQ`, `DISABLE_ACCEL` (interrupt setup + driver probe)
- xhcid: `RESET_DELAY_MS`, `NO_MSI`, `NO_MSIX`, `FORCE_LEGACY_IRQ` (interrupt selection + port reset delay)
- xhcid (USB device path): `NO_SET_CONFIG`, `NO_STRING_FETCH`, `BAD_DESCRIPTOR`, `NO_USB3`, `NO_LPM`, `NO_U1U2` (enumeration/configuration/BOS handling)
- amdgpu: `NEED_FIRMWARE` (hard firmware gate), with real quirk-aware logging for `NO_ASPM`, `NEED_IOMMU`, `NO_MSI`, `NO_MSIX`

**Infrastructure (data flows, reporting, and partial integration):**
- pcid-spawner: computes `PCI_QUIRK_FLAGS` by calling the canonical `redox-driver-sys` lookup on synthesized `PciDeviceInfo`, then passes the env var onward
- linux-kpi: `pci_get_quirk_flags()` / `pci_has_quirk()` C FFI is available for C drivers and is now consumed by the Red Bear amdgpu path
- redbear-info: `--quirks` reads `/etc/quirks.d/*.toml` and reports configured PCI/USB/DMI entries
- lspci: shows active quirk flags per PCI device (via redox-driver-sys lookup)
- lsusb: shows active quirk flags per USB device (via redox-driver-sys lookup)
- DMI compiled-in rules: 8 entries match systems by vendor/product/board (served through `acpid` at `/scheme/acpi/dmi`)

**Observed/logged but not yet strongly enforced in runtime policy:**
- `NO_ASPM`, `NEED_IOMMU`, `NO_MSI`, `NO_MSIX` in the amdgpu path are surfaced in quirk-aware logs before broader driver policy exists.

**Defined but not yet consumed by any real driver path:**
- `NO_PM`, `NO_D3COLD`, `DMA_32BIT_ONLY`, `BUS_MASTER_DELAY`, `NO_IOMMU`, etc.

`firmware-loader` itself does not interpret `NEED_FIRMWARE`; that policy is now enforced in the amdgpu driver path instead.

`NEED_RESET` remains defined for USB devices but is not yet consumed by a runtime USB driver path.

**Remaining infrastructure work:**
- none in the current quirks scope

`pcid-spawner` now brokers quirks through the canonical `redox-driver-sys` lookup instead of carrying a separate in-tree PCI quirk engine.
