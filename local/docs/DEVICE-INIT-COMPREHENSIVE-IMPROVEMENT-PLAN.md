# Red Bear OS Low-Level Device Initialization — Comprehensive Improvement Plan

**Date:** 2026-04-30
**Scope:** Complete reassessment of boot-time device initialization: daemon inventory, firmware loading, driver model, bus enumeration, controller support, hardware validation
**Reference:** Linux 7.0 kernel device init model (full source available for comparison)
**Status:** Assessment phase — this document is the execution plan

## 1. Executive Summary

Red Bear OS has crossed the fundamental bring-up threshold: the system boots to a login prompt on
both QEMU and bounded bare-metal hardware (AMD Ryzen), device daemons start in a defined order,
and major subsystems (ACPI, PCI, USB/xHCI, NVMe, network) have in-tree implementations.

However, the device initialization stack is **not release-grade**. Key deficiencies vs Linux 7.0:

| Gap | Severity | Impact |
|-----|----------|--------|
| No proper device driver model (bus/device/driver binding) | CRITICAL | No deferred probing, no async init, no hotplug |
| No uevent/hotplug infrastructure (udev-shim is static enumerator only) | CRITICAL | No device add/remove notification; `udev-shim` is misnamed — it does a single PCI scan, not real udev |
| No EHCI/OHCI/UHCI USB controllers | HIGH | USB keyboard not reliable on bare metal |
| initfs vs rootfs driver duality — drivers started in initfs may conflict with rootfs drivers | HIGH | No explicit handoff contract for devices initialized in initfs |
| No hardware validation for MSI-X, IOMMU, xHCI interrupts | HIGH | QEMU-proven only; real hardware behavior unknown |
| No suspend/resume or runtime power management | HIGH | No S3/S4 sleep, no device power gating |
| No CPU frequency scaling or thermal management | MEDIUM | Battery life, thermal throttling absent |
| No hardware RNG daemon, no SMBIOS/DMI runtime | MEDIUM | Missing entropy source, missing quirk data |
| No PCIe AER, no advanced error reporting | MEDIUM | Silent device failures |
| Firmware loading GPU-only (no Wi-Fi, audio, media) | MEDIUM | Blocks iwlwifi, Bluetooth, media acceleration |
| No device naming policy or persistent device names | MEDIUM | `/dev/` names unstable across boots |
| No kernel cmdline for device parameterization | LOW | No runtime device config without rebuild |
| ACPI startup still carries panic-grade `expect` paths | HIGH | Boot fragility on diverse hardware |
| `acpid` `_S5` shutdown not release-grade | HIGH | Unclean shutdown on some platforms |
| Wi-Fi transport asserts on MSI-X (no legacy IRQ fallback) | HIGH | Wi-Fi won't work on older platforms |
| No EHCI companion controller routing for USB keyboards | HIGH | USB keyboard may be unreachable on some bare metal |
| No io_uring or epoll for async I/O in device daemons | LOW | Throughput ceiling for NVMe |

### Bottom Line

**Red Bear OS boots, but device initialization is naive by Linux 7.0 standards.** The microkernel
scheme-based driver model is architecturally sound, but the implementation lacks the maturity,
error resilience, hardware coverage, and power management depth that Linux 7.0 has accumulated
over 30 years of driver development.

This plan defines a structured path to close these gaps over 5 phases (26-40 weeks).

## 2. Current State Assessment

### 2.1 Boot Flow

```
UEFI firmware → Bootloader → Kernel (kstart→kmain) →
userspace_init → bootstrap (procmgr) → initfs init →
├── Phase 1 (initfs): logd, nulld, randd, zerod, rtcd, ramfs
├── Phase 1 (initfs): inputd, lived
├── Phase 1 (initfs): vesad, fbbootlogd, fbcond (graphics target)
├── Phase 1 (initfs): hwd, pcid-spawner-initfs, ps2d (drivers target)
├── Phase 1 (initfs): rootfs mount → switchroot
├── Phase 2 (rootfs): ipcd, ptyd, pcid-spawner (base target)
│   ├── pcid-spawner spawns drivers matching PCI IDs:
│   │   ├── Storage: ahcid, ided, nvmed, virtio-blkd, usbscsid
│   │   ├── Network: e1000d, rtl8168d, rtl8139d, ixgbed, virtio-netd
│   │   ├── Graphics: vesad, ihdgd, virtio-gpud
│   │   ├── Input: ps2d, usbhidd
│   │   ├── Audio: ihdad, ac97d, sb16d
│   │   └── USB: xhcid, usbhubd
│   ├── smolnetd → dhcpd (network target)
│   ├── firmware-loader, udev-shim, evdevd, wifictl
│   ├── dbus-daemon → redbear-sessiond, seatd
│   └── console/getty → login prompt
```

### 2.2 Daemon Inventory — Existence and Quality

#### Core Initfs Daemons (20 services)

| Daemon | Quality | Notes |
|--------|---------|-------|
| `logd` | ✅ Hardened | Zero unwrap/expect; file descriptors, setrens, process loop |
| `nulld` | ✅ Hardened | Zero unwrap/expect |
| `randd` | ✅ Hardened | CPUID chain hardened; 8 test-only unwraps |
| `zerod` | ✅ Hardened | Args default + graceful exit |
| `rtcd` | ✅ Present | x86 RTC driver; minimal attack surface |
| `ramfs@` | ✅ Present | Template service for RAM filesystems |
| `inputd` | ✅ Hardened | 14 panic sites converted; partial vt events, buffer sizes |
| `lived` | ✅ Present | Live disk daemon |
| `vesad` | ✅ Hardened | 20 fixes; FRAMEBUFFER env, EventQueue, event loop, scheme |
| `fbbootlogd` | ✅ Hardened | 14 fixes; VT handle, graphics handle, dirty_fb |
| `fbcond` | ✅ Hardened | 14 fixes; VT parse, event loop, writes, scheme, display |
| `hwd` | ✅ Present | ACPI/DeviceTree boot handler |
| `pcid-spawner-initfs` | ✅ Hardened | initfs variant; oneshot_async |
| `ps2d` | ✅ Hardened | Controller init drains stale output; QEMU proof |
| `bcm2835-sdhcid` | ✅ Present | ARM-only (Raspberry Pi) |

#### Core Rootfs Daemons (9 base services)

| Daemon | Quality | Notes |
|--------|---------|-------|
| `ipcd` | ✅ Present | IPC daemon |
| `ptyd` | ✅ Present | Pseudo-terminal daemon |
| `pcid-spawner` | ✅ Hardened | Changed to oneshot_async (was blocking init); logs device info |
| `sudo` | ✅ Present | Privilege daemon |
| `smolnetd`/`netstack` | ✅ Present | TCP/IP stack |
| `dhcpd` | ✅ Present | DHCP client |
| `audiod` | ✅ Present | Audio multiplexer |

#### PCI-Matched Device Drivers (pcid-spawner, 25+ drivers)

| Category | Drivers | Quality |
|----------|---------|---------|
| Storage | ahcid, ided, nvmed, virtio-blkd, usbscsid | ✅ All hardened (Wave 4 complete) |
| Network | e1000d, rtl8168d, rtl8139d, ixgbed, virtio-netd | ✅ All hardened |
| Graphics | vesad, ihdgd, virtio-gpud | ✅ All hardened |
| Input | ps2d, usbhidd | ✅ All hardened |
| Audio | ihdad, ac97d, sb16d | ✅ All hardened |
| USB | xhcid, usbhubd, usbctl, ucsid | ✅ xhcid has 88 Red Bear patches |
| GPIO/I2C | gpiod, i2cd, intel-gpiod, amd-mp2-i2cd, dw-acpi-i2cd, i2c-gpio-expanderd, i2c-hidd, intel-thc-hidd, intel-lpss-i2cd | ✅ Present |
| System | pcid, pcid-spawner, acpid | ✅ Core infra; pcid hardened Wave 1-2 |
| VirtualBox | vboxd | ✅ x86 only |

#### Custom Red Bear Daemons

| Daemon | Quality | Notes |
|--------|---------|-------|
| `firmware-loader` | ✅ Well-tested | 18 unit tests; scheme:firmware with read/mmap; no signing |
| `redox-drm` | 🚡 Bounded compile | AMD+Intel+VirtIO display; 68 tests; no HW validation |
| `amdgpu` | 🚡 Bounded compile | Imported Linux DC/TTM/core; partial display glue |
| `iommu` | 🚡 QEMU-proven | AMD-Vi detection + first-use proof; no HW validation |
| `udev-shim` | ✅ Present | Scheme:udev with device enumeration |
| `evdevd` | ✅ Present | Linux-compatible evdev interface |
| `redbear-sessiond` | ✅ Present | D-Bus login1 session broker |
| `redbear-wifictl` | 🚡 Host-tested | Wi-Fi control daemon; no real hardware |
| `redbear-iwlwifi` | 🚡 Host-tested | Intel transport; ~2450 lines C + ~1550 lines Rust; 119 tests |
| `redbear-btusb` | 🔴 Experimental | BLE-first; USB-attached only; QEMU validation in progress |
| `redbear-authd` | ✅ Present | Local-user authentication |
| `redbear-greeter` | 🚡 Partial | Greeter orchestrator; Qt Wayland integration broken |
| `redbear-netctl` | ✅ Present | Network profile management |
| `redbear-hwutils` | ✅ Present | lspci, lsusb, phase checkers |

### 2.3 Firmware Loading

**What exists:**
- `scheme:firmware` daemon (`firmware-loader`) indexes blobs from `/lib/firmware/`
- `linux-kpi` provides `request_firmware()` via Rust FFI
- AMD GPU blobs (675 .bin files) in `local/firmware/amdgpu/` (gitignored, fetched from linux-firmware)
- Intel DMC display blobs fetchable via `fetch-firmware.sh --vendor intel --subset dmc`
- Two fetch mechanisms: standalone script (selective) + build-time meta-package (full linux-firmware)
- `PCI_QUIRK_NEED_FIRMWARE` flag defined (bit 11), but never checked by any driver

**What is MISSING vs Linux 7.0 `firmware_class`:**
- No firmware signing/verification (no `module_sig_check` equivalent)
- No `request_firmware_nowait` with uevent dispatch to userspace helper (Linux uses `/sys/$DEVPATH/loading` + `/sys/$DEVPATH/data` + uevent to notify udev)
- No persistent firmware cache between boots (in-memory only; Linux caches during suspend for resume-fastpath)
- No fallback firmware variant search (if dmcub_dcn31.bin missing, try dmcub_dcn30.bin; Linux has per-driver firmware search paths)
- No `/sys/firmware/` interface (Linux exposes firmware loading status via sysfs)
- No firmware preloading at driver bind time
- No timeout for synchronous `request_firmware` (blocks forever; Linux times out after ~60s with uevent fallback)
- No platform firmware fallback (Linux can search UEFI firmware volumes via `firmware_request_platform()`)
- No Wi-Fi firmware blobs (iwlwifi, ath10k, etc.)
- No Bluetooth firmware blobs
- No audio/media codec firmware
- Firmware lookup limited to 3 hardcoded paths (Linux searches: `/lib/firmware/`, `/lib/firmware/updates/`, `/lib/firmware/$KVER/`, `/usr/lib/firmware/`, `/usr/share/firmware/`, plus custom path via kernel param)

### 2.4 Hardware Validation Status

| Subsystem | QEMU | Bare Metal | Notes |
|-----------|------|------------|-------|
| ACPI boot | ✅ | ✅ (AMD) | Boot-baseline; `_S5` shutdown not release-grade |
| x2APIC/SMP | ✅ | ✅ | Multi-core works |
| PCI enumeration | ✅ | ✅ | pcid enumerates devices |
| MSI-X | ✅ (virtio-net) | ❌ | No hardware proof |
| IOMMU/AMD-Vi | ✅ (first-use) | ❌ | Detection works; no HW validation |
| xHCI interrupt | ✅ | ❌ | Interrupt mode proven; no HW |
| USB storage | ✅ (readback) | ❌ | QEMU mass-storage proof |
| NVMe | ✅ | ❌ | Builds; no HW |
| AHCI | ✅ | ❌ | Builds; no HW |
| Network (e1000/virtio) | ✅ | ❌ | QEMU only |
| PS/2 keyboard | ✅ | ✅ | QEMU + AMD bare metal |
| USB keyboard | ✅ (QEMU HID) | ⚠️ | Not reliable on bare metal |
| Wi-Fi | ❌ | ❌ | Host-tested transport only |
| Bluetooth | ❌ | ❌ | Experimental BLE; QEMU in progress |

### 2.5 Comparison with Linux 7.0 Device Init Model

#### 2.5.1 Linux Initcall Ordering (Reference)

Linux uses a 10-level initcall system for boot-phase ordering:

| Level | Macro | Typical Count | Example Uses |
|-------|-------|---------------|--------------|
| 0 | `pure_initcall` | ~few | Pure infrastructure |
| early | `early_initcall` | ~446 | mm init, early console, DT scan |
| 1 | `core_initcall` | ~614 | Workqueues, RCU, memory allocators |
| 2 | `postcore_initcall` | ~150 | Clocksource, scheduler, IRQ core |
| 3 | `arch_initcall` | ~751 | PCI bus init, ACPI table parsing, CPU bringup |
| 4 | `subsys_initcall` | ~573 | PCI enumerate, USB core, networking core, block |
| 5 | `fs_initcall` | ~1372 | Filesystem registration |
| 6 | `device_initcall` | ~1211 | Most drivers; `module_init()` maps here |
| 7 | `late_initcall` | ~440 | Late init, debug, tracing |

Red Bear OS has **no equivalent ordering mechanism** — the TOML-based init uses `requires_weak`
for loose ordering but has no topological sort depth, no `Before`/`After` fields, no explicit
init phases beyond the coarse initfs/rootfs split.

#### 2.5.2 Feature Comparison Table

| Feature | Linux 7.0 | Red Bear OS | Gap |
|---------|-----------|-------------|-----|
| **Driver model** | `bus_type` → `device_driver` → `probe()` binding with match tables | `pcid-spawner` spawns drivers by PCI class/vendor/device | 🟡 Partial — single-shot spawn, no rebinding |
| **Deferred probing** | `driver_deferred_probe` — retries when dependency arrives; `-EPROBE_DEFER` triggers retry on any successful probe | None | 🔴 Missing — must be present at boot |
| **Async probing** | `async_probe` — parallel driver init via kthreadd workers | Sequential spawn only | 🟡 Partial — oneshot_async for launch but not true async init |
| **Hotplug** | uevent netlink → udev → driver bind/unbind; `/sbin/hotplug` path | `udev-shim` is a **static PCI enumerator** — one scan at boot, no event callbacks, no device removal handling | 🔴 Missing — no hotplug infrastructure at all |
| **Firmware loading** | `firmware_class` with `request_firmware`, user helper, caching | `scheme:firmware` + `linux-kpi` request_firmware | 🟡 Partial — no uevent/helper/caching |
| **USB controllers** | xHCI, EHCI, OHCI, UHCI — all supported | xHCI only | 🔴 Missing — EHCI/OHCI/UHCI absent |
| **USB device classes** | HID, storage, audio, video, CDC, vendor, etc. | HID, hub, storage (BOT), CSI (UCSI) | 🟡 Partial — many classes missing |
| **Power management** | Suspend/resume, runtime PM, CPU freq scaling, thermal | `_S5` shutdown only | 🔴 Missing — no S3/S4/PM |
| **Interrupt handling** | Full APIC/x2APIC, MSI/MSI-X, affinity, NMI, MCE | APIC/x2APIC; MSI-X via quirks | 🟡 Partial — no affinity, no NMI watchdog |
| **IOMMU** | AMD-Vi, Intel VT-d with DMA remapping + IR | AMD-Vi detection + first-use proof | 🟡 Partial — no VT-d, no hardware |
| **ACPI namespace** | Full namespace: devices, thermal, battery, processor, etc. | Boot-baseline: MADT, FADT, `_S5`, bounded power | 🟡 Partial — many ACPI objects missing |
| **PCIe features** | AER, ACS, ATS, PRI, PASID, SR-IOV | Basic PCI config space only | 🔴 Missing — no advanced PCIe |
| **Device naming** | Predictable network/storage names (systemd udev) | None | 🟡 Partial — no naming policy |
| **Hardware RNG** | `hw_random` framework, multiple drivers | None | 🔴 Missing |
| **CPU frequency** | `cpufreq` governors | None | 🔴 Missing |
| **Thermal management** | `thermal` framework + drivers | None | 🔴 Missing |
| **SMBIOS/DMI** | Full DMI table exposure via sysfs | Quirks system has DMI data | 🟡 Partial — not runtime-exposed |
| **Kernel cmdline** | Device parameters via boot cmdline | None | 🔴 Missing |

## 3. Implementation Phases

### Phase 1 — Driver Model Maturation (Weeks 1-8)

**Goal:** Establish a proper device driver model with binding semantics, deferred probing,
and error resilience — bringing the driver infrastructure to Linux 7.0 par without rewriting
existing drivers.

#### 1.1 Device-Driver Binding Model (Week 1-3)

Create a `redox-driver-core` library providing Linux-style bus/device/driver abstractions:

```
Device → Driver matching:
  pcid: class=0x01, subclass=0x08 → nvmed
  pcid: vendor=0x8086, device=0x10D3 → e1000d

Driver probe() returns:
  Ok(())       → device bound, driver active
  Err(ENODEV)  → device not supported by this driver
  Err(EAGAIN)  → dependency not available, DEFER probe
  Err(...)      → fatal error, device unusable
```

**Deliverables:**
- `redox-driver-core` crate with `Bus`, `Device`, `Driver` traits
- `pcid` exposes devices via new scheme: `scheme:pci/devices/{id}/bind`
- `pcid-spawner` replaced by `driver-manager` daemon that:
  - Reads driver match tables from `/lib/drivers.d/*.toml`
  - Probes drivers in priority order
  - Supports deferred probing (EAGAIN → retry when dependency appears)
  - Supports driver unbind/rebind
- All existing `pcid.d/*.toml` match files migrated to new format
- Backward compatible: existing pcid-spawner behavior preserved as fallback

#### 1.2 Async Device Probing (Week 4-5)

**Deliverables:**
- `driver-manager` probes independent device trees in parallel (using Rust async or threads)
- Device init order defined by dependency DAG, not sequential spawn
- Timing observability: log probe duration per driver
- `CONFIG_PARALLEL_PROBE` equivalent: max concurrent probes tunable via config TOML

#### 1.3 Driver Parameter System (Week 6-7)

**Deliverables:**
- Kernel cmdline parsing in bootloader (e.g., `redbear.nvme.irq_mode=msi`)
- `/scheme/sys/driver/{name}/parameters` read/write
- Driver authors declare parameters via derive macro
- `lspci -v` shows per-device parameters

#### 1.4 Hotplug Infrastructure (Week 7-8)

**Deliverables:**
- PCIe hotplug: `pcid` detects surprise removal/addition, emits uevent
- USB hotplug: `xhcid` emits uevent on device attach/detach
- `udev-shim` enhanced to receive uevents and trigger driver binding
- `driver-manager` handles hot-add (probe driver) and hot-remove (unbind driver)
- Initial scope: PCIe hotplug and USB hotplug only; Thunderbolt deferred

**Phase 1 Exit Criteria:**
- New driver binding model functional for 3+ existing drivers (nvmed, e1000d, xhcid)
- Deferred probing works: driver returning EAGAIN retries when dependency scheme appears
- Async probing measurable: 2+ independent PCI devices probe concurrently
- Hotplug works: USB device attach/detach triggers udev-shim + driver bind/unbind in QEMU
- All 25+ existing drivers still compile and function (backward compatibility)

### Phase 2 — Controller Coverage & Hardware Validation (Weeks 5-14)

**Goal:** Fill the critical controller gaps (USB EHCI/OHCI/UHCI) and validate the
existing controller stack on real hardware — especially MSI-X, IOMMU, and xHCI.

#### 2.1 USB Controller Family Completion (Week 5-9)

This is the **highest-impact controller gap** because it directly blocks reliable
USB keyboard input on bare metal where the keyboard may be routed through companion
controllers rather than xHCI.

**Deliverables:**
- `ehcid` daemon — EHCI (USB 2.0) host controller driver
- `ohcid` daemon — OHCI (USB 1.1) host controller driver for non-Intel chipsets
- `uhcid` daemon — UHCI (USB 1.1) host controller driver for Intel chipsets
- USB companion controller routing: when xHCI owns the ports, companion controllers
  hand off low/full-speed devices to xHCI transparently
- `usb-manager` daemon orchestrates multi-controller topology:
  - Single `scheme:usb` root exposing all buses
  - Device path stability across controller types
  - Port routing table for companion controller ownership handoff
- USB 3.1/3.2 SuperSpeedPlus support in xhcid (10 Gbps, 20 Gbps)
- USB-C PD/alt-mode awareness in `ucsid`

**Implementation approach:**
- EHCI: Reference Linux `drivers/usb/host/ehci-hcd.c` (~6000 lines) and FreeBSD `sys/dev/usb/controller/ehci.c`
- OHCI: Reference Linux `drivers/usb/host/ohci-hcd.c` (~3000 lines)
- UHCI: Reference Linux `drivers/usb/host/uhci-hcd.c` (~2500 lines)
- All three controllers use the same `scheme:usb` interface — class daemons (usbhubd, usbhidd, usbscsid) work unchanged

#### 2.2 xHCI Device-Level Hardening (Week 8-10)

Per the existing `XHCID-DEVICE-IMPROVEMENT-PLAN.md`:

**Deliverables:**
- Atomic device attach publication (prevent half-attached devices)
- Bounded device detach and purge
- Configure rollback on failure
- Real PM sequencing (U0/U1/U2/U3 transitions)
- Enumerator cleanup and timing hardening
- Growable event ring under sustained activity

#### 2.3 MSI-X Hardware Validation (Week 8-11)

Per the existing `IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md` Priority 1:

**Deliverables:**
- AMD GPU MSI-X validation: prove MSI-X vectors fire correctly on real AMD hardware
- Intel GPU MSI-X validation: prove MSI-X on Intel hardware
- NVMe MSI-X validation: prove per-queue interrupt vectors
- xHCI MSI-X validation: prove interrupt-driven event ring on real hardware (not just QEMU)
- Verified MSI-X → MSI → legacy IRQ fallback on all tested hardware
- Logged CPU/vector affinity behavior
- At minimum one AMD and one Intel bare-metal test report per device class

#### 2.4 IOMMU Hardware Bring-Up (Week 9-14)

Per the existing `IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md` Priority 2:

**Deliverables:**
- Validated AMD-Vi initialization on real AMD hardware
- Device table / command buffer / event log validation
- Interrupt remapping validation
- Intel VT-d initial detection and register mapping (not full bring-up)
- IOMMU fault-path validation: inject fault, verify event log capture
- DMA remapping proof: verify device DMA is translated through IOMMU page tables
- Negative-result documentation if hardware still fails

#### 2.5 ACPI Wave 1-2 Completion (Week 10-12)

Per the existing `ACPI-IMPROVEMENT-PLAN.md` Waves 1-2:

**Deliverables:**
- Finish replacing panic-grade `expect` paths in `acpid` startup
- Define and document AML bootstrap contract (explicit RSDP_ADDR producer)
- Table-specific reject/warn/degrade/fail rules implemented
- Deterministic `_S5` derivation (not dependent on PCI timing)
- Explicit shutdown/reboot result semantics
- Bounded shutdown proof on real AMD and Intel hardware
- Sleep-state scope explicit: S5 only; S3/S4 explicitly deferred

**Phase 2 Exit Criteria:**
- At least one EHCI or OHCI/UHCI driver functional in QEMU
- USB keyboard reliably reachable on bare metal AMD and Intel (via xHCI, EHCI, or companion routing)
- MSI-X validated on at least one real AMD GPU and one real Intel GPU
- IOMMU AMD-Vi validated on at least one real AMD machine
- ACPI `_S5` shutdown works on at least one real AMD and one real Intel machine
- ACPI startup contains zero panic-grade paths reachable from firmware input

### Phase 3 — Power Management & Platform Services (Weeks 12-20)

**Goal:** Add suspend/resume, CPU frequency scaling, thermal management, and hardware
RNG — bringing platform services to Linux 7.0 par for basic functionality.

#### 3.1 ACPI Power Management (Week 12-14)

Per the existing `ACPI-IMPROVEMENT-PLAN.md` Waves 3-4:

**Deliverables:**
- Honest `/scheme/acpi/power` surface: exposes only behavior with runtime evidence
- Consumer-visible distinction between unsupported, unavailable, and populated power state
- Reduced surface: remove misleading empty-success defaults
- AML physmem/EC failure propagation: no correctness-critical fabricated values
- EC error typing and documented widened-access behavior
- Documented AML mutex timeout behavior

#### 3.2 Suspend/Resume (S3 Sleep) — Initial Implementation (Week 13-16)

**Deliverables:**
- Kernel: save/restore CPU context (CR0-CR4, MSRs, IDT/GDT, FPU/SSE/AVX state)
- Kernel: ACPI S3 (suspend-to-RAM) entry via `_S3` AML method
- Kernel: wake vector registration and resume path
- `acpid`: expose `/scheme/acpi/sleep` with `S3` and `S5` states
- Device contract: `suspend()` callback on each scheme daemon
  - Storage: flush caches, park heads (if spinning)
  - Network: bring link down, save MAC filter state
  - USB: save controller/port state
  - Graphics: save mode, blank display
- `driver-manager`: suspend devices in dependency order, resume in reverse
- Initial scope: S3 only on test hardware; S4 (hibernate) explicitly deferred

#### 3.3 CPU Frequency Scaling (Week 14-16)

**Deliverables:**
- `cpufreqd` daemon reading ACPI `_PSS` / `_PPC` objects
- Intel: P-state MSR writes (IA32_PERF_CTL)
- AMD: P-state MSR writes + CPPC awareness
- Governors: `performance` (max freq), `powersave` (min freq), `ondemand` (load-based)
- `/scheme/cpufreq` for reading/setting governor and frequency
- `redbear-info` shows current frequency and governor

#### 3.4 Thermal Management (Week 15-17)

**Deliverables:**
- `thermald` daemon reading ACPI thermal zone objects (`_TMP`, `_PSV`, `_TC1`, `_TC2`)
- Active cooling: fan control via ACPI `_SCP`
- Passive cooling: CPU throttling via cpufreqd integration
- Critical shutdown: if temperature exceeds `_CRT`, initiate clean shutdown
- `/scheme/thermal` for reading zone temperatures and trip points
- `redbear-info` shows thermal zone status

#### 3.5 Hardware RNG (Week 16-17)

**Deliverables:**
- `hwrngd` daemon reading hardware RNG sources:
  - x86 RDRAND/RDSEED instructions
  - TPM 2.0 random number generator (if present)
  - VirtIO entropy device
- `scheme:hwrng` feeding into `randd` entropy pool
- `/scheme/hwrng` exposes raw entropy and health status
- Linux 7.0 `hw_random` framework ported conceptually (not literally)

#### 3.6 PCIe Advanced Error Reporting (Week 17-18)

**Deliverables:**
- `pcid` exposes AER capability registers via `/scheme/pci/{dev}/aer`
- AER error detection: correctable and uncorrectable error status registers
- Error logging: decode error source (data link, transaction, poison TLP, etc.)
- `aer-inject` utility for testing error paths
- Initial scope: error detection and logging only; error recovery (device reset path) deferred

#### 3.7 SMBIOS/DMI Runtime Exposure (Week 18-20)

**Deliverables:**
- `dmidecode`-equivalent utility using `acpid` DMI scheme
- `/scheme/dmi` exposes SMBIOS entry point and table data
- `lspci -v` shows DMI-based quirk annotations
- DMI data feeding into `redbear-info` for platform identification
- Integration with existing quirks system: DMI match rules validated at runtime

**Phase 3 Exit Criteria:**
- S3 suspend/resume works on at least one real machine (AMD or Intel)
- CPU frequency scaling observable via `redbear-info`
- Thermal zone temperature readable and critical shutdown testable
- Hardware RNG feeding entropy pool
- PCIe AER errors logged on capable hardware
- DMI data accessible via scheme and tools
- All new schemes documented with test procedures

### Phase 4 — Firmware Infrastructure & Wi-Fi Validation (Weeks 16-24)

**Goal:** Close firmware loading gaps, complete Wi-Fi hardware validation with real
firmware, and establish firmware management as a first-class platform service.

#### 4.1 Firmware Loading Gap Closure (Week 16-18)

**Deliverables:**
- `request_firmware_nowait` with proper uevent dispatch:
  - Async request → uevent → `udev-shim` listens → `firmware-loader` serves blob
  - Timeout: if firmware not available within configurable timeout, fail gracefully
- Firmware fallback variant search:
  - If `dmcub_dcn31.bin` not found, try `dmcub_dcn30.bin`, `dmcub_dcn20.bin`
  - Per-driver fallback chain defined in `/etc/firmware-fallbacks.d/*.toml`
- Persistent firmware cache (`/var/lib/firmware/`):
  - Loaded blobs cached on first use; survive daemon restart
  - Cache invalidation on firmware version change
- `PCI_QUIRK_NEED_FIRMWARE` enforcement:
  - Drivers actually check the flag via `pci_has_quirk()`
  - When flag is set: require firmware at probe time, fail probe if absent
  - When flag is absent: firmware is optional, warn if missing but continue
- Fetch Intel Wi-Fi firmware blobs: `fetch-firmware.sh --vendor intel --subset wifi`
- Fetch Bluetooth firmware blobs where applicable
- Firmware manifest: `/lib/firmware/MANIFEST.txt` lists all blobs, versions, sources

#### 4.2 Wi-Fi Hardware Validation (Week 16-22)

Per the existing `WIFI-IMPLEMENTATION-PLAN.md`:

**Deliverables:**
- Real Intel Wi-Fi device (e.g., AX200/AX201/AX210) validated end-to-end
- `redbear-iwlwifi` transport:
  - Firmware loaded via `request_firmware()` → `scheme:firmware`
  - DMA ring operation validated (TX reclaim, RX restock, command dispatch)
  - Interrupt handling validated (MSI-X or MSI path)
  - Association/authentication cycle completed with real AP
- `redbear-wifictl` control plane:
  - Scan → connect → DHCP → disconnect cycle validated
  - WPA2-PSK and open network profiles functional
  - Profile persistence and boot-time application
- `redbear-netctl` Wi-Fi profiles:
  - SSID/Security/Key parsing validated
  - Bounded Wi-Fi lifecycle (prepare → init-transport → activate-nic → connect → disconnect)
- Wi-Fi runtime diagnostics:
  - `redbear-phase5-wifi-check` reports link quality, signal strength, connected AP
  - `redbear-info --verbose` shows Wi-Fi adapter status
- At minimum one real Intel Wi-Fi chipset validated
- Legacy IRQ fallback for platforms where MSI-X is unavailable (via quirks)

#### 4.3 Wi-Fi Desktop API (Week 20-24)

**Deliverables:**
- D-Bus Wi-Fi API on system bus: `org.freedesktop.NetworkManager` subset
  - `GetDevices`, `GetAccessPoints`, `ActivateConnection`, `DeactivateConnection`
  - Signal: `AccessPointAdded`, `AccessPointRemoved`, `StateChanged`
- `redbear-wifictl` exposes D-Bus interface for desktop consumption
- `redbear-netctl` GUI client for scanning and connecting (Qt6-based, optional)
- Desktop status bar Wi-Fi indicator (future KDE plasma-nm integration)

**Phase 4 Exit Criteria:**
- `request_firmware_nowait` with uevent dispatch functional in QEMU
- PCI_QUIRK_NEED_FIRMWARE enforced in at least one driver (amdgpu or iwlwifi)
- Intel Wi-Fi chipset validated end-to-end with real AP
- Wi-Fi scan → connect → DHCP → internet access completed on real hardware
- Wi-Fi D-Bus API functional for at least get_devices and get_accesspoints
- Firmware manifest tracks all loaded blobs with versions

### Phase 5 — Bluetooth, Device Policy, Polish (Weeks 20-30)

**Goal:** Bring Bluetooth to validated experimental status, establish device naming policy,
and polish remaining gaps.

#### 5.1 Bluetooth Hardware Validation (Week 20-24)

Per the existing `BLUETOOTH-IMPLEMENTATION-PLAN.md`:

**Deliverables:**
- `redbear-btusb` transport validated with real USB Bluetooth adapter
- `redbear-btctl` HCI host validated:
  - Controller init sequence (reset, read local features, set event mask)
  - Device discovery (LE scan → advertising report → connect)
  - GATT service discovery
  - Basic data exchange (battery service, device info)
- BLE peripheral connect/disconnect cycle validated
- Bluetooth classic (BR/EDR) detection and basic inquiry (connect deferred)
- `redbear-bluetooth-battery-check` works on real hardware
- At minimum one real USB Bluetooth adapter validated

#### 5.2 Device Naming Policy (Week 22-24)

**Deliverables:**
- Predictable network interface names:
  - `enp0s1` instead of `eth0` (PCIe bus/device/function based)
  - `/etc/systemd/network/` equivalent rules in `/etc/udev/rules.d/`
- Predictable storage device names:
  - NVMe: `nvme0n1` instead of raw scheme path
  - AHCI: `sd{a,b,c}` assigned by port order
  - USB storage: `sdX` with stable enumeration
- `/dev/disk/by-id/`, `/dev/disk/by-path/`, `/dev/disk/by-uuid/` symlinks
- `udev-shim` enhanced with rule matching (vendor, model, serial, path patterns)

#### 5.3 Device Init Observability (Week 23-25)

**Deliverables:**
- Boot-time device init timeline: log each device probe start/end with duration
- `redbear-info --boot` shows device init timeline post-boot
- Per-device init status: `redbear-info --device pci/00:02.0`
- Kernel cmdline `redbear.init_verbose` enables verbose device init logging
- Boot-time warning summary: all drivers that probed with warnings or deferrals
- Device init health dashboard: `redbear-info --health` shows init status of all subsystems

#### 5.4 Remaining Gaps (Week 24-30)

**Deliverables:**
- `nvmed` hardware validation: prove NVMe I/O on real hardware
- `ahcid` hardware validation: prove SATA I/O on real hardware
- `ihdad` hardware validation: prove audio output on real hardware
- USB device class coverage expanded:
  - USB CDC ACM (serial): `usbcdcd` daemon
  - USB CDC ECM/NCM (ethernet): `usbnetd` daemon (or integrate into existing net drivers)
  - USB Audio Class 1/2: `usbaudiod` daemon
- GPU hardware acceleration readiness:
  - Mesa radeonsi backend proof-of-concept (single draw call)
  - KMS atomic modesetting proof on real hardware (not just QEMU)
- `redbear-btusb` autospawn via USB class matching
- `kstop` shutdown event: gracefully stop all device daemons before power-off

**Phase 5 Exit Criteria:**
- Bluetooth BLE discovery and basic data exchange works on real hardware
- Network interfaces use predictable names on QEMU and bare metal
- Device init timeline observable via `redbear-info --boot`
- NVMe I/O validated on at least one real NVMe drive
- Real audio output validated on at least one HDA codec
- At least one USB device class beyond HID/storage validated (audio, serial, or ethernet)
- All 25+ existing drivers maintain backward compatibility

## 4. Dependency Graph

```
Phase 1 (Driver Model) ─────────────────────────────┐
  ├── 1.1 Binding Model                              │
  ├── 1.2 Async Probing (after 1.1)                  │
  ├── 1.3 Driver Parameters (after 1.1)              │
  └── 1.4 Hotplug (after 1.1)                        │
                                                     │
Phase 2 (Controllers) ───────────────────────────────┤
  ├── 2.1 USB EHCI/OHCI/UHCI (parallel with 1.2)     │
  ├── 2.2 xHCI Hardening (parallel with 1.2)         │
  ├── 2.3 MSI-X HW Validation (after 1.1)            │
  ├── 2.4 IOMMU HW Bring-Up (parallel with 2.3)      │
  └── 2.5 ACPI Wave 1-2 (parallel with 2.3)          │
                                                     │
Phase 3 (Power Mgmt) ────────────────────────────────┤
  ├── 3.1 ACPI Wave 3-4 (after 2.5)                  │
  ├── 3.2 Suspend/Resume (after 3.1)                 │
  ├── 3.3 CPU Freq Scaling (parallel with 3.2)       │
  ├── 3.4 Thermal Mgmt (after 3.1, parallel 3.3)     │
  ├── 3.5 Hardware RNG (parallel with 3.3)           │
  ├── 3.6 PCIe AER (after 2.3)                       │
  └── 3.7 SMBIOS/DMI (parallel with 3.6)             │
                                                     │
Phase 4 (Firmware + Wi-Fi) ──────────────────────────┤
  ├── 4.1 Firmware Gaps (after 1.1)                  │
  ├── 4.2 Wi-Fi HW (after 4.1, parallel with 2.3)    │
  └── 4.3 Wi-Fi Desktop API (after 4.2)              │
                                                     │
Phase 5 (Bluetooth + Polish) ────────────────────────┤
  ├── 5.1 BT HW Validation (parallel with 4.2)       │
  ├── 5.2 Device Naming (after 1.1)                  │
  ├── 5.3 Init Observability (after 1.2)             │
  └── 5.4 Remaining Gaps (after 3.2, 4.2, 5.1)      │
```

## 5. Resource Estimates

| Phase | Duration | Engineers | Key Risk |
|-------|----------|-----------|----------|
| Phase 1 | 8 weeks | 2 | Over-engineering the driver model; must stay backward compatible |
| Phase 2 | 6-9 weeks | 3 (parallelizable) | Real hardware availability; USB controller complexity |
| Phase 3 | 8 weeks | 2-3 | ACPI firmware quality varies wildly on real hardware |
| Phase 4 | 8 weeks | 2 | Wi-Fi hardware procurement; firmware licensing |
| Phase 5 | 10 weeks | 2 | Long tail of device class drivers |

**Total:** 26-40 weeks (~6-10 months) with 2-3 engineers, depending on parallelism and
hardware availability.

## 6. Risk Register

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| No access to AMD GPU with MSI-X | Medium | High | Partner with community; use Intel GPU as alternative |
| No access to AMD machine with IOMMU | Medium | High | Prioritize Intel VT-d if AMD hardware unavailable |
| USB EHCI/OHCI/UHCI significantly harder than estimated | Medium | High | Scope to EHCI-only initially; UHCI/OHCI deferred |
| ACPI firmware corruption on test machines causes false failures | High | Medium | Test on 3+ machines per platform class |
| Wi-Fi firmware licensing prevents redistribution | Low | Medium | Keep firmware external (fetched, not committed) |
| Existing driver regression from new driver model | Medium | High | Extensive backward compat testing; parallel old/new paths |
| S3 suspend/resume crashes unrecoverably on some hardware | High | Medium | Gate behind config flag; S3 is opt-in initially |

## 7. Success Criteria (Definition of Done)

This plan is complete when:

1. **Driver Model:** New driver binding model works for all existing drivers; deferred probing
   retries correctly; async probing measurably parallel; hotplug adds/removes devices without reboot.

2. **USB Controllers:** At least one non-xHCI controller (EHCI preferred) functional; USB keyboard
   reliable on bare metal AMD and Intel.

3. **Hardware Validation:** MSI-X proven on real AMD + Intel GPU; IOMMU AMD-Vi proven on real
   AMD machine; ACPI `_S5` shutdown proven on real AMD + Intel; NVMe I/O proven on real hardware.

4. **Power Management:** S3 suspend/resume works on at least one real machine; CPU frequency
   scaling observable; thermal shutdown testable.

5. **Firmware:** `request_firmware_nowait` with uevent dispatch; `PCI_QUIRK_NEED_FIRMWARE`
   enforced; Wi-Fi firmware loaded end-to-end on real hardware.

6. **Wi-Fi:** Intel Wi-Fi chipset validated end-to-end with real AP; scan → connect → DHCP →
   internet access verified.

7. **Bluetooth:** BLE discovery and basic data exchange on real hardware; HCI init sequence
   validated; GATT service discovery functional.

8. **Observability:** Device init timeline observable; per-device init status queryable;
   boot-time warning summary available.

9. **No regressions:** All 25+ existing drivers still work; all QEMU validation scripts still pass;
   `redbear-mini` and `redbear-full` still boot to login prompt.

## 8. Relationship to Existing Plans

This plan is the **canonical device initialization plan**. It supersedes or integrates with:

| Existing Plan | Relationship |
|---------------|-------------|
| `IRQ-AND-LOWLEVEL-CONTROLLERS-ENHANCEMENT-PLAN.md` | Absorbed: MSI-X (P1), IOMMU (P2) become Phase 2.3-2.4 here |
| `ACPI-IMPROVEMENT-PLAN.md` | Integrated: Waves 1-4 become Phase 2.5 + Phase 3.1-3.2 here |
| `USB-IMPLEMENTATION-PLAN.md` | Integrated: xHCI hardening + controller gaps become Phase 2.1-2.2 here |
| `XHCID-DEVICE-IMPROVEMENT-PLAN.md` | Integrated: 7-phase xhcid plan consolidated into Phase 2.2 here |
| `WIFI-IMPLEMENTATION-PLAN.md` | Absorbed: Wi-Fi hardware validation becomes Phase 4.2 here |
| `BLUETOOTH-IMPLEMENTATION-PLAN.md` | Absorbed: BT validation becomes Phase 5.1 here |
| `BOOT-PROCESS-ASSESSMENT.md` | Input: boot flow, service ordering, pcid-spawner fix already applied |
| `BOOT-PROCESS-IMPROVEMENT-PLAN.md` | Input: kernel 4GiB fix, DRM/KMS, greeter UI (already addressed) |
| `CONSOLE-TO-KDE-DESKTOP-PLAN.md` | Orthogonal: this plan focuses on device init, not desktop path |

Existing plans remain as reference material for historical detail and subsystem-specific
technical depth. This plan is the execution authority for sequencing and acceptance criteria.

## 9. Immediate Next Actions (Week 1 Priorities)

1. **Create `redox-driver-core` crate** — define `Bus`, `Device`, `Driver` traits
2. **Read Linux 7.0 `drivers/base/driver.c`** — understand the driver binding model to adapt
3. **Audit `pcid` scheme interface** — what device info is already exposed vs what's needed
4. **Select USB EHCI reference implementation** — Linux `ehci-hcd.c` or FreeBSD `ehci.c`
5. **Procure test hardware** — at minimum: one AMD machine with AMD GPU + one Intel machine with Intel GPU
6. **Set up USB keyboard test matrix** — catalog existing USB keyboards and host controllers
7. **Create firmware manifest template** — define format for `/lib/firmware/MANIFEST.txt`
8. **Schedule MSI-X hardware validation session** — reserve time on test machines for Phase 2.3

---

*This plan will be updated as implementation progresses. Each phase section will receive
detailed task breakdown (similar to the ACPI and IRQ plans' execution slice format) before
that phase begins.*
