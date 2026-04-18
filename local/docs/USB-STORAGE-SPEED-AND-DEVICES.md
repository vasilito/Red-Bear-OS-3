# Red Bear OS USB Storage, Speed, and Device Integration

## Purpose

This document covers USB subsystem areas that the main USB implementation plan
(`USB-IMPLEMENTATION-PLAN.md`) treats at a higher level: mass storage quality, filesystem
integration, device speed handling, backwards compatibility, and integrated USB device paths.

It is a companion document, not a replacement. Read both together for the complete USB picture.

## Current Headline

USB mass storage is **present in the codebase but disabled**. The driver table entry for
`usbscsid` is commented out in `drivers.toml` with the note "#TODO: causes XHCI errors". HID
(keyboard/mouse) and hub handling are enabled and functional in QEMU.

## USB Mass Storage

### Architecture

```
USB device → xhcid (scheme:usb) → usbscsid → driver_block::DiskScheme → /scheme/disk.usb-*
                                                                         → filesystem (redoxfs/ext4d)
```

| Layer | Component | Status |
|---|---|---|
| Transport | BOT (Bulk-Only Transport) | ✅ Implemented — CBW/CSW signatures validated, tag matching, stall recovery |
| Protocol | SCSI SBC | ⚠️ Partial — READ(16)/WRITE(16) only, missing READ(10)/WRITE(10) |
| Block device | driver_block::DiskScheme | ✅ Functional — registers as `/scheme/disk.usb-{scheme}+{port}-scsi` |
| Partitions | partitionlib (MBR/GPT) | ✅ Parsed on init — exposes `0p0`, `0p1`, etc. |
| Filesystems | redoxfs, ext4d | ✅ Can mount USB block devices via scheme path |

### BOT Transport Quality

The BOT transport in `usbscsid/src/protocol/bot.rs` is well-implemented:

- **CBW handling**: Correct signature (`0x43425355`), per-command tag increment, direction bit, LUN field
- **CSW handling**: Signature validation, tag matching, residue tracking, short packet tolerance
- **Stall recovery**: `ClearFeature(ENDPOINT_HALT)` on both endpoints, `Bulk-Only Mass Storage Reset`
  class request (0xFF) for full reset recovery, re-check for persistent stalls
- **Phase errors**: Detected and reported via `ProtocolError`

### SCSI Command Completeness

| Command | CDB Size | Status | Notes |
|---|---|---|---|
| INQUIRY | 6 | ✅ | Standard + vendor inquiry data |
| REQUEST SENSE | 6 | ✅ | Fixed-format sense data |
| READ CAPACITY(10) | 10 | ✅ | 32-bit LBA, used as first probe |
| READ CAPACITY(16) | 16 | ✅ | 64-bit LBA, auto-fallback from RC(10) max |
| READ(16) | 16 | ✅ | Primary read path |
| WRITE(16) | 16 | ✅ | Primary write path |
| MODE SENSE(6) | 6 | ✅ | Block descriptor fallback |
| MODE SENSE(10) | 10 | ✅ | Primary block size/count source |
| READ(10) | 10 | ❌ **Missing** | Required for older/simpler devices |
| WRITE(10) | 10 | ❌ **Missing** | Required for older/simpler devices |
| SYNCHRONIZE CACHE | 10 | ⚠️ Opcode only | Never issued |
| START STOP UNIT | 6 | ⚠️ Opcode only | Never issued |
| TEST UNIT READY | 6 | ⚠️ Opcode only | Never issued |
| REPORT LUNS | 12 | ❌ **Missing** | Needed for multi-LUN devices |
| MODE SELECT(6/10) | 6/10 | ❌ **Missing** | Needed for parameter negotiation |
| FORMAT UNIT | 6 | ❌ **Missing** | Rarely needed |

**Critical gap**: READ(10)/WRITE(10) are not implemented. The daemon uses 16-byte CDBs exclusively.
Devices that only support 10-byte SCSI commands (some older USB flash drives, embedded firmware
devices) will fail. Adding READ(10)/WRITE(10) with automatic fallback (similar to how
READ CAPACITY(10)→(16) already works) is a concrete, bounded improvement.

### LUN Support

`get_max_lun()` reads the device's max LUN count via class-specific request, but the daemon
hardcodes `cbw.lun = 0` for all commands. Multi-LUN devices (card readers, multi-slot adapters)
will only expose the first LUN. Supporting multiple LUNs requires:

1. Iterating from 0 to max_lun
2. Creating separate `UsbDisk` instances per LUN
3. Registering separate `DiskScheme` paths per LUN

### UAS (USB Attached SCSI)

UAS is not implemented. The protocol factory in `protocol/mod.rs` only matches protocol 0x50 (BOT).
Protocol 0x62 (UAS) is absent. UAS provides:

- Multiple simultaneous command pipes (vs BOT's single serialize-execute-wait)
- Stream-based transfers for higher throughput
- Better error recovery semantics

For USB 3.0 SuperSpeed devices, UAS can provide significantly higher throughput than BOT.

### Transfer Size Limitations

- BOT max transfer: 64KB per command (driver_interface.rs hard limit)
- Stream transfer chunk: 32KB per iteration (hardcoded in TransferStream)
- No scatter-gather: all data must fit in a single buffer (explicit TODO in scsi/mod.rs)

### Why usbscsid is Disabled

The comment says "#TODO: causes XHCI errors". This likely relates to:

1. Bulk endpoint configuration issues in the xHCI driver
2. The 64KB transfer limit causing multi-block reads to fragment incorrectly
3. Missing endpoint stall handling during initial enumeration

**Re-enabling usbscsid is a prerequisite for USB storage validation.**

## USB Speed Handling

### Speed Detection

The xHCI driver detects device speed via PORTSC register bits 10–13. The `ProtocolSpeed` struct
(in `xhci/extended.rs`) classifies speeds:

| Speed | Bitrate | Detection | Status |
|---|---|---|---|
| Low Speed | 1.5 Mbps | `is_lowspeed()` | ✅ Detected |
| Full Speed | 12 Mbps | `is_fullspeed()` | ✅ Detected |
| High Speed | 480 Mbps | `is_highspeed()` | ✅ Detected |
| SuperSpeed Gen1 x1 | 5 Gbps | `is_superspeed_gen1x1()` | ✅ Detected |
| SuperSpeedPlus Gen2 x1 | 10 Gbps | `is_superspeedplus_gen2x1()` | ✅ Detected |
| SuperSpeedPlus Gen1 x2 | 10 Gbps x2 | `is_superspeedplus_gen1x2()` | ✅ Detected |
| SuperSpeedPlus Gen2 x2 | 20 Gbps x2 | `is_superspeedplus_gen2x2()` | ✅ Detected |

### Default Control Pipe Max Packet Size

The driver sets the default control pipe max packet size based on speed:

| Speed | Max Packet Size | Location |
|---|---|---|
| Low/Full Speed | 8 bytes | mod.rs:1128 |
| High Speed | 64 bytes | mod.rs:1131 |
| SuperSpeed | 512 bytes | mod.rs:1134 |

### Transfer Type Support

| Transfer Type | USB Role | Status | Notes |
|---|---|---|---|
| Control | Configuration, enumeration | ✅ Works | Endpoint 0 only |
| Bulk | Mass storage, network | ✅ Works | Used by usbscsid |
| Interrupt | HID, hub status | ✅ Works | Used by usbhubd, usbhidd |
| Isochronous | Audio, video | ❌ ENOSYS | `scheme.rs` explicitly returns `ENOSYS` |

Isochronous transfers are required for USB audio devices, webcams, and streaming applications.
The driver returns `ENOSYS` (function not implemented) for all isochronous endpoint requests.

## Backwards Compatibility

### Transaction Translator (TT) Handling — STUBBED

USB 1.x Low Speed and Full Speed devices connected behind USB 2.0 High Speed hubs require a
Transaction Translator (TT) to convert between USB 1.x and USB 2.0 protocols. The xHCI
specification handles TT internally in the controller, but the driver must provide correct
parent hub information in the Slot Context during device addressing.

**Current state**: All TT-related fields are hardcoded:

| Field | Value | Should Be | Location |
|---|---|---|---|
| `mtt` (Multi-TT) | `false` | Read from parent hub descriptor | mod.rs:1057 |
| `ttt` (TT Think Time) | `0` | Encoded from parent hub descriptor | mod.rs:1114 |
| `needs_parent_info` | `true` (forced) | Based on actual device speed topology | mod.rs:1070 |

The TODOs at mod.rs:1066–1068 explicitly state the values need to be determined from actual
device speed and hub topology. Without correct TT information:

- Low Speed devices (1.5 Mbps) behind USB 2.0 hubs may not enumerate correctly on real hardware
- Full Speed devices (12 Mbps) behind USB 2.0 hubs may fail during bulk transfers
- Multi-TT hubs with multiple LS/FS devices attached may have timing violations

### USB 1.x Compatibility

- **Low Speed (1.5 Mbps)**: Speed detection works. Default control pipe size correct (8 bytes).
  TT handling stubbed — may fail on real hardware behind HS hubs.
- **Full Speed (12 Mbps)**: Speed detection works. Default control pipe size correct (8 bytes).
  Same TT limitation.

### USB 2.0 Compatibility

- **High Speed (480 Mbps)**: Primary tested speed in QEMU. Bulk, Interrupt, Control all functional.
  Max packet size 64 bytes correctly set.

### USB 3.x Compatibility

- **SuperSpeed (5 Gbps)**: Protocol speed detection works. BOS descriptor fetching implemented.
  Max packet size 512 bytes correctly set. SuperSpeed Companion Descriptor parsed.
- **SuperSpeedPlus (10–20 Gbps)**: Protocol speed detection works. SuperSpeedPlus Isochronous
  Companion Descriptor parsed. No functional testing.

### Speed-Specific Gaps

- No speed-specific timeout or retry tuning — all controller-level timeouts are 1-second hardcoded
- No burst transaction support for SuperSpeed bulk endpoints (burst field parsed but not used in
  transfer scheduling)
- No streams support for USB 3.0 bulk endpoints (Stream ID capability parsed but not exercised)

## Integrated USB Devices

### Device Autospawn Flow

```
1. Device plugs in
2. xhcid detects port status change
3. xhcid resets port, enumerates device, reads config descriptor
4. spawn_drivers() iterates interfaces:
   - For each interface with alternate_setting == 0:
     - Match class code (+ optional subclass) against drivers.toml
     - If match: spawn daemon with $SCHEME, $PORT, $IF_NUM/$IF_PROTO
5. Each spawned daemon opens its USB interface via xhcid_interface
6. Daemon registers its own scheme or connects to existing schemes
```

### Driver Table (`drivers.toml`)

```toml
# Mass Storage — DISABLED (#TODO: causes XHCI errors)
#[[drivers]]
#name = "SCSI over USB"
#class = 8; subclass = 6
#command = ["usbscsid", "$SCHEME", "$PORT", "$IF_PROTO"]

[[drivers]]
name = "USB HUB";    class = 9;  subclass = -1
command = ["usbhubd", "$SCHEME", "$PORT", "$IF_NUM"]

[[drivers]]
name = "USB HID";    class = 3;  subclass = -1
command = ["usbhidd", "$SCHEME", "$PORT", "$IF_NUM"]
```

### Supported Device Paths

| Device Class | Daemon | Scheme Path | Integration |
|---|---|---|---|
| **Hub** (class 9) | `usbhubd` | Manages child ports via xhci scheme | Triggers nested device enumeration |
| **HID** (class 3) | `usbhidd` | Writes to `/scheme/input/producer` via inputd | Legacy display/input consumers read `/scheme/input` |
| **Mass Storage** (class 8) | `usbscsid` | Registers `disk.usb-{scheme}+{port}-scsi` | Filesystems mount via scheme path |

### HID Integration Detail

```
USB keyboard/mouse → xhcid → usbhidd → inputd (scheme:input) → display/input consumer
                                                      ↑
                                           /scheme/input/producer  (drivers write here)
                                           /scheme/input/consumer  (display server reads here)
                                           /scheme/input/handle/{name}  (per-device handles)
```

- `usbhidd` implements boot protocol HID (keyboard, mouse, scroll, button)
- Events: `orbclient::KeyEvent` for keyboards, `orbclient::MouseEvent`/`ButtonEvent`/`ScrollEvent`
- The `inputd` multiplexer collects from all input producers (USB HID, PS/2 via `ps2d`, etc.)

### Storage Integration Detail

```
USB flash drive → xhcid → usbscsid → DiskScheme → /scheme/disk.usb-usb+1-scsi
                                                           ↓
                                                   redoxfs/ext4d mount
                                                           ↓
                                                   /scheme/file/{mount-point}
```

- `DiskScheme` from `driver-block` provides block I/O via scheme
- Partition table parsing via `partitionlib` (MBR + GPT)
- Partitions exposed as `0p0`, `0p1`, etc. under the disk scheme

### Composite Device Handling

Composite USB devices (e.g., keyboard+mouse combo, keyboard+trackpad) are partially supported:

- xhcid iterates **all interfaces** in the first configuration
- Each interface matching a `drivers.toml` entry spawns its own daemon process
- Alternate settings (`alternate_setting != 0`) are explicitly skipped
- No vendor/product ID matching — class code only

**What works**: A keyboard+mouse combo with two HID interfaces will spawn two `usbhidd` processes,
each handling one interface. Both produce input events through inputd.

**What doesn't work**: Devices requiring alternate settings for full functionality. Devices needing
vendor-specific drivers.

### Unsupported Device Classes

These USB device classes have no driver in Red Bear OS:

| Class | Name | Use Case | Blocker |
|---|---|---|---|
| 0x01 | Audio | USB headsets, speakers | Isochronous transfers not implemented (ENOSYS) |
| 0x0E | Video | Webcams | Isochronous transfers not implemented |
| 0x02 | CDC/ACM | USB serial, modems | No driver written |
| 0x0A | CDC-Data | USB networking | No driver written |
| 0x0B | Chip Card | Smart card readers | No driver written |
| 0x0D | Content Security | Conditional access | No driver written |
| 0x0F | Personal Healthcare | Medical devices | No driver written |
| 0x06 | Still Image | Cameras (PTP/MTP) | No driver written |
| 0x07 | Printer | USB printers | No driver written |
| 0x10 | Audio/Video | AV devices | Isochronous required |
| 0x11 | Billboard | USB-C alternate mode | No driver written |
| 0x12 | USB Type-C Bridge | USB-C muxes | No driver written |
| 0xDC | Diagnostic | USB debug | No driver written |
| 0xE0 | Wireless Controller | Bluetooth, Wi-Fi dongles | Separate (redbear-btusb) |
| 0xEF | Miscellaneous | Firmware update, etc. | No driver written |
| 0xFF | Vendor Specific | Custom devices | No driver written |

## Implementation Priorities

### Priority 1: Re-enable USB Mass Storage

The most impactful single change. Requires diagnosing and fixing the "causes XHCI errors" issue.
Likely causes:

1. Bulk endpoint configuration in scheme.rs — endpoint type mismatch during configuration
2. Transfer size handling — the 64KB limit may fragment BOT CBW/CSW sequences
3. Missing stall recovery during initial BOT reset

### Priority 2: Add READ(10)/WRITE(10)

Implement 10-byte CDB variants with automatic fallback. Pattern already exists for READ CAPACITY.
Required for device compatibility:

```rust
// Proposed fallback pattern (matching existing RC10→RC16 pattern):
pub fn read(&mut self, lba: u64, buf: &mut [u8], protocol: &mut dyn Protocol) -> Result<()> {
    if lba <= u32::MAX as u64 && buf.len() <= u32::MAX as usize {
        // Try READ(10) first — wider device compatibility
        let cmd = self.cmd_read10()?;
        *cmd = cmds::Read10::new(lba as u32, ...);
        ...
    }
    // Fall back to READ(16) for large addresses
}
```

### Priority 3: Fix Transaction Translator Handling

Replace hardcoded TT values with actual parent hub descriptor data. This is required for real
hardware where LS/FS devices sit behind HS hubs.

### Priority 4: Multi-LUN Support

Iterate device LUNs and create separate disk scheme instances per LUN. Required for card readers
and multi-slot adapters.

### Priority 5: Isochronous Transfers

Implement the Isoch TRB path in scheme.rs to enable USB audio and video device classes.

### Priority 6: UAS Transport

Add USB Attached SCSI protocol support for SuperSpeed storage devices. Higher throughput than BOT
but requires stream ID support in the xHCI driver.

## Summary

USB mass storage exists in the codebase with a well-implemented BOT transport, proper SCSI command
set (with gaps in READ/WRITE(10)), and functional block device integration — but it is **disabled**
due to xHCI errors during device configuration. The most impactful work is diagnosing and fixing
that issue, then adding READ(10)/WRITE(10) for wider device compatibility.

Speed handling covers the full range from Low Speed (1.5 Mbps) to SuperSpeedPlus (20 Gbps) at the
detection level, but TT handling is stubbed and isochronous transfers return ENOSYS. Backwards
compatibility for USB 1.x devices behind USB 2.0 hubs requires TT fix work.

Device integration supports hubs and HID via autospawn. Composite devices get all interfaces
handled. No vendor/product matching exists. No audio, video, serial, or networking USB device
classes have drivers.
