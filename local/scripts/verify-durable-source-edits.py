#!/usr/bin/env python3
import argparse
import subprocess
import sys
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parents[2]
UPSTREAM_OWNED = {
    "kernel": PROJECT_ROOT / "recipes/core/kernel/source",
    "base": PROJECT_ROOT / "recipes/core/base/source",
    "relibc": PROJECT_ROOT / "recipes/core/relibc/source",
    "bootloader": PROJECT_ROOT / "recipes/core/bootloader/source",
    "installer": PROJECT_ROOT / "recipes/core/installer/source",
}


def git_has_changes(repo: Path) -> tuple[bool, list[str]]:
    if not (repo / ".git").exists():
        return False, []
    proc = subprocess.run(
        ["git", "status", "--short"],
        cwd=repo,
        check=False,
        capture_output=True,
        text=True,
    )
    lines = [line.rstrip() for line in proc.stdout.splitlines() if line.strip()]
    return bool(lines), lines


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--strict", action="store_true")
    args = parser.parse_args()

    dirty = False
    for label, repo in UPSTREAM_OWNED.items():
        has_changes, lines = git_has_changes(repo)
        if not has_changes:
            continue
        dirty = True
        print(f"{label}\tDIRTY\t{repo.relative_to(PROJECT_ROOT)}")
        for line in lines[:20]:
            print(f"  {line}")
        if len(lines) > 20:
            print(f"  ... {len(lines) - 20} more")

    return 1 if args.strict and dirty else 0


if __name__ == "__main__":
    sys.exit(main())
