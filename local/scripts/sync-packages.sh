#!/usr/bin/env bash
# Red Bear OS — Package Repository Sync
# Copies all built stage.pkgar files to Packages/ directory.
# This is the canonical binary package repository for Red Bear OS.
#
# Usage:
#   ./local/scripts/sync-packages.sh           Sync all built packages
#   ./local/scripts/sync-packages.sh --verify   Verify package integrity
set -euo pipefail
cd "$(dirname "$0")/../.."

PKG_DIR="Packages"
mkdir -p "${PKG_DIR}"

if [ "${1:-}" = "--verify" ]; then
    echo "=== Package Integrity Check ==="
    ok=0; bad=0
    for pkgar in "${PKG_DIR}"/*.pkgar; do
        [ -f "$pkgar" ] || continue
        pkg=$(basename "$pkgar" .pkgar)
        if [ -s "$pkgar" ]; then
            ok=$((ok+1))
        else
            echo "  EMPTY: $pkg"
            bad=$((bad+1))
        fi
    done
    echo "Valid: $ok, Empty: $bad"
    exit $bad
fi

echo "=== Syncing Packages ==="
count=0
while IFS= read -r pkgar; do
    pkg_path=$(dirname "$(dirname "$(dirname "$pkgar")")")
    pkg=$(basename "$pkg_path")
    dest="${PKG_DIR}/${pkg}.pkgar"
    if [ ! -f "$dest" ] || [ "$pkgar" -nt "$dest" ]; then
        cp "$pkgar" "$dest" && count=$((count+1))
    fi
done < <(find recipes local/recipes -name "stage.pkgar" -path "*/target/x86_64-unknown-redox/*" 2>/dev/null)

echo "Synced $count packages to ${PKG_DIR}/"
echo "Total: $(ls ${PKG_DIR}/*.pkgar 2>/dev/null | wc -l) pkgar files"
