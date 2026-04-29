# Source Archival Policy — Red Bear OS

**Effective:** 2026-04-29
**Status:** Active / Enforceable

## Principle

Every source archive exported to `sources/<target-triple>/` must include the package version
number in its filename, and must contain the fully-patched source tree as it was used during the
build. No archive may be named solely by category — every archive filename must carry a version
qualifier.

## Naming Convention

```
sources/<target-triple>/<category>-<pkgname>-v<version>-patched.tar.gz
```

| Component | Meaning | Example |
|---|---|---|
| `<category>` | Recipe category directory | `core`, `libs`, `wip` |
| `<pkgname>` | Package name from recipe directory | `base`, `dbus`, `qtbase` |
| `<version>` | Source version from recipe (tar/git rev) | `1.16.2`, `6.11.0`, `463f76b` |
| `patched` | Indicates all recipe patches are applied | always present |

**Examples:**
```
core-base-v463f76b-patched.tar.gz
wip-services-dbus-v1.16.2-patched.tar.gz
wip-qt-qtbase-v6.11.0-patched.tar.gz
wip-qt-qtdeclarative-v6.11.0-patched.tar.gz
core-relibc-v2025-10-03-patched.tar.gz
```

## Version Sources

The version is extracted from the recipe's `[source]` block:

| Source type | Version extraction |
|---|---|
| `tar = "https://.../pkg-X.Y.Z.tar.xz"` | Extract `X.Y.Z` from URL |
| `git = "https://...repo.git"` + `rev = "abc123"` | Use git rev short hash (`abc123`) |
| `path = "source"` (local) | Use the recipe's `[source]` section name or a manual version marker |

## Archive Contents

Each versioned archive must contain:

1. The **fully patched source tree** at `recipes/<category>/<pkgname>/source/` — after all `patches = [...]` have been applied
2. The **recipe file** (`recipe.toml`) that defines the build
3. A **metadata file** (`source-info.json`) with: package name, version, source type, patch list, and build date

### Metadata format

```json
{
    "package": "dbus",
    "version": "1.16.2",
    "source_type": "tar",
    "source_url": "https://dbus.freedesktop.org/releases/dbus/dbus-1.16.2.tar.xz",
    "blake3": "b1d1f22858a8f04665e5dca29d194f892620f00fd3e3f4e89dd208e78868436e",
    "patches": ["redox.patch"],
    "build_date": "2026-04-29T00:00:00Z",
    "target": "x86_64-unknown-redox"
}
```

## Enforcement

- The `packages.txt` manifest in `sources/<target-triple>/` lists all exported packages with versions
- Every CI/documentation run that exports sources must use versioned naming
- An archive without a version number is considered incomplete — it must be regenerated
- The `make sources` target (when created) will auto-generate versioned archives

## Existing Non-Versioned Archives (Migration)

Current archives in `sources/x86_64-unknown-redox/` named like `core-base.tar.gz` are legacy.
They must be migrated to the versioned naming convention on next rebuild:

| Old name | New name |
|---|---|
| `core-base.tar.gz` | `core-base-v463f76b-patched.tar.gz` |
| `core-kernel.tar.gz` | `core-kernel-v<rev>-patched.tar.gz` |
| `core-relibc.tar.gz` | `core-relibc-v<rev>-patched.tar.gz` |
| `libs-mesa.tar.gz` | `libs-mesa-v<ver>-patched.tar.gz` |

## Related

- `../AGENTS.md` — repository structure and durability policy
- `docs/06-BUILD-SYSTEM-SETUP.md` — build system mechanics
- `local/docs/PATCH-GOVERNANCE.md` — patch governance policy
