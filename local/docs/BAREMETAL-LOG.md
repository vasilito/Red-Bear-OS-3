# Bare Metal Test Log — AMD Hardware

Template for recording test results when booting Redox on AMD hardware.
Fill one section per test run. Date is ISO 8601.

## How to Test

```bash
# 1. Build the image
./local/scripts/build-redbear.sh redbear-desktop

# 2. Burn to USB (DANGEROUS — verify target device!)
./local/scripts/test-baremetal.sh --device /dev/sdX

# 3. Boot from USB on target hardware
# 4. Record results below
```

## Serial Console Setup

For boot debugging, connect a serial console before powering on:
- Baud rate: 115200
- Use a USB-to-TTL serial adapter on the motherboard header
- Or use IPMI/BMC serial-over-LAN if available

---

## Test Run Template

```
### [DATE] — [HARDWARE MODEL]

**Hardware:**
- Vendor: 
- Model: 
- CPU: (e.g., AMD Ryzen 9 7940HS)
- GPU: (e.g., AMD Radeon 780M integrated)
- Motherboard firmware: UEFI / BIOS
- RAM: (e.g., 32GB DDR5)
- Storage: (e.g., NVMe SSD)

**Build:**
- Redox version: (git rev-parse --short HEAD)
- Config: (e.g., my-amd-desktop)
- Kernel patch version: (checksum of local/patches/kernel/P0-amd-acpi-x2apic.patch)

**Result:** Booting / Broken / Recommended

**Boot log (serial output):**
```
(paste kernel log here, especially ACPI-related lines)
```

**Observations:**
- ACPI tables detected: (list any `kernel::acpi` output)
- APIC mode: xAPIC / x2APIC
- CPU count: (how many cores detected)
- Crash location: (if broken, what function/line)
- Display: VESA / GOP / none
- Input: PS/2 keyboard / PS/2 mouse / USB / none
- Network: working / not detected
- Audio: working / not detected

**Issues:**
1. (describe any problems)
```

---

## Test Results

### 2026-04-11 — Framework Laptop 16 (AMD Ryzen 7040)

**Hardware:**
- Vendor: Framework
- Model: Laptop 16 (AMD Ryzen 7040 Series)
- CPU: AMD Ryzen 9 7940HS (13 cores, x2APIC)
- GPU: AMD Radeon 780M (RDNA3, integrated)
- Motherboard firmware: UEFI
- RAM: 32GB DDR5
- Storage: NVMe SSD

**Build:**
- Redox version: (pending first test with P0 patches applied)
- Config: my-amd-desktop
- Kernel patch: P0-amd-acpi-x2apic.patch (with timeout + SIPI fixes)

**Result:** PENDING TEST

**Known from HARDWARE.md:**
- Previous status: **Broken** — crash due to unimplemented ACPI function
- Reference: jackpot51/acpi#3
- With P0 patches applied, x2APIC should now work; need to verify the specific
  ACPI function that was missing

---

### 2025-11-09 — Lenovo ThinkCentre M83

**Hardware:**
- Vendor: Lenovo
- Model: ThinkCentre M83
- CPU: (Intel, x86_64)
- Motherboard firmware: UEFI

**Result:** Broken

**Known issues from HARDWARE.md:**
- `acpid/src/acpi.rs:256:68: Called Result::unwrap() on an Err value: Aml(NoCurrentOp)`
- `acpid/src/main.rs:147:39: acpid: failed to daemonize: Error I/O error 5`
- Display logs offset past left edge of screen
- `[@hwd:40 ERROR] failed to probe with error No such device (os error 19)`

**Analysis:**
- AML interpreter hits unsupported opcode (`NoCurrentOp`)
- This is in the userspace acpid, not the kernel
- Likely needs AML opcode support added to `aml_physmem.rs` or `acpi.rs`

---

### 2024-09-20 — ASUS PRIME B350M-E (Custom Desktop)

**Hardware:**
- Vendor: ASUS
- Model: PRIME B350M-E (custom)
- CPU: AMD (B350 chipset = Ryzen 1st/2nd gen)
- Motherboard firmware: UEFI

**Result:** Booting

**Known issues from HARDWARE.md:**
- Partial PS/2 keyboard support
- PS/2 mouse broken
- No GPU acceleration (VESA/GOP only)

**Analysis:**
- Boots successfully with xAPIC (Ryzen 1000/2000 uses APIC IDs < 255)
- I2C devices unsupported (touchpad)
- Good candidate for testing P0 patches (verifies no regression on xAPIC systems)
