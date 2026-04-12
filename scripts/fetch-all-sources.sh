#!/usr/bin/env bash
# fetch-all-sources.sh — Download ALL Redox OS + Red Bear OS package sources.
#
# Usage:
#   ./scripts/fetch-all-sources.sh                    # Fetch for default desktop config
#   ./scripts/fetch-all-sources.sh redbear-full       # Fetch for a specific config
#   ./scripts/fetch-all-sources.sh --all-configs      # Fetch for every config
#   ./scripts/fetch-all-sources.sh --recipe kernel    # Fetch a single recipe
#   ./scripts/fetch-all-sources.sh --list             # List recipes that would be fetched
#   ./scripts/fetch-all-sources.sh --status           # Show which sources already exist
#
# Prerequisites: rustup + nightly, git, wget, tar. The script builds the
# cookbook `repo` binary if not already built.
#
# Sources are placed in recipes/<category>/<name>/source/ for git/tar recipes,
# and are left in-place for local/recipes/ (path-based sources).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

REPO_BIN="./target/release/repo"
CONFIG_NAME="${1:-desktop}"
ACTION="fetch"

# ── Argument parsing ────────────────────────────────────────────────
usage() {
    echo "Usage: $0 [OPTIONS] [CONFIG_NAME]"
    echo ""
    echo "Download all package sources needed to build Red Bear OS."
    echo ""
    echo "Options:"
    echo "  --all-configs    Fetch sources for every config in config/"
    echo "  --recipe NAME    Fetch a single recipe by name"
    echo "  --list           List recipes that would be fetched (no download)"
    echo "  --status         Show which sources already exist locally"
    echo "  --help           Show this help"
    echo ""
    echo "Configs: desktop, redbear-full, redbear-minimal, server, minimal, wayland, x11"
    echo "Default config: desktop"
}

ALL_CONFIGS=0
SINGLE_RECIPE=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --all-configs)
            ALL_CONFIGS=1
            shift
            ;;
        --recipe)
            SINGLE_RECIPE="${2:?--recipe requires a recipe name}"
            shift 2
            ;;
        --list)
            ACTION="list"
            shift
            ;;
        --status)
            ACTION="status"
            shift
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        -*)
            echo "Unknown option: $1"
            usage
            exit 1
            ;;
        *)
            CONFIG_NAME="$1"
            shift
            ;;
    esac
done

# ── Build cookbook repo binary if needed ────────────────────────────
build_repo() {
    if [ ! -x "$REPO_BIN" ]; then
        echo "==> Building cookbook repo binary..."
        cargo build --release --manifest-path Cargo.toml
    fi
}

# ── Resolve FILESYSTEM_CONFIG for a given config name ───────────────
resolve_config() {
    local name="$1"
    if [ -f "config/${name}.toml" ]; then
        echo "config/${name}.toml"
    elif [ -f "config/x86_64/${name}.toml" ]; then
        echo "config/x86_64/${name}.toml"
    else
        echo "ERROR: config/${name}.toml not found" >&2
        return 1
    fi
}

# ── Fetch sources for a config ──────────────────────────────────────
fetch_for_config() {
    local config_name="$1"
    local config_file
    config_file="$(resolve_config "$config_name")" || return 1

    echo ""
    echo "==> Fetching sources for config: $config_name"
    echo "    Config file: $config_file"
    echo ""

    export PATH="$(pwd)/prefix/x86_64-unknown-redox/relibc-install/bin:${PATH:-}"
    export COOKBOOK_HOST_SYSROOT="$(pwd)/prefix/x86_64-unknown-redox/relibc-install"

    "$REPO_BIN" fetch "--filesystem=$config_file" --with-package-deps
    echo "==> Done fetching for $config_name"
}

# ── Fetch a single recipe ──────────────────────────────────────────
fetch_single_recipe() {
    local recipe_name="$1"
    echo ""
    echo "==> Fetching single recipe: $recipe_name"
    echo ""

    export PATH="$(pwd)/prefix/x86_64-unknown-redox/relibc-install/bin:${PATH:-}"
    export COOKBOOK_HOST_SYSROOT="$(pwd)/prefix/x86_64-unknown-redox/relibc-install"

    "$REPO_BIN" fetch "$recipe_name"
    echo "==> Done fetching $recipe_name"
}

# ── List recipes for a config ───────────────────────────────────────
list_for_config() {
    local config_name="$1"
    local config_file
    config_file="$(resolve_config "$config_name")" || return 1

    echo ""
    echo "==> Recipes for config: $config_name ($config_file)"
    echo ""

    "$REPO_BIN" cook-tree "--filesystem=$config_file" --with-package-deps 2>/dev/null || {
        echo "    (cook-tree unavailable — listing recipe directories instead)"
        find recipes -name "recipe.toml" -not -path "*/source/*" | sort | \
            sed 's|recipes/||; s|/recipe.toml||'
    }
}

# ── Status: show which sources exist ────────────────────────────────
show_status() {
    echo "==> Source status for all recipes"
    echo ""

    local total=0 fetched=0 local_src=0 missing=0

    while IFS= read -r recipe_toml; do
        recipe_dir="$(dirname "$recipe_toml")"
        recipe_name="$(basename "$recipe_dir")"
        category="$(basename "$(dirname "$recipe_dir")")"

        total=$((total + 1))

        if [ -d "$recipe_dir/source" ]; then
            # Check if it's a symlink (local recipe)
            if [ -L "$recipe_dir/source" ] || grep -q '^path = "source"' "$recipe_toml" 2>/dev/null; then
                local_src=$((local_src + 1))
            else
                fetched=$((fetched + 1))
            fi
        else
            # Check if source section exists
            if grep -q '^\[source\]' "$recipe_toml" 2>/dev/null; then
                missing=$((missing + 1))
                echo "  MISSING  $category/$recipe_name"
            fi
        fi
    done < <(find recipes -name "recipe.toml" -not -path "*/source/*" | sort)

    # Also check local recipes
    while IFS= read -r recipe_toml; do
        recipe_dir="$(dirname "$recipe_toml")"
        recipe_name="$(basename "$recipe_dir")"
        total=$((total + 1))
        local_src=$((local_src + 1))
    done < <(find local/recipes -name "recipe.toml" -not -path "*/source/*" 2>/dev/null | sort)

    echo ""
    echo "Total recipes:    $total"
    echo "Sources fetched:  $fetched"
    echo "Local sources:    $local_src"
    echo "Missing:          $missing"
    echo ""
    if [ "$missing" -gt 0 ]; then
        echo "Run '$0 ${CONFIG_NAME}' to fetch missing sources."
    else
        echo "All sources are present."
    fi
}

# ── Main ────────────────────────────────────────────────────────────

case "$ACTION" in
    status)
        show_status
        ;;
    list)
        build_repo
        if [ "$ALL_CONFIGS" -eq 1 ]; then
            for cfg in desktop redbear-full redbear-minimal server minimal wayland x11; do
                list_for_config "$cfg" 2>/dev/null || true
            done
        else
            list_for_config "$CONFIG_NAME"
        fi
        ;;
    fetch)
        build_repo

        if [ -n "$SINGLE_RECIPE" ]; then
            fetch_single_recipe "$SINGLE_RECIPE"
        elif [ "$ALL_CONFIGS" -eq 1 ]; then
            echo "==> Fetching sources for ALL configs"
            echo "    This ensures every recipe needed by any config is downloaded."
            for cfg in desktop redbear-full redbear-minimal server minimal wayland x11; do
                fetch_for_config "$cfg" 2>/dev/null || {
                    echo "    WARNING: failed to fetch for $cfg (some recipes may not exist)"
                    echo ""
                }
            done
            echo ""
            echo "==> All sources fetched. Summary:"
            show_status
        else
            fetch_for_config "$CONFIG_NAME"
            echo ""
            show_status
        fi
        ;;
esac
