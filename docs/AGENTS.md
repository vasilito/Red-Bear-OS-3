# DOCS — ARCHITECTURE & INTEGRATION DOCUMENTATION

7 comprehensive technical documents covering Redox architecture, gap analysis, and integration paths.
For AMD-first integration, see `local/docs/AMD-FIRST-INTEGRATION.md`.

## STRUCTURE

```
docs/
├── 01-REDOX-ARCHITECTURE.md   # Microkernel design, scheme system, driver model, Orbital
├── 02-GAP-ANALYSIS.md         # Dependency chain, gap matrix, milestone roadmap
├── 03-WAYLAND-ON-REDOX.md     # Wayland implementation path (5 steps, ~26 weeks)
├── 04-LINUX-DRIVER-COMPAT.md  # LinuxKPI-style driver compat layer (3 crates)
├── 05-KDE-PLASMA-ON-REDOX.md  # KDE Plasma port (3 phases, ~38 weeks)
├── 06-BUILD-SYSTEM-SETUP.md   # Build prerequisites, config, commands, troubleshooting
└── README.md                  # Index of all docs
```

## WHERE TO LOOK

| Question | Document | Key Section |
|----------|----------|-------------|
| How does the kernel work? | 01 | §1 Microkernel, §2 Scheme System |
| How do drivers access hardware? | 01 | §3 Driver Model, §6 Build System |
| What's missing for Wayland? | 02 | Layer 1-4 gap matrix |
| How to fix POSIX gaps? | 03 | §1 (signalfd, timerfd, eventfd implementations) |
| How to build evdevd? | 03 | §2 (evdev input daemon architecture) |
| How to build DRM/KMS? | 03 | §3 (drmd daemon, Intel driver) |
| How to port a Wayland compositor? | 03 | §4 (Smithay Redox backends) |
| How to run Linux GPU drivers? | 04 | Architecture diagram, i915 porting example |
| What is redox-driver-sys? | 04 | Crate 1: memory, IRQ, PCI, DMA wrappers |
| What is linux-kpi? | 04 | Crate 2: C headers translating Linux→Redox APIs |
| How to port Qt? | 05 | Phase KDE-A (qtbase patches, ~500-800 lines) |
| How to port KDE Frameworks? | 05 | Phase KDE-B (25 frameworks, tiered approach) |
| How to port KDE Plasma? | 05 | Phase KDE-C (KWin, Plasma Shell, session config) |
| How to set up the build? | 06 | Prerequisites per distro, build commands |
| What's the milestone timeline? | 02 | M1-M8 roadmap, parallel execution plan |

## KEY NUMBERS

- **POSIX gaps**: 7 APIs blocking libwayland (signalfd, timerfd, eventfd, F_DUPFD_CLOEXEC, MSG_CMSG_CLOEXEC, MSG_NOSIGNAL, open_memstream)
- **Wayland recipes**: 21 in `recipes/wip/wayland/`
- **KDE apps**: 9 WIP recipes in `recipes/wip/kde/`
- **To Wayland compositor**: ~26 weeks (2 developers)
- **To KDE Plasma**: ~38 weeks (2 developers)
- **To Linux driver compat**: ~24 weeks (parallel track)
