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

## What's Different from Upstream Redox

| Component | Status | Detail |
|-----------|--------|--------|
| AMD GPU driver (amdgpu) | ✅ Compiles | LinuxKPI compat + AMD DC modesetting + MSI-X (no HW validation) |
| Intel GPU driver | ✅ Compiles | Display pipe modesetting + MSI-X (no HW validation) |
| ext4 filesystem | ✅ Compiles | Read/write ext4 alongside RedoxFS |
| ACPI for AMD bare metal | ✅ Complete | x2APIC, MADT, FADT shutdown/reboot, power methods |
| Custom branding | ✅ | Boot identity, hostname, os-release |
| POSIX gaps (relibc) | 🚧 In progress | eventfd, signalfd, timerfd, open_memstream |

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
