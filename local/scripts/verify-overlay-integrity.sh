#!/usr/bin/env bash
# verify-overlay-integrity.sh — ensure Red Bear overlay is present and repairable.
#
# Usage:
#   ./local/scripts/verify-overlay-integrity.sh
#   ./local/scripts/verify-overlay-integrity.sh --repair

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
REPAIR=0

if [ "${1:-}" = "--repair" ]; then
    REPAIR=1
fi

cd "$REPO_ROOT"

fail() {
    echo "overlay-integrity: ERROR: $*" >&2
    exit 1
}

need_file() {
    local path="$1"
    [ -f "$path" ] || fail "missing file: $path"
}

need_symlink_target() {
    local link="$1"
    local expected="$2"
    local current

    if [ ! -L "$link" ]; then
        return 1
    fi

    current="$(readlink "$link")"
    [ "$current" = "$expected" ]
}

ensure_link() {
    local link="$1"
    local target="$2"
    if ! need_symlink_target "$link" "$target"; then
        if [ "$REPAIR" -eq 1 ] && [ -x "$REPO_ROOT/local/scripts/apply-patches.sh" ]; then
            "$REPO_ROOT/local/scripts/apply-patches.sh" >/dev/null
        fi
        need_symlink_target "$link" "$target" || fail "bad symlink: $link -> expected $target"
    fi
}

has_marker() {
    local file="$1"
    local pattern="$2"
    [ -f "$file" ] && grep -q "$pattern" "$file"
}

ensure_marker() {
    local file="$1"
    local pattern="$2"
    if ! has_marker "$file" "$pattern"; then
        if [ "$REPAIR" -eq 1 ]; then
            if [ -x "$REPO_ROOT/local/scripts/apply-patches.sh" ]; then
                "$REPO_ROOT/local/scripts/apply-patches.sh" >/dev/null || true
            fi
            apply_patch_dir "$REPO_ROOT/local/patches/base" "$REPO_ROOT/recipes/core/base/source"
            apply_patch_dir "$REPO_ROOT/local/patches/kernel" "$REPO_ROOT/recipes/core/kernel/source"
            apply_patch_dir "$REPO_ROOT/local/patches/relibc" "$REPO_ROOT/recipes/core/relibc/source"
        fi
    fi
    has_marker "$file" "$pattern" || fail "missing marker in $file: $pattern"
}

check_local_over_wip_priority() {
    local local_dir
    local rel
    local name
    local wip_match
    local active_path

    for local_dir in "$REPO_ROOT"/local/recipes/*/*; do
        [ -d "$local_dir" ] || continue
        rel="${local_dir#"$REPO_ROOT/local/recipes/"}"
        name="${local_dir##*/}"
        wip_match="$(find "$REPO_ROOT/recipes/wip" -mindepth 2 -maxdepth 2 -type d -name "$name" 2>/dev/null | head -n1 || true)"

        # Policy: if local package conflicts with an upstream WIP package of the same name,
        # the active recipe must be our local overlay (symlink in recipes/*/<name>).
        [ -n "$wip_match" ] || continue

        active_path="$(find "$REPO_ROOT/recipes" -mindepth 2 -maxdepth 2 -type l -name "$name" ! -path "$REPO_ROOT/recipes/wip/*" 2>/dev/null | head -n1 || true)"
        # Only enforce for actively mounted local overlays.
        [ -n "$active_path" ] || continue

        if [ "$(readlink "$active_path")" != "../../local/recipes/${rel}" ]; then
            fail "local-over-wip policy violated: '$active_path' does not point to ../../local/recipes/${rel}"
        fi
    done
}

apply_patch_dir() {
    local patch_dir="$1"
    local target_dir="$2"
    local patch_file

    [ -d "$patch_dir" ] || return 0
    [ -d "$target_dir" ] || return 0

    for patch_file in "$patch_dir"/*.patch; do
        [ -f "$patch_file" ] || continue
        if patch --dry-run -p1 -d "$target_dir" < "$patch_file" >/dev/null 2>&1; then
            patch -p1 -d "$target_dir" < "$patch_file" >/dev/null 2>&1 || true
        fi
    done
}

need_file "local/patches/kernel/redox.patch"
need_file "local/patches/base/redox.patch"
need_file "local/patches/relibc/redox.patch"

ensure_link "recipes/core/kernel/redox.patch" "../../../local/patches/kernel/redox.patch"
ensure_link "recipes/core/base/redox.patch" "../../../local/patches/base/redox.patch"
ensure_link "recipes/core/bootloader/P2-live-preload-guard.patch" "../../../local/patches/bootloader/P2-live-preload-guard.patch"
ensure_link "recipes/core/bootloader/P3-uefi-live-image-safe-read.patch" "../../../local/patches/bootloader/P3-uefi-live-image-safe-read.patch"
ensure_link "recipes/core/grub" "../../local/recipes/core/grub"
check_local_over_wip_priority

# Critical runtime markers in source trees (if sources are present locally).
if [ -d "recipes/core/base/source" ]; then
    ensure_marker \
        "recipes/core/base/source/drivers/acpid/src/acpi.rs" \
        "AML interpreter requires PCI registration before initialization"
    ensure_marker \
        "recipes/core/base/source/drivers/acpid/src/scheme.rs" \
        "wait_for_pci_ready"
    ensure_marker \
        "recipes/core/base/source/drivers/input/ps2d/src/controller.rs" \
        "continuing without second port"
fi

echo "overlay-integrity: OK"
