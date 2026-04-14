# 07 — Red Bear OS Implementation Plan

## Purpose

This document defines a clear, practical implementation plan for Red Bear OS. It focuses on
building a usable, validated system through disciplined packaging, profile-based composition, and
incremental hardware enablement, while preserving Redox architecture: userspace drivers, services,
and capability-oriented system boundaries.

This is the canonical public plan for how Red Bear OS should evolve at the repository level.
Detailed subsystem notes still live in focused documents such as
`local/docs/AMD-FIRST-INTEGRATION.md`, `docs/03-WAYLAND-ON-REDOX.md`, and
`docs/05-KDE-PLASMA-ON-REDOX.md`.

## Core Principles

### Preserve the architecture

- Keep drivers and services in userspace.
- Preserve clear system boundaries and capability-based design.
- Stay aligned with upstream Redox where practical.

### Packaging is the integration layer

- Deliver functionality as packages and package groups.
- Compose profiles from packages rather than monolithic changes.
- Prefer packaging and configuration work over invasive core-tree rewrites when possible.

### Keep Red Bear changes isolated

- Place Red Bear-specific work under `local/` whenever possible.
- Keep upstream-facing areas clean and rebase-friendly.
- Use scripts and overlays to make sync/rebase work predictable.

### Profiles are products

The supported product surfaces are:

- `redbear-minimal`
- `redbear-desktop`
- `redbear-full`
- `redbear-live`

Each profile must be buildable, testable, and documented.

### Validation over claims

- “Compiles” is not the same as “supported”.
- Every user-visible feature must map to a profile.
- Support status must be explicit, reproducible, and evidence-backed.

## System Structure

### Layers

1. **Upstream platform** — Redox kernel, libc, and core services
2. **Red Bear integration (`local/`)** — patches, recipes, configs, scripts, overlays
3. **Profiles** — concrete system builds assembled from packages and package groups

### Profiles

#### `redbear-minimal`

Console-focused.

- Boot
- Storage
- Package manager
- Wired networking

This is the primary development and validation target.

#### `redbear-desktop`

Wayland plus base desktop services.

This is the main integration environment for graphics, input, and desktop bring-up.

#### `redbear-full`

KDE Plasma target.

This profile should only include graphics and networking paths that are validated enough to be
presented as real user-facing system behavior.

#### `redbear-live`

Live, demo, and rescue environment.

This profile should prioritize diagnostics, recovery workflows, and installability.

## Packaging Model

### Package groups

- `base-core`
- `storage-base`
- `desktop-base`
- `wayland-base`
- `kde-base`
- `net-base`
- `net-vm`
- `net-wired-common`
- `net-wifi-experimental`
- `gpu-intel-experimental`
- `gpu-amd-experimental`
- `redbear-branding`
- `redbear-live-tools`
- `redbear-hwdiag`

### Rules

- All functionality is delivered via packages.
- Drivers are packaged individually.
- Profiles depend on package groups.
- Package metadata should make support status obvious.

## Workstreams

### 1. Repository discipline

Define and maintain contribution rules.

- Keep all custom work under `local/` where possible.
- Provide scripts for rebasing, diffing, and resyncing upstream.
- Make repository governance visible and repeatable.

**Outputs**

- `local/docs/repo-governance.md`
- Maintenance scripts

**Acceptance**

- Custom work is easy to identify.
- Upstream sync work is predictable and documented.

### 2. Profiles and packaging

Formalize profile definitions and package-group composition.

- Map each profile to package groups.
- Standardize package metadata and support labeling.
- Keep profile behavior reproducible.

**Acceptance**

- Profiles are reproducible and documented.

### 3. Driver substrate

Implement and stabilize the shared driver base.

- `redox-driver-sys` for shared driver plumbing
- PCI, MMIO, IRQ, and DMA support
- Driver daemon template for new device work

**Acceptance**

- A new driver can be created from a known template.

### 4. Graphics and Wayland

Drive the graphical stack through concrete milestones.

**Milestones**

- Run one Wayland application
- Start KWin
- Launch Plasma shell

**Acceptance**

- At least one profile runs a real graphical session.

### 5. Networking

#### Architecture

- Per-NIC driver daemons
- Network service for interfaces, DHCP, and routing
- D-Bus compatibility surface where desktop software expects it

#### Milestones

**N1: VM networking**

- `virtio-net`
- DHCP and package access

**N2: Wired hardware**

- Intel NICs
- Realtek NICs

**N3: KDE integration**

- NetworkManager-compatible D-Bus subset

**N4: Wi-Fi (experimental)**

- Single chipset family first

**Acceptance**

- Wired networking works in at least one real profile.

### 6. KDE integration

- Package the D-Bus session path
- Provide a networking shim where KDE expects one
- Provide an audio compatibility layer where needed
- Package session startup and startup dependencies

**Acceptance**

- A KDE session launches and its limitations are documented.

### 7. Live image and diagnostics

- Hardware detection tools
- Network diagnostics
- Graphics diagnostics

**Acceptance**

- The live image can diagnose common failure cases.

### 8. Validation and CI

#### Support labels

- `builds`
- `boots`
- `network`
- `wayland`
- `kde`
- `validated`
- `experimental`

#### Tasks

- VM-based tests
- Hardware validation matrix

**Acceptance**

- Support status is explicit and reproducible.

## Development Phases

### Phase A — Structure

- Repository rules
- Profile definitions

### Phase B — Minimal system

- Boot
- Package management
- VM networking

### Phase C — Driver base

- Shared driver layer

### Phase D — Graphics

- Wayland
- Qt application bring-up

### Phase E — Networking

- Wired networking
- KDE-visible networking path

### Phase F — Desktop

- KDE session becomes usable

### Phase G — Hardware validation

- One fully validated profile

### Phase H — Wi-Fi

- Experimental expansion

## Task Template

Every substantial work item should capture:

- **Title**
- **Objective**
- **Scope**
- **Files affected**
- **Dependencies**
- **Implementation notes**
- **Acceptance criteria**
- **Validation steps**
- **Risks**

## Final Direction

Red Bear OS should evolve as a profile-driven, package-defined system built on Redox architecture.
Progress should be measured by working profiles, not theoretical completeness.

### Priority order

1. Repository discipline
2. Profile reproducibility
3. VM usability
4. Graphics bring-up
5. Wired networking
6. KDE integration
7. Hardware validation
8. Wi-Fi expansion

## Related Documents

- [Root README](../README.md)
- [Architecture Overview](01-REDOX-ARCHITECTURE.md)
- [Gap Analysis & Roadmap](02-GAP-ANALYSIS.md)
- [Wayland on Redox](03-WAYLAND-ON-REDOX.md)
- [Linux Driver Compatibility Layer](04-LINUX-DRIVER-COMPAT.md)
- [KDE Plasma on Redox](05-KDE-PLASMA-ON-REDOX.md)
- [`local/docs/AMD-FIRST-INTEGRATION.md`](../local/docs/AMD-FIRST-INTEGRATION.md)
