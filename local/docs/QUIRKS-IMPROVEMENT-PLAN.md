# Hardware Quirks Improvement Plan

## Purpose

This plan replaces vague “quirks support” follow-up work with a concrete path to:

1. keep quirks data and reporting honest,
2. integrate quirks into real runtime driver behavior,
3. reduce duplicated quirk logic,
4. leave DMI and USB device quirks in a maintainable state.

## Current status snapshot

Completed from this plan:

- runtime DMI TOML loading in `redox-driver-sys`,
- subsystem-gated PCI TOML matching in both the canonical path and `pcid-spawner`,
- shipped DMI TOML overrides in the brokered `pcid-spawner` env-var path,
- direct canonical `redox-driver-sys` quirk lookup from `pcid-spawner` instead of a separate in-tree PCI quirk engine,
- real USB device quirk consumption in `xhcid`,
- first real linux-kpi quirk consumption in the Red Bear amdgpu path.

Still open after this implementation wave:

- no remaining implementation items in the current quirks scope.

The runtime-behavior milestone from this plan is now implemented. The remaining work is
maintenance, validation depth, and future refinement rather than missing quirks behavior for the
shipped paths.

It is based on the current in-tree state of:

- `redox-driver-sys` as the canonical quirks library,
- `pcid-spawner` as an upstream-owned PCI launch broker that now brokers canonical quirks,
- `redox-drm`, `xhcid`, and the amdgpu Redox glue/runtime path as real runtime PCI quirk consumers,
- `lspci`, `lsusb`, and `redbear-info` as reporting surfaces.

## Reassessment Summary

### What is real today

- `redox-driver-sys` owns the canonical PCI/USB quirk flag definitions and lookup helpers.
- `redox-drm` consumes PCI quirks for interrupt fallback and `DISABLE_ACCEL`.
- `xhcid` consumes PCI controller quirks via `PCI_QUIRK_FLAGS` for IRQ mode selection and reset delay.
- `linux-kpi` exposes `pci_get_quirk_flags()` / `pci_has_quirk()` for C drivers, and amdgpu now consumes them in its Redox init path.
- `lspci` and `lsusb` surface active PCI/USB quirk flags for discovered devices.
- `redbear-info --quirks` reports configured TOML entries and DMI rule counts.

### What is still weak

- USB quirks now have a first real runtime consumer in `xhcid`, but broader USB-driver adoption is still missing.
- The `linux-kpi` bridge now has a first real in-tree C consumer: amdgpu uses it for firmware gating and quirk-aware IRQ expectation logging. Broader C-driver adoption is still missing.
- `pcid-spawner` still synthesizes a partial `PciDeviceInfo` instead of reusing a richer canonical PCI object, because it operates as an upstream-owned broker with a narrow interface.

### What should not be “fixed” in the wrong layer

- `firmware-loader` should stay a generic scheme service. `NEED_FIRMWARE` belongs in device driver policy, not in the firmware scheme daemon.
- `redbear-info` should describe configured and observable state; it should not pretend to prove runtime quirk application.

## Target Architecture

### Upstream-preference policy

When upstream Redox already provides the same functionality, the upstream path wins by default
unless the Red Bear-local implementation is materially better. For quirks and driver support,
this means the canonical path should converge on `redox-driver-sys` instead of preserving
lower-quality duplicate quirk engines as a steady state.

### Canonical rule

`redox-driver-sys` remains the authoritative quirks model:

- flag definitions,
- compiled-in tables,
- TOML parsing semantics,
- DMI matching behavior.

All other code should either:

1. call the canonical lookup directly, or
2. receive lookup results from a single broker that is guaranteed to use the same semantics.

### Driver integration rule

- **Rust PCI drivers using `redox-driver-sys`** should call `info.quirks()` directly.
- **C drivers using `linux-kpi`** should call `pci_has_quirk()` / `pci_get_quirk_flags()` directly in probe/init paths.
- **Upstream base drivers that cannot depend on `redox-driver-sys`** may continue using brokered quirk bits from `pcid-spawner`, but only if that broker is made semantically identical to the canonical library.
- **USB device quirks** should be consumed inside `xhcid` device enumeration/configuration logic, not only in tooling.

## Concrete Work Plan

### Wave 1 — Cleanup and truthfulness

#### Task 1.1: Keep docs and reporting surfaces honest

Scope:

- `local/docs/QUIRKS-SYSTEM.md`
- `local/recipes/system/redbear-info/source/src/main.rs`
- related AGENTS references if needed

Goals:

- separate reporting surfaces from real runtime consumers,
- remove claims that imply driver integration where only tooling exists,
- keep “not yet implemented” items explicit.

QA:

- `cargo test` in `local/recipes/system/redbear-info/source`
- review `redbear-info --help` text and `--quirks` output strings

#### Task 1.2: Remove stale equivalence claims from extraction/documentation

Scope:

- `local/scripts/extract-linux-quirks.py`
- `local/docs/QUIRKS-SYSTEM.md`

Goals:

- avoid mapping Linux flags to incorrect Red Bear flags,
- clearly mark heuristic extraction limits for PCI handler-name mode.

QA:

- run the script on a small synthetic USB/PXI input sample,
- confirm output omits unsupported PCI flag mappings instead of inventing equivalents.

### Wave 2 — Unify PCI quirk semantics

#### Task 2.1: Eliminate semantic drift between `pcid-spawner` and `redox-driver-sys`

Constraint:

- `pcid-spawner` is upstream-owned base code, so any convergence work must be implemented as upstream-base changes carried by Red Bear patching until upstream absorbs them.

Best approach:

- make `pcid-spawner` consume generated/shared quirk data instead of hand-maintained duplicated tables and flag maps.

Preferred implementation options, in order:

1. **Shared generated data module** used by both `redox-driver-sys` and `pcid-spawner`.
2. **Protocol extension** where a single canonical broker calculates quirk bits and hands them to drivers.
3. Keep duplication only as a short-term fallback if generation is not yet practical.

Do **not** continue manually editing two separate PCI quirk engines long-term.

Success criteria:

- one authoritative source for compiled PCI quirk entries and flag name mapping,
- subsystem matching behavior aligned,
- explicit decision on whether DMI is brokered by `pcid-spawner` or left to driver-local lookup.

QA:

- compare quirk outputs for the same synthetic PCI info through both paths,
- verify `PCI_QUIRK_FLAGS` emitted by `pcid-spawner` matches canonical lookup for representative devices.

#### Task 2.2: Decide DMI ownership clearly

Decision needed:

- either `pcid-spawner` becomes DMI-aware and brokers the final PCI quirk bitmask,
- or `pcid-spawner` remains PCI/TOML-only and DMI stays driver-local in `redox-driver-sys` consumers.

Recommendation:

- near term: document the split clearly,
- medium term: move toward one brokered result for upstream base drivers.

QA:

- one design note added to the docs explaining the chosen ownership model.

### Wave 3 — Real driver integration

#### Task 3.1: Integrate USB device quirks in `xhcid`

Best integration points:

- after device descriptor read,
- before SetConfiguration,
- before enabling LPM/U1/U2 or USB3-specific behavior,
- after reset paths where extra delay or reset-after-probe is needed.

Minimum runtime behaviors to wire first:

- `NO_SET_CONFIG`
- `NEED_RESET`
- `NO_LPM`
- `NO_U1U2`
- `BAD_DESCRIPTOR`

Success criteria:

- `xhcid` calls `lookup_usb_quirks()` for enumerated devices,
- these flags alter runtime behavior in concrete branches,
- tooling and runtime logic agree on the same device-level quirks.

QA:

- unit/integration tests for selector logic where possible,
- manual logging proof that a known vendor/product entry triggers the expected path.

#### Task 3.2: Consume linux-kpi quirks in `amdgpu`

Best integration points:

- probe path,
- IRQ mode selection,
- firmware gating,
- memory/power-management setup.

First flags to consume:

- `NO_MSI`
- `NO_MSIX`
- `NEED_FIRMWARE`
- `NO_ASPM`
- `NEED_IOMMU`

Success criteria:

- at least one real C driver uses `pci_has_quirk()` in production code,
- runtime logs show quirk-informed decision making.

Current state:

- `local/recipes/gpu/amdgpu/source/amdgpu_redox_main.c` now queries linux-kpi PCI quirks in the real Redox runtime path,
- `PCI_QUIRK_NEED_FIRMWARE` turns missing DMCUB firmware into an init failure instead of a warning-only fallback,
- logs now show the active quirk bitmask plus the implied IRQ fallback policy.

QA:

- `grep` shows real in-tree call sites in amdgpu,
- build passes for linux-kpi + amdgpu recipe path.

#### Task 3.3: Keep firmware policy in drivers, not firmware-loader

Action:

- when a driver has `NEED_FIRMWARE`, the driver should gate initialization until the firmware load succeeds.
- `firmware-loader` remains a transport/provider only.

Success criteria:

- docs stop implying that firmware-loader interprets quirk flags,
- driver init paths own the policy decision.

QA:

- driver code path shows firmware gating tied to quirks or explicit device rules.

### Wave 4 — DMI completion

#### Task 4.1: DMI TOML runtime loading

Scope:

- `toml_loader.rs` parses `[[dmi_system_quirk]]`,
- matching uses live DMI info served by `acpid` at `/scheme/acpi/dmi`,
- resulting PCI quirk overrides flow through the canonical `redox-driver-sys` DMI path.

Success criteria:

- `50-system.toml` entries are no longer config-only,
- runtime DMI TOML behavior is testable and documented through the live `acpid` DMI scheme.

QA:

- tests for TOML parsing,
- one mock DMI input path proving a TOML DMI rule applies flags.

#### Task 4.2: ACPI blacklist/override layer

Current state:

- `acpid` now supports narrow `[[acpi_table_quirk]]` skip rules, optionally gated by the same
  DMI-style `match.*` fields used elsewhere.
- The implementation is intentionally limited to table suppression during ACPI table load; it is
  not a broad AML patching or firmware replacement framework.

## Suggested Immediate Deliverables

If work resumes right away, the next concrete implementation sequence should be:

1. clean remaining stale quirks docs/reporting text,
2. write a design note for canonical PCI quirk ownership,
3. integrate `lookup_usb_quirks()` into `xhcid` enumeration/configuration,
4. add first real `pci_has_quirk()` use in `amdgpu`,
5. validate and extend shipped DMI TOML coverage as needed.

## Exit Criteria For The Next Quirks Milestone

The next milestone is complete when all are true:

- `pcid-spawner` and `redox-driver-sys` no longer drift semantically,
- `xhcid` consumes USB device quirks at runtime,
- at least one real C driver consumes linux-kpi quirks,
- docs distinguish clearly between reporting, infrastructure, and true runtime behavior,
- DMI TOML entries are either runtime-applied or removed from shipped config.
