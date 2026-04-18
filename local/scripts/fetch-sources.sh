#!/usr/bin/env bash
# fetch-sources.sh — Download Redox recipe sources for browsing and editing
#
# The build system fetches on-demand during `make all`, but you need sources
# present BEFORE building to read, edit, and patch them. This script bulk-fetches
# sources so they're available in recipes/<category>/<pkg>/source/.
#
# Usage:
#   ./local/scripts/fetch-sources.sh --upstream     # All sources
#   ./local/scripts/fetch-sources.sh --upstream core           # Core packages only
#   ./local/scripts/fetch-sources.sh --upstream libs tools     # Multiple categories
#   ./local/scripts/fetch-sources.sh --list         # Show available categories
#   ./local/scripts/fetch-sources.sh --status       # Show fetch progress
#   ./local/scripts/fetch-sources.sh --help         # Show help
#
# After fetching, sources live at:
#   recipes/<category>/<pkg>/source/       (git repos, tar extractions)
#   local/recipes/<category>/<pkg>/source/ (our custom overlay packages)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
REPO_BIN="$PROJECT_ROOT/target/release/repo"

cd "$PROJECT_ROOT"

ALLOW_UPSTREAM=0
MODE="fetch"
CATEGORIES=()

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS] [CATEGORY ...]

Fetch recipe sources for browsing and editing.

Options:
  --upstream          Allow Redox/upstream source fetch
  --list              Show available categories
  --status            Show fetch progress
  -h, --help          Show this help
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --upstream)
            ALLOW_UPSTREAM=1
            ;;
        --list)
            MODE="list"
            ;;
        --status)
            MODE="status"
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        -*)
            echo "Unknown option: $1" >&2
            usage >&2
            exit 1
            ;;
        *)
            CATEGORIES+=("$1")
            ;;
    esac
    shift
done

if [ ! -f "$REPO_BIN" ]; then
    echo ">>> Building cookbook binary..."
    cargo build --release
fi

ALL_CATEGORIES=$(ls -d recipes/*/ 2>/dev/null | xargs -I{} basename {} | sort)

list_categories() {
    echo "Available recipe categories:"
    for cat in $ALL_CATEGORIES; do
        count=$(find "recipes/$cat" -mindepth 1 -maxdepth 1 -type d | wc -l)
        printf "  %-20s %3d recipes\n" "$cat" "$count"
    done
}

show_status() {
    total=0; fetched=0
    for d in $(find recipes -mindepth 2 -maxdepth 2 -type d); do
        [ -f "$d/recipe.toml" ] || continue
        total=$((total + 1))
        if [ -d "$d/source" ] && [ "$(ls -A "$d/source/" 2>/dev/null)" ]; then
            fetched=$((fetched + 1))
        fi
    done
    local pct=0
    [ "$total" -gt 0 ] && pct=$((fetched * 100 / total))
    echo "Sources fetched: $fetched / $total ($pct%)"
}

if [ "$MODE" = "list" ]; then
    list_categories
    exit 0
fi

if [ "$MODE" = "status" ]; then
    show_status
    exit 0
fi

if [ "$ALLOW_UPSTREAM" -ne 1 ]; then
    echo "ERROR: Redox/upstream source fetch is disabled by default." >&2
    echo "Re-run with --upstream to fetch sources." >&2
    exit 1
fi

echo "========================================"
echo "   Redox Source Fetcher"
echo "========================================"
show_status
echo ""

fetch_one() {
    local recipe_path="$1"
    local name
    name=$(basename "$recipe_path")

    if [ -d "$recipe_path/source" ] && [ "$(ls -A "$recipe_path/source/" 2>/dev/null)" ]; then
        return 0
    fi

    "$REPO_BIN" fetch "$recipe_path" 2>/dev/null && return 0

    echo "  FAIL $name"
    return 1
}

fetch_category() {
    local cat="$1"
    local ok=0 fail=0 skip=0

    for pkg_dir in "recipes/$cat"/*/; do
        [ -f "$pkg_dir/recipe.toml" ] || continue
        local name
        name=$(basename "$pkg_dir")

        if [ -d "$pkg_dir/source" ] && [ "$(ls -A "$pkg_dir/source/" 2>/dev/null)" ]; then
            skip=$((skip + 1))
            continue
        fi

        if "$REPO_BIN" fetch "$pkg_dir" 2>/dev/null; then
            ok=$((ok + 1))
        else
            echo "  FAIL $name"
            fail=$((fail + 1))
        fi
    done

    [ "$ok" -gt 0 ] && echo "  ✅ $cat: $ok fetched, $skip cached, $fail failed"
    [ "$ok" -eq 0 ] && [ "$skip" -gt 0 ] && echo "  ⏭️  $cat: $skip cached"
    [ "$ok" -eq 0 ] && [ "$skip" -eq 0 ] && [ "$fail" -gt 0 ] && echo "  ❌ $cat: $fail failed"
}

if [ ${#CATEGORIES[@]} -eq 0 ]; then
    echo ">>> Fetching ALL recipe sources..."
    echo ""
    for cat in $ALL_CATEGORIES; do
        fetch_category "$cat"
    done
else
    for cat in "${CATEGORIES[@]}"; do
        if [ -d "recipes/$cat" ]; then
            echo ">>> Fetching category: $cat"
            fetch_category "$cat"
        else
            echo "WARNING: Unknown category '$cat' (skipping)"
        fi
    done
fi

echo ""
echo "========================================"
show_status
echo "========================================"
echo ""
echo "Sources are at: recipes/<category>/<pkg>/source/"
echo "Our overlay:    local/recipes/<category>/<pkg>/source/"
