#!/usr/bin/env bash
# archive-sources.sh — Export fully-patched source archives for Red Bear OS.
#
# Usage:
#   ./local/scripts/archive-sources.sh [--all] [--recipe <path>] [--target <triple>]
#
# Creates versioned, fully-patched source archives in sources/<target>/:
#   <category>-<pkgname>-v<version>-patched.tar.gz
#
# Each archive contains: source/ (fully patched) + recipe.toml

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TARGET="${TARGET:-x86_64-unknown-redox}"
SOURCES_DIR="${PROJECT_ROOT}/sources/${TARGET}"
MANIFEST="${SOURCES_DIR}/packages.txt"

mkdir -p "${SOURCES_DIR}"

GREEN='\033[1;32m'
RED='\033[1;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

status()  { echo -e "${GREEN}==>${NC} $*"; }
warn()   { echo -e "${YELLOW}WARN${NC}: $*"; }
err()    { echo -e "${RED}ERROR${NC}: $*" >&2; }

extract_version() {
    local recipe="$1"
    local recipe_dir
    recipe_dir=$(dirname "$recipe")
    local ver=""

    # Try tar URL version extraction
    ver=$(grep -oP 'tar\s*=\s*".*?/[\w-]+-(\d+\.\d+(?:\.\d+)?)\.tar' "$recipe" 2>/dev/null | grep -oP '\d+\.\d+(?:\.\d+)?' | head -1)
    if [ -n "$ver" ]; then
        echo "$ver"
        return
    fi

    # Try explicit rev field
    ver=$(grep -oP 'rev\s*=\s*"([a-f0-9]+)"' "$recipe" 2>/dev/null | grep -oP '[a-f0-9]{7,}' | head -1)
    if [ -n "$ver" ]; then
        echo "${ver:0:7}"
        return
    fi

    # Try git HEAD if source directory exists with .git
    local source_dir="${recipe_dir}/source"
    if [ -d "${source_dir}/.git" ]; then
        ver=$(git -C "$source_dir" rev-parse --short HEAD 2>/dev/null)
        if [ -n "$ver" ]; then
            echo "$ver"
            return
        fi
    fi

    # Fallback
    echo "unknown"
}

extract_pkgname() {
    local recipe_dir="$1"
    basename "$recipe_dir"
}

extract_category() {
    local recipe_dir="$1"
    # recipe_dir is like recipes/core/base or local/recipes/system/redbear-authd
    # Extract the category (parent of pkgname)
    local parent
    parent=$(dirname "$recipe_dir")
    basename "$parent"
}

archive_recipe() {
    local recipe_dir="$1"
    local recipe="${recipe_dir}/recipe.toml"

    if [ ! -f "$recipe" ]; then
        warn "No recipe.toml at $recipe_dir — skipping"
        return 1
    fi

    local pkgname version category archive_name
    pkgname=$(extract_pkgname "$recipe_dir")
    version=$(extract_version "$recipe")
    category=$(extract_category "$recipe_dir")
    archive_name="${category}-${pkgname}-v${version}-patched.tar.gz"
    local archive_path="${SOURCES_DIR}/${archive_name}"

    # Check if source directory exists
    local source_dir="${recipe_dir}/source"
    if [ ! -d "$source_dir" ]; then
        warn "No source/ in $recipe_dir — skipping (may be a meta-package)"
        return 1
    fi

    # Create archive with source + recipe
    status "Archiving ${pkgname} v${version}..."
    if tar -czf "$archive_path" -C "$(dirname "$recipe_dir")" "$(basename "$recipe_dir")/source" "$(basename "$recipe_dir")/recipe.toml" 2>/dev/null; then
        local size
        size=$(du -h "$archive_path" | cut -f1)
        echo -e "  ${GREEN}✓${NC} ${archive_name} (${size})"
        echo "${archive_name}" >> "$MANIFEST"
        return 0
    else
        err "Failed to archive ${pkgname}"
        return 1
    fi
}

archive_all() {
    > "$MANIFEST"
    local count=0 failed=0 skipped=0

    # Find all recipe directories with recipe.toml files
    while IFS= read -r -d '' recipe_file; do
        local recipe_dir
        recipe_dir=$(dirname "$recipe_file")

        # Skip if already in sources/ (avoid double-archiving)
        local pkgname
        pkgname=$(basename "$recipe_dir")

        if archive_recipe "$recipe_dir"; then
            count=$((count + 1))
        else
            failed=$((failed + 1))
        fi
    done < <(find "${PROJECT_ROOT}/recipes" "${PROJECT_ROOT}/local/recipes" -name "recipe.toml" -print0 2>/dev/null)

    echo ""
    status "Archive complete: ${count} packages, ${failed} failures"
}

# ── Main ────────────────────────────────────────────────────────────

case "${1:-}" in
    --recipe)
        if [ -z "${2:-}" ]; then
            err "--recipe requires a path"
            exit 1
        fi
        > "$MANIFEST"
        archive_recipe "${PROJECT_ROOT}/${2}"
        ;;
    --all)
        archive_all
        ;;
    *)
        echo "Usage: $0 --all | --recipe <path>"
        echo ""
        echo "  --all           Archive all recipes with source directories"
        echo "  --recipe PATH   Archive a specific recipe (e.g. recipes/core/base)"
        echo ""
        echo "  Environment: TARGET=x86_64-unknown-redox (default)"
        exit 1
        ;;
esac
