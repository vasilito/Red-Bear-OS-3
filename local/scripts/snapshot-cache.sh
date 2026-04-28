#!/usr/bin/env bash
# Red Bear OS — Build Cache Snapshot
# Saves all recipe stage.pkgar files to a compressed archive in local/cache/
# This makes the build system resilient to make clean / make distclean.
#
# Usage:
#   ./local/scripts/snapshot-cache.sh              # Full snapshot
#   ./local/scripts/snapshot-cache.sh --essential   # Only essential packages
#   ./local/scripts/snapshot-cache.sh --list        # List available snapshots

set -euo pipefail

CACHE_DIR="${CACHE_DIR:-local/cache}"
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
SNAPSHOT_NAME="rbos-cache-${TIMESTAMP}"
SNAPSHOT_DIR="${CACHE_DIR}/${SNAPSHOT_NAME}"

# Essential packages for boot — small enough to commit to git
ESSENTIAL_PKGS=(
    "kernel" "relibc" "base" "base-initfs" "bootloader" "init"
    "ion" "installer" "redoxfs"
)

mkdir -p "${CACHE_DIR}"

if [ "${1:-}" = "--list" ]; then
    echo "Available cache snapshots:"
    ls -1t "${CACHE_DIR}"/rbos-cache-* 2>/dev/null | while read d; do
        size=$(du -sh "$d" 2>/dev/null | cut -f1)
        echo "  $(basename "$d")  $size"
    done
    exit 0
fi

MODE="${1:---full}"

echo "=== Red Bear OS Cache Snapshot ==="
echo "Mode: ${MODE}"
echo "Snapshot: ${SNAPSHOT_DIR}"

mkdir -p "${SNAPSHOT_DIR}"

count=0
total_size=0

if [ "$MODE" = "--essential" ]; then
    PKGS=("${ESSENTIAL_PKGS[@]}")
else
    # Find ALL recipes with stage.pkgar
    PKGS=()
    while IFS= read -r pkgar; do
        recipe_dir=$(dirname "$(dirname "$(dirname "$pkgar")")")
        pkg_name=$(basename "$recipe_dir")
        PKGS+=("$pkg_name:$recipe_dir")
    done < <(find recipes -name "stage.pkgar" -path "*/target/x86_64-unknown-redox/*" 2>/dev/null)
fi

for entry in "${PKGS[@]}"; do
    if [ "$MODE" = "--essential" ]; then
        pkg_name="$entry"
        # Find the recipe directory
        recipe_dir=$(find recipes -maxdepth 3 -name "$pkg_name" -type d 2>/dev/null | head -1)
        if [ -z "$recipe_dir" ]; then
            echo "  SKIP $pkg_name: recipe not found"
            continue
        fi
    else
        pkg_name="${entry%%:*}"
        recipe_dir="${entry#*:}"
    fi

    stage_dir="${recipe_dir}/target/x86_64-unknown-redox"
    stage_pkgar="${stage_dir}/stage.pkgar"

    if [ ! -f "$stage_pkgar" ]; then
        if [ "$MODE" != "--essential" ]; then
            echo "  SKIP $pkg_name: no stage.pkgar"
        fi
        continue
    fi

    # Create package dir in snapshot
    pkg_snapshot="${SNAPSHOT_DIR}/${pkg_name}"
    mkdir -p "$(dirname "$pkg_snapshot")"

    # Copy stage.pkgar and auto_deps.toml
    cp "$stage_pkgar" "${pkg_snapshot}.pkgar"
    if [ -f "${stage_dir}/auto_deps.toml" ]; then
        cp "${stage_dir}/auto_deps.toml" "${pkg_snapshot}.deps"
    fi

    size=$(stat -c%s "$stage_pkgar" 2>/dev/null || echo 0)
    total_size=$((total_size + size))
    count=$((count + 1))
    echo "  SAVED $pkg_name ($(numfmt --to=iec $size 2>/dev/null || echo ${size}B))"
done

# Create manifest
cat > "${SNAPSHOT_DIR}/manifest.toml" << EOF
[snapshot]
name = "${SNAPSHOT_NAME}"
timestamp = "${TIMESTAMP}"
mode = "${MODE}"
packages = ${count}
total_size = ${total_size}
EOF

echo ""
echo "=== Snapshot Complete ==="
echo "Packages: $count"
echo "Total size: $(numfmt --to=iec $total_size 2>/dev/null || echo ${total_size}B)"
echo "Location: ${SNAPSHOT_DIR}"
echo ""
echo "To restore: ./local/scripts/restore-cache.sh ${SNAPSHOT_NAME}"

# Clean up old snapshots (keep last 5)
snapshots=($(ls -1dt "${CACHE_DIR}"/rbos-cache-* 2>/dev/null))
if [ ${#snapshots[@]} -gt 5 ]; then
    for old in "${snapshots[@]:5}"; do
        echo "Cleaning old snapshot: $(basename "$old")"
        rm -rf "$old"
    done
fi
