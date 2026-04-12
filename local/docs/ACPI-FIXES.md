# ACPI Fixes — P0 Phase Tracker

Status of ACPI fixes for AMD bare metal boot. Cross-referenced with
`HARDWARE.md` crash reports and kernel/acpid source TODOs.

## Crash Reports

| Hardware | Symptom | Root Cause | Status |
|----------|---------|------------|--------|
| Framework Laptop 16 (AMD 7040) | Crash on boot | Unimplemented ACPI function (jackpot51/acpi#3) | Under investigation |
| Lenovo ThinkCentre M83 | `Aml(NoCurrentOp)` panic at acpid acpi.rs:256 | AML interpreter encounters unsupported opcode | Under investigation |
| HP Compaq nc6120 | Crash after `kernel::acpi` prints APIC info | Unknown — may be ACPI or APIC init | Under investigation |

## Known Missing ACPI Table Parsers

| Table | Location | Status | Impact |
|-------|----------|--------|--------|
| DSDT (Differentiated System Description Table) | Parsed by `acpi` crate AML interpreter | Working | Platform-specific device config via AML bytecode |
| SSDT (Secondary System Description Table) | Parsed by `acpi` crate AML interpreter | Working | Secondary AML tables (hotplug, etc.) |
| FACP/FADT | Partially parsed in acpid | Partial | PM registers, reset register, sleep states |
| IVRS (AMD-Vi IOMMU) | ✅ Implemented in acpid | P2+ | AMD IOMMU for device passthrough |
| MCFG (PCI Express config space) | ✅ Implemented in acpid | P1 | PCIe extended config space access |
| DBG2 (Debug port) | Not implemented | Low | Serial debug port discovery |
| BGRT (Boot graphics) | Not implemented | Low | Boot logo preservation |
| FPDT (Firmware perf data) | Not implemented | Low | Boot performance metrics |

## Implemented ACPI Tables

| Table | Kernel | Userspace (acpid) | Notes |
|-------|--------|-------------------|-------|
| RSDP | `acpi/rsdp.rs` | N/A | Signature + checksum validated ✅ |
| RSDT/XSDT | `acpi/rsdt.rs`, `acpi/xsdt.rs` | N/A | Root table pointer iteration |
| MADT (APIC) | `acpi/madt/` | N/A | xAPIC + x2APIC (type 0x9) |
| HPET | `acpi/hpet.rs` | N/A | Assumes single HPET |
| DMAR (Intel VT-d) | N/A | `acpi/dmar/` | Iterator bug fixed, re-enabled |
| FADT | N/A | `acpi.rs` | Partial parse |
| SPCR | `acpi/spcr.rs` | N/A | ARM64 serial console |
| GTDT | `acpi/gtdt.rs` | N/A | ARM64 timers |

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

| Fix | File | Description |
|-----|------|-------------|
| x2APIC Type 9 support | `kernel redox.patch` | MadtLocalX2Apic struct + AP boot via ICR |
| AP startup timeout | `kernel redox.patch` | 100M-iteration bounded waits prevent infinite hang |
| Second SIPI | `kernel redox.patch` | Universal Startup Algorithm compliance |
| x2APIC ICR delivery polling | `kernel redox.patch` | Pre/post wrmsr PENDING bit check |
| MadtIter zero-length guard | `kernel redox.patch` | `entry_len < 2` returns None |
| RSDP checksum validation | `kernel rsdp.rs` | Signature + ACPI 1.0/2.0+ checksum validation |
| DMAR iterator hardening | `base redox.patch` | `len < 4` guard + type_bytes fix |
| Trampoline W+X | `kernel redox.patch` | Documented W^X limitation |
| CPUID arch split | `kernel redox.patch` | Separate x86/x86_64 cpuid functions |
| Memory alignment | `kernel redox.patch` | `find_free_near_aligned` with power-of-two assert |
| MCFG parser | `acpid acpi/mcfg/` | PCIe ECAM base address discovery |
| IVRS parser | `acpid acpi/ivrs/` | AMD IOMMU (AMD-Vi) hardware unit discovery |
