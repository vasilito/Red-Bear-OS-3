# RECIPES — PACKAGE RECIPE SYSTEM

26 categories of package recipes. Each recipe = `recipe.toml` defining fetch→build→stage.

## STRUCTURE

```
recipes/
├── core/        # kernel, bootloader, relibc, init, base drivers — AGENTS.md
├── wip/         # Wayland, KDE, GNOME, driver WIP ports — AGENTS.md
├── libs/        # Libraries: mesa, cairo, SDL, zlib, openssl (~100+)
├── gui/         # Orbital display server, orbterm, orbutils
├── net/         # curl, wget, openssh, iperf3, smolnetd
├── dev/         # git, cmake, meson, cargo, rustc
├── games/       # spacecadetpinball, dosbox
├── shells/      # bash, ion, fish, zsh
├── tools/       # diffutils, findutils, coreutils, grep
├── sound/       # alsa-lib, pulseaudio, vorbis
├── terminal/    # Terminal emulators
├── video/       # ffmpeg
├── web/         # netsurf, firefox (WIP)
├── fonts/       # dejavu, freefont
├── icons/       # adwaita, cosmic, pop
├── archives/    # tar, unzip, zstd, bzip2
├── demos/       # orbclient demos, osdemo
├── other/       # Uncategorised packages
└── tests/       # Test suites
```

## WHERE TO LOOK

| Task | Location |
|------|----------|
| Add a Rust app | `recipes/<category>/<name>/recipe.toml` with `template = "cargo"` |
| Add a C/C++ app | `template = "cmake"` or `"configure"` or `"custom"` |
| Find a dependency | Search `recipes/*/recipe.toml` for package name |
| Fix a port | Look for `redox.patch` in the recipe dir |
| Track upstream | Check `upstream =` field in `[source]` |

## HOW TO ADD A RECIPE

```bash
mkdir -p recipes/<category>/<name>
cat > recipes/<category>/<name>/recipe.toml << 'EOF'
#TODO: describe what's missing (required for WIP)

[source]
git = "https://github.com/user/repo.git"
upstream = "https://github.com/original/repo.git"
branch = "redox"

[build]
template = "cargo"  # or cmake, meson, make, configure, custom
dependencies = [
    "dep1",
    "dep2",
]
EOF
```

### Recipe Environment Variables

| Variable | Purpose |
|----------|---------|
| `COOKBOOK_SOURCE` | Extracted source directory |
| `COOKBOOK_STAGE` | Install target (staging dir) |
| `COOKBOOK_SYSROOT` | Sysroot with built dependencies |
| `COOKBOOK_TARGET` | Target triple (e.g. `x86_64-unknown-redox`) |
| `COOKBOOK_CARGO` | Cargo with correct target |
| `COOKBOOK_MAKE` | Make with correct flags |

### Build Templates

| Template | Use For |
|----------|---------|
| `cargo` | Rust projects |
| `cmake` | CMake-based C/C++ |
| `meson` | Meson-based projects |
| `configure` | GNU Autotools |
| `make` | Simple Makefile projects |
| `custom` | Anything else (use `script = """..."""`) |

## CONVENTIONS

- WIP recipes: MUST start with `#TODO` comment
- Production recipes: BLAKE3 hash required for tar sources
- Patches: `redox.patch` in recipe dir, applied automatically
- Source: `git =` for git repos, `tar =` for tarballs, can use both
- Fork tracking: `git =` points to Redox fork, `upstream =` to original
- Dynamic linking: use `DYNAMIC_INIT` macro in custom scripts
