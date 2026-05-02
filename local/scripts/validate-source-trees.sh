#!/usr/bin/env bash
# validate-source-trees.sh — Check all required source trees exist before building.
# Delegates to validate-source-trees.py for config parsing and validation.
set -eo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CONFIG="${1:-redbear-full}"
cd "$PROJECT_ROOT"
exec python3 "$SCRIPT_DIR/validate-source-trees.py" "$CONFIG"
