#!/usr/bin/env python3
"""Validate that all source trees required by a build config exist."""
import sys, tomllib
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parents[2]
CONFIG = sys.argv[1] if len(sys.argv) > 1 else "redbear-full"

def build_lookup():
    lookup = {}
    for root in (Path("recipes"), Path("local/recipes")):
        for rt in root.rglob("recipe.toml"):
            parts = rt.parts
            if "source" in parts or "target" in parts:
                continue
            pkg = rt.parent.name
            if pkg not in lookup:
                lookup[pkg] = rt.parent
    return lookup

def resolve_config(cp, visited=None):
    if visited is None: visited = set()
    cp = cp.resolve()
    if cp in visited: return {}
    visited.add(cp)
    with open(cp, "rb") as f: c = tomllib.load(f)
    pkgs = dict(c.get("packages", {}))
    for inc in c.get("include", []):
        ip = cp.parent / inc
        if ip.exists():
            incd = resolve_config(ip, visited)
            for k, v in pkgs.items(): incd[k] = v
            pkgs = incd
    return pkgs

def main():
    config_path = Path("config") / f"{CONFIG}.toml"
    if not config_path.exists():
        print(f"Config not found: {config_path}", file=sys.stderr)
        return 1

    lookup = build_lookup()
    pkgs = resolve_config(config_path)

    print(f"=== Validating source trees for config: {CONFIG} ===")
    missing = 0
    present = 0
    for pkg_name, pkg_conf in sorted(pkgs.items()):
        if str(pkg_conf) == "ignore": continue
        # Meta packages have no source requirement
        if pkg_name in ("libgcc", "libstdcxx"):
            continue
        rd = lookup.get(pkg_name)
        if not rd:
            print(f"  NOT FOUND: {pkg_name}")
            missing += 1
            continue
        src = rd / "source"
        if src.is_dir() and any(src.iterdir()):
            present += 1
        else:
            print(f"  MISSING: {str(rd)}")
            missing += 1

    print(f"\n  Total (config): {present + missing}")
    print(f"  Present:        {present}")
    print(f"  Missing:        {missing}")
    if missing:
        print("\nTo restore:  ./local/scripts/restore-sources.sh --release=0.1.0")
        return 1
    print("All source trees present.")
    return 0

if __name__ == "__main__":
    sys.exit(main())
