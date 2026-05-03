# Absorbed Patches

These patches have been **consolidated into `local/patches/base/redox.patch`** (the
mega-patch applied automatically to the base recipe source tree).

**Do not wire these patches into `recipes/core/base/recipe.toml`.** They are kept here
for reference and git history only. If a patch referenced by a recipe.toml symlink
breaks, that symlink should be updated to point to a current, active patch in
`local/patches/base/` (NOT one in this directory).

## Consolidation timeline

| Date | Action |
|------|--------|
| 2026-04-30 | P0 + P2 patches consolidated into redox.patch |
| 2026-04-30 | P1-P2 driver/ACPI patches consolidated |
| 2026-04-30 | P3 ACPI/PCI patches absorbed |

## Active patches (NOT in this directory)

The active patches applied on top of `redox.patch` live in `local/patches/base/`:

- `P3-ps2d-led-feedback.patch` — PS/2 LED state + InputProducer migration
- `P3-inputd-keymap-bridge.patch` — InputProducer enum + keymap bridge
- `P3-usbhidd-hardening.patch` — USB HID descriptor validation, retry, lookup table
- `P3-init-colored-output.patch` — ANSI-colored init daemon output
- `P9-fix-so-pecred.patch` — shared-object credential fix
- `redox.patch` — cumulative mega-patch (applied first, automatically)
