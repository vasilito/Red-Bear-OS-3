#!/usr/bin/env bash
# sync-upstream.sh — RETIRED. Red Bear OS is now a release-based fork.
#
# This script no longer performs upstream synchronization.
# Red Bear OS sources are frozen at the current baseline (0.1.0).
# Sources are immutable — never auto-refreshed from upstream.
#
# To check for newer Redox OS snapshots:
#   ./local/scripts/check-upstream-releases.sh
#
# To provision a new release from a Redox ref:
#   ./local/scripts/provision-release.sh --ref=<redox-tag> --release=0.2.0
#
# To restore archived sources:
#   ./local/scripts/restore-sources.sh --release=0.1.0
#
# Documentation:
#   local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md

set -euo pipefail

GREEN='\033[1;32m'
BLUE='\033[1;34m'
NC='\033[0m'

echo ""
echo -e "${GREEN}sync-upstream.sh has been retired.${NC}"
echo ""
echo "Red Bear OS is now a release-based fork."
echo "Current baseline: 0.1.0 (f55acba68)"
echo "Sources are immutable — never auto-refreshed from upstream."
echo ""
echo -e "${BLUE}Available commands:${NC}"
echo "  check-upstream-releases.sh     See new Redox snapshots (read-only)"
echo "  provision-release.sh           Provision a new release"
echo "  restore-sources.sh             Restore sources from archives"
echo ""
exit 0
