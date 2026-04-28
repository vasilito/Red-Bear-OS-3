# Changelog

This file tracks user-visible changes in Red Bear OS.

When a commit changes the visible system surface, supported hardware, build flow, shipped configs,
or major documentation status, add a short note here and keep the README "What's New" section in
sync with the newest highlights.

## 2026-04-14

- Added a canonical GitHub-visible Red Bear OS implementation plan under `docs/` and linked it from the main README and docs index.
- Added a user-visible GitHub-facing "What's New" section to the root README and linked it to this running changelog.
- Added a new `redbear-kde` configuration and documented current KDE bring-up status as in-progress rather than not started.
- Refreshed top-level and docs status notes so historical roadmap documents no longer read as the current repo state.
- Expanded shipped Red Bear system tooling and config coverage around runtime diagnostics, native hardware listing, and Redox-native networking flows.
- Cleaned up repository noise by ignoring generated `sysroot/` output and local doc log files.

## 2026-04-27 — Boot Process Overhaul

### Real Wayland Compositor
- New `redbear-compositor` package: 690-line Rust Wayland display server
- Full XDG shell protocol support (15/15 Wayland protocols)
- Replaces KWin stubs that created placeholder sockets
- `redbear-compositor-check` diagnostic tool
- Integration test suite verifying protocol compliance

### Intel GPU Driver Expansion
- Gen8-Gen12 supported: Skylake, Kaby Lake, Coffee Lake, Cannon Lake, Ice Lake, Tiger Lake, Alder Lake, DG2, Meteor Lake, Arrow Lake, Lunar Lake, Battlemage
- 200+ device IDs from Linux 7.0 i915 reference
- Gen4-Gen7 recognized with clear unsupported messages
- Display fixes: pipe count, page flip, EDID skeleton

### VirtIO GPU Driver
- New VirtIO GPU DRM/KMS backend for QEMU testing
- Full GpuDriver trait implementation (11 methods)

### Kernel Fixes
- 4GB RAM boot hang fixed (MEMORY_MAP overflow at 512 entries)
- Canary chain added for boot diagnosis

### Live ISO
- Preload capped at 1 GiB for large ISOs
- Partial preload with informative messaging

### DRM/KMS Integration
- KWIN_DRM_DEVICES wired through entire greeter chain
- Compositor auto-detects DRM device with 5-second wait

### Boot Daemons
- dhcpd: auto-detects network interface
- i2c-gpio-expanderd/ucsid: hardened I2C decode with retry

### Documentation
- BOOT-PROCESS-IMPROVEMENT-PLAN.md
- PROFILE-MATRIX.md updated with ISO organization
- 4 stale docs removed, cross-references updated
