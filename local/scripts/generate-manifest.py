#!/usr/bin/env python3
"""Generate an authoritative Red Bear OS release manifest as JSON."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tomllib


PROJECT_ROOT = Path(__file__).resolve().parents[2]
RECIPES_DIR = PROJECT_ROOT / "recipes"
LOCAL_RECIPES_DIR = PROJECT_ROOT / "local" / "recipes"
ARCHIVES_DIR = PROJECT_ROOT / "sources" / "x86_64-unknown-redox"
HASH_TOOL = shutil.which("b3sum")

TAR_VERSION_PATTERNS = (
    re.compile(r"/archive/v?(\d+\.\d+(?:\.\d+)?)/"),
    re.compile(r"(?:^|[/-])v?(\d+\.\d+(?:\.\d+)?)(?=\.tar(?:\.[^./]+)+(?:/download)?$)"),
)
HEX_REV_RE = re.compile(r"[0-9a-fA-F]{7,}")
SAFE_VERSION_RE = re.compile(r"[^A-Za-z0-9._-]+")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate authoritative manifest.json content for a Red Bear OS release."
    )
    parser.add_argument("--release", required=True, help="Release version to record in the manifest")
    parser.add_argument("--staging", action="store_true", help="Look for archives in staging directory")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    global ARCHIVES_DIR
    if args.staging:
        ARCHIVES_DIR = PROJECT_ROOT / "sources" / ".staging" / f"redbear-{args.release}" / "tarballs"
    else:
        ARCHIVES_DIR = PROJECT_ROOT / "sources" / "redbear-{args.release}" / "tarballs"
    # Fallback to shared pool if release dir has no tarballs yet
    if not list(ARCHIVES_DIR.glob("*.tar.gz")):
        ARCHIVES_DIR = PROJECT_ROOT / "sources" / "x86_64-unknown-redox"
    args = parse_args()
    recipe_files = collect_recipe_files()
    entries = {}

    for relative_recipe_path, recipe_file in recipe_files.items():
        entries[relative_recipe_path] = build_entry(relative_recipe_path, recipe_file, recipe_files)

    manifest = {
        "release": args.release,
        "build_system_rev": resolve_build_system_rev(),
        "entries": {key: entries[key] for key in sorted(entries)},
    }

    json.dump(manifest, sys.stdout, indent=2)
    sys.stdout.write("\n")
    return 0


def collect_recipe_files() -> dict[str, Path]:
    recipe_files: dict[str, Path] = {}

    for root in (LOCAL_RECIPES_DIR, RECIPES_DIR):
        if not root.is_dir():
            continue

        for dirpath, dirnames, filenames in os.walk(root, followlinks=False):
            dirnames[:] = sorted(
                name for name in dirnames if name not in {"source", "target", ".git", "__pycache__"}
            )

            if "recipe.toml" not in filenames:
                continue

            recipe_file = Path(dirpath) / "recipe.toml"
            if not recipe_file.is_file():
                continue

            relative_recipe_path = recipe_file.relative_to(root).parent.as_posix()
            recipe_files.setdefault(relative_recipe_path, recipe_file)

    return recipe_files


def build_entry(
    relative_recipe_path: str, recipe_file: Path, recipe_files: dict[str, Path]
) -> dict[str, object]:
    recipe_dir = recipe_file.parent
    recipe_data = load_recipe_metadata(recipe_file)
    source_data = recipe_data.get("source") if isinstance(recipe_data, dict) else None
    source = source_data if isinstance(source_data, dict) else {}
    recipe_type = classify_recipe(source)

    entry: dict[str, object] = {
        "type": recipe_type,
        "restore_to": f"recipes/{relative_recipe_path}/source",
    }

    if recipe_type != "meta":
        archive_name = expected_archive_name(
            relative_recipe_path,
            recipe_type,
            source,
            recipe_dir,
            recipe_files,
        )
        archive_name = resolve_archive_name(relative_recipe_path, archive_name)
        archive_path = ARCHIVES_DIR / archive_name

        entry["archive"] = archive_name
        entry["blake3"] = blake3_file(archive_path) if archive_path.is_file() else None

    if recipe_type == "git":
        rev = get_git_rev(source, recipe_dir)
        entry["git_url"] = source.get("git")
        entry["rev"] = rev
    elif recipe_type == "tar":
        entry["tar_url"] = source.get("tar")
        source_blake3 = source.get("blake3") or source.get("b3sum")
        if source_blake3:
            entry["source_blake3"] = source_blake3
    elif recipe_type == "path":
        path_value = source.get("path")
        entry["path"] = path_value
        source_path = resolve_source_path(recipe_dir, path_value)
        if source_path and source_path.exists():
            entry["tree_blake3"] = blake3_tree(source_path)
    elif recipe_type == "same_as":
        entry["target"] = normalize_recipe_reference(recipe_dir, str(source.get("same_as", "")))
    elif recipe_type == "meta":
        entry["meta"] = "no_source"

    return entry


def load_recipe_metadata(path: Path) -> dict[str, object]:
    text = path.read_text(encoding="utf-8")

    try:
        data = tomllib.loads(text)
    except tomllib.TOMLDecodeError:
        return {"source": parse_source_block(text)}

    return data if isinstance(data, dict) else {}


def parse_source_block(text: str) -> dict[str, object]:
    source: dict[str, object] = {}
    in_source = False

    for raw_line in text.splitlines():
        stripped = raw_line.strip()

        if stripped.startswith("[") and stripped.endswith("]"):
            if stripped == "[source]":
                in_source = True
                continue

            if in_source:
                break

            continue

        if not in_source or not stripped or stripped.startswith("#") or "=" not in raw_line:
            continue

        key, value = raw_line.split("=", 1)
        key = key.strip()
        value = value.split("#", 1)[0].strip()
        if not key or not value:
            continue

        try:
            source[key] = tomllib.loads(f"value = {value}")["value"]
        except tomllib.TOMLDecodeError:
            continue

    return source


def classify_recipe(source: dict[str, object]) -> str:
    if source.get("git"):
        return "git"
    if source.get("tar"):
        return "tar"
    if source.get("path"):
        return "path"
    if source.get("same_as"):
        return "same_as"
    return "meta"


def expected_archive_name(
    relative_recipe_path: str,
    recipe_type: str,
    source: dict[str, object],
    recipe_dir: Path,
    recipe_files: dict[str, Path],
) -> str:
    path = Path(relative_recipe_path)
    pkg_name = path.name
    category = path.parent.name if path.parent.as_posix() != "." else "root"
    version = derive_archive_version(
        relative_recipe_path,
        recipe_type,
        source,
        recipe_dir,
        recipe_files,
        {relative_recipe_path},
    )
    return f"{category}-{pkg_name}-v{version}-patched.tar.gz"


def derive_archive_version(
    relative_recipe_path: str,
    recipe_type: str,
    source: dict[str, object],
    recipe_dir: Path,
    recipe_files: dict[str, Path],
    seen: set[str],
) -> str:
    if recipe_type == "tar":
        tar_url = str(source.get("tar", ""))
        version = extract_tar_version(tar_url)
        if version:
            return version

    if recipe_type == "git":
        rev = get_git_rev(source, recipe_dir)
        if isinstance(rev, str) and rev:
            if HEX_REV_RE.fullmatch(rev):
                return rev[:7]
            return sanitize_version(rev)

    if recipe_type == "same_as":
        target = normalize_recipe_reference(recipe_dir, str(source.get("same_as", "")))
        if target and target not in seen:
            target_file = recipe_files.get(target)
            if target_file is not None:
                target_data = load_recipe_metadata(target_file)
                target_source_data = target_data.get("source") if isinstance(target_data, dict) else None
                target_source = target_source_data if isinstance(target_source_data, dict) else {}
                target_type = classify_recipe(target_source)
                return derive_archive_version(
                    target,
                    target_type,
                    target_source,
                    target_file.parent,
                    recipe_files,
                    seen | {target},
                )

    return "unknown"


def resolve_archive_name(relative_recipe_path: str, archive_name: str) -> str:
    archive_path = ARCHIVES_DIR / archive_name
    if archive_path.is_file():
        return archive_name

    recipe_path = Path(relative_recipe_path)
    category = recipe_path.parent.name if recipe_path.parent.as_posix() != "." else "root"
    pkg_name = recipe_path.name
    matches = sorted(ARCHIVES_DIR.glob(f"{category}-{pkg_name}-v*-patched.tar.gz"))
    if len(matches) == 1:
        return matches[0].name

    return archive_name


def extract_tar_version(tar_url: str) -> str | None:
    for pattern in TAR_VERSION_PATTERNS:
        match = pattern.search(tar_url)
        if match:
            return match.group(1)
    return None


def get_git_rev(source: dict[str, object], recipe_dir: Path) -> str | None:
    rev = source.get("rev")
    if isinstance(rev, str) and rev.strip():
        return rev.strip()
    return resolve_git_head(recipe_dir / "source")


def resolve_git_head(repo_dir: Path) -> str | None:
    git_dir = repo_dir / ".git"
    if not git_dir.exists():
        return None

    result = subprocess.run(
        ["git", "-C", str(repo_dir), "rev-parse", "--short", "HEAD"],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        return None

    head = result.stdout.strip()
    return head or None


def resolve_build_system_rev() -> str | None:
    result = subprocess.run(
        ["git", "-C", str(PROJECT_ROOT), "rev-parse", "--short=9", "HEAD"],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        return None
    value = result.stdout.strip()
    return value or None


def resolve_source_path(recipe_dir: Path, raw_path: object) -> Path | None:
    if not isinstance(raw_path, str) or not raw_path:
        return None

    path = Path(raw_path)
    candidate = path if path.is_absolute() else recipe_dir / path

    try:
        resolved = candidate.resolve(strict=True)
    except FileNotFoundError:
        return None

    try:
        resolved.relative_to(PROJECT_ROOT.resolve())
    except ValueError:
        return None

    return resolved


def normalize_recipe_reference(recipe_dir: Path, raw_reference: str) -> str:
    if not raw_reference:
        return raw_reference

    candidate = (recipe_dir / raw_reference).resolve(strict=False)
    for root in (RECIPES_DIR, LOCAL_RECIPES_DIR):
        try:
            return candidate.relative_to(root).as_posix()
        except ValueError:
            continue

    return raw_reference


def sanitize_version(value: str) -> str:
    cleaned = SAFE_VERSION_RE.sub("-", value).strip("-.")
    return cleaned or "unknown"


def require_hash_tool() -> str:
    if HASH_TOOL:
        return HASH_TOOL
    raise RuntimeError("b3sum is required to compute BLAKE3 hashes")


def blake3_file(path: Path) -> str:
    result = subprocess.run(
        [require_hash_tool(), "--no-names", str(path)],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        stderr = result.stderr.strip() or f"failed to hash {path}"
        raise RuntimeError(stderr)
    return result.stdout.strip().split()[0]


def blake3_tree(root: Path) -> str:
    process = subprocess.Popen(
        [require_hash_tool(), "--no-names"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

    try:
        assert process.stdin is not None
        with tarfile.open(fileobj=process.stdin, mode="w|") as tar:
            for entry in iter_tree_entries(root):
                arcname = entry.relative_to(root).as_posix()
                tar_info = tar.gettarinfo(str(entry), arcname=arcname)
                tar_info.uid = 0
                tar_info.gid = 0
                tar_info.uname = ""
                tar_info.gname = ""
                tar_info.mtime = 0

                if tar_info.isreg():
                    with entry.open("rb") as handle:
                        tar.addfile(tar_info, handle)
                else:
                    tar.addfile(tar_info)
    finally:
        if process.stdin and not process.stdin.closed:
            process.stdin.close()

    stdout, stderr = process.communicate()
    if process.returncode != 0:
        message = stderr.decode().strip() or f"failed to hash tree {root}"
        raise RuntimeError(message)
    return stdout.decode().strip().split()[0]


def iter_tree_entries(root: Path) -> list[Path]:
    entries: list[Path] = []

    def walk(directory: Path) -> None:
        children = sorted(directory.iterdir(), key=lambda path: path.name)
        for child in children:
            entries.append(child)
            if child.is_dir() and not child.is_symlink():
                walk(child)

    if root.exists() and root.is_dir():
        walk(root)
    return entries


if __name__ == "__main__":
    raise SystemExit(main())
