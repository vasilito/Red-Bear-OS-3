#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

CONFIG="redbear-full"
RELEASE="${REDBEAR_RELEASE:-}"
STRICT_DURABILITY="${REDBEAR_STRICT_DURABILITY:-0}"
STRICT_METADATA="${REDBEAR_STRICT_METADATA:-0}"
EXTRA_PACKAGES=()

usage() {
    cat <<EOF
Usage: $(basename "$0") [--config=<name>] [--release=<ver>] [--strict-durability] [--strict-metadata] [--extra-package=<pkg> ...]
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --config=*) CONFIG="${1#*=}" ;;
        --release=*) RELEASE="${1#*=}" ;;
        --strict-durability) STRICT_DURABILITY=1 ;;
        --strict-metadata) STRICT_METADATA=1 ;;
        --extra-package=*) EXTRA_PACKAGES+=("${1#*=}") ;;
        -h|--help) usage; exit 0 ;;
        *) echo "Unknown: $1" >&2; usage >&2; exit 1 ;;
    esac
    shift
done

cd "$PROJECT_ROOT"

echo ">>> Build preflight: $CONFIG"

if [ -x "$SCRIPT_DIR/verify-overlay-integrity.sh" ]; then
    if ! "$SCRIPT_DIR/verify-overlay-integrity.sh" --quiet; then
        echo ">>> Preflight note: overlay integrity script reported legacy issues; continuing with build-focused checks."
    fi
fi

if [ -n "$RELEASE" ]; then
    bash "$SCRIPT_DIR/build-release-mode.sh" --release="$RELEASE" --config="$CONFIG" "${EXTRA_PACKAGES[@]/#/--extra-package=}"
fi

python3 "$SCRIPT_DIR/validate-source-trees.py" "$CONFIG" "${EXTRA_PACKAGES[@]/#/--extra-package=}"

python3 - "$PROJECT_ROOT" "$CONFIG" "$STRICT_METADATA" "${EXTRA_PACKAGES[@]}" <<'PY'
import sys
import tomllib
from pathlib import Path

project_root = Path(sys.argv[1])
config_name = sys.argv[2]
strict_metadata = sys.argv[3] == "1"
extra_packages = sys.argv[4:]

def build_lookup():
    lookup = {}
    for root in (project_root / "recipes", project_root / "local/recipes"):
        for recipe_toml in root.rglob("recipe.toml"):
            if not recipe_toml.exists():
                continue
            parts = recipe_toml.parts
            if "source" in parts or "target" in parts:
                continue
            package_name = recipe_toml.parent.name
            lookup.setdefault(package_name, recipe_toml)
    return lookup

def resolve_config(config_path, visited=None):
    if visited is None:
        visited = set()
    config_path = config_path.resolve()
    if config_path in visited:
        return {}
    visited.add(config_path)
    config = tomllib.loads(config_path.read_text())
    packages = dict(config.get("packages", {}))
    for include in config.get("include", []):
        include_path = config_path.parent / include
        if include_path.exists():
            included = resolve_config(include_path, visited)
            for name, value in packages.items():
                included[name] = value
            packages = included
    return packages

lookup = build_lookup()
config_path = project_root / "config" / f"{config_name}.toml"
requested = resolve_config(config_path)
for pkg in extra_packages:
    requested.setdefault(pkg, {})

errors = []
warnings = []
for package_name, package_conf in sorted(requested.items()):
    if str(package_conf) == "ignore" or package_name in {"libgcc", "libstdcxx"}:
        continue
    recipe_toml = lookup.get(package_name)
    if recipe_toml is None:
        continue
    recipe = tomllib.loads(recipe_toml.read_text())
    source = recipe.get("source", {})
    rel = recipe_toml.relative_to(project_root).as_posix()
    is_wip_or_local = rel.startswith("recipes/wip/") or rel.startswith("local/recipes/")
    if isinstance(source, dict) and "tar" in source and "blake3" not in source:
        msg = f"missing blake3 for tar recipe: {recipe_toml.relative_to(project_root)}"
        (errors if strict_metadata and not is_wip_or_local else warnings).append(msg)
    for patch in source.get("patches", []):
        patch_path = (recipe_toml.parent / patch).resolve()
        if not patch_path.exists():
            msg = f"missing patch file: {patch} for {recipe_toml.relative_to(project_root)}"
            (errors if strict_metadata else warnings).append(msg)

for warning in warnings:
    print(f"WARN: {warning}", file=sys.stderr)
if errors:
    for error in errors:
        print(f"ERROR: {error}", file=sys.stderr)
    raise SystemExit(1)
PY

if [ -x "$SCRIPT_DIR/classify-patch-state.py" ]; then
    python3 "$SCRIPT_DIR/classify-patch-state.py"
fi

if [ -x "$SCRIPT_DIR/verify-durable-source-edits.py" ]; then
    args=()
    if [ "$STRICT_DURABILITY" = "1" ]; then
        args+=(--strict)
    fi
    python3 "$SCRIPT_DIR/verify-durable-source-edits.py" "${args[@]}"
fi

if [ "$CONFIG" = "redbear-full" ]; then
    relibc_include="$(PROJECT_ROOT="$PROJECT_ROOT" bash -c 'source "$0"; redbear_relibc_stage_include_dir' "$SCRIPT_DIR/lib/relibc-surface.sh")"
    relibc_lib_dir="$(PROJECT_ROOT="$PROJECT_ROOT" bash -c 'source "$0"; redbear_relibc_stage_lib_dir' "$SCRIPT_DIR/lib/relibc-surface.sh")"
    if [ -d "$relibc_include" ] && [ -f "$relibc_lib_dir/libc.so" ]; then
        for hdr in sys/signalfd.h sys/timerfd.h sys/eventfd.h threads.h; do
            if [ ! -f "$relibc_include/$hdr" ]; then
                echo ">>> Preflight note: relibc staged surface missing $hdr; top-level build should refresh/sync it before compilation."
            fi
        done
        if ! grep -q 'strtold' "$relibc_include/stdlib.h" 2>/dev/null; then
            echo ">>> Preflight note: relibc staged stdlib.h missing strtold declaration; top-level build should refresh/sync it before compilation."
        fi
        if ! readelf -Ws "$relibc_lib_dir/libc.so" | grep -q '_Z7strtoldPKcPPc'; then
            echo ">>> Preflight note: relibc staged libc.so missing C++ strtold compatibility export; top-level build should refresh/sync it before compilation."
        fi
    else
        echo ">>> Preflight note: relibc staged surface not present yet; build will refresh it if needed."
    fi
fi

echo ">>> Build preflight passed"
