# DOCS — ARCHITECTURE & INTEGRATION DOCUMENTATION

Public `docs/` files now mix three roles:

- canonical repository-level policy/current-state docs,
- architecture/reference docs,
- and older roadmap/design docs that are still useful but partly historical.

Do not assume everything under `docs/` is equally current.

For current Red Bear OS status, also read:

- `docs/README.md` — canonical docs index + status matrix
- `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md` — canonical public implementation plan
- `local/docs/DESKTOP-STACK-CURRENT-STATUS.md` — current desktop stack build/runtime truth
- `local/docs/DRM-MODERNIZATION-EXECUTION-PLAN.md` — current DRM-focused execution plan beneath the canonical desktop path
- `local/docs/QT6-PORT-STATUS.md` — current Qt/KF6 package-level status
- `local/docs/AMD-FIRST-INTEGRATION.md` — deeper AMD/graphics technical roadmap, useful detail but not the canonical desktop plan
- `local/docs/WIP-MIGRATION-LEDGER.md` — current WIP ownership status
- `local/docs/SCRIPT-BEHAVIOR-MATRIX.md` — current script guarantees and non-guarantees

## STRUCTURE

```
docs/
├── 01-REDOX-ARCHITECTURE.md   # Architecture reference: microkernel, scheme system, driver model, display architecture
├── 02-GAP-ANALYSIS.md         # Historical gap matrix with corrected current-state notes
├── 04-LINUX-DRIVER-COMPAT.md  # Driver-compat architecture reference + historical porting path
├── 05-KDE-PLASMA-ON-REDOX.md  # Historical KDE implementation path + deeper rationale
├── 06-BUILD-SYSTEM-SETUP.md   # Build/setup mechanics guide (not canonical policy)
├── 07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md # Canonical public implementation plan
└── README.md                  # Canonical docs index + status matrix
```

## WHERE TO LOOK

| Question | Document | Key Section |
|----------|----------|-------------|
| How does the kernel work? | 01 | §1 Microkernel, §2 Scheme System |
| How do drivers access hardware? | 01 | §3 Driver Model, §6 Build System |
| What is the canonical current implementation plan? | 07 | Entire document |
| Which docs are current vs historical? | README | Document Status Matrix |
| What is the current WIP ownership policy? | local/docs/WIP-MIGRATION-LEDGER.md | Entire document |
| What do the main sync/fetch/apply/build scripts actually guarantee? | local/docs/SCRIPT-BEHAVIOR-MATRIX.md | Entire document |
| What is the current Wi-Fi architecture and validation path? | local/docs/WIFI-IMPLEMENTATION-PLAN.md / local/docs/WIFI-VALIDATION-RUNBOOK.md | Entire document |
| What is the current desktop-stack truth? | local/docs/DESKTOP-STACK-CURRENT-STATUS.md | Entire document |
| What is the current Qt/KF6 status? | local/docs/QT6-PORT-STATUS.md | Entire document |
| What's missing for Wayland? | local/docs/WAYLAND-IMPLEMENTATION-PLAN.md | Entire document |
| How to fix POSIX gaps? | local/docs/RELIBC-COMPLETENESS-AND-ENHANCEMENT-PLAN.md | Current relibc completeness work |
| What is the current Wayland plan? | local/docs/WAYLAND-IMPLEMENTATION-PLAN.md | Entire document |
| How to run Linux GPU drivers? | 04 | Architecture diagram, i915 porting example |
| What is redox-driver-sys? | 04 | Crate 1: memory, IRQ, PCI, DMA wrappers |
| What is linux-kpi? | 04 | Crate 2: C headers translating Linux→Redox APIs |
| How to port Qt? | 05 | Phase KDE-A (qtbase patches, ~500-800 lines) |
| How to port KDE Frameworks? | 05 | Phase KDE-B (25 frameworks, tiered approach) |
| How to port KDE Plasma? | 05 | Phase KDE-C (KWin, Plasma Shell, session config) |
| How to set up the build? | 06 | Prerequisites per distro, build commands |
| What is the current work ordering? | 07 | Workstream Order + Blocker chain |

## READING RULE

When a current-state local document conflicts with an older public roadmap/design file, prefer the
current local subsystem plan or the canonical public implementation plan.
