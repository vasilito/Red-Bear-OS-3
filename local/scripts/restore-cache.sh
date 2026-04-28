#!/usr/bin/env bash
# Red Bear OS — Build Cache Restore
# Restores recipe stage.pkgar files from a cache snapshot.
# Automatically restores the latest snapshot if no argument given.
#
# Usage:
#   ./local/scripts/restore-cache.sh                    # Auto: latest snapshot
#   ./local/scripts/restore-cache.sh rbos-cache-XXXXXX  # Specific snapshot
#   ./local/scripts/restore-cache.sh --verify           # Verify cache integrity

set -euo pipefail

CACHE_DIR="${CACHE_DIR:-local/cache}"

if [ "${1:-}" = "--verify" ]; then
    echo "=== Cache Integrity Check ==="
    latest=$(ls -1t "${CACHE_DIR}"/rbos-cache-* 2>/dev/null | head -1)
    if [ -z "$latest" ]; then
        echo "No cache snapshots found in ${CACHE_DIR}"
        exit 1
    fi
    SNAPSHOT="$latest"
    echo "Checking: $(basename "$SNAPSHOT")"
    
    ok=0; missing=0
    for pkgar in "${SNAPSHOT}"/*.pkgar; do
        [ -f "$pkgar" ] || continue
        pkg=$(basename "$pkgar" .pkgar)
        recipe_dir=$(find recipes -maxdepth 3 -name "$pkg" -type d 2>/dev/null | head -1)
        if [ -z "$recipe_dir" ]; then
            echo "  WARN $pkg: recipe not found"
            missing=$((missing + 1))
        else
            ok=$((ok + 1))
        fi
    done
    echo "Valid: $ok, Missing recipes: $missing"
    exit $missing
fi

SNAPSHOT="${1:-}"
if [ -z "$SNAPSHOT" ]; then
    SNAPSHOT=$(ls -1t "${CACHE_DIR}"/rbos-cache-* 2>/dev/null | head -1)
    if [ -z "$SNAPSHOT" ]; then
        echo "No cache snapshots found in ${CACHE_DIR}"
        echo "Run ./local/scripts/snapshot-cache.sh first"
        exit 1
    fi
else
    SNAPSHOT="${CACHE_DIR}/${SNAPSHOT}"
fi

if [ ! -d "$SNAPSHOT" ]; then
    echo "Snapshot not found: $SNAPSHOT"
    echo "Available:"
    ls -1t "${CACHE_DIR}"/rbos-cache-* 2>/dev/null | while read d; do
        echo "  $(basename "$d")"
    done
    exit 1
fi

echo "=== Red Bear OS Cache Restore ==="
echo "Snapshot: $(basename "$SNAPSHOT")"

if [ -f "${SNAPSHOT}/manifest.toml" ]; then
    echo "Manifest:"
    grep -E "packages|total_size|timestamp" "${SNAPSHOT}/manifest.toml" | sed 's/^/  /'
fi

count=0
for pkgar in "${SNAPSHOT}"/*.pkgar; do
    [ -f "$pkgar" ] || continue
    pkg=$(basename "$pkgar" .pkgar)
    recipe_dir=$(find recipes -maxdepth 3 -name "$pkg" -type d 2>/dev/null | head -1)
    
    if [ -z "$recipe_dir" ]; then
        echo "  SKIP $pkg: recipe not found in tree"
        continue
    fi

    stage_dir="${recipe_dir}/target/x86_64-unknown-redox"
    mkdir -p "$stage_dir"

    # Only restore if stage.pkgar is missing or older
    if [ ! -f "${stage_dir}/stage.pkgar" ] || [ "$pkgar" -nt "${stage_dir}/stage.pkgar" ]; then
        cp "$pkgar" "${stage_dir}/stage.pkgar"
        echo "  RESTORED $pkg"
        count=$((count + 1))
    else
        echo "  SKIP $pkg: already up to date"
    fi

    # Restore deps if present
    deps_file="${SNAPSHOT}/${pkg}.deps"
    if [ -f "$deps_file" ] && [ ! -f "${stage_dir}/auto_deps.toml" ]; then
        cp "$deps_file" "${stage_dir}/auto_deps.toml"
    fi
done

echo ""
echo "=== Restore Complete ==="
echo "Restored: $count packages"
echo ""
echo "Ready to build: make all CONFIG_NAME=redbear-full"
