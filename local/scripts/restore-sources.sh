#!/usr/bin/env bash
# restore-sources.sh — Extract patched source archives back to recipe directories.
#
# Usage:
#   ./local/scripts/restore-sources.sh --release=0.1.0 [recipe ...]
#
# Reads sources/redbear-<release>/manifest.txt to find archives.
# Extracts each archive to recipes/<cat>/<name>/source/.
# Skips extraction if source/ already exists and has matching rev.

set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
RELEASE=""
RECIPES=()

usage() {
    cat <<EOF
Usage: $(basename "$0") --release=<ver> [recipe ...]

Restore recipe sources from release archives.

Options:
  --release=<ver>   Release version (e.g., 0.1.0)
  --force           Overwrite existing source directories
  -h, --help        Show this help

If no recipes specified, restores ALL recipes in the manifest.
EOF
}

FORCE=0
while [ $# -gt 0 ]; do
    case "$1" in
        --release=*) RELEASE="${1#*=}" ;;
        --force)      FORCE=1 ;;
        -h|--help)    usage; exit 0 ;;
        *)            RECIPES+=("$1") ;;
    esac
    shift
done

if [ -z "$RELEASE" ]; then
    echo "ERROR: --release is required" >&2
    usage >&2
    exit 1
fi

ARCHIVE_DIR="$PROJECT_ROOT/sources/redbear-$RELEASE"
MANIFEST="$ARCHIVE_DIR/manifest.txt"

if [ ! -f "$MANIFEST" ]; then
    echo "ERROR: Release manifest not found: $MANIFEST" >&2
    echo "Run: ./local/scripts/provision-release.sh --release=$RELEASE" >&2
    exit 1
fi

cd "$PROJECT_ROOT"

GREEN='\033[1;32m'
YELLOW='\033[1;33m'
RED='\033[1;31m'
NC='\033[0m'

status() { echo -e "${GREEN}==>${NC} $*"; }
warn()  { echo -e "${YELLOW}WARN${NC}: $*"; }
err()   { echo -e "${RED}ERROR${NC}: $*" >&2; }

restored=0
skipped=0
failed=0

# Read manifest and restore each recipe
while IFS= read -r line; do
    [[ "$line" =~ ^# ]] && continue
    [[ -z "$line" ]] && continue
    
    # Parse: category/name type=... key=value ...
    pkg_path=$(echo "$line" | awk '{print $1}')
    pkg_type=$(echo "$line" | awk '{print $2}' | cut -d= -f1)
    
    # If specific recipes requested, filter
    if [ ${#RECIPES[@]} -gt 0 ]; then
        match=0
        for r in "${RECIPES[@]}"; do
            [[ "$pkg_path" == "$r" ]] && match=1
        done
        [ "$match" -eq 0 ] && continue
    fi
    
    source_dir="$PROJECT_ROOT/recipes/$pkg_path/source"
    
    # Skip if source exists and not forced
    if [ -d "$source_dir" ] && [ "$FORCE" -eq 0 ]; then
        warn "source exists: recipes/$pkg_path/source/ (use --force to overwrite)"
        skipped=$((skipped + 1))
        continue
    fi
    
    # Exact archive lookup in release tarballs directory
    archive_name=""
    if [ -f "$ARCHIVE_DIR/manifest.json" ]; then
        archive_name=$(python3 -c "
import json, sys
with open('$ARCHIVE_DIR/manifest.json') as f:
    data = json.load(f)
entry = data.get('entries', {}).get('$pkg_path', {})
if entry.get('type') == 'same_as':
    target = entry.get('target', '')
    target_entry = data.get('entries', {}).get(target, {})
    print(target_entry.get('archive', target_entry.get('snapshot', '')))
elif entry.get('type') == 'path':
    print('__LOCAL_PATH__')
else:
    print(entry.get('archive', ''))
" 2>/dev/null)
    fi
    
    if [ -n "$archive_name" ]; then
        if [ "$archive_name" = "__LOCAL_PATH__" ]; then
            warn "local path source (no archive): $pkg_path"
            skipped=$((skipped + 1))
            continue
        fi
        archive="$ARCHIVE_DIR/tarballs/$archive_name"
        if [ ! -f "$archive" ]; then
            archive="$ARCHIVE_DIR/snapshots/$archive_name"
        fi
    fi
    
    # Fallback: try glob pattern in release tarballs dir
    if [ -z "$archive" ] || [ ! -f "$archive" ]; then
        cat_name=$(dirname "$pkg_path")
        pkg_name=$(basename "$pkg_path")
        shopt -s nullglob
        for f in "$ARCHIVE_DIR/tarballs/${cat_name}-${pkg_name}-"*.tar.gz; do
            [ -f "$f" ] || continue
            archive="$f"
            break
        done
        shopt -u nullglob
    fi
    
    if [ -z "$archive" ]; then
        err "no archive found for $pkg_path in $ARCHIVE_DIR/tarballs/"
        failed=$((failed + 1))
        continue
    fi
    
    # Extract with format auto-detection
    mkdir -p "$(dirname "$source_dir")"
    rm -rf "$source_dir"
    status "restoring: $pkg_path"
    first_entry=$(tar tf "$archive" 2>/dev/null | head -1)
    case "$first_entry" in
        source/*)
            tar xzf "$archive" -C "$source_dir/.." 2>/dev/null ;;
        */source/*)
            tar xzf "$archive" -C "$(dirname "$(dirname "$source_dir")")" 2>/dev/null ;;
        *)
            tar xzf "$archive" -C "$(dirname "$source_dir")" 2>/dev/null ;;
    esac
    
    # Verify extraction
    if [ -d "$source_dir" ]; then
        restored=$((restored + 1))
    else
        err "extraction failed: $pkg_path (archive: $archive)"
        failed=$((failed + 1))
    fi
done < "$MANIFEST"

echo ""
echo "========================================="
echo "  Restore complete"
echo "  Restored: $restored"
echo "  Skipped:  $skipped"
echo "  Failed:   $failed"
echo "========================================="

[ "$failed" -eq 0 ] || exit 1
