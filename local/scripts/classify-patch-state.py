#!/usr/bin/env python3
import argparse
import subprocess
import sys
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parents[2]

PATCH_TARGETS = {
    "kernel": PROJECT_ROOT / "recipes/core/kernel/source",
    "base": PROJECT_ROOT / "recipes/core/base/source",
    "relibc": PROJECT_ROOT / "recipes/core/relibc/source",
    "bootloader": PROJECT_ROOT / "recipes/core/bootloader/source",
    "installer": PROJECT_ROOT / "recipes/core/installer/source",
}


def run_patch(target: Path, patch_file: Path, reverse: bool) -> bool:
    cmd = ["patch", "--dry-run", "-p1", "-d", str(target)]
    if reverse:
        cmd.insert(1, "-R")
    with patch_file.open("rb") as handle:
        proc = subprocess.run(cmd, stdin=handle, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return proc.returncode == 0


def classify_patch(target: Path, patch_file: Path) -> str:
    if not target.exists():
        return "missing_target"
    if run_patch(target, patch_file, reverse=False):
        return "applies_cleanly"
    if run_patch(target, patch_file, reverse=True):
        return "already_applied"
    return "drifted_or_obsolete"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--strict", action="store_true")
    args = parser.parse_args()

    had_problem = False
    for label, target in PATCH_TARGETS.items():
        patch_dir = PROJECT_ROOT / "local/patches" / label
        if not patch_dir.is_dir():
            continue
        for patch_file in sorted(patch_dir.glob("*.patch")):
            status = classify_patch(target, patch_file)
            print(f"{label}\t{patch_file.name}\t{status}")
            if status in {"missing_target", "drifted_or_obsolete"}:
                had_problem = True

    return 1 if args.strict and had_problem else 0


if __name__ == "__main__":
    sys.exit(main())
