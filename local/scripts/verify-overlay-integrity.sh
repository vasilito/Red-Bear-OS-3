#!/usr/bin/env bash
# verify-overlay-integrity.sh — Verify the Red Bear OS overlay structure is intact.
#
# Checks:
#   1. All recipe symlinks (recipes/ → local/recipes/) resolve correctly
#   2. All patch symlinks (recipes/ → local/patches/) resolve correctly
#   3. No circular symlink references
#   4. Critical local/patches/ files exist
#   5. Critical config/redbear-*.toml files exist
#
# Usage:
#   ./local/scripts/verify-overlay-integrity.sh           # Check and report
#   ./local/scripts/verify-overlay-integrity.sh --repair  # Re-run apply-patches.sh on failures
#   ./local/scripts/verify-overlay-integrity.sh --quiet   # Exit code only (for CI)
#
# Exit codes:
#   0 — all checks pass
#   1 — one or more checks failed

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
REPAIR=0
QUIET=0

for arg in "$@"; do
    case "$arg" in
        --repair) REPAIR=1 ;;
        --quiet)  QUIET=1 ;;
        --help|-h)
            echo "Usage: $0 [--repair] [--quiet]"
            echo "  --repair  Re-run apply-patches.sh if overlay checks fail"
            echo "  --quiet   Exit code only, no output"
            exit 0
            ;;
        *)
            echo "Unknown argument: $arg" >&2
            exit 1
            ;;
    esac
done

cd "$REPO_ROOT"

ERRORS=0
WARNINGS=0

log() {
    [ "$QUIET" = "0" ] && echo "$@"
}

log_error() {
    [ "$QUIET" = "0" ] && echo "  ERROR: $*" >&2
    ERRORS=$((ERRORS + 1))
}

log_warn() {
    [ "$QUIET" = "0" ] && echo "  WARN: $*"
    WARNINGS=$((WARNINGS + 1))
}

# ── 1. Recipe symlinks (recipes/ → local/recipes/) ──────────────────
log "==> Checking recipe symlinks (recipes/ → local/recipes/)..."
RECIPE_SYMLINK_COUNT=0
BROKEN_RECIPE_SYMLINKS=0

while IFS= read -r link; do
    RECIPE_SYMLINK_COUNT=$((RECIPE_SYMLINK_COUNT + 1))
    target="$(readlink -f "$link" 2>/dev/null || true)"

    if [ -z "$target" ]; then
        log_error "broken symlink: $link (cannot resolve target)"
        BROKEN_RECIPE_SYMLINKS=$((BROKEN_RECIPE_SYMLINKS + 1))
        continue
    fi

    # Check if target is inside local/recipes/
    if echo "$target" | grep -q "/local/recipes/"; then
        if [ ! -e "$target" ]; then
            log_error "dangling symlink: $link -> $target (target does not exist)"
            BROKEN_RECIPE_SYMLINKS=$((BROKEN_RECIPE_SYMLINKS + 1))
        fi
    fi

    # Check for circular symlinks
    if [ -L "$target" ]; then
        # The target itself is a symlink — could be circular
        resolved_inner="$(readlink -f "$target" 2>/dev/null || true)"
        if [ -n "$resolved_inner" ] && [ "$resolved_inner" = "$(readlink -f "$link" 2>/dev/null)" ]; then
            log_error "circular symlink: $link -> $target -> (circular)"
            BROKEN_RECIPE_SYMLINKS=$((BROKEN_RECIPE_SYMLINKS + 1))
        fi
    fi
done < <(find recipes -maxdepth 3 -type l 2>/dev/null | grep -v '/target$\|/stage\|/sysroot$\|/source$' | sort)

log "    $RECIPE_SYMLINK_COUNT recipe symlinks checked, $BROKEN_RECIPE_SYMLINKS broken"

# ── 2. Patch symlinks (recipes/ → local/patches/) ───────────────────
log "==> Checking patch symlinks (recipes/ → local/patches/)..."
PATCH_SYMLINK_COUNT=0
BROKEN_PATCH_SYMLINKS=0

EXPECTED_PATCH_SYMLINKS=(
    "recipes/core/kernel/redox.patch"
    "recipes/core/base/redox.patch"
    "recipes/core/base/P2-boot-runtime-fixes.patch"
    "recipes/core/relibc/redox.patch"
    "recipes/core/installer/redox.patch"
    "recipes/core/bootloader/redox.patch"
    "recipes/core/bootloader/P2-live-preload-guard.patch"
    "recipes/core/bootloader/P3-uefi-live-image-safe-read.patch"
    "recipes/gui/orbutils/redox.patch"
)

for patch_link in "${EXPECTED_PATCH_SYMLINKS[@]}"; do
    PATCH_SYMLINK_COUNT=$((PATCH_SYMLINK_COUNT + 1))
    if [ ! -L "$patch_link" ]; then
        if [ -f "$patch_link" ]; then
            log_warn "$patch_link exists as regular file (not a symlink to local/patches/)"
        else
            log_error "$patch_link missing (should be symlink to local/patches/)"
            BROKEN_PATCH_SYMLINKS=$((BROKEN_PATCH_SYMLINKS + 1))
        fi
        continue
    fi
    target="$(readlink -f "$patch_link" 2>/dev/null || true)"
    if [ -z "$target" ] || [ ! -f "$target" ]; then
        log_error "dangling patch symlink: $patch_link -> $target"
        BROKEN_PATCH_SYMLINKS=$((BROKEN_PATCH_SYMLINKS + 1))
    fi
done

log "    $PATCH_SYMLINK_COUNT patch symlinks checked, $BROKEN_PATCH_SYMLINKS broken"

# ── 3. Critical local/patches/ files ────────────────────────────────
log "==> Checking critical local/patches/ files..."
CRITICAL_PATCHES=(
    "local/patches/kernel/redox.patch"
    "local/patches/base/redox.patch"
    "local/patches/relibc/redox.patch"
    "local/patches/installer/redox.patch"
    "local/patches/bootloader/redox.patch"
    "local/patches/build-system/001-rebrand-and-build.patch"
    "local/patches/build-system/002-cookbook-fixes.patch"
    "local/patches/build-system/003-config.patch"
    "local/patches/build-system/004-docs-and-cleanup.patch"
)

MISSING_PATCHES=0
for patch_file in "${CRITICAL_PATCHES[@]}"; do
    if [ ! -f "$patch_file" ]; then
        log_error "missing critical patch: $patch_file"
        MISSING_PATCHES=$((MISSING_PATCHES + 1))
    fi
done

if [ "$MISSING_PATCHES" = "0" ]; then
    log "    All critical patches present"
fi

# ── 4. Config files ─────────────────────────────────────────────────
log "==> Checking config/redbear-*.toml files..."
CRITICAL_CONFIGS=(
    "config/redbear-full.toml"
    "config/redbear-live-full.toml"
    "config/redbear-minimal.toml"
    "config/redbear-live-minimal.toml"
    "config/redbear-desktop.toml"
    "config/redbear-device-services.toml"
    "config/redbear-legacy-base.toml"
    "config/redbear-legacy-desktop.toml"
    "config/redbear-netctl.toml"
    "config/redbear-greeter-services.toml"
)

MISSING_CONFIGS=0
for config_file in "${CRITICAL_CONFIGS[@]}"; do
    if [ ! -f "$config_file" ]; then
        log_error "missing config: $config_file"
        MISSING_CONFIGS=$((MISSING_CONFIGS + 1))
    fi
done

TOTAL_REDBEAR_CONFIGS=$(find config -maxdepth 1 -name 'redbear-*.toml' 2>/dev/null | wc -l)
log "    $TOTAL_REDBEAR_CONFIGS redbear-*.toml files found, $MISSING_CONFIGS critical configs missing"

# ── 5. local/ directory structure ───────────────────────────────────
log "==> Checking local/ directory structure..."
REQUIRED_LOCAL_DIRS=(
    "local/recipes"
    "local/recipes/drivers"
    "local/recipes/gpu"
    "local/recipes/system"
    "local/recipes/core"
    "local/recipes/libs"
    "local/recipes/branding"
    "local/recipes/kde"
    "local/patches"
    "local/patches/kernel"
    "local/patches/base"
    "local/patches/relibc"
    "local/patches/bootloader"
    "local/patches/installer"
    "local/patches/build-system"
    "local/docs"
    "local/scripts"
    "local/Assets"
)

MISSING_DIRS=0
for dir in "${REQUIRED_LOCAL_DIRS[@]}"; do
    if [ ! -d "$dir" ]; then
        log_error "missing directory: $dir"
        MISSING_DIRS=$((MISSING_DIRS + 1))
    fi
done

if [ "$MISSING_DIRS" = "0" ]; then
    log "    All required local/ directories present"
fi

# ── Summary ─────────────────────────────────────────────────────────
log ""
log "=== Overlay Integrity Summary ==="
log "  Recipe symlinks:  $RECIPE_SYMLINK_COUNT ($BROKEN_RECIPE_SYMLINKS broken)"
log "  Patch symlinks:   $PATCH_SYMLINK_COUNT ($BROKEN_PATCH_SYMLINKS broken)"
log "  Critical patches: ${#CRITICAL_PATCHES[@]} ($MISSING_PATCHES missing)"
log "  Critical configs: ${#CRITICAL_CONFIGS[@]} ($MISSING_CONFIGS missing)"
log "  Required dirs:    ${#REQUIRED_LOCAL_DIRS[@]} ($MISSING_DIRS missing)"
log "  Warnings:         $WARNINGS"
log "  Errors:           $ERRORS"

if [ "$ERRORS" -gt 0 ]; then
    log ""
    log "!! Overlay integrity check FAILED ($ERRORS errors)"
    if [ "$REPAIR" = "1" ]; then
        log "==> Attempting repair via apply-patches.sh..."
        if [ -f local/scripts/apply-patches.sh ]; then
            bash local/scripts/apply-patches.sh
            log "==> Repair complete. Re-running verification..."
            exec "$0" --quiet
        else
            log_error "apply-patches.sh not found — cannot repair"
            exit 1
        fi
    else
        log "    Run with --repair to attempt automatic fix, or:"
        log "      ./local/scripts/apply-patches.sh"
        exit 1
    fi
else
    if [ "$QUIET" = "0" ]; then
        log ""
        log "  Overlay integrity check PASSED"
    fi
    exit 0
fi
