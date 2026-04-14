# IOMMU Specification Reference — AMD-Vi & Intel VT-d

**Purpose**: Implementation-ready hardware register and data structure reference for Red Bear OS IOMMU support. Based on AMD IOMMU Specification 48882 Rev 3.10 and Intel Virtualization Technology for Directed I/O (VT-d) Rev 5.0.

**Status**: The `iommu` daemon now builds in-tree, but hardware validation is still missing in the AMD-first integration plan (see `AMD-FIRST-INTEGRATION.md`). This document provides the register and data-structure reference for finishing AMD-Vi and Intel VT-d bring-up.

---

## Table of Contents

1. [AMD-Vi (AMD IOMMU)](#1-amd-vi-amd-iommu)
2. [Intel VT-d](#2-intel-vt-d)
3. [Rust Struct Definitions](#3-rust-struct-definitions)

---

## 1. AMD-Vi (AMD IOMMU)

### 1.1 MMIO Register Map

Base address obtained from ACPI IVRS table (IVHD entry `IOMMUInfo` field).

| Offset | Name | Size | Access | Description |
|--------|------|------|--------|-------------|
| 0x0000 | DevTableBar | 64-bit | R/W | Device Table Base Address. Bits 12:51 hold physical address. Bits 0:8 = DeviceTableSize (entries = 2^(size+1), max 65536). Must be 4KiB-aligned. |
| 0x0008 | CmdBufBar | 64-bit | R/W | Command Buffer Base Address. Bits 12:51 hold physical address. Bits 0:8 = CmdBufLen (size = 2^(len+2) × 16 bytes). Must be 4KiB-aligned. |
| 0x0010 | EvtLogBar | 64-bit | R/W | Event Log Base Address. Bits 12:51 hold physical address. Bits 0:8 = EvtLogLen (size = 2^(len+2) × 16 bytes). Must be 4KiB-aligned. |
| 0x0018 | Control | 32-bit | R/W | IOMMU Control Register. See bit layout below. |
| 0x0020 | ExclusionBase | 64-bit | R/W | Exclusion Range Base Address. Physical address of excluded region start. |
| 0x0028 | ExclusionLimit | 64-bit | R/W | Exclusion Range Limit Address. Physical address of excluded region end. |
| 0x0030 | ExtendedFeature | 64-bit | RO | Extended Feature Register. Capability flags. Read to determine supported features. |
| 0x0038 | PprLogBar | 64-bit | R/W | Peripheral Page Request Log Base Address. Bits 12:51 = address, Bits 0:8 = log length. |
| 0x0030 | ExtendedFeature | 64-bit | RO | Extended Feature Register (alias for capability query). |
| 0x2000 | CmdBufHead | 64-bit | R/W | Command Buffer Head Pointer. Index into command buffer (byte offset / 16). |
| 0x2008 | CmdBufTail | 64-bit | R/W | Command Buffer Tail Pointer. Written by software to submit commands. |
| 0x2010 | EvtLogHead | 64-bit | R/W | Event Log Head Pointer. Written by software after reading events. |
| 0x2018 | EvtLogTail | 64-bit | RO | Event Log Tail Pointer. Updated by IOMMU hardware after writing event. |
| 0x2020 | Status | 32-bit | RO | IOMMU Status Register. See bit layout below. |
| 0x2028 | PprLogHead | 64-bit | R/W | PPR Log Head Pointer. |
| 0x2030 | PprLogTail | 64-bit | RO | PPR Log Tail Pointer. |

#### Control Register (0x0018) Bit Layout

| Bit | Name | Description |
|-----|------|-------------|
| 0 | IOMMUEnable | 0 = IOMMU translations disabled, 1 = enabled. Must be set last after all other config. |
| 1 | HTTunEn | HyperTransport Tunnel Enable. Set 0 for modern systems. |
| 2 | EventLogEn | Event Log Enable. Set 1 to enable event logging. |
| 3 | EventIntEn | Event Log Interrupt Enable. Set 1 to generate interrupts on event log overflow. |
| 4 | ComWaitIntEn | Completion Wait Interrupt Enable. |
| 5 | CmdBufEn | Command Buffer Enable. Set 1 to enable command processing. |
| 6 | PprLogEn | Peripheral Page Request Log Enable. |
| 7 | PprIntEn | PPR Log Interrupt Enable. |
| 8 | PprEn | Peripheral Page Request Processing Enable. |
| 9 | GTEn | Guest Translation Enable. |
| 10 | GAEn | Guest APIC (Advanced Programmable Interrupt Controller) Enable. |
| 12 | CRW | IOMMU Reset. Write 1 to clear errors after reset. |
| 13 | SMifEn | SMI Filter Enable. |
| 14 | SlFWEn | Self-Modify Firmware Enable. |
| 15 | SMifLogEn | SMI Filter Log Enable. |
| 16 | GAMEn_0 | Guest APIC Mode bit 0. |
| 17 | GAMEn_1 | Guest APIC Mode bit 1. |
| 18 | GAMEn_2 | Guest APIC Mode bit 2. |
| 22 | XTEn | x2APIC Enabled. |
| 23 | NXEn | No-Execute Enable. |
| 24 | IRQTableLEn | Interrupt Remap Table Length Enable. |

#### Status Register (0x2020) Bit Layout

| Bit | Name | Description |
|-----|------|-------------|
| 0 | IOMMURunning | 1 = IOMMU is processing commands or translations. |
| 1 | EventOverflow | 1 = Event log overflow occurred. Write 1 to clear. |
| 2 | EventLogInt | 1 = Event log interrupt pending. |
| 3 | ComWaitInt | 1 = Completion wait interrupt pending. |
| 4 | PprOverflow | 1 = PPR log overflow. |
| 5 | PprInt | 1 = PPR log interrupt pending. |
| 31 | RsvdP | Reserved (polling status bits). |

#### Extended Feature Register (0x0030) Bit Layout

| Bit | Name | Description |
|-----|------|-------------|
| 0 | PrefSup | Prefetch Support. |
| 1 | PPRSup | Peripheral Page Request Support. |
| 2 | XTSup | x2APIC Support. |
| 3 | NXSup | No-Execute Support. |
| 4 | GTSup | Guest Translation Support. |
| 5 | bit5 | Reserved. |
| 6 | IASup | Invalidate IOMMU All Support. |
| 7 | GASup | Guest APIC Support. |
| 8 | HESup | Hardware Error Registers Support. |
| 9 | PCSup | Performance Counters Support. |
| 12:15 | MsiNumPPR | MSI message number for PPR. |
| 27 | PASMax | Maximum PASID support. |
| 46:52 | PASMax | Physical Address Space Max (1 = 48-bit, 2 = 52-bit). |
| 57 | GISup | Global Invalidate Support. |
| 58 | HASup | Host Address Translation Size. |

### 1.2 Device Table Entry (DTE)

The Device Table holds up to 65536 entries indexed by BDF (Bus:Device:Function). Each entry is 256 bits (32 bytes). The table must be contiguous in physical memory.

**Table size**: entries × 32 bytes. With 65536 entries, max 2 MiB.

```
DTE layout (256 bits = data[0] data[1] data[2] data[3], each u64):

data[0] (bits 0-63):
  [ 0]    V       — Valid. 1 = entry is valid.
  [ 1]    TV      — Translation Valid. 1 = address translation enabled for this device.
  [ 2:3]  Reserved
  [ 4]    IW      — Write permission (when Mode != 0). 1 = device may write.
  [ 5]    IR      — Read permission (when Mode != 0). 1 = device may read.
  [ 6:7]  Reserved
  [ 8]    SE      — Snoop Enable. 1 = device requests are snooped.
  [ 9:11] Mode    — Translation mode:
                      000 = No translation (pass-through if TV=0)
                      001 = 1-level page table
                      010 = 2-level page table
                      011 = 3-level page table
                      100 = 4-level page table
                      101 = 5-level page table
                      110 = 6-level page table
                      111 = Reserved
  [12:51] PTP     — Page Table Root Pointer. Physical address of top-level page table.
                     Must be 4KiB-aligned. Bits 0:11 of the address are assumed zero.
  [52:55] GCR3Trp0 — Guest CR3 Table Root Pointer bits 12:15.
  [56:58] GV      — Guest Translation Valid bits.
  [59]    GLX     — Guest Levels bit 0.
  [60]    GLX     — Guest Levels bit 1.
  [61]    IR      — Interrupt Remapping Enable. 1 = interrupts from this device are remapped.
  [62]    IW      — Interrupt Write permission. 1 = device may generate interrupt writes.
  [63]    Reserved

data[1] (bits 64-127):
  [0:3]   IntTabLen — Interrupt Remap Table Length. Number of entries = 2^(IntTabLen+1).
                        0 = 2 entries, 1 = 4 entries, ..., 10 = 2048 entries, 11 = 4096 entries.
  [4:5]   IntCtl    — Interrupt Control. 00 = abort, 01 = pass-through (no remap),
                        10 = remapped, 11 = reserved.
  [6:51]  IRTP      — Interrupt Remap Table Pointer. Physical address of interrupt
                        remap table. Must be 4KiB-aligned (bits 0:11 assumed zero).
  [52:63] Reserved

data[2] (bits 128-191):
  [0:51]  GCR3Trp1 — Guest CR3 Table Root Pointer bits 16:63.
  [52:63] Reserved

data[3] (bits 192-255):
  [0:15]  GCR3Trp2 — Guest CR3 Table Root Pointer bits 64:79.
  [16]    AttrRsvd  — Reserved attribute bit.
  [17]    AttrU     — User bit for device-specific use.
  [18:20] Mode2     — Alias to Mode bits (duplicate for hardware).
  [21:63] Reserved
```

**Key constants from Linux** (`drivers/iommu/amd/amd_iommu_types.h`):

```c
#define DTE_FLAG_V    (1ULL << 0)
#define DTE_FLAG_TV   (1ULL << 1)
#define DTE_FLAG_IR   (1ULL << 61)
#define DTE_FLAG_IW   (1ULL << 62)
#define DTE_MODE_MASK 0x0E00ULL         // bits 9:11
#define DTE_PT_ADDR_MASK  0x0FFFFFFFFFF000ULL  // bits 12:51
#define DEV_DOMID_MASK    0x0FFFFULL    // domain ID in bits 0:15 (when TV=0)
```

### 1.3 Interrupt Remapping Table Entry (IRTE)

The Interrupt Remap Table is pointed to by the IRTP field in the DTE. Each entry is 128 bits (16 bytes). Length is 2^(IntTabLen+1) entries.

```
IRTE (128 bits = data[0] data[1], each u64):

data[0]:
  [0]     RemapEn   — Remap Enable. 1 = this entry is valid for remapping.
  [1]     SupIOPF   — Suppress I/O Page Faults. 1 = suppress faults from this interrupt.
  [2]     IntType   — Interrupt Type:
                        000 = Fixed (edge or level, determined by trigger mode)
                        001 = Arbitrated
                        010 = SMI
                        011 = NMI
                        100 = INIT
                        101 = EXTINT
                        111 = Hardware-specific
  [3:4]   IntType bits continued (3-bit field uses bits 2:4)
  [5]     Rsvd      — Reserved.
  [5:7]   DM        — Delivery Mode. 0 = Fixed, 1 = Lowest Priority.
  [8]     IRrsvd    — Reserved.
  [9:10]  GV        — Guest Vector.
  [11]    GDstMode  — Guest Destination Mode. 0 = Physical, 1 = Logical.
  [12]    DstMode   — Destination Mode. 0 = Physical APIC ID, 1 = Logical.
  [13:15] Rsvd      — Reserved.
  [16:31] DstID     — Destination APIC ID. For x2APIC, full 32-bit ID (low 16 bits here).
  [16:31] DstLo     — Low 16 bits of destination APIC ID.
  [32:63] Vector    — Interrupt vector (0x10..0xFE).

data[1]:
  [0:31]  DstHi     — High 32 bits of x2APIC destination ID. Zero for xAPIC.
  [32:63] Rsvd      — Reserved. Must be zero.
```

**IRTE bit layout for x2APIC mode (when XTSup=1 in ExtendedFeature)**:

```
data[0]:
  [0]     RemapEn   — 1 = valid
  [1]     SupIOPF   — Suppress IO Page Fault
  [2:4]   IntType   — Interrupt type (same as above)
  [5:7]   Rsvd
  [8]     DstMode   — 0 = physical, 1 = logical
  [9:10]  Rsvd
  [16:31] DstIDLo   — Low 16 bits of x2APIC ID
  [32:39] Vector    — Interrupt vector
  [40:63] Rsvd

data[1]:
  [0:31]  DstIDHi   — High 32 bits of x2APIC destination ID
  [32:63] Rsvd
```

### 1.4 Command Buffer Entry

The command buffer is a circular queue. Each entry is 128 bits (16 bytes = 4 × u32). Software writes to the tail, hardware reads from the head. Base address in CmdBufBar, head/tail pointers in CmdBufHead/CmdBufTail.

**Buffer sizing**: 8192 bytes default (512 entries). Size = 2^(CmdBufLen+2) × 16 bytes.

```
Command Buffer Entry (128 bits = word[0] word[1] word[2] word[3], each u32):

word[0]:
  [0:3]   Opcode   — Command opcode (see below)
  [4:31]  Varies   — Opcode-specific operands

word[1], word[2], word[3]:
  Opcode-specific payload. See each command format below.
```

#### COMPLETION_WAIT (Opcode 0x01)

Used to poll for command completion. Can generate an interrupt or write a value to memory.

```
word[0]: [0:3]=0x01, [4]=Store (1=write to memory), [5]=Interrupt (1=generate IRQ),
          [6:31] Reserved
word[1]: [0:31] Store Address low 32 bits (physical, must be 8-byte aligned)
word[2]: [0:31] Store Address high 32 bits
word[3]: [0:31] Store Data — value written to Store Address when command completes
```

#### INVALIDATE_DEVTAB_ENTRY (Opcode 0x02)

Invalidates a single device table entry. Must be issued after modifying a DTE.

```
word[0]: [0:3]=0x02, [4:31] Reserved
word[1]: [0:15] DeviceId (BDF format: Bus[15:8] | Dev[7:3] | Func[2:0])
          [16:31] Reserved
word[2]: [0:31] Reserved
word[3]: [0:31] Reserved
```

#### INVALIDATE_IOMMU_PAGES (Opcode 0x03)

Invalidates translation cache (TLB) entries for a range of pages.

```
word[0]: [0:3]=0x03, [4]=S (Size: 0=invalidate one page, 1=invalidate all pages for domain),
          [5]=PDE (Page Directory Entry: 1=invalidate PDE cache too),
          [6:31] Reserved
word[1]: [0:15] DomainId — domain to invalidate
          [16:31] Reserved
word[2]: [0:51] Address — virtual address to invalidate (page-aligned). Ignored if S=1.
          [52:63] Reserved
word[3]: [0:31] Reserved
```

#### INVALIDATE_INTERRUPT_TABLE (Opcode 0x04)

Invalidates the interrupt remap cache for a device.

```
word[0]: [0:3]=0x04, [4:31] Reserved
word[1]: [0:15] DeviceId (BDF format)
          [16:31] Reserved
word[2]: [0:31] Reserved
word[3]: [0:31] Reserved
```

#### INVALIDATE_IOMMU_ALL (Opcode 0x05)

Invalidates all IOMMU caches (TLB, DTE, IRTE). Available when IASup=1.

```
word[0]: [0:3]=0x05, [4:31] Reserved
word[1]: [0:31] Reserved
word[2]: [0:31] Reserved
word[3]: [0:31] Reserved
```

### 1.5 Event Log Entry

The event log is a circular queue written by the IOMMU hardware. Each entry is 128 bits (16 bytes = 4 × u32). Base address in EvtLogBar.

**Buffer sizing**: 8192 bytes default (512 entries). Size = 2^(EvtLogLen+2) × 16 bytes.

```
Event Log Entry (128 bits = word[0] word[1] word[2] word[3]):

word[0]:
  [0:15] EventCode — Event type code (see below)
  [16:31] EventFlags — Event-specific flags

word[1], word[2], word[3]:
  Event-specific data. See each event type below.
```

#### IO_PAGE_FAULT (Event Code 0x01)

Generated when a device accesses an address that fails translation.

```
word[0]: [0:15]=0x01, [16] TR (Translation Response: 1=fault in translation),
          [17] RZ (Read/Zero: 1=read of zero page), [18] I (Interrupt: 1=interrupt request),
          [19] PE (Permission Error: 1=permission violation), [20] RW (1=write, 0=read),
          [21] PR (Present: 1=PTE was present), [22] Rsvd
word[1]: [0:15] DeviceId (BDF), [16:31] Reserved or PASID
word[2]: [0:31] Fault Address low 32 bits
word[3]: [0:31] Fault Address high 32 bits
```

#### INVALIDATE_DEVICE_TABLE (Event Code 0x02)

Generated when hardware detects an invalid DTE during a transaction.

```
word[0]: [0:15]=0x02, [16:31] Reserved
word[1]: [0:15] DeviceId (BDF), [16:31] Reserved
word[2]: [0:31] Reserved
word[3]: [0:31] Reserved
```

#### INVALIDATE_COMMAND (Event Code 0x03)

Generated when an invalid command is detected in the command buffer.

```
word[0]: [0:15]=0x03, [16:31] Reserved
word[1]: [0:15] Reserved, [16:31] Reserved
word[2]: [0:31] Physical address of the illegal command (low)
word[3]: [0:31] Physical address of the illegal command (high)
```

#### COMMAND_HARDWARE_ERROR (Event Code 0x05)

Hardware error during command processing.

```
word[0]: [0:15]=0x05, [16:31] Error flags
word[1]: [0:31] Error address or type
word[2]: [0:31] Error address low
word[3]: [0:31] Error address high
```

### 1.6 IVRS ACPI Table

The IVRS (I/O Virtualization Reporting Structure) is the ACPI table that describes AMD IOMMU topology. Found by scanning ACPI tables with signature "IVRS" (0x56534949).

#### IVRS Header (36 bytes)

```
Offset  Size  Field              Description
0x00    4     Signature          "IVRS" (0x56534949)
0x04    4     Length             Total table length in bytes
0x08    1     Revision           2 = revision 2 (AMD-Vi), 3 = revision 3
0x09    1     Checksum           ACPI checksum (sum of all bytes = 0)
0x0A    6     OemId              OEM identifier
0x10    8     OemTableId         OEM table identifier
0x18    4     OemRevision        OEM revision
0x1C    4     CreatorId          ASL compiler vendor
0x20    4     CreatorRevision    ASL compiler revision
0x24    4     IvInfo             IOMMU Virtualization Info:
                                    [0:7]   = Virtualization Spec Revision (40 = rev 4.0)
                                    [8:9]   = EFRSup (Extended Feature Register supported)
                                    [10:11] = Reserved
                                    [31]    = HT AtsResv (HT ATS reserved)
```

#### IVHD Entry (I/O Virtualization Hardware Definition)

Describes a single IOMMU unit. There can be multiple IVHD entries for multiple IOMMUs.

```
Offset  Size  Field              Description
0x00    1     Type               0x10 = IVHD type 10 (rev 2), 0x11 = IVHD type 11 (rev 3, 64-bit)
0x01    1     Flags              Feature flags:
                                    [0] = HtTunEn (HT tunnel enable)
                                    [1] = PassPW (Pass posted writes)
                                    [2] = ResPassPW (Reset PassPW)
                                    [3] = Isoc (Isoc support)
                                    [4] = IotlbSup (IOTLB support)
                                    [5] = Coherent (Coherent IOMMU)
                                    [6] = PrefSup (Prefetch support)
                                    [7] = PPRSup (PPR support)
0x02    2     Length             Total length of this IVHD entry including device entries
0x04    2     DeviceId           BDF of the IOMMU PCI device
0x06    2     CapabilityOffset   PCI capability offset for IOMMU capability block
0x08    8     IOMMUBaseAddress   Physical MMIO base address of IOMMU registers
                                (type 10: bits 0:51 valid, type 11: full 64-bit)
0x10    2     PciSegmentGroup    PCI segment group number
0x12    2     IommuInfo          IOMMU Info:
                                    [0:5]   = MSI number for event log
                                    [6:12]  = Unit ID (IOMMU hardware unit ID)
                                    [13:15] = Reserved
0x14    4     IommuEfr           Extended Feature Register attributes (type 11 only)
0x18    ...   DeviceEntries      Variable-length device entry list follows
```

#### IVHD Device Entry Types

Each device entry in an IVHD starts with a type byte followed by data.

| Type | Name | Size | Description |
|------|------|------|-------------|
| 0x00 | IVHD_ALL | 4 | Select all devices (except those listed in other entries). Data = all zeros. |
| 0x01 | IVHD_SEL | 4 | Select a single device. Bytes 2:3 = DeviceId (BDF). Byte 4 = Data (LSA flags). |
| 0x02 | IVHD_SOR | 4 | Start of Range. Bytes 2:3 = first DeviceId in range. |
| 0x03 | IVHD_EOR | 4 | End of Range. Bytes 2:3 = last DeviceId in range. |
| 0x42 | IVHD_PAD4 | 8 | 4-byte PAD entry (reserved extension). |
| 0x43 | IVHD_PAD8 | 12 | 8-byte PAD entry (reserved extension). |
| 0x44 | IVHD_VAR | Variable | Variable-length entry. Byte 1 = length. Used for alias, extended selections. |

#### IVHD Device Entry Data Byte

```
Bits of the Data byte in IVHD_SEL/IVHD_SOR:
  [0]   Lint0Pass  — LINT0 remapping passthrough
  [1]   Lint1Pass  — LINT1 remapping passthrough
  [2]   SysMgt     — System Management:
                       00 = No system management
                       01 = System Management at request level
                       10 = System Management at fault level
  [3]   SysMgt     — (continued)
  [4]   NMIPass    — NMI remapping passthrough
  [5]   ExtIntPass — External Interrupt remapping passthrough
  [6]   InitPass   — INIT remapping passthrough
  [7]   Rsvd       — Reserved
```

#### IVMD Entry (I/O Virtualization Memory Definition)

Describes a memory region that has special IOMMU handling. Appears after IVHD entries.

```
Offset  Size  Field              Description
0x00    1     Type               0x20 = IVMD type 20 (rev 2), 0x21 = IVMD type 21 (rev 3)
0x01    1     Flags              Memory block flags:
                                    [0] = Unity (untranslated/unity mapping)
                                    [1] = Read (device may read)
                                    [2] = Write (device may write)
                                    [3] = ExclRange (exclusion range)
0x02    2     Length             Total length of this IVMD entry (16 or 24 bytes)
0x04    2     DeviceId           Start DeviceId (BDF) or 0x0000 for all devices
0x06    2     AuxData            Auxiliary data (reserved in most implementations)
0x08    8     StartAddress       Physical start address of the memory region (type 20: 32-bit in low bits)
0x10    8     MemoryLength       Length of the memory region in bytes (type 20: 32-bit in low bits)
```

### 1.7 Page Table Entry (PTE)

AMD-Vi page tables use multi-level radix tree. The number of levels is set by the DTE Mode field (1 to 6 levels). Each PTE is 64 bits.

```
PTE (64 bits):
  [0]    PR      — Present. 1 = this entry maps a valid page or points to next level.
  [1]    U       — User/Supervisor. 1 = accessible from user level. (only with NXSup)
  [2]    IW      — Write permission. 1 = device may write to this page.
  [3]    IR      — Read permission. 1 = device may read this page.
  [4:8]  Rsvd    — Reserved. Must be zero.
  [9:11] NextLevel — Next page table level (0=PTE/leaf, 1=PDE, 2=PDPTE, 3=PML4E, 4=PML5E).
                      At leaf level (PR=1, NextLevel=0): bits 12:51 = physical page frame.
                      At non-leaf level (PR=1, NextLevel>0): bits 12:51 = next table address.
  [12:51] OutputAddr — Physical address of page frame (leaf) or next-level table (non-leaf).
                         Must be 4KiB-aligned (bits 0:11 assumed zero).
  [51:58] Rsvd    — Reserved. Must be zero.
  [59]    FC      — Force Coherent. 1 = force coherent transactions for this page.
  [60]    Rsvd    — Reserved.
  [61]    IR      — Interrupt Remap (alias in page tables, platform-specific).
  [62]    IW      — Interrupt Write (alias in page tables, platform-specific).
  [63]    NX      — No-Execute. 1 = instruction fetches from this page are blocked (only with NXSup).
```

**Level-to-address-bits mapping**:

| Levels | Address Bits | Max Physical Address |
|--------|-------------|---------------------|
| 1 | 21 | 2 MiB |
| 2 | 30 | 1 GiB |
| 3 | 39 | 512 GiB |
| 4 | 48 | 256 TiB |
| 5 | 57 | 128 PiB |
| 6 | 63 | ~8 EiB |

**Linux page table macros** (`drivers/iommu/amd/amd_iommu_types.h`):

```c
#define PM_LEVEL_SHIFT  9
#define PM_LEVEL_SIZE   (1UL << PM_LEVEL_SHIFT)
#define PM_LEVEL_INDEX(level, address) \
    (((address) >> (12 + (((level) - 1) * 9))) & 0x1FF)
#define PM_LEVEL_ENC(level, address) \
    ((address) | (((level) - 1) << 9) | 1ULL)  // PR=1, NextLevel=level-1
#define PM_PTE_LEVEL(pte)   (((pte) >> 9) & 0x7)
```

### 1.8 Initialization Sequence (AMD-Vi)

Step-by-step register programming to bring up AMD-Vi IOMMU.

```
Step 1: Discover IOMMU hardware
  - Scan ACPI tables for IVRS signature
  - Parse IVHD entries to find MMIO base address
  - Read ExtendedFeature (0x0030) to determine capabilities

Step 2: Disable IOMMU (ensure clean state)
  - Control = 0x00000000  (IOMMUEnable=0, all features off)
  - Wait until Status[0] (IOMMURunning) = 0

Step 3: Allocate and zero Device Table
  - Alloc 2 MiB contiguous physical memory (65536 × 32 bytes)
  - Zero all entries
  - Write DevTableBar (0x0000):
      Bits 0:8   = DevTableSize (0x0F for 65536 entries: 2^(0x0F+1) = 65536)
      Bits 12:51 = Physical address of table

Step 4: Allocate and zero Command Buffer
  - Alloc 8192 bytes contiguous physical (512 entries × 16 bytes)
  - Zero all entries
  - Write CmdBufBar (0x0008):
      Bits 0:8   = CmdBufLen (0x08 for 512 entries: 2^(0x08+2) = 4096 bytes... use 0x09 for 8192)
      Bits 12:51 = Physical address

Step 5: Allocate and zero Event Log
  - Alloc 8192 bytes contiguous physical (512 entries × 16 bytes)
  - Zero all entries
  - Write EvtLogBar (0x0010):
      Bits 0:8   = EvtLogLen (0x09 for 8192 bytes)
      Bits 12:51 = Physical address

Step 6: Set up exclusion range (optional)
  - Write ExclusionBase (0x0020) = start of excluded physical range
  - Write ExclusionLimit (0x0028) = end of excluded physical range
  - Skip if no exclusion needed

Step 7: Reset head/tail pointers
  - CmdBufHead (0x2000) = 0
  - CmdBufTail (0x2008) = 0
  - EvtLogHead (0x2010) = 0
  - (EvtLogTail is RO, hardware sets it)

Step 8: Allocate and zero Interrupt Remap Table (if IR needed)
  - Alloc 4096 × 16 bytes = 64 KiB (for IntTabLen=11, max 4096 entries)
  - Zero all entries
  - Configure each device's DTE with IRTP pointing to this table

Step 9: Configure DTEs for devices
  - For each device that needs translation:
      Set V=1, TV=1, Mode=4 (4-level), PTP=root page table address
      Set IR=1, IW=1 if interrupt remapping is used
      Set IntCtl=0x02 (remapped), IntTabLen, IRTP

Step 10: Enable features in Control register
  - Control = 0x00000000 | bits for enabled features:
      Bit 2  (EventLogEn)    = 1
      Bit 5  (CmdBufEn)      = 1
      Bit 22 (XTEn)          = 1  (if x2APIC supported and in use)
      Bit 23 (NXEn)          = 1  (if NX supported)
  - DO NOT set bit 0 (IOMMUEnable) yet

Step 11: Flush caches via command buffer
  - Submit INVALIDATE_IOMMU_ALL (0x05) if supported, or:
      INVALIDATE_DEVTAB_ENTRY for each modified device
      INVALIDATE_INTERRUPT_TABLE for each device with IR
  - Submit COMPLETION_WAIT (0x01) to synchronize
  - Wait for completion

Step 12: Enable IOMMU translations
  - Set Control bit 0 (IOMMUEnable) = 1
  - Read Status to verify IOMMURunning = 1

Step 13: Enable interrupts (optional)
  - Set Control bit 3 (EventIntEn) = 1
  - Configure MSI delivery for the IOMMU PCI device
```

---

## 2. Intel VT-d

### 2.1 MMIO Register Map

Base address obtained from ACPI DMAR table (DRHD entry `RegisterBase` field).

| Offset | Name | Size | Access | Description |
|--------|------|------|--------|-------------|
| 0x00 | VER_REG | 32-bit | RO | Architecture Version. [0:7] = Minor, [8:15] = Major. |
| 0x08 | CAP_REG | 64-bit | RO | Capability Register. See bit layout below. |
| 0x10 | ECAP_REG | 64-bit | RO | Extended Capability Register. See bit layout below. |
| 0x18 | GCMD_REG | 32-bit | WO | Global Command Register. Write to request operations. |
| 0x1C | GSTS_REG | 32-bit | RO | Global Status Register. Reflects GCMD results. |
| 0x20 | RTADDR_REG | 64-bit | R/W | Root Table Address. Bit 0 = RTT (Root Table Type: 0=legacy, 1=extended). Bits 12:63 = physical address. |
| 0x28 | CCMD_REG | 64-bit | R/W | Context Command Register. For invalidating context caches. |
| 0x30 | FSTS_REG | 32-bit | RO | Fault Status Register. |
| 0x34 | FECTL_REG | 32-bit | R/W | Fault Event Control Register. |
| 0x38 | FEDATA_REG | 32-bit | R/W | Fault Event Data Register. MSI data. |
| 0x3C | FEADDR_REG | 32-bit | R/W | Fault Event Address Register. MSI address low. |
| 0x40 | FEUADDR_REG | 32-bit | R/W | Fault Event Upper Address Register. MSI address high. |
| 0x48 | AFLOG_REG | 64-bit | R/W | Advanced Fault Log Register. |
| 0x58 | PMEN_REG | 32-bit | R/W | Protected Memory Enable Register. |
| 0x5C | PLMBASE_REG | 32-bit | R/W | Protected Low Memory Base Register. |
| 0x60 | PLMLIMIT_REG | 32-bit | R/W | Protected Low Memory Limit Register. |
| 0x68 | PHMBASE_REG | 64-bit | R/W | Protected High Memory Base Register. |
| 0x70 | PHMLIMIT_REG | 64-bit | R/W | Protected High Memory Limit Register. |
| 0x78 | IQH_REG | 64-bit | RO | Invalidation Queue Head Register. |
| 0x80 | IQT_REG | 64-bit | R/W | Invalidation Queue Tail Register. |
| 0x88 | IQA_REG | 64-bit | R/W | Invalidation Queue Address Register. |
| 0x90 | ICS_REG | 32-bit | RO | Invalidation Completion Status Register. |
| 0x94 | IECTL_REG | 32-bit | R/W | Invalidation Event Control Register. |
| 0x98 | IEDATA_REG | 32-bit | R/W | Invalidation Event Data Register. |
| 0x9C | IEADDR_REG | 32-bit | R/W | Invalidation Event Address Register. |
| 0xA0 | IEUADDR_REG | 32-bit | R/W | Invalidation Event Upper Address Register. |
| 0xB0 | IRTA_REG | 64-bit | R/W | Interrupt Remapping Table Address Register. |

#### CAP_REG (0x08) Bit Layout

| Bit | Name | Description |
|-----|------|-------------|
| 0 | ND (bits 0:2) | Number of Domains Supported. 0=4, 1=16, 2=64, 3=256, 4=1024, 5=4K, 6=16K, 7=64K. |
| 3:7 | ZLR | Zero Length Read. 1 = supported. |
| 8 | AFL | Advanced Fault Logging. 1 = supported. |
| 9 | RWBF | Required Write-Buffer Flushing. 1 = software must flush write buffers before invalidations. |
| 10:11 | PLMR | Protected Low Memory Region. 1 = supported. |
| 12:13 | PHMR | Protected High Memory Region. 1 = supported. |
| 14 | CM | Caching Mode. 1 = IOMMU operates in caching mode (no explicit invalidation needed). |
| 15:23 | SAGAW | Supported Adjusted Guest Address Widths. Bit N set = (N+1)-level page tables supported. |
| 24:33 | MGAW | Maximum Guest Address Width. Actual address width = MGAW + 1. |
| 34:35 | MAMV | Maximum Address Mask Value. For interrupt remapping. |
| 36 | ZAM | Zero Address/Mask. For interrupt remapping. |
| 37:39 | Rsvd | Reserved. |
| 40 | FL1GP | First Level 1-GByte Page Support. |
| 41:43 | Rsvd | Reserved. |
| 44 | PSI | Page Selective Invalidation. 1 = supported. |
| 45:51 | Rsvd | Reserved. |
| 52 | SPS | Super Page Support. Bits indicate 2MiB, 1GiB, 512GiB support. |
| 52:55 | FR | Fault Recording Register count minus 1. |
| 56:60 | Rsvd | Reserved. |
| 61:63 | Rsvd | Reserved. |

#### ECAP_REG (0x10) Bit Layout

| Bit | Name | Description |
|-----|------|-------------|
| 0 | C | Page Request (PRI) support. |
| 1 | QI | Queued Invalidation support. 1 = IQ mechanism supported. |
| 2 | DT | Device TLB support. |
| 3 | IR | Interrupt Remapping support. 1 = supported. |
| 4 | EIM | Extended Interrupt Mode. 1 = x2APIC mode supported for IR. |
| 5:7 | Rsvd | Reserved. |
| 8 | PT | Pass Through. 1 = second-level translation bypass supported. |
| 9:17 | Rsvd | Reserved. |
| 18 | SC | Snoop Control. |
| 19:24 | Rsvd | Reserved. |
| 25:34 | IRO | IOTLB Register Offset. Offset from base for IOTLB registers. |
| 35:43 | Rsvd | Reserved. |
| 44:47 | MHMV | Maximum Handle Mask Value. |
| 48 | ECS | Extended Context Support. |
| 49 | MTS | Memory Type Support. |
| 50 | NEST | Nested Translation Support. |
| 51:63 | Rsvd | Reserved. |

#### GCMD_REG (0x18) Bit Layout (Write-Only)

| Bit | Name | Description |
|-----|------|-------------|
| 31 | TE | Translation Enable. Write 1 to enable/disable. |
| 30 | SRTP | Set Root Table Pointer. Write 1, hardware sets GSTS.RTPS when done. |
| 29 | SFL | Set Fault Log. Write 1 to set fault log pointer. |
| 28 | EAFL | Enable Advanced Fault Log. |
| 27 | WBF | Write Buffer Flush. Write 1, hardware sets GSTS.WBFS when done. |
| 26 | QIE | Queued Invalidation Enable. Write 1 to enable. |
| 25 | SIRTP | Set Interrupt Remap Table Pointer. Write 1, hardware sets GSTS.IRTPS. |
| 24 | CFI | Compatibility Format Interrupt. Write 1 to block compatibility interrupts. |
| 23 | IR | Interrupt Remap. Write 1 to enable interrupt remapping. |
| 0:22 | Rsvd | Reserved. Must write zero. |

#### GSTS_REG (0x1C) Bit Layout (Read-Only)

| Bit | Name | Description |
|-----|------|-------------|
| 31 | TES | Translation Enable Status. 1 = enabled. |
| 30 | RTPS | Root Table Pointer Status. 1 = root table pointer set. |
| 29 | FLS | Fault Log Status. |
| 28 | AFLS | Advanced Fault Log Status. |
| 27 | WBFS | Write Buffer Flush Status. 1 = flush complete. |
| 26 | QIES | Queued Invalidation Enable Status. |
| 25 | IRTPS | Interrupt Remap Table Pointer Status. |
| 24 | CFIS | Compatibility Format Interrupt Status. |
| 23 | IRES | Interrupt Remap Enable Status. |
| 0:22 | Rsvd | Reserved. |

### 2.2 Root Table Entry

The Root Table is pointed to by RTADDR_REG. It contains 256 entries (one per PCI bus). Each entry is 128 bits (16 bytes). Must be 4KiB-aligned.

```
Root Entry (128 bits = data[0] data[1], each u64):

data[0]:
  [0]    P       — Present. 1 = this bus has context entries.
  [1:63] CTP     — Context Table Pointer. Physical address of the context table
                    for this bus. Bits 12:63 hold address. Must be 4KiB-aligned.

data[1]:
  [0:63] Rsvd    — Reserved. Must be zero.
```

### 2.3 Context Entry

Each Context Table contains 256 entries (one per device:function on a bus). Each entry is 128 bits (16 bytes).

```
Context Entry (128 bits = data[0] data[1], each u64):

data[0]:
  [0]    P       — Present. 1 = entry is valid.
  [1]    FPD     — Fault Processing Disable. 1 = faults from this device are suppressed.
  [2:3]  TT      — Translation Type:
                     00 = Legacy mode (second-level translation only)
                     01 = PASID-granular translation
                     10 = Pass-through (no second-level translation, bypass)
                     11 = Reserved
  [4:11] Rsvd    — Reserved.
  [12:63] SLPTPTR — Second Level Page Table Pointer. Physical address of the
                      second-level (guest) page table root. Must be 4KiB-aligned.

data[1]:
  [0:15] DID     — Domain Identifier. Associates this device with a domain.
  [16:63] Rsvd   — Reserved. Must be zero.
```

**Extended Context Entry** (when ECS=1 in ECAP):

```
data[0]:
  [0]    P       — Present
  [1]    FPD     — Fault Processing Disable
  [2:3]  TT      — Translation Type (same as above)
  [4:11] Rsvd
  [12:63] SLPTPTR — Page table pointer (same as above)

data[1]:
  [0:15] DID     — Domain Identifier
  [16:19] AW     — Address Width. 0=3-level, 1=4-level, 2=5-level, 3=6-level.
  [20:63] Rsvd   — Reserved
```

### 2.4 DMAR ACPI Table

The DMAR (DMA Remapping) table describes Intel VT-d IOMMU topology. Found by scanning ACPI tables with signature "DMAR" (0x52414D44).

#### DMAR Header (48 bytes)

```
Offset  Size  Field              Description
0x00    4     Signature          "DMAR" (0x52414D44)
0x04    4     Length             Total table length in bytes
0x08    1     Revision           1
0x09    1     Checksum           ACPI checksum (sum of all bytes = 0)
0x0A    6     OemId              OEM identifier
0x10    8     OemTableId         OEM table identifier
0x18    4     OemRevision        OEM revision
0x1C    4     CreatorId          ASL compiler vendor
0x20    4     CreatorRevision    ASL compiler revision
0x24    1     HostAddressWidth   DMA physical address width (e.g., 46 for 64 TiB)
0x25    1     Flags              [0] = INTR_REMAP (interrupt remapping supported)
                                    [1] = X2APIC_OPT_OUT (firmware requests no x2APIC)
0x26    6     Reserved           Reserved
0x2C    ...   RemappingStructures  Variable-length list of DRHD/RMRR/ATSR/etc entries
```

#### DRHD (DMA Remapping Hardware Unit Definition)

Describes a single IOMMU unit. Multiple DRHD entries for systems with multiple IOMMUs.

```
Offset  Size  Field              Description
0x00    2     Type               0x0001 = DRHD
0x02    2     Length             Total length of this entry including device scope
0x04    1     Flags              [0] = INCLUDE_PCI_ALL (1=this IOMMU handles all PCI devices
                                     not covered by other non-ALL DRHD entries)
0x05    1     Reserved           Reserved
0x06    2     SegmentNumber      PCI Segment Group number
0x08    8     RegisterBaseAddress Physical MMIO base address of IOMMU registers
0x10    ...   DeviceScope        Variable-length device scope entries follow
```

#### DRHD Device Scope Entry

```
Offset  Size  Field              Description
0x00    1     Type               Device scope type:
                                    0x01 = PCI Endpoint Device
                                    0x02 = PCI SubHierarchy
                                    0x03 = IOAPIC
                                    0x04 = MSI Capable HPET
                                    0x05 = ACPI Name-Space Device
0x01    1     Length             Total length of this scope entry
0x02    1     EnumerationId     Enumeration ID (e.g., IOAPIC ID for type 0x03)
0x03    1     StartBusNumber    Starting PCI bus number
0x04    ...   Path               PCI path entries (each 2 bytes: Device, Function)
```

#### RMRR (Reserved Memory Region Reporting)

Describes memory regions that must be identity-mapped for specific devices (e.g., USB controllers, graphics).

```
Offset  Size  Field              Description
0x00    2     Type               0x0002 = RMRR
0x02    2     Length             Total length of this entry
0x04    2     Reserved           Reserved
0x06    2     SegmentNumber      PCI Segment Group
0x08    8     BaseAddress        Physical start address of reserved region
0x10    8     EndAddress         Physical end address of reserved region (inclusive)
0x18    ...   DeviceScope        Device scope entries for devices that access this region
```

#### Other DMAR Sub-Table Types

| Type | Name | Description |
|------|------|-------------|
| 0x0000 | Reserved | Reserved. |
| 0x0001 | DRHD | DMA Remapping Hardware Unit Definition. |
| 0x0002 | RMRR | Reserved Memory Region Reporting. |
| 0x0003 | ATSR | Root Port ATS (Address Translation Service) Capability Reporting. |
| 0x0004 | RHSA | Remapping Hardware Static Affinity (NUMA locality). |
| 0x0005 | ANDD | ACPI Name-space Device Declaration. |

### 2.5 Page Table Entry (Intel VT-d)

Intel VT-d uses multi-level page tables. The number of levels depends on SAGAW in CAP_REG. Typically 3 or 4 levels. Each PTE is 64 bits.

```
PTE (64 bits):
  [0]    R       — Read permission. 1 = device may read.
  [1]    W       — Write permission. 1 = device may write.
  [2:11] Rsvd    — Reserved. Must be zero unless extended features.
  [12:63] ADDR   — Physical address. For non-leaf: next-level table address (4KiB-aligned).
                    For leaf: page frame address.
                    Mask depends on page size:
                      4KiB:  bits 12:63
                      2MiB:  bits 21:63 (super page)
                      1GiB:  bits 30:63 (super page)
```

**Extended PTE with Supervisor bit** (when CAP_REG supports it):

```
  [2]    S       — Supervisor. 1 = supervisor-mode page.
  [3]    AW      — Access/Dirty (for first-level translation).
  [4]    PSE     — Page Size Extension (1 = super page at this level).
  [5]    A       — Accessed flag.
  [6]    D       — Dirty flag.
  [7:11] Rsvd    — Reserved.
```

### 2.6 Initialization Sequence (Intel VT-d)

Step-by-step register programming to bring up Intel VT-d.

```
Step 1: Discover IOMMU hardware
  - Scan ACPI tables for DMAR signature
  - Parse DRHD entries to find MMIO base addresses
  - Read CAP_REG (0x08) for capabilities
  - Read ECAP_REG (0x10) for extended capabilities
  - Read VER_REG (0x00) for architecture version

Step 2: Ensure IOMMU is disabled
  - Verify GSTS_REG.TES = 0 (translation not enabled)
  - If TES=1, write GCMD_REG with TE=0, wait for TES to clear

Step 3: Allocate and zero Root Table
  - Alloc 4 KiB (256 entries × 16 bytes)
  - Zero all entries
  - Write RTADDR_REG (0x20):
      Bit 0 = 0 (legacy root table type)
      Bits 12:63 = physical address

Step 4: Set Root Table Pointer
  - Write GCMD_REG bit 30 (SRTP) = 1
  - Poll GSTS_REG bit 30 (RTPS) until it reads 1

Step 5: Allocate and zero Context Tables (per bus)
  - For each bus with devices:
      Alloc 4 KiB (256 entries × 16 bytes)
      Zero all entries
      Set Root Entry P=1, CTP=context table address

Step 6: Configure Context Entries
  - For each device:function:
      Set P=1, TT=00 (legacy), SLPTPTR=page table root, DID=domain ID
  - For pass-through: Set P=1, TT=10, DID=domain ID

Step 7: Build page tables (per domain)
  - Create page table hierarchy matching SAGAW levels (typically 3 or 4)
  - Map device-visible physical addresses to host physical addresses
  - For identity mapping: GPA = HPA

Step 8: Handle RMRR regions
  - Identity-map all RMRR regions for their respective devices
  - These regions must always be accessible to the listed devices

Step 9: Allocate Interrupt Remap Table (if ECAP_REG.IR=1)
  - Alloc table: 2^(IRTA_REG.TableSize+1) × 16 bytes
  - Zero all entries
  - Write IRTA_REG (0xB0):
      Bits 0:6   = TableSize (e.g., 0xF = 65536 entries)
      Bits 6:7   = IRTE Mode (00=remapped, 01=posted)
      Bits 12:63 = Physical address
      Bit 4      = EIME (Extended Interrupt Mode Enable) if x2APIC

Step 10: Enable Interrupt Remapping
  - Write GCMD_REG bit 25 (SIRTP) = 1
  - Poll GSTS_REG bit 25 (IRTPS) until 1
  - Write GCMD_REG bit 24 (CFI) = 1 to block compatibility format interrupts
  - Poll GSTS_REG bit 24 (CFIS) until 1
  - Write GCMD_REG bit 23 (IR) = 1
  - Poll GSTS_REG bit 23 (IRES) until 1

Step 11: Invalidate caches
  - If QI (Queued Invalidation) supported (ECAP_REG.QI=1):
      Set up Invalidation Queue (IQA_REG)
      Submit queue-based invalidation descriptors
  - Else use register-based invalidation:
      Write CCMD_REG for context cache invalidation
      Write IOTLB registers for TLB invalidation

Step 12: Enable translation
  - Write GCMD_REG bit 31 (TE) = 1
  - Poll GSTS_REG bit 31 (TES) until 1

Step 13: Enable fault handling
  - Program FEDATA_REG, FEADDR_REG, FEUADDR_REG for MSI delivery
  - Write FECTL_REG to enable fault interrupts
```

---

## 3. Rust Struct Definitions

These `#[repr(C, packed)]` structs can be used directly in the Red Bear OS IOMMU implementation. All bitfield access should go through helper methods (shown below) to ensure correct masking.

### 3.1 AMD-Vi Structs

```rust
// AMD-Vi MMIO Registers

/// AMD-Vi IOMMU MMIO register block.
/// Base address from ACPI IVRS IVHD entry.
#[repr(C)]
pub struct AmdViMmio {
    pub dev_table_bar: u64,        // 0x0000
    pub cmd_buf_bar: u64,          // 0x0008
    pub evt_log_bar: u64,          // 0x0010
    pub control: u32,              // 0x0018
    _pad0: u32,                    // 0x001C
    pub exclusion_base: u64,       // 0x0020
    pub exclusion_limit: u64,      // 0x0028
    pub extended_feature: u64,     // 0x0030
    pub ppr_log_bar: u64,          // 0x0038
    _pad1: [u64; 0x03F0],          // 0x0040..0x1FFC (padding to 0x2000)
    pub cmd_buf_head: u64,         // 0x2000
    pub cmd_buf_tail: u64,         // 0x2008
    pub evt_log_head: u64,         // 0x2010
    pub evt_log_tail: u64,         // 0x2018
    pub status: u32,               // 0x2020
}
// Static assertions for offset verification
const _: () = assert!(core::mem::offset_of!(AmdViMmio, dev_table_bar) == 0x0000);
const _: () = assert!(core::mem::offset_of!(AmdViMmio, control) == 0x0018);
const _: () = assert!(core::mem::offset_of!(AmdViMmio, cmd_buf_head) == 0x2000);

/// AMD-Vi Control Register bits.
pub mod amd_control {
    pub const IOMMU_ENABLE: u32 = 1 << 0;
    pub const HT_TUN_EN: u32 = 1 << 1;
    pub const EVENT_LOG_EN: u32 = 1 << 2;
    pub const EVENT_INT_EN: u32 = 1 << 3;
    pub const COM_WAIT_INT_EN: u32 = 1 << 4;
    pub const CMD_BUF_EN: u32 = 1 << 5;
    pub const PPR_LOG_EN: u32 = 1 << 6;
    pub const PPR_INT_EN: u32 = 1 << 7;
    pub const PPR_EN: u32 = 1 << 8;
    pub const GT_EN: u32 = 1 << 9;
    pub const GA_EN: u32 = 1 << 10;
    pub const XT_EN: u32 = 1 << 22;
    pub const NX_EN: u32 = 1 << 23;
}

/// AMD-Vi Status Register bits.
pub mod amd_status {
    pub const IOMMU_RUNNING: u32 = 1 << 0;
    pub const EVENT_OVERFLOW: u32 = 1 << 1;
    pub const EVENT_LOG_INT: u32 = 1 << 2;
    pub const COM_WAIT_INT: u32 = 1 << 3;
    pub const PPR_OVERFLOW: u32 = 1 << 4;
    pub const PPR_INT: u32 = 1 << 5;
}

/// AMD-Vi Extended Feature Register bits.
pub mod amd_ext_feature {
    pub const PREF_SUP: u64 = 1 << 0;
    pub const PPR_SUP: u64 = 1 << 1;
    pub const XT_SUP: u64 = 1 << 2;
    pub const NX_SUP: u64 = 1 << 3;
    pub const GT_SUP: u64 = 1 << 4;
    pub const IA_SUP: u64 = 1 << 6;
    pub const GA_SUP: u64 = 1 << 7;
    pub const HE_SUP: u64 = 1 << 8;
    pub const PC_SUP: u64 = 1 << 9;
    pub const GI_SUP: u64 = 1 << 57;
}

/// AMD-Vi Device Table Entry (256 bits = 32 bytes).
/// Index by BDF: (bus << 8) | (dev << 3) | func.
/// Table holds up to 65536 entries.
#[repr(C, packed)]
pub struct AmdDte {
    pub data: [u64; 4],
}

impl AmdDte {
    /// Create a zeroed (invalid) DTE.
    pub const fn zeroed() -> Self {
        Self { data: [0; 4] }
    }

    // data[0] accessors

    pub fn valid(&self) -> bool {
        self.data[0] & (1 << 0) != 0
    }

    pub fn set_valid(&mut self, v: bool) {
        if v { self.data[0] |= 1 << 0; } else { self.data[0] &= !(1 << 0); }
    }

    pub fn translation_valid(&self) -> bool {
        self.data[0] & (1 << 1) != 0
    }

    pub fn set_translation_valid(&mut self, v: bool) {
        if v { self.data[0] |= 1 << 1; } else { self.data[0] &= !(1 << 1); }
    }

    /// Translation mode (bits 9:11). 0=no translation, 4=4-level page table.
    pub fn mode(&self) -> u64 {
        (self.data[0] >> 9) & 0x7
    }

    pub fn set_mode(&mut self, m: u64) {
        self.data[0] = (self.data[0] & !(0x7 << 9)) | ((m & 0x7) << 9);
    }

    /// Page Table Root Pointer (bits 12:51 of data[0]).
    /// Address must be 4KiB-aligned.
    pub fn page_table_root(&self) -> u64 {
        (self.data[0] >> 12) & 0x000F_FFFF_FFFF_FFFF
    }

    pub fn set_page_table_root(&mut self, addr: u64) {
        self.data[0] = (self.data[0] & !(0x000F_FFFF_FFFF_FFFF << 12))
                      | ((addr >> 12) << 12);
    }

    /// Interrupt Remapping Enable (bit 61 of data[0]).
    pub fn interrupt_remap(&self) -> bool {
        self.data[0] & (1 << 61) != 0
    }

    pub fn set_interrupt_remap(&mut self, v: bool) {
        if v { self.data[0] |= 1 << 61; } else { self.data[0] &= !(1 << 61); }
    }

    /// Interrupt Write permission (bit 62 of data[0]).
    pub fn interrupt_write(&self) -> bool {
        self.data[0] & (1 << 62) != 0
    }

    pub fn set_interrupt_write(&mut self, v: bool) {
        if v { self.data[0] |= 1 << 62; } else { self.data[0] &= !(1 << 62); }
    }

    // data[1] accessors

    /// Interrupt Remap Table Length (bits 0:3 of data[1]).
    /// Number of IRTEs = 2^(len+1).
    pub fn int_table_len(&self) -> u64 {
        self.data[1] & 0xF
    }

    pub fn set_int_table_len(&mut self, len: u64) {
        self.data[1] = (self.data[1] & !0xF) | (len & 0xF);
    }

    /// Interrupt Control (bits 4:5 of data[1]).
    /// 00=abort, 01=pass-through, 10=remapped.
    pub fn int_control(&self) -> u64 {
        (self.data[1] >> 4) & 0x3
    }

    pub fn set_int_control(&mut self, ctl: u64) {
        self.data[1] = (self.data[1] & !(0x3 << 4)) | ((ctl & 0x3) << 4);
    }

    /// Interrupt Remap Table Pointer (bits 6:51 of data[1]).
    /// Address must be 4KiB-aligned.
    pub fn int_remap_table_ptr(&self) -> u64 {
        (self.data[1] >> 6) & 0x000F_FFFF_FFFF_FFFF
    }

    pub fn set_int_remap_table_ptr(&mut self, addr: u64) {
        self.data[1] = (self.data[1] & !(0x000F_FFFF_FFFF_FFFF << 6))
                       | ((addr >> 6) << 6);
    }
}

const _: () = assert!(core::mem::size_of::<AmdDte>() == 32);

/// AMD-Vi Interrupt Remapping Table Entry (128 bits = 16 bytes).
#[repr(C, packed)]
pub struct AmdIrte {
    pub data: [u64; 2],
}

impl AmdIrte {
    pub const fn zeroed() -> Self {
        Self { data: [0; 2] }
    }

    /// Remap enable (bit 0 of data[0]).
    pub fn remap_enabled(&self) -> bool {
        self.data[0] & (1 << 0) != 0
    }

    pub fn set_remap_enabled(&mut self, v: bool) {
        if v { self.data[0] |= 1 << 0; } else { self.data[0] &= !(1 << 0); }
    }

    /// Suppress IO Page Fault (bit 1).
    pub fn suppress_io_pf(&self) -> bool {
        self.data[0] & (1 << 1) != 0
    }

    pub fn set_suppress_io_pf(&mut self, v: bool) {
        if v { self.data[0] |= 1 << 1; } else { self.data[0] &= !(1 << 1); }
    }

    /// Interrupt type (bits 2:4 of data[0]).
    pub fn int_type(&self) -> u64 {
        (self.data[0] >> 2) & 0x7
    }

    pub fn set_int_type(&mut self, t: u64) {
        self.data[0] = (self.data[0] & !(0x7 << 2)) | ((t & 0x7) << 2);
    }

    /// Destination mode (bit 2 of data[0], when using xAPIC logical).
    /// 0=physical APIC ID, 1=logical.
    pub fn dst_mode(&self) -> bool {
        self.data[0] & (1 << 2) != 0
    }

    /// Destination APIC ID (bits 16:31 of data[0], low 16 bits).
    /// For x2APIC, high 32 bits in data[1] bits 0:31.
    pub fn destination(&self) -> u32 {
        ((self.data[0] >> 16) & 0xFFFF) as u32 | ((self.data[1] & 0xFFFF_FFFF) as u32) << 16
    }

    pub fn set_destination(&mut self, apic_id: u32) {
        self.data[0] = (self.data[0] & !(0xFFFF << 16)) | (((apic_id & 0xFFFF) as u64) << 16);
        self.data[1] = (self.data[1] & !0xFFFF_FFFF) | ((apic_id >> 16) as u64);
    }

    /// Vector (bits 32:39 of data[0], but stored in low byte of upper word).
    pub fn vector(&self) -> u8 {
        ((self.data[0] >> 32) & 0xFF) as u8
    }

    pub fn set_vector(&mut self, v: u8) {
        self.data[0] = (self.data[0] & !(0xFF_u64 << 32)) | ((v as u64) << 32);
    }
}

const _: () = assert!(core::mem::size_of::<AmdIrte>() == 16);

/// AMD-Vi Command Buffer Entry (128 bits = 16 bytes = 4 × u32).
#[repr(C, packed)]
pub struct AmdCmdEntry {
    pub word: [u32; 4],
}

impl AmdCmdEntry {
    pub const fn zeroed() -> Self {
        Self { word: [0; 4] }
    }

    pub fn opcode(&self) -> u8 {
        (self.word[0] & 0xF) as u8
    }

    pub fn set_opcode(&mut self, op: u8) {
        self.word[0] = (self.word[0] & !0xF) | (op as u32 & 0xF);
    }
}

const _: () = assert!(core::mem::size_of::<AmdCmdEntry>() == 16);

/// AMD-Vi Command Opcodes.
pub mod amd_cmd_opcode {
    pub const COMPLETION_WAIT: u8 = 0x01;
    pub const INVALIDATE_DEVTAB_ENTRY: u8 = 0x02;
    pub const INVALIDATE_IOMMU_PAGES: u8 = 0x03;
    pub const INVALIDATE_INTERRUPT_TABLE: u8 = 0x04;
    pub const INVALIDATE_IOMMU_ALL: u8 = 0x05;
}

/// Build a COMPLETION_WAIT command.
pub fn amd_cmd_completion_wait(store_addr: u64, store_data: u32) -> AmdCmdEntry {
    let mut cmd = AmdCmdEntry::zeroed();
    cmd.set_opcode(amd_cmd_opcode::COMPLETION_WAIT);
    cmd.word[0] |= 1 << 4; // Store = 1
    cmd.word[1] = store_addr as u32;
    cmd.word[2] = (store_addr >> 32) as u32;
    cmd.word[3] = store_data;
    cmd
}

/// Build an INVALIDATE_DEVTAB_ENTRY command for a given BDF.
pub fn amd_cmd_invalidate_devtab(bdf: u16) -> AmdCmdEntry {
    let mut cmd = AmdCmdEntry::zeroed();
    cmd.set_opcode(amd_cmd_opcode::INVALIDATE_DEVTAB_ENTRY);
    cmd.word[1] = bdf as u32;
    cmd
}

/// Build an INVALIDATE_IOMMU_PAGES command.
/// If size=true, invalidates all pages for the domain (address ignored).
pub fn amd_cmd_invalidate_pages(domain_id: u16, address: u64, size: bool) -> AmdCmdEntry {
    let mut cmd = AmdCmdEntry::zeroed();
    cmd.set_opcode(amd_cmd_opcode::INVALIDATE_IOMMU_PAGES);
    if size { cmd.word[0] |= 1 << 4; } // S bit
    cmd.word[1] = domain_id as u32;
    cmd.word[2] = address as u32;
    cmd.word[3] = (address >> 32) as u32;
    cmd
}

/// Build an INVALIDATE_INTERRUPT_TABLE command.
pub fn amd_cmd_invalidate_int_table(bdf: u16) -> AmdCmdEntry {
    let mut cmd = AmdCmdEntry::zeroed();
    cmd.set_opcode(amd_cmd_opcode::INVALIDATE_INTERRUPT_TABLE);
    cmd.word[1] = bdf as u32;
    cmd
}

/// Build an INVALIDATE_IOMMU_ALL command.
pub fn amd_cmd_invalidate_all() -> AmdCmdEntry {
    let mut cmd = AmdCmdEntry::zeroed();
    cmd.set_opcode(amd_cmd_opcode::INVALIDATE_IOMMU_ALL);
    cmd
}

/// AMD-Vi Event Log Entry (128 bits = 16 bytes = 4 × u32).
#[repr(C, packed)]
pub struct AmdEvtEntry {
    pub word: [u32; 4],
}

impl AmdEvtEntry {
    pub const fn zeroed() -> Self {
        Self { word: [0; 4] }
    }

    /// Event code (bits 0:15 of word[0]).
    pub fn event_code(&self) -> u16 {
        (self.word[0] & 0xFFFF) as u16
    }

    /// Device ID / BDF (bits 0:15 of word[1]).
    pub fn device_id(&self) -> u16 {
        (self.word[1] & 0xFFFF) as u16
    }

    /// Fault address (word[2] | word[3] << 32).
    pub fn fault_address(&self) -> u64 {
        self.word[2] as u64 | ((self.word[3] as u64) << 32)
    }

    /// Flags from word[0] bits 16:22 (for IO_PAGE_FAULT).
    pub fn fault_flags(&self) -> u16 {
        ((self.word[0] >> 16) & 0x7F) as u16
    }

    /// Read/write direction from fault flags bit 4 (RW).
    pub fn is_write(&self) -> bool {
        self.word[0] & (1 << 20) != 0
    }

    /// Permission error from fault flags bit 3 (PE).
    pub fn is_permission_error(&self) -> bool {
        self.word[0] & (1 << 19) != 0
    }
}

const _: () = assert!(core::mem::size_of::<AmdEvtEntry>() == 16);

/// AMD-Vi Event Codes.
pub mod amd_evt_code {
    pub const ILLEGAL_DEV_TABLE_ENTRY: u16 = 0x01;
    pub const IO_PAGE_FAULT: u16 = 0x02;
    pub const DEV_TABLE_HW_ERROR: u16 = 0x03;
    pub const PAGE_TABLE_HW_ERROR: u16 = 0x04;
    pub const ILLEGAL_COMMAND: u16 = 0x05;
    pub const COMMAND_HW_ERROR: u16 = 0x06;
    pub const IOTLB_INV_TIMEOUT: u16 = 0x07;
    pub const INVALID_DEV_REQUEST: u16 = 0x08;
}

/// AMD-Vi Page Table Entry (64 bits).
#[repr(C, packed)]
pub struct AmdPte(pub u64);

impl AmdPte {
    /// Present bit (bit 0).
    pub fn present(&self) -> bool {
        self.0 & (1 << 0) != 0
    }

    pub fn set_present(&mut self, v: bool) {
        if v { self.0 |= 1 << 0; } else { self.0 &= !(1 << 0); }
    }

    /// Next level (bits 9:11). 0 = leaf PTE, 1-5 = pointer to next table.
    pub fn next_level(&self) -> u64 {
        (self.0 >> 9) & 0x7
    }

    pub fn set_next_level(&mut self, level: u64) {
        self.0 = (self.0 & !(0x7 << 9)) | ((level & 0x7) << 9);
    }

    /// Output address (bits 12:51). Physical frame or next-table address.
    pub fn output_addr(&self) -> u64 {
        self.0 & (0x000F_FFFF_FFFF_FFFF << 12)
    }

    pub fn set_output_addr(&mut self, addr: u64) {
        self.0 = (self.0 & !(0x000F_FFFF_FFFF_FFFF << 12)) | (addr & (0x000F_FFFF_FFFF_FFFF << 12));
    }

    /// No-execute (bit 63). Only valid when NXSup=1.
    pub fn no_execute(&self) -> bool {
        self.0 & (1 << 63) != 0
    }

    pub fn set_no_execute(&mut self, v: bool) {
        if v { self.0 |= 1 << 63; } else { self.0 &= !(1 << 63); }
    }
}

/// Build a leaf PTE that maps addr with Read+Write permissions.
pub fn amd_pte_leaf(addr: u64) -> AmdPte {
    let mut pte = AmdPte(0);
    pte.set_present(true);
    pte.set_next_level(0); // leaf
    pte.set_output_addr(addr);
    pte.0 |= (1 << 2) | (1 << 3); // IW + IR (write + read permission)
    pte
}

/// Build a non-leaf PTE that points to the next-level table at addr.
pub fn amd_pte_pointer(addr: u64, level: u64) -> AmdPte {
    let mut pte = AmdPte(0);
    pte.set_present(true);
    pte.set_next_level(level);
    pte.set_output_addr(addr);
    pte
}
```

### 3.2 Intel VT-d Structs

```rust
/// Intel VT-d IOMMU MMIO register block.
/// Base address from ACPI DMAR DRHD entry.
#[repr(C)]
pub struct IntelVtdMmio {
    pub ver_reg: u32,              // 0x00  Version
    _pad0: u32,                    // 0x04
    pub cap_reg: u64,              // 0x08  Capability
    pub ecap_reg: u64,             // 0x10  Extended Capability
    pub gcmd_reg: u32,             // 0x18  Global Command (write-only)
    pub gsts_reg: u32,             // 0x1C  Global Status (read-only)
    pub rtaddr_reg: u64,           // 0x20  Root Table Address
    pub ccmd_reg: u64,             // 0x28  Context Command
    _pad1: u64,                    // 0x30
    pub fsts_reg: u32,             // 0x34  Fault Status
    pub fectl_reg: u32,            // 0x38  Fault Event Control
    pub fedata_reg: u32,           // 0x3C  Fault Event Data
    pub feaddr_reg: u32,           // 0x40  Fault Event Address
    pub feuaddr_reg: u32,          // 0x44  Fault Event Upper Address
    _pad2: u32,                    // 0x48
    pub aflog_reg: u64,            // 0x4C  Advanced Fault Log (note: spec says 0x48 for 64-bit)
    _pad3: u32,                    // padding
    pub pmen_reg: u32,             // 0x64  Protected Memory Enable (spec: 0x64)
    pub plmbase_reg: u32,          // 0x68  Protected Low Memory Base
    pub plmlimit_reg: u32,         // 0x6C  Protected Low Memory Limit
    _pad4: u32,
    pub phmbase_reg: u64,          // 0x70  Protected High Memory Base
    pub phmlimit_reg: u64,         // 0x78  Protected High Memory Limit
    pub iqh_reg: u64,              // 0x80  Invalidation Queue Head
    pub iqt_reg: u64,              // 0x88  Invalidation Queue Tail
    pub iqa_reg: u64,              // 0x90  Invalidation Queue Address
    pub ics_reg: u32,              // 0x98  Invalidation Completion Status
    _pad5: u32,
    pub iectl_reg: u32,            // 0xA0  Invalidation Event Control
    pub iedata_reg: u32,           // 0xA4  Invalidation Event Data
    pub ieaddr_reg: u32,           // 0xA8  Invalidation Event Address
    pub ieuaddr_reg: u32,          // 0xAC  Invalidation Event Upper Address
    _pad6: [u32; 2],               // 0xB0..0xB7 (IRTA is separate below)
    pub irta_reg: u64,             // 0xB8  Interrupt Remapping Table Address
}
// Note: The VT-d register layout has vendor-specific gaps. For production code,
// use volatile read/write helpers with explicit offsets rather than relying
// purely on struct field offsets. The struct above serves as a reference.
// The IRTA_REG offset is 0xB8 per VT-d spec 5.0 (some earlier specs say 0xB0).

/// Intel VT-d CAP_REG bits.
pub mod vtd_cap {
    pub const ND_MASK: u64 = 0x7;
    pub const ZLR: u64 = 1 << 8;
    pub const AFL: u64 = 1 << 9;
    pub const RWBF: u64 = 1 << 10;
    pub const PLMR: u64 = 1 << 11;
    pub const PHMR: u64 = 1 << 13;
    pub const CM: u64 = 1 << 14;
    pub const SAGAW: u64 = 0xFF << 16;
    pub const SAGAW_3LVL: u64 = 1 << 18;  // 3-level page tables
    pub const SAGAW_4LVL: u64 = 1 << 19;  // 4-level page tables
    pub const SAGAW_5LVL: u64 = 1 << 20;  // 5-level page tables
    pub const SAGAW_6LVL: u64 = 1 << 21;  // 6-level page tables
    pub const MGAW_SHIFT: u64 = 24;
    pub const MGAW_MASK: u64 = 0x3F << 24;
}

/// Intel VT-d ECAP_REG bits.
pub mod vtd_ecap {
    pub const C: u64 = 1 << 0;       // Page Request
    pub const QI: u64 = 1 << 1;      // Queued Invalidation
    pub const DT: u64 = 1 << 2;      // Device TLB
    pub const IR: u64 = 1 << 3;      // Interrupt Remapping
    pub const EIM: u64 = 1 << 4;     // Extended Interrupt Mode (x2APIC)
    pub const PT: u64 = 1 << 8;      // Pass Through
    pub const SC: u64 = 1 << 18;     // Snoop Control
    pub const IRO_SHIFT: u64 = 25;
    pub const IRO_MASK: u64 = 0x3FF << 25;
}

/// Intel VT-d GCMD_REG bits (write-only).
pub mod vtd_gcmd {
    pub const TE: u32 = 1 << 31;     // Translation Enable
    pub const SRTP: u32 = 1 << 30;   // Set Root Table Pointer
    pub const SFL: u32 = 1 << 29;    // Set Fault Log
    pub const EAFL: u32 = 1 << 28;   // Enable Advanced Fault Log
    pub const WBF: u32 = 1 << 27;    // Write Buffer Flush
    pub const QIE: u32 = 1 << 26;    // Queued Invalidation Enable
    pub const SIRTP: u32 = 1 << 25;  // Set Interrupt Remap Table Pointer
    pub const CFI: u32 = 1 << 24;    // Compatibility Format Interrupt
    pub const IR: u32 = 1 << 23;     // Interrupt Remap Enable
}

/// Intel VT-d GSTS_REG bits (read-only).
pub mod vtd_gsts {
    pub const TES: u32 = 1 << 31;    // Translation Enable Status
    pub const RTPS: u32 = 1 << 30;   // Root Table Pointer Status
    pub const FLS: u32 = 1 << 29;    // Fault Log Status
    pub const AFLS: u32 = 1 << 28;   // Advanced Fault Log Status
    pub const WBFS: u32 = 1 << 27;   // Write Buffer Flush Status
    pub const QIES: u32 = 1 << 26;   // Queued Invalidation Enable Status
    pub const IRTPS: u32 = 1 << 25;  // Interrupt Remap Table Pointer Status
    pub const CFIS: u32 = 1 << 24;   // Compatibility Format Interrupt Status
    pub const IRES: u32 = 1 << 23;   // Interrupt Remap Enable Status
}

/// Intel VT-d Root Table Entry (128 bits = 16 bytes).
/// 256 entries (one per PCI bus). 4KiB-aligned.
#[repr(C, packed)]
pub struct VtdRootEntry {
    pub data: [u64; 2],
}

impl VtdRootEntry {
    pub const fn zeroed() -> Self {
        Self { data: [0; 2] }
    }

    /// Present (bit 0 of data[0]).
    pub fn present(&self) -> bool {
        self.data[0] & (1 << 0) != 0
    }

    pub fn set_present(&mut self, v: bool) {
        if v { self.data[0] |= 1 << 0; } else { self.data[0] &= !(1 << 0); }
    }

    /// Context Table Pointer (bits 12:63 of data[0]).
    pub fn context_table_ptr(&self) -> u64 {
        self.data[0] & !0xFFF
    }

    pub fn set_context_table_ptr(&mut self, addr: u64) {
        self.data[0] = (self.data[0] & 0xFFF) | (addr & !0xFFF);
    }
}

const _: () = assert!(core::mem::size_of::<VtdRootEntry>() == 16);

/// Intel VT-d Context Entry (128 bits = 16 bytes).
/// 256 entries per bus (one per device:function). 4KiB-aligned table.
#[repr(C, packed)]
pub struct VtdContextEntry {
    pub data: [u64; 2],
}

impl VtdContextEntry {
    pub const fn zeroed() -> Self {
        Self { data: [0; 2] }
    }

    /// Present (bit 0 of data[0]).
    pub fn present(&self) -> bool {
        self.data[0] & (1 << 0) != 0
    }

    pub fn set_present(&mut self, v: bool) {
        if v { self.data[0] |= 1 << 0; } else { self.data[0] &= !(1 << 0); }
    }

    /// Fault Processing Disable (bit 1 of data[0]).
    pub fn fault_processing_disable(&self) -> bool {
        self.data[0] & (1 << 1) != 0
    }

    pub fn set_fault_processing_disable(&mut self, v: bool) {
        if v { self.data[0] |= 1 << 1; } else { self.data[0] &= !(1 << 1); }
    }

    /// Translation Type (bits 2:3 of data[0]).
    /// 00=legacy, 01=PASID, 10=pass-through, 11=reserved.
    pub fn translation_type(&self) -> u64 {
        (self.data[0] >> 2) & 0x3
    }

    pub fn set_translation_type(&mut self, tt: u64) {
        self.data[0] = (self.data[0] & !(0x3 << 2)) | ((tt & 0x3) << 2);
    }

    /// Second Level Page Table Pointer (bits 12:63 of data[0]).
    pub fn slpt_ptr(&self) -> u64 {
        self.data[0] & !0xFFF
    }

    pub fn set_slpt_ptr(&mut self, addr: u64) {
        self.data[0] = (self.data[0] & 0xFFF) | (addr & !0xFFF);
    }

    /// Domain Identifier (bits 0:15 of data[1]).
    pub fn domain_id(&self) -> u16 {
        (self.data[1] & 0xFFFF) as u16
    }

    pub fn set_domain_id(&mut self, id: u16) {
        self.data[1] = (self.data[1] & !0xFFFF) | (id as u64);
    }
}

const _: () = assert!(core::mem::size_of::<VtdContextEntry>() == 16);

/// Intel VT-d Translation Type constants.
pub mod vtd_tt {
    pub const LEGACY: u64 = 0b00;
    pub const PASID: u64 = 0b01;
    pub const PASS_THROUGH: u64 = 0b10;
}

/// Intel VT-d Page Table Entry (64 bits).
#[repr(C, packed)]
pub struct VtdPte(pub u64);

impl VtdPte {
    /// Read permission (bit 0).
    pub fn read(&self) -> bool {
        self.0 & (1 << 0) != 0
    }

    pub fn set_read(&mut self, v: bool) {
        if v { self.0 |= 1 << 0; } else { self.0 &= !(1 << 0); }
    }

    /// Write permission (bit 1).
    pub fn write(&self) -> bool {
        self.0 & (1 << 1) != 0
    }

    pub fn set_write(&mut self, v: bool) {
        if v { self.0 |= 1 << 1; } else { self.0 &= !(1 << 1); }
    }

    /// Page frame or next-table address (bits 12:63).
    pub fn addr(&self) -> u64 {
        self.0 & !0xFFF
    }

    pub fn set_addr(&mut self, a: u64) {
        self.0 = (self.0 & 0xFFF) | (a & !0xFFF);
    }
}

/// Build a leaf PTE for Intel VT-d with read+write.
pub fn vtd_pte_leaf(addr: u64) -> VtdPte {
    let mut pte = VtdPte(0);
    pte.set_read(true);
    pte.set_write(true);
    pte.set_addr(addr);
    pte
}

/// Build a non-leaf PTE for Intel VT-d pointing to next-level table.
pub fn vtd_pte_pointer(addr: u64) -> VtdPte {
    let mut pte = VtdPte(0);
    pte.set_read(true);
    pte.set_write(true);
    pte.set_addr(addr);
    pte
}
```

### 3.3 ACPI Table Structs

```rust
/// Common ACPI table header (24 bytes).
#[repr(C, packed)]
pub struct AcpiTableHeader {
    pub signature: [u8; 4],
    pub length: u32,
    pub revision: u8,
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub oem_table_id: [u8; 8],
    pub oem_revision: u32,
    pub creator_id: [u8; 4],
    pub creator_revision: u32,
}

const _: () = assert!(core::mem::size_of::<AcpiTableHeader>() == 36);

/// IVRS ACPI Table Header.
#[repr(C, packed)]
pub struct IvrsTable {
    pub header: AcpiTableHeader,    // 36 bytes
    pub iv_info: u32,               // IOMMU Virtualization Info
    // Followed by variable-length IVHD/IVMD entries.
}

/// IVHD Entry (I/O Virtualization Hardware Definition).
#[repr(C, packed)]
pub struct IvhdEntry {
    pub entry_type: u8,             // 0x10 or 0x11
    pub flags: u8,                  // Feature flags
    pub length: u16,                // Total length including device entries
    pub device_id: u16,             // BDF of IOMMU PCI device
    pub capability_offset: u16,     // PCI capability offset
    pub iommu_base_address: u64,    // MMIO base address
    pub pci_segment_group: u16,     // PCI segment group
    pub iommu_info: u16,            // IOMMU info (MSI number, unit ID)
    pub iommu_efr: u32,             // Extended features (type 11 only)
    // Followed by variable-length device entries.
}

/// IVMD Entry (I/O Virtualization Memory Definition).
#[repr(C, packed)]
pub struct IvmdEntry {
    pub entry_type: u8,             // 0x20 or 0x21
    pub flags: u8,                  // Memory block flags
    pub length: u16,                // Total length
    pub device_id: u16,             // Start DeviceId (BDF) or 0x0000 for all
    pub aux_data: u16,              // Auxiliary data
    pub start_address: u64,         // Physical start address
    pub memory_length: u64,         // Length in bytes
}

/// IVHD Device Entry (4 bytes minimum).
#[repr(C, packed)]
pub struct IvhdDeviceEntry {
    pub dev_type: u8,               // Device entry type (0x00..0x44)
    pub data: u8,                   // LSA flags
    pub device_id: u16,             // BDF for SEL/SOR/EOR
}

/// DMAR ACPI Table Header.
#[repr(C, packed)]
pub struct DmarTable {
    pub header: AcpiTableHeader,    // 36 bytes
    pub host_address_width: u8,     // DMA physical address width
    pub flags: u8,                  // [0]=INTR_REMAP, [1]=X2APIC_OPT_OUT
    pub reserved: [u8; 10],         // Reserved
    // Followed by variable-length DRHD/RMRR entries.
}

const _: () = assert!(core::mem::size_of::<DmarTable>() == 48);

/// DRHD Entry (DMA Remapping Hardware Unit Definition).
#[repr(C, packed)]
pub struct DrhdEntry {
    pub entry_type: u16,            // 0x0001
    pub length: u16,                // Total length including device scope
    pub flags: u8,                  // [0]=INCLUDE_PCI_ALL
    pub reserved: u8,               // Reserved
    pub segment_number: u16,        // PCI segment group
    pub register_base_address: u64, // Physical MMIO base address
    // Followed by variable-length device scope entries.
}

/// DRHD Device Scope Entry.
#[repr(C, packed)]
pub struct DmarDeviceScope {
    pub scope_type: u8,             // 0x01=PCI EP, 0x02=PCI sub-hierarchy, 0x03=IOAPIC, 0x04=HPET
    pub length: u8,                 // Total length including path entries
    pub enumeration_id: u8,         // Enumeration ID (IOAPIC ID, etc.)
    pub start_bus_number: u8,       // Starting PCI bus number
    // Followed by path entries (each 2 bytes: device, function).
}

/// RMRR Entry (Reserved Memory Region Reporting).
#[repr(C, packed)]
pub struct RmrrEntry {
    pub entry_type: u16,            // 0x0002
    pub length: u16,                // Total length
    pub reserved: u16,              // Reserved
    pub segment_number: u16,        // PCI segment group
    pub base_address: u64,          // Physical start address
    pub end_address: u64,           // Physical end address (inclusive)
    // Followed by variable-length device scope entries.
}

/// DMAR Sub-Table Types.
pub mod dmar_type {
    pub const DRHD: u16 = 0x0001;
    pub const RMRR: u16 = 0x0002;
    pub const ATSR: u16 = 0x0003;
    pub const RHSA: u16 = 0x0004;
    pub const ANDD: u16 = 0x0005;
}

/// DMAR Device Scope Types.
pub mod dmar_scope_type {
    pub const PCI_ENDPOINT: u8 = 0x01;
    pub const PCI_SUBHIERARCHY: u8 = 0x02;
    pub const IOAPIC: u8 = 0x03;
    pub const MSI_HPET: u8 = 0x04;
    pub const ACPI_NAMESPACE: u8 = 0x05;
}
```

### 3.4 Utility Types

```rust
/// BDF (Bus:Device:Function) packed as u16.
/// Format: bus[15:8] | device[7:3] | function[2:0].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bdf(pub u16);

impl Bdf {
    pub fn new(bus: u8, device: u8, function: u8) -> Self {
        Self(((bus as u16) << 8) | ((device as u16 & 0x1F) << 3) | (function as u16 & 0x7))
    }

    pub fn bus(&self) -> u8 {
        (self.0 >> 8) as u8
    }

    pub fn device(&self) -> u8 {
        ((self.0 >> 3) & 0x1F) as u8
    }

    pub fn function(&self) -> u8 {
        (self.0 & 0x7) as u8
    }

    /// Index into the AMD Device Table (same as raw BDF value).
    pub fn dev_table_index(&self) -> usize {
        self.0 as usize
    }
}

/// Domain ID. Used to group devices sharing a page table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DomainId(pub u16);

/// Page table level constants.
pub mod pt_level {
    /// AMD-Vi levels (Mode field in DTE).
    pub const AMD_1_LEVEL: u64 = 1;
    pub const AMD_2_LEVEL: u64 = 2;
    pub const AMD_3_LEVEL: u64 = 3;
    pub const AMD_4_LEVEL: u64 = 4;
    pub const AMD_5_LEVEL: u64 = 5;
    pub const AMD_6_LEVEL: u64 = 6;

    /// Intel VT-d levels (SAGAW field).
    pub const VTd_3_LEVEL: u64 = 3;
    pub const VTd_4_LEVEL: u64 = 4;
    pub const VTd_5_LEVEL: u64 = 5;
    pub const VTd_6_LEVEL: u64 = 6;
}
```

### 3.5 Size Constants

```rust
/// AMD-Vi sizing constants.
pub mod amd_sizes {
    /// Maximum Device Table entries.
    pub const MAX_DEV_TABLE_ENTRIES: usize = 65536;
    /// Device Table Entry size.
    pub const DTE_SIZE: usize = 32;
    /// Maximum Device Table size (65536 × 32 bytes).
    pub const MAX_DEV_TABLE_SIZE: usize = MAX_DEV_TABLE_ENTRIES * DTE_SIZE; // 2 MiB

    /// Default Command Buffer entries.
    pub const CMD_BUF_ENTRIES: usize = 512;
    /// Command Buffer Entry size.
    pub const CMD_ENTRY_SIZE: usize = 16;
    /// Default Command Buffer size.
    pub const CMD_BUF_SIZE: usize = CMD_BUF_ENTRIES * CMD_ENTRY_SIZE; // 8 KiB

    /// Default Event Log entries.
    pub const EVT_LOG_ENTRIES: usize = 512;
    /// Event Log Entry size.
    pub const EVT_ENTRY_SIZE: usize = 16;
    /// Default Event Log size.
    pub const EVT_LOG_SIZE: usize = EVT_LOG_ENTRIES * EVT_ENTRY_SIZE; // 8 KiB

    /// IRTE size (128 bits).
    pub const IRTE_SIZE: usize = 16;
    /// Maximum Interrupt Remap Table entries (IntTabLen=11 → 2^12 = 4096).
    pub const MAX_IRT_ENTRIES: usize = 4096;
    /// Maximum Interrupt Remap Table size.
    pub const MAX_IRT_SIZE: usize = MAX_IRT_ENTRIES * IRTE_SIZE; // 64 KiB

    /// Page table entry size (both AMD and Intel).
    pub const PTE_SIZE: usize = 8;
    /// Entries per page table page (4KiB / 8 bytes).
    pub const PTES_PER_PAGE: usize = 512;
}

/// Intel VT-d sizing constants.
pub mod vtd_sizes {
    /// Root Table entries (one per PCI bus).
    pub const ROOT_TABLE_ENTRIES: usize = 256;
    /// Root/Context Entry size.
    pub const ENTRY_SIZE: usize = 16;
    /// Root Table size.
    pub const ROOT_TABLE_SIZE: usize = ROOT_TABLE_ENTRIES * ENTRY_SIZE; // 4 KiB

    /// Context Table entries (one per device:function per bus).
    pub const CTX_TABLE_ENTRIES: usize = 256;
    /// Context Table size.
    pub const CTX_TABLE_SIZE: usize = CTX_TABLE_ENTRIES * ENTRY_SIZE; // 4 KiB

    /// Page table entry size.
    pub const PTE_SIZE: usize = 8;
    /// Entries per page table page.
    pub const PTES_PER_PAGE: usize = 512;
}

/// PCI BDF address space: 256 buses × 32 devices × 8 functions = 65536.
pub const PCI_BDF_COUNT: usize = 256 * 32 * 8;
```

---

## Appendix: Linux Kernel Reference

The Linux kernel IOMMU drivers are the primary reference implementation. Key files:

| Path | Description |
|------|-------------|
| `drivers/iommu/amd/amd_iommu_types.h` | AMD-Vi type definitions, DTE/IRTE/PTE formats, register constants |
| `drivers/iommu/amd/amd_iommu.c` | AMD-Vi main driver: init, command buffer, device table management |
| `drivers/iommu/amd/init.c` | AMD-Vi initialization, IVRS parsing, early setup |
| `drivers/iommu/amd/irq.c` | AMD-Vi interrupt remapping |
| `drivers/iommu/intel/dmar.c` | Intel VT-d DMAR table parsing |
| `drivers/iommu/intel/iommu.c` | Intel VT-d main driver |
| `drivers/iommu/intel/irq_remapping.c` | Intel VT-d interrupt remapping |
| `include/linux/intel-iommu.h` | Intel VT-d register definitions, struct definitions |
| `drivers/iommu/io-pgtable.c` | Generic page table allocation |

### Key Linux Constants for Cross-Reference

```c
// AMD DTE bits (from amd_iommu_types.h)
#define DTE_FLAG_V    (1ULL << 0)
#define DTE_FLAG_TV   (1ULL << 1)
#define DTE_FLAG_IR   (1ULL << 61)
#define DTE_FLAG_IW   (1ULL << 62)
#define DTE_FLAG_SE   (1ULL << 8)

// AMD page table modes (DTE Mode field)
#define DTE_MODE_4LVL  4   // 4-level page tables (most common)

// AMD command opcodes
#define CMD_COMPLETION_WAIT            0x01
#define CMD_INVALIDATE_DEVTAB_ENTRY    0x02
#define CMD_INVALIDATE_IOMMU_PAGES     0x03
#define CMD_INVALIDATE_INTERRUPT_TABLE 0x04

// Intel DMAR flags
#define DMAR_INTR_REMAP    0x1
#define DMAR_X2APIC_OPT_OUT 0x2

// Intel context entry TT (Translation Type)
#define CONTEXT_TT_MULTI_LEVEL  0
#define CONTEXT_TT_DEV_IOTLB    1
#define CONTEXT_TT_PASS_THROUGH 2
```

---

*Document generated for Red Bear OS IOMMU implementation. Sources: AMD IOMMU Specification 48882 Rev 3.10, Intel VT-d Specification Rev 5.0, Linux kernel v6.x source.*
