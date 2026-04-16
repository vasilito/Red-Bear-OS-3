# ACPI Fixes — P0 Phase Tracker

> **Numbering note:** "P0" refers to the historical hardware-enablement phase (ACPI boot),
> not the v2.0 desktop plan phases in `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md`.

Status of ACPI fixes for AMD bare metal boot. Cross-referenced with
`HARDWARE.md` crash reports and kernel/acpid source TODOs.

P0 ACPI work is **complete**. Kernel patch is 574 lines, base/acpid patch is 558 lines.

## Crash Reports

| Hardware | Symptom | Root Cause | Status |
|----------|---------|------------|--------|
| Framework Laptop 16 (AMD 7040) | Crash on boot | Unimplemented ACPI function (jackpot51/acpi#3) | ✅ Fixed (RSDP/SDT checksums, MADT NMI types, FADT parse) |
| Lenovo ThinkCentre M83 | `Aml(NoCurrentOp)` panic at acpid acpi.rs:256 | AML interpreter encounters unsupported opcode | Under investigation (upstream AML issue) |
| HP Compaq nc6120 | Crash after `kernel::acpi` prints APIC info | xAPIC APIC ID read returned raw value, caused page fault on Intel | ✅ Fixed (xAPIC `id()` now shifts `read(0x20) >> 24`) |

## Known Missing ACPI Table Parsers

| Table | Location | Status | Impact |
|-------|----------|--------|--------|
| DSDT (Differentiated System Description Table) | Parsed by `acpi` crate AML interpreter | Working | Platform-specific device config via AML bytecode |
| SSDT (Secondary System Description Table) | Parsed by `acpi` crate AML interpreter | Working | Secondary AML tables (hotplug, etc.) |
| FACP/FADT | ✅ Full parse in acpid | ✅ Done | PM registers, reset register, sleep states, `\_S5` |
| IVRS (AMD-Vi IOMMU) | Removed from acpid stub path | Handled by `iommu` daemon path | ACPI-side broken stub removed; runtime AMD-Vi handling now lives in the separate daemon |
| MCFG (PCI Express config space) | Removed (broken stub) | ✅ Handled by pcid | pcid /config endpoint provides direct PCI config space access |
| DBG2 (Debug port) | Not implemented | Low | Serial debug port discovery |
| BGRT (Boot graphics) | Not implemented | Low | Boot logo preservation |
| FPDT (Firmware perf data) | Not implemented | Low | Boot performance metrics |

IVRS was previously listed as "implemented" but the acpid stub was broken, so it was removed from
acpid. AMD-Vi runtime handling now lives in the separate `iommu` daemon path rather than in acpid.
MCFG is now handled by pcid's /config endpoint (P1 complete) which provides direct PCI config space
access.

## Implemented ACPI Tables

| Table | Kernel | Userspace (acpid) | Notes |
|-------|--------|-------------------|-------|
| RSDP | `acpi/rsdp.rs` | N/A | Signature + checksum validated (ACPI 1.0 + 2.0+ extended) |
| RSDT/XSDT | `acpi/rsdt.rs`, `acpi/xsdt.rs` | N/A | Root table pointer iteration + SDT checksum validation |
| MADT (APIC) | `acpi/madt/` | N/A | xAPIC + x2APIC (type 0x9) + NMI (0x4, 0xA) + address override (0x5) |
| HPET | `acpi/hpet.rs` | N/A | Assumes single HPET |
| DMAR (Intel VT-d) | N/A | `acpi/dmar/` | Iterator bug fixed, re-enabled, safe on AMD (early return) |
| FADT | N/A | `acpi.rs` | Full: PM1a/b CNT, reset register, `\_S5` sleep types, GenericAddress I/O |
| Power Methods | N/A | `acpi.rs` | `\_PS0`/`\_PS3`/`\_PPC` AML evaluation for device power control |
| SPCR | `acpi/spcr.rs` | N/A | ARM64 serial console |
| GTDT | `acpi/gtdt.rs` | N/A | ARM64 timers |

## ACPI MADT Entry Types

All MADT entry types parsed by the kernel. The MADT loop in `x86.rs` dispatches
each type to the appropriate handler.

| Type | Name | Struct | Size | Kernel Action |
|------|------|--------|------|---------------|
| 0x0 | Processor Local APIC | `MadtLocalApic` | 8 bytes | AP boot via SIPI |
| 0x1 | I/O APIC | `MadtIoApic` | 12 bytes | Enumerated |
| 0x2 | Interrupt Source Override | `MadtIntSrcOverride` | 10 bytes | IRQ remapping |
| 0x4 | Local APIC NMI | `MadtLocalApicNmi` | 4 bytes | LVT NMI programming (xAPIC 0x350/0x360) |
| 0x5 | LAPIC Address Override | `MadtLapicAddressOverride` | 10 bytes | Logged (64-bit address) |
| 0x9 | Local x2APIC | `MadtLocalX2Apic` | 16 bytes | AP boot via x2APIC ICR (MSR) |
| 0xA | Local x2APIC NMI | `MadtLocalX2ApicNmi` | 10 bytes | x2APIC LVT NMI MSR (0x835/0x836) |

All structs include compile-time size assertions (`assert!(size_of::<T>() == N)`)
to catch ABI mismatches early.

## Kernel ACPI TODOs

From `recipes/core/kernel/source/src/acpi/`:

| File | Line | TODO | Priority |
|------|------|------|----------|
| `mod.rs` | 132 | Don't touch ACPI tables in kernel? (move to userspace) | Future |
| `mod.rs` | 147 | Enumerate processors in userspace | Future |
| `mod.rs` | 154 | Let userspace setup HPET | Future |
| `rsdp.rs` | ~~21~~ | ~~Validate RSDP checksum~~ ✅ Done | ~~P0~~ Done |
| `hpet.rs` | 56 | Assumes only one HPET | Low |
| `spcr.rs` | 38,86,100,110 | Optional fields, more interrupt types | ARM64 only |
| `madt/mod.rs` | 134 | Optional field in ACPI 6.5 (trbe_interrupt) | Low |
| `madt/mod.rs` | — | ~~NMI entry parsing~~ ✅ Done (types 0x4, 0xA) | ~~P0~~ Done |
| `madt/mod.rs` | — | ~~LVT NMI programming~~ ✅ Done (xAPIC + x2APIC) | ~~P0~~ Done |
| `madt/mod.rs` | — | ~~LAPIC address override~~ ✅ Done (type 0x5) | ~~P0~~ Done |
| `madt/mod.rs` | — | ~~xAPIC APIC ID fix~~ ✅ Done (`read(0x20) >> 24`) | ~~P0~~ Done |
| `madt/mod.rs` | — | ~~SDT checksum validation~~ ✅ Done (warn-only) | ~~P0~~ Done |

## ACPID (Userspace) TODOs — UPSTREAM, NOT AMD-FIRST P0/P1

These are pre-existing upstream acpid issues. They are NOT part of the
AMD-first P0/P1 scope. They exist in mainline Redox acpid and affect all
platforms, not just AMD.

| File | Line | TODO | Priority | Scope |
|------|------|------|----------|-------|
| `acpi.rs` | 266 | Use parsed tables for rest of acpid | Upstream | Mainline acpid improvement |
| `acpi.rs` | 643 | Handle SLP_TYPb for sleep states | Upstream | Mainline power management |
| `aml_physmem.rs` | 418,423,428 | Mutex create/acquire/release | Upstream | Mainline AML interpreter |
| `ec.rs` | 193+ (8 occurrences) | Proper error types | Upstream | Mainline EC handler |
| `dmar/mod.rs` | 7 | Move DMAR to separate driver | Upstream | Mainline driver refactor |

## P0 Fixes Applied

### Kernel ACPI (local/patches/kernel/redox.patch — 574 lines)

| # | Fix | Description |
|---|-----|-------------|
| 1 | xAPIC APIC ID fix | `id()` returns `read(0x20) >> 24` for xAPIC mode (was raw, caused Intel page fault) |
| 2 | x2APIC MADT type 0x9 | `MadtLocalX2Apic` struct + AP boot via ICR with universal startup algorithm |
| 3 | ICR pending wait | Pre/post wrmsr PENDING bit check for x2APIC `set_icr()` |
| 4 | ICR constants | `ICR_INIT_ASSERT (0x4500)`, `ICR_STARTUP (0x4600)` with bit-layout comments |
| 5 | MADT entry length guard | `entry_len < 2` returns None (prevents infinite loop on malformed tables) |
| 6 | RSDP checksum validation | ACPI 1.0 + 2.0+ extended checksum |
| 7 | SDT checksum validation | `validate_checksum()` method + warn-only on failure |
| 8 | CPUID arch split | Separate x86/x86_64 cpuid functions |
| 9 | Memory alignment | `find_free_near_aligned()` with power-of-two assert |
| 10 | Trampoline W+X | Documented limitation (code must be writable + executable during AP init) |
| 11 | MADT type 0x4 (Local APIC NMI) | `MadtLocalApicNmi` struct (4 bytes), compile-time size assertion |
| 12 | MADT type 0x5 (LAPIC Address Override) | `MadtLapicAddressOverride` struct (10 bytes), logged |
| 13 | MADT type 0xA (x2APIC NMI) | `MadtLocalX2ApicNmi` struct (10 bytes), compile-time size assertion |
| 14 | LVT NMI programming | `set_lvt_nmi()` method for xAPIC (0x350/0x360) and x2APIC (0x835/0x836 MSRs) |
| 15 | NMI processing in x86.rs | LocalApicNmi, LocalX2ApicNmi, LapicAddressOverride handling in MADT loop |
| 16 | AP startup timeout | 100M-iteration bounded waits prevent infinite hang |
| 17 | Second SIPI | Universal Startup Algorithm compliance (Intel spec requires two SIPIs) |

### Userspace Acpid (local/patches/base/redox.patch — 558 lines)

| # | Fix | Description |
|---|-----|-------------|
| 1 | DMAR iterator fix | `type_bytes` renamed to `len_bytes` bug fix + `len < 4` guard |
| 2 | DMAR init re-enabled | Safe on AMD (no DMAR table = early return, no crash) |
| 3 | FADT shutdown | `acpi_shutdown()` using PM1a/PM1b CNT_BLK writes with `\_S5` sleep types |
| 4 | FADT reboot | `acpi_reboot()` using ACPI reset register via GenericAddress |
| 5 | Keyboard controller fallback | `Pio::<u8>::new(0x64).write(0xFE)` when reset_reg unavailable |
| 6 | Power methods | `evaluate_acpi_method()`, `device_power_on()` (`\_PS0`), `device_power_off()` (`\_PS3`), `device_get_performance()` (`\_PPC`) |
| 7 | GenericAddress rename | `GenericAddressStructure` renamed to `GenericAddress` with `is_empty()`, `write_u8()` |
| 8 | Reboot wiring | `reboot_requested` flag in main.rs, scheme path detection |
| 9 | ivrs/mcfg removed | Broken stub references eliminated (deferred to P2+, handled by pcid) |
