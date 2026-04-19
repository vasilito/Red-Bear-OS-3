#!/usr/bin/env bash
# fetch-all-sources.sh — Download ALL Redox OS + Red Bear OS package sources.
#
# Smart re-download: skips sources whose local checksum matches the recipe's
# blake3.  Falls back to file-size comparison when no blake3 is recorded.
#
# Usage:
#   ./scripts/fetch-all-sources.sh                    # Fetch for the tracked KWin default config
#   ./scripts/fetch-all-sources.sh redbear-full       # Fetch for a specific config
#   ./scripts/fetch-all-sources.sh --all-configs      # Fetch for every config
#   ./scripts/fetch-all-sources.sh --recipe kernel    # Fetch a single recipe
#   ./scripts/fetch-all-sources.sh --list             # List recipes that would be fetched
#   ./scripts/fetch-all-sources.sh --status           # Show which sources already exist
#   ./scripts/fetch-all-sources.sh --preflight        # Smart checksum/size check (no download)
#
# Prerequisites: rustup + nightly, git, wget, tar, curl, b3sum.
# The script builds the cookbook `repo` binary if not already built.
# If b3sum is not installed, it will be installed via cargo.
#
# Sources are placed in recipes/<category>/<name>/source/ for git/tar recipes,
# and are left in-place for local/recipes/ (path-based sources).
#
# WIP policy note:
# upstream WIP recipes are still useful fetch inputs, but Red Bear may ship the maintained version
# from local/recipes/ instead. Fetching upstream WIP source does not by itself make that upstream
# tree the durable shipping source of truth.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

REPO_BIN="./target/release/repo"
CONFIG_NAME="${1:-redbear-full}"
ACTION="fetch"

# ── Colors (disabled when not a terminal) ───────────────────────────
if [ -t 1 ]; then
    C_GREEN="\033[0;32m" C_YELLOW="\033[0;33m" C_RED="\033[0;31m"
    C_CYAN="\033[0;36m" C_BOLD="\033[1m" C_RESET="\033[0m"
else
    C_GREEN="" C_YELLOW="" C_RED="" C_CYAN="" C_BOLD="" C_RESET=""
fi

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
    echo "  --preflight      Smart blake3/size check — show what needs updating"
    echo "  --force          Force re-download even if checksums match"
    echo "  --help           Show this help"
    echo ""
    echo "Configs: redbear-full, redbear-minimal, redbear-live-full, redbear-live-minimal"
    echo "Default config: redbear-full"
}

ALL_CONFIGS=0
SINGLE_RECIPE=""
FORCE_FETCH=0
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
        --preflight)
            ACTION="preflight"
            shift
            ;;
        --force)
            FORCE_FETCH=1
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

# ── Checksum / size utilities ───────────────────────────────────────

# Ensure b3sum is available
ensure_b3sum() {
    if ! command -v b3sum &>/dev/null; then
        echo "  Installing b3sum (blake3 CLI tool)..."
        cargo install b3sum 2>&1 | tail -1
        if ! command -v b3sum &>/dev/null; then
            echo "  WARNING: b3sum not available. Size-based fallback only."
        fi
    fi
}

# Compute blake3 of a file (returns empty string if b3sum unavailable)
compute_blake3() {
    local file="$1"
    if command -v b3sum &>/dev/null && [ -f "$file" ]; then
        b3sum --no-names "$file" | awk '{print $1}'
    fi
}

# Get remote file size via HTTP HEAD (follows redirects)
get_remote_size() {
    local url="$1"
    # -sI: silent, HEAD only, -L: follow redirects, --max-time: timeout
    curl -sI -L --max-time 15 "$url" 2>/dev/null \
        | grep -i '^content-length:' \
        | tail -1 \
        | awk '{print $2}' \
        | tr -d '\r\n'
}

# Get local file size (portable across Linux/macOS)
get_local_size() {
    local file="$1"
    if [ -f "$file" ]; then
        stat -c%s "$file" 2>/dev/null || stat -f%z "$file" 2>/dev/null || echo ""
    fi
}

# ── TOML field extraction (simple, no dependencies) ─────────────────

# Extract a quoted string field from recipe.toml: field = "value"
recipe_str_field() {
    local file="$1" field="$2"
    grep "^${field} *= *\"" "$file" 2>/dev/null | head -1 | sed 's/^[^"]*"\([^"]*\)".*/\1/'
}

# ── Per-recipe smart check ──────────────────────────────────────────
#
# Returns one of: "cached" | "missing" | "mismatch" | "no-checksum"
# Prints reason to stdout.

check_recipe_source() {
    local recipe_dir="$1"
    local recipe_toml="$recipe_dir/recipe.toml"
    local source_dir="$recipe_dir/source"
    local source_tar="$recipe_dir/source.tar"

    # No recipe file
    [ -f "$recipe_toml" ] || { echo "no-recipe"; return; }

    # Path-based sources — always cached
    if grep -q '^path *= *"source"' "$recipe_toml" 2>/dev/null; then
        echo "cached:path"
        return
    fi

    # ── Tar source ──────────────────────────────────────────────
    local tar_url
    tar_url=$(recipe_str_field "$recipe_toml" "tar")
    if [ -n "$tar_url" ]; then
        # No local tar at all
        if [ ! -f "$source_tar" ]; then
            # source dir might exist from a previous extract — check blake3 of
            # the recipe against nothing: we just need to download
            echo "missing"
            return
        fi

        # Tar exists — check blake3
        local blake3_expected
        blake3_expected=$(recipe_str_field "$recipe_toml" "blake3")

        if [ -n "$blake3_expected" ]; then
            local blake3_local
            blake3_local=$(compute_blake3 "$source_tar")
            if [ -n "$blake3_local" ] && [ "$blake3_local" = "$blake3_expected" ]; then
                echo "cached:blake3"
                return
            else
                echo "mismatch:blake3"
                return
            fi
        fi

        # No blake3 in recipe — fall back to size comparison
        local local_size remote_size
        local_size=$(get_local_size "$source_tar")
        remote_size=$(get_remote_size "$tar_url")

        if [ -n "$remote_size" ] && [ -n "$local_size" ] && [ "$local_size" = "$remote_size" ]; then
            echo "cached:size"
            return
        else
            echo "mismatch:size"
            return
        fi
    fi

    # ── Git source ──────────────────────────────────────────────
    if grep -q '^git *= *"' "$recipe_toml" 2>/dev/null; then
        if [ -d "$source_dir/.git" ]; then
            echo "cached:git"
            return
        elif [ -d "$source_dir" ]; then
            echo "cached:git-dir"
            return
        else
            echo "missing"
            return
        fi
    fi

    # ── same_as source ──────────────────────────────────────────
    if grep -q '^same_as *= *"' "$recipe_toml" 2>/dev/null; then
        echo "cached:same_as"
        return
    fi

    # Unknown — let repo handle it
    echo "missing"
}

# ── Preflight: scan all recipes and report status ───────────────────

preflight_scan() {
    local label="${1:-all recipes}"
    local total=0 cached=0 missing=0 mismatch=0 no_checksum=0
    local missing_list=() mismatch_list=()

    echo ""
    printf "${C_BOLD}==> Smart preflight scan: %s${C_RESET}\n" "$label"
    echo "    Checking blake3 checksums and file sizes..."
    echo ""

    while IFS= read -r recipe_toml; do
        local recipe_dir recipe_name category
        recipe_dir="$(dirname "$recipe_toml")"
        recipe_name="$(basename "$recipe_dir")"
        category="$(basename "$(dirname "$recipe_dir")")"

        # Skip recipes without a [source] section
        grep -q '^\[source\]' "$recipe_toml" 2>/dev/null || continue

        total=$((total + 1))
        local status reason
        status=$(check_recipe_source "$recipe_dir")
        reason="${status#*:}"
        status="${status%%:*}"

        case "$status" in
            cached)
                cached=$((cached + 1))
                ;;
            missing)
                missing=$((missing + 1))
                printf "  ${C_YELLOW}MISSING  %-30s  %s${C_RESET}\n" "$category/$recipe_name" "$reason"
                missing_list+=("$category/$recipe_name")
                ;;
            mismatch)
                mismatch=$((mismatch + 1))
                printf "  ${C_RED}CHANGED  %-30s  %s${C_RESET}\n" "$category/$recipe_name" "$reason"
                mismatch_list+=("$category/$recipe_name")
                ;;
            *)
                # no-recipe, same_as, etc. — skip
                ;;
        esac
    done < <(find recipes local/recipes -name "recipe.toml" -not -path "*/source/*" 2>/dev/null | sort)

    echo ""
    printf "  ${C_BOLD}Total recipes:${C_RESET}   %3d\n" "$total"
    printf "  ${C_GREEN}Cached (skip):${C_RESET}  %3d\n" "$cached"
    printf "  ${C_YELLOW}Missing:${C_RESET}       %3d\n" "$missing"
    printf "  ${C_RED}Changed:${C_RESET}        %3d\n" "$mismatch"
    echo ""

    if [ "$((missing + mismatch))" -eq 0 ]; then
        printf "  ${C_GREEN}✓ All sources are up to date.${C_RESET}\n"
        return 1  # nothing to do
    else
        printf "  ${C_BOLD}%d source(s) need downloading.${C_RESET}\n" "$((missing + mismatch))"
        return 0
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

    # Find recipe directory
    local recipe_dir=""
    for d in $(find recipes local/recipes -maxdepth 2 -name "$recipe_name" -type d 2>/dev/null); do
        if [ -f "$d/recipe.toml" ]; then
            recipe_dir="$d"
            break
        fi
    done

    if [ -z "$recipe_dir" ]; then
        echo "ERROR: recipe '$recipe_name' not found" >&2
        return 1
    fi

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
    local config_filter="${1:-}"
    echo "==> Source status${config_filter:+ for config: $config_filter}"
    echo ""

    local total=0 fetched=0 local_src=0 missing=0

    local recipe_list
    if [ -n "$config_filter" ] && [ -x "$REPO_BIN" ]; then
        local config_file
        config_file="$(resolve_config "$config_filter")" 2>/dev/null || {
            config_filter=""
        }
        if [ -n "$config_filter" ]; then
            recipe_list=$("$REPO_BIN" cook-tree "--filesystem=$config_file" --with-package-deps 2>/dev/null | grep -v '^=' | grep -v '^$')
        fi
    fi

    check_one_recipe() {
        local recipe_toml="$1"
        recipe_dir="$(dirname "$recipe_toml")"
        recipe_name="$(basename "$recipe_dir")"
        category="$(basename "$(dirname "$recipe_dir")")"

        total=$((total + 1))

        if [ -d "$recipe_dir/source" ]; then
            if [ -L "$recipe_dir/source" ] || grep -q '^path *= *"source"' "$recipe_toml" 2>/dev/null; then
                local_src=$((local_src + 1))
            else
                fetched=$((fetched + 1))
            fi
        else
            if grep -q '^\[source\]' "$recipe_toml" 2>/dev/null; then
                missing=$((missing + 1))
                echo "  MISSING  $category/$recipe_name"
            fi
        fi
    }

    if [ -n "${recipe_list:-}" ]; then
        while IFS= read -r recipe_name; do
            local found=0
            while IFS= read -r recipe_toml; do
                check_one_recipe "$recipe_toml"
                found=1
                break
            done < <(find recipes local/recipes -path "*/${recipe_name}/recipe.toml" -not -path "*/source/*" 2>/dev/null | head -1)
            if [ "$found" -eq 0 ]; then
                total=$((total + 1))
                missing=$((missing + 1))
                echo "  MISSING  $recipe_name (no recipe.toml found)"
            fi
        done <<< "$recipe_list"
    else
        while IFS= read -r recipe_toml; do
            check_one_recipe "$recipe_toml"
        done < <(find recipes -name "recipe.toml" -not -path "*/source/*" | sort)

        while IFS= read -r recipe_toml; do
            recipe_dir="$(dirname "$recipe_toml")"
            total=$((total + 1))
            local_src=$((local_src + 1))
        done < <(find local/recipes -name "recipe.toml" -not -path "*/source/*" 2>/dev/null | sort)
    fi

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

# Ensure b3sum is available for checksum-based checking
ensure_b3sum

case "$ACTION" in
    status)
        show_status ""
        ;;
    preflight)
        build_repo
        if [ "$ALL_CONFIGS" -eq 1 ]; then
            for cfg in redbear-kde redbear-live redbear-full redbear-minimal redbear-wayland; do
                preflight_scan "$cfg" || true
            done
        else
            preflight_scan "$CONFIG_NAME"
        fi
        ;;
    list)
        build_repo
        if [ "$ALL_CONFIGS" -eq 1 ]; then
            for cfg in redbear-kde redbear-live redbear-full redbear-minimal redbear-wayland; do
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
            for cfg in redbear-kde redbear-live redbear-full redbear-minimal redbear-wayland; do
                fetch_for_config "$cfg" 2>/dev/null || {
                    echo "    WARNING: failed to fetch for $cfg (some recipes may not exist)"
                    echo ""
                }
            done
            echo ""
            echo "==> All sources fetched. Summary:"
            show_status ""
        else
            fetch_for_config "$CONFIG_NAME"
            echo ""
            show_status "$CONFIG_NAME"
        fi
        ;;
esac
