<p align="center">
<img alt="Red Bear OS" width="200" src="assets/redbear-icon.png">
</p>

<h1 align="center">Red Bear OS</h1>

<p align="center">
<strong>Microkernel operating system in Rust — based on <a href="https://www.redox-os.org">Redox OS</a></strong>
</p>

<p align="center">
<a href="./LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT License"></a>
<img src="https://img.shields.io/badge/architecture-microkernel-orange.svg" alt="Microkernel">
<img src="https://img.shields.io/badge/language-Rust-000000.svg" alt="Rust">
</p>

---

Red Bear OS is a derivative of [Redox OS](https://www.redox-os.org) — a general-purpose, Unix-like, microkernel-based operating system written in Rust. It tracks upstream Redox, incorporating its improvements while adding custom drivers, filesystems, and hardware support.

## What's New

- KDE bring-up moved forward: `config/redbear-kde.toml` exists, the Qt6 stack builds in-tree, and the KDE recipe tree is now populated.
- Native Red Bear runtime tooling expanded with `redbear-info`, `redbear-hwutils` (`lspci`, `lsusb`), and a Redox-native `netctl` flow.
- Build and status docs were refreshed to distinguish current in-tree progress from older historical roadmap text.

See [CHANGELOG.md](./CHANGELOG.md) for the running user-visible change log.

## Current Phase Snapshot

| Phase | Status | Notes |
|---|---|---|
| P0 ACPI boot | ✅ Complete | In-tree and documented in `local/docs/ACPI-FIXES.md` |
| P1 driver infra | ✅ Complete | Compile-oriented infrastructure present |
| P2 DRM / display | ✅ Code complete | Hardware validation still pending |
| P3 POSIX + input | 🚧 In progress | relibc exports now cover the rebuilt `signalfd`/`timerfd`/`eventfd`/`open_memstream` consumer path; runtime validation remains |
| P4 Wayland runtime | 🚧 Partial | `libwayland` and `seatd` now build, and KDE config starts seatd, but compositor/DRM/input runtime validation is still incomplete |
| P5 AMD accel / IOMMU | 🚧 Partial | `iommu` daemon now builds, but hardware validation and full acceleration are still open |
| P6 KDE Plasma | 🚧 In progress | Mix of real builds, shims, and stubs |

There is no distinct first-class **P7** phase artifact in this repository today; later work appears as milestone-style follow-on work beyond the tracked P6 boundary.

## What's Different from Upstream Redox

| Component | Status | Detail |
|-----------|--------|--------|
| AMD GPU driver (amdgpu) | ✅ Compiles | LinuxKPI compat + AMD DC modesetting + MSI-X (no HW validation) |
| Intel GPU driver | ✅ Compiles | Display pipe modesetting + MSI-X (no HW validation) |
| ext4 filesystem | ✅ Compiles | Read/write ext4 alongside RedoxFS |
| ACPI for AMD bare metal | ✅ Complete | x2APIC, MADT, FADT shutdown/reboot, power methods |
| Wired networking | 🚧 Improved | native net stack present, Redox-native `netctl` shipped, RTL8125 autoload wired through the existing Realtek path |
| Custom branding | ✅ | Boot identity, hostname, os-release |
| POSIX gaps (relibc) | 🚧 In progress | implementations exist in-tree; runtime validation against Wayland stack is still ongoing |

## Project Structure

```
├── config/           # Build configs (TOML) — desktop, minimal, redbear-*
├── recipes/          # Package recipes (~100+ packages, 26 categories)
├── mk/               # Makefile build orchestration
├── src/              # Cookbook Rust tool (repo binary, cook logic)
├── local/            # ← Red Bear OS custom work (survives upstream updates)
│   ├── patches/      #   Kernel, base, relibc patches
│   ├── recipes/      #   Custom packages (drivers, GPU, system, branding)
│   ├── scripts/      #   sync-upstream.sh, apply-patches.sh
│   ├── Assets/       #   Branding (icon, boot background)
│   └── docs/         #   Integration documentation
├── docs/             # Architecture guides
├── scripts/          # Helper scripts
└── Makefile          # Root build orchestrator
```

## Build

Requires a Linux x86_64 host with Rust nightly, QEMU, and standard build tools. See the [Redox Build Instructions](https://doc.redox-os.org/book/podman-build.html) for full prerequisites.

```bash
make all CONFIG_NAME=redbear-full        # Full desktop + custom drivers
make all CONFIG_NAME=redbear-minimal     # Minimal server
make live CONFIG_NAME=redbear-full       # Live ISO (redbear-live.iso)
make qemu                                # Boot in QEMU
```

## Native hardware listing tools

Red Bear configs now include a small native `redbear-hwutils` package that ships `lspci` and
`lsusb`. `lspci` reads the existing `/scheme/pci/.../config` surface, while `lsusb` walks the
native `usb.*` schemes exposed by `xhcid`, so there is no dependency on the unfinished WIP
`pciutils` or `usbutils` ports.

## Networking

Red Bear ships the existing native Redox wired networking path (`pcid-spawner` → NIC daemon →
`smolnetd`/`dhcpd`/`netcfg`) together with a small Redox-native `netctl` compatibility command.
Profiles live under `/etc/netctl`, the shipped examples live under `/etc/netctl/examples`, and the
boot service applies the enabled profile with `netctl --boot`.

RTL8125 is wired into the existing native Realtek autoload path by matching `10ec:8125` in the
`rtl8168d` driver config. This keeps the implementation in the Redox userspace driver model rather
than introducing a separate Linux netdevice compatibility layer.

## Runtime diagnostics

Red Bear ships `redbear-info` as the canonical runtime integration/debugging command. It is a
passive report over live system surfaces and is intended to help answer questions like:

- which Red Bear integrations are merely installed versus actually active,
- whether the networking stack is up, with current IP, DNS, and default route,
- whether hardware discovery surfaces such as PCI, USB, DRM, and RTL8125 are visible.

Use `redbear-info --verbose` for evidence-backed human output, `redbear-info --json` for machine-
readable diagnostics, and `redbear-info --test` for suggested follow-up commands.

## Sync with Upstream Redox

```bash
./local/scripts/sync-upstream.sh              # Rebase onto latest Redox
./local/scripts/sync-upstream.sh --dry-run    # Preview conflicts first
```

The `local/` directory is never touched by upstream updates. Recipe patches for kernel and base are symlinked from `local/patches/` — protected from `make clean` and `make distclean`.

## Resources

- [Upstream Redox website](https://www.redox-os.org)
- [Redox Book](https://doc.redox-os.org/book/)
- [Hardware Support](https://doc.redox-os.org/book/hardware-support.html)
- [Contributing](CONTRIBUTING.md)

## AI Policy

We welcome contributions made with the assistance of LLMs and AI tools. If you use AI to help write code, documentation, or patches, that's great — we care about the quality of the result, not how it was produced.

## License

[MIT](./LICENSE) — same as upstream Redox OS.
