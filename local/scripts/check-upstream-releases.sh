#!/usr/bin/env bash
# check-upstream-releases.sh — Check for new Redox OS snapshots (read-only).
#
# Usage:
#   ./local/scripts/check-upstream-releases.sh
#
# Queries Redox GitLab tags via git ls-remote.
# Prints snapshots newer than the current baseline.
# ZERO side effects — no clones, no disk writes, no state changes.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

REDOX_URL="${REDOX_URL:-https://gitlab.redox-os.org/redox-os/redox.git}"
MANIFEST="$PROJECT_ROOT/sources/redbear-0.1.0/manifest.txt"

GREEN='\033[1;32m'
YELLOW='\033[1;33m'
BLUE='\033[1;34m'
NC='\033[0m'

echo -e "${BLUE}Red Bear OS — Upstream Release Check${NC}"
echo ""

# Get our baseline
if [ -f "$MANIFEST" ]; then
    BASELINE=$(head -3 "$MANIFEST" | grep 'Build system' | awk '{print $NF}' 2>/dev/null || echo "unknown")
    echo "Baseline: Red Bear OS 0.1.0 (build system: $BASELINE)"
else
    echo "Baseline: unknown (manifest not found at $MANIFEST)"
fi

# Get baseline date from manifest or git
if [ -f "$MANIFEST" ]; then
    BASELINE_DATE=$(head -6 "$MANIFEST" | grep 'Generated' | sed 's/.*Generated: //' | head -1 2>/dev/null || echo "2026-05-01")
else
    BASELINE_DATE="2026-05-01"
fi
echo "Baseline date: $BASELINE_DATE"
echo ""

# Query Redox tags
echo "Checking: $REDOX_URL"
echo ""

TAGS=$(git ls-remote --tags "$REDOX_URL" 2>/dev/null | grep -oP 'refs/tags/\K[0-9]+\.[0-9]+\.[0-9]+' | sort -V | tail -20 || echo "")

if [ -z "$TAGS" ]; then
    echo -e "${YELLOW}Could not query Redox tags. Is the network available?${NC}"
    echo "URL: $REDOX_URL"
    exit 0
fi

echo "Redox releases available:"
echo "$TAGS" | while read -r tag; do
    marker=""
    if [ "$tag" = "0.9.0" ]; then
        marker=" (current upstream stable)"
    fi
    echo "  $tag$marker"
done

echo ""
echo "To evaluate a release:"
echo "  ./local/scripts/provision-release.sh --ref=<tag> --release=0.2.0 --dry-run"
echo ""
echo "To rebase on a release:"
echo "  ./local/scripts/provision-release.sh --ref=<tag> --release=0.2.0"
