# Red Bear OS Patch Governance

## Purpose

This document prevents loss of implemented work. It establishes rules that AI agents
and human contributors must follow when modifying patches, recipes, or build configs.

## Incident: 2026-04-26 Driver Code Loss

A previous agent session removed 8 patches and 9 BINS entries from
`recipes/core/base/recipe.toml` to make the build succeed, instead of fixing
patch conflicts. This deleted GPIO/I2C/UCSI driver source code that took a full
day to implement (commits `dc3f1f996`, `3054adc5d`).

The code was recovered from git history, but this must never happen again.

## Rules

### 1. Never remove patches to fix build failures

When a patch fails to apply:

- **Rebase the patch** against the current cumulative state
- **Fix the context lines** so the hunk applies cleanly
- **Split the patch** if only some hunks fail (keep the working hunks)
- **Document** the failure reason in the patch file header

Do NOT remove the patch from the recipe.toml patches list without explicit
user approval. If a patch must be temporarily disabled, comment it with a TODO
explaining why and what needs to be fixed.

### 2. Never remove BINS entries to fix build failures

When a driver binary fails to compile:

- **Fix the compilation error** in the driver source
- **Add the driver to EXISTING_BINS** filter if source is incomplete
- **Document** the failure

Do NOT remove the driver from the BINS array without explicit user approval.

### 3. Patch ordering matters

Patches in `recipes/core/base/recipe.toml` must be applied in the listed order.
Some patches have interdependencies:

- `P2-acpi-i2c-resources.patch` must apply before `P2-daemon-hardening.patch`
  (workspace entries reference source files created by the former)
- `P2-boot-runtime-fixes.patch` modifies hwd/acpi.rs (must apply cleanly to upstream)
- `P2-init-acpid-wiring.patch` adds 41_acpid.service and pcid-spawner retry logic
  (acpid spawn removal is in P2-boot-runtime-fixes, do NOT duplicate)

When reordering patches, test the FULL chain: remove source, rebuild, verify.

### 4. Recipe.toml is tracked, source trees are not

`recipes/core/base/recipe.toml` is git-tracked. Changes to it are durable.
`recipes/core/base/source/` is a fetched working copy — destroyed by `make clean`,
`make distclean`, source refresh, and sync-upstream.

Any change to source/ MUST be preserved as a patch in `local/patches/base/`.

### 5. Before removing anything, check git history

```bash
git log --oneline --all -- <file>
```

If a previous commit added substantial work (driver implementations, features),
the removal MUST be approved by the user. Agent sessions MUST NOT delete
implemented work to bypass build failures.

### 6. Build validation after patch changes

After ANY change to the patches list or patch files:

1. Remove the source tree: `rm -rf recipes/core/base/source`
2. Full rebuild: `REDBEAR_ALLOW_PROTECTED_FETCH=1 CI=1 make r.base`
3. Verify NO "FAILED" or "rejects" in output
4. Verify all expected binaries in stage: `ls stage/usr/bin/ stage/usr/lib/drivers/`
5. Full image build: `CI=1 make all CONFIG_NAME=redbear-full`

## Known Issues

| Patch | Status | Notes |
|-------|--------|-------|
| P2-acpid-core-refactor.patch | Needs rebasing | 13/15 hunks fail on acpid/scheme.rs; removed from recipe.toml with TODO |
| P2-acpi-i2c-resources.patch | Recovered & rebased → P2-i2c-gpio-ucsi-drivers.patch | Original couldn't apply to current source revision; extracted driver sources, fixed PCI API calls (try_mem→map_bar, try_map_bar→map_bar), regenerated as P2-i2c-gpio-ucsi-drivers.patch (5938 lines, 32 files) |
| P2-boot-runtime-fixes.patch | Needs rebasing | Context lines from monolith split are stale; hwd/acpi.rs hunk fails on clean upstream |
| P2-init-acpid-wiring.patch | Deduplicated | Removed acpi.rs hunk that duplicated P2-boot-runtime-fixes |

## Recipe.toml Fix Log

| Date | Change | Why |
|------|--------|-----|
| 2026-04-30 | Recovered I2C/GPIO/UCSI drivers | P2-acpi-i2c-resources.patch couldn't apply; regenerated as P2-i2c-gpio-ucsi-drivers.patch (5938 lines, 12 drivers: gpiod, i2cd, amd-mp2-i2cd, dw-acpi-i2cd, intel-lpss-i2cd, i2c-interface, intel-gpiod, i2c-gpio-expanderd, i2c-hidd, intel-thc-hidd, ucsid, acpi-resource) |
| 2026-04-30 | Fixed amd-mp2-i2cd PCI API | .try_mem() removed from PciBar; replaced with PciFunctionHandle::map_bar(0) |
| 2026-04-30 | Fixed intel-thc-hidd PCI API | .try_map_bar() removed from PciFunctionHandle; replaced with .map_bar(0) |
| 2026-04-30 | Added P0-bootstrap-workspace-fix.patch | [workspace] in bootstrap Cargo.toml prevents parent workspace auto-detection; fixes base-initfs from-scratch build |
| 2026-04-30 | Added symlinks to integrate-redbear.sh | P0-bootstrap-workspace-fix.patch and P2-i2c-gpio-ucsi-drivers.patch symlinks now auto-created |
| 2026-04-26 | Restored 8 removed patches | Agent deleted them to bypass conflicts; restored all from git HEAD |
| 2026-04-26 | Restored 9 BINS entries | Agent deleted i2cd, gpiod, ucsid, etc. to bypass missing sources |
| 2026-04-26 | Added EXISTING_BINS grep loop | Gracefully handles missing driver source instead of build failure |
| 2026-04-26 | Fixed grep/find variables | `${GREP}` and `${FIND}` are unset in redoxer env; use bare `grep`/`find` |
| 2026-04-26 | Fixed TOML escaping | `\"` in TOML triple-quotes becomes `"` in bash; use `\\\"` for literal `"` |
