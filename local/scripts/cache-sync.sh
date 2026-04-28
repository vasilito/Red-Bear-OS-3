#!/usr/bin/env bash
# Red Bear OS — Git-Tracked Build Cache
# Automatically syncs recipe stage.pkgar files to local/cache/pkgar/
# and commits them to git so the cache survives make clean AND git clone.
#
# The cache is organized as: local/cache/pkgar/{pkgname}/stage.pkgar
# This keeps individual files small enough for git (<100MB each).
#
# Usage:
#   ./local/scripts/cache-sync.sh              # Sync all built packages to cache
#   ./local/scripts/cache-sync.sh --commit     # Sync + git commit
#   ./local/scripts/cache-sync.sh --restore    # Restore cache to recipe targets
#   ./local/scripts/cache-sync.sh --status     # Show cache vs build state

set -euo pipefail
cd "$(dirname "$0")/../.."

CACHE_ROOT="local/cache/pkgar"
mkdir -p "${CACHE_ROOT}"

MODE="${1:-}"

if [ "$MODE" = "--status" ]; then
    echo "=== Red Bear Cache Status ==="
    cached=0; stale=0; missing=0
    for pkgar in "${CACHE_ROOT}"/*/stage.pkgar; do
        [ -f "$pkgar" ] || continue
        pkg=$(basename "$(dirname "$pkgar")")
        recipe_dir=$(find recipes -maxdepth 3 -name "$pkg" -type d 2>/dev/null | head -1)
        if [ -z "$recipe_dir" ]; then
            echo "  ORPHAN $pkg (no recipe)"
            stale=$((stale + 1))
            continue
        fi
        target="${recipe_dir}/target/x86_64-unknown-redox/stage.pkgar"
        if [ -f "$target" ]; then
            if [ "$pkgar" -nt "$target" ]; then
                echo "  STALE  $pkg (cache newer than build)"
                stale=$((stale + 1))
            else
                echo "  SYNCED $pkg"
                cached=$((cached + 1))
            fi
        else
            echo "  CACHED $pkg (no build)"
            cached=$((cached + 1))
        fi
    done
    # Check built but not cached
    while IFS= read -r target; do
        pkg=$(echo "$target" | sed 's|recipes/[^/]*/[^/]*/||; s|/target/.*||')
        if [ ! -f "${CACHE_ROOT}/${pkg}/stage.pkgar" ]; then
            size=$(stat -c%s "$target" 2>/dev/null || echo 0)
            echo "  UNCACHED $pkg ($(numfmt --to=iec $size 2>/dev/null || echo ${size}B))"
            missing=$((missing + 1))
        fi
    done < <(find recipes -name "stage.pkgar" -path "*/target/x86_64-unknown-redox/*" 2>/dev/null | head -100)
    echo ""
    echo "Synced: $cached  Stale: $stale  Uncached: $missing"
    exit 0
fi

if [ "$MODE" = "--restore" ]; then
    echo "=== Restoring Cache to Recipes ==="
    # Restore signing keys first (pkgar signatures depend on them)
    if [ -d "${CACHE_ROOT}/../keys" ]; then
        mkdir -p build
        cp -f "${CACHE_ROOT}/../keys/id_ed25519"* build/ 2>/dev/null && echo "Keys restored"
    fi
    count=0
    for pkgar in "${CACHE_ROOT}"/*/stage.pkgar; do
        [ -f "$pkgar" ] || continue
        pkg=$(basename "$(dirname "$pkgar")")
        recipe_dir=$(find recipes -maxdepth 4 -name "$pkg" -type d 2>/dev/null | head -1)
        if [ -z "$recipe_dir" ]; then continue; fi
        target="${recipe_dir}/target/x86_64-unknown-redox/stage.pkgar"
        if [ ! -f "$target" ] || [ "$pkgar" -nt "$target" ]; then
            mkdir -p "$(dirname "$target")"
            cp "$pkgar" "$target"
            count=$((count + 1))
        fi
    done
    echo "Restored $count packages"
    exit 0
fi

# Default: --sync mode
echo "=== Syncing Build Cache ==="
synced=0
while IFS= read -r target; do
    pkg_path="${target%/target/x86_64-unknown-redox/stage.pkgar}"
    # Extract package name: recipes/{category}/{name}/target/... → {name}
    pkg=$(basename "$pkg_path")
    [ -z "$pkg" ] && continue
    
    cache_dir="${CACHE_ROOT}/${pkg}"
    cache_file="${cache_dir}/stage.pkgar"
    
    # Only copy if build is newer
    if [ -f "$cache_file" ] && [ ! "$target" -nt "$cache_file" ]; then
        continue
    fi
    
    mkdir -p "$cache_dir"
    cp "$target" "$cache_file"
    
    # Also save auto_deps
    deps_file="${pkg_path}/target/x86_64-unknown-redox/auto_deps.toml"
    if [ -f "$deps_file" ]; then
        cp "$deps_file" "${cache_dir}/auto_deps.toml"
    fi
    
    synced=$((synced + 1))
done < <(find recipes -name "stage.pkgar" -path "*/target/x86_64-unknown-redox/*" 2>/dev/null)

echo "Synced $synced packages to ${CACHE_ROOT}/"

if [ "$MODE" = "--commit" ] && [ $synced -gt 0 ]; then
    echo ""
    echo "=== Committing Cache ==="
    git add "${CACHE_ROOT}/"
    
    # Only commit if there are staged changes
    if git diff --cached --quiet; then
        echo "No cache changes to commit"
    else
        commit_msg="cache: $(date +%Y-%m-%d) — ${synced} packages"
        git commit -m "$commit_msg"
        echo "Committed: $commit_msg"
        echo "To push: git push"
    fi
fi
