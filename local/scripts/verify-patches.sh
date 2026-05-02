#!/usr/bin/env bash
# verify-patches.sh — Check which Red Bear patches need rebasing against current source trees.
#
# Usage:
#   ./local/scripts/verify-patches.sh [--component=base|kernel|relibc] [--all]
#
# Dry-runs all patches against their target source trees and reports:
#   OK    — patch applies cleanly
#   REV   — reversed/already applied (upstream absorbed)
#   CONFLICT — genuine conflict, needs rebasing
#
# Exit code: number of CONFLICT patches

set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
COMPONENT="${1:-all}"
MODE="${2:-}"

cd "$PROJECT_ROOT"

GREEN='\033[1;32m'
RED='\033[1;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

ok=0
rev=0
conflict=0

check_patches() {
    local patch_dir="$1"
    local target_dir="$2"
    local label="$3"
    
    [ -d "$patch_dir" ] || return
    [ -d "$target_dir" ] || { echo "  ${RED}SKIP${NC} $label: target not found"; return; }
    
    echo "=== $label ==="
    for patch in "$patch_dir"/*.patch; do
        [ -f "$patch" ] || continue
        local name=$(basename "$patch")
        local result=$(patch -p1 --dry-run -d "$target_dir" < "$patch" 2>&1) || true
        
        if echo "$result" | grep -q 'Reversed\|previously applied'; then
            echo "  ${YELLOW}REV${NC} $name (upstream absorbed)"
            rev=$((rev + 1))
        elif echo "$result" | grep -q 'FAILED\|hunks\? FAILED'; then
            echo "  ${RED}CONFLICT${NC} $name"
            conflict=$((conflict + 1))
        else
            echo "  ${GREEN}OK${NC} $name"
            ok=$((ok + 1))
        fi
    done
}

case "$COMPONENT" in
    base|all)
        check_patches "local/patches/base" "recipes/core/base/source" "base"
        ;;
esac
case "$COMPONENT" in
    kernel|all)
        check_patches "local/patches/kernel" "recipes/core/kernel/source" "kernel"
        # Fallback: kernel source may be nested from archive extraction
        if [ ! -d "recipes/core/kernel/source" ] && [ -d "recipes/core/kernel/kernel/source" ]; then
            check_patches "local/patches/kernel" "recipes/core/kernel/kernel/source" "kernel"
        fi
        ;;
esac
case "$COMPONENT" in
    relibc|all)
        check_patches "local/patches/relibc" "recipes/core/relibc/source" "relibc"
        ;;
esac

echo ""
echo "========================================="
echo "  OK:       $ok"
echo "  Reversed: $rev (upstream absorbed)"
echo "  Conflict: $conflict (needs rebase)"
echo "========================================="

exit $conflict
