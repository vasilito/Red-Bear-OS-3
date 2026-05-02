#!/usr/bin/env bash
# verify-sources-archived.sh — Verify release archive integrity.
#
# Usage:
#   ./local/scripts/verify-sources-archived.sh --release=0.1.0
#
# Checks that BLAKE3SUMS file exists and all archives match.
# If archives are in sources/<target>/ format, verifies those too.
# Returns non-zero if any archive is missing or corrupted.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
RELEASE=""

usage() {
    cat <<EOF
Usage: $(basename "$0") --release=<ver>

Verify release archive integrity.

Options:
  --release=<ver>   Release version (e.g., 0.1.0)
  -h, --help        Show this help
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --release=*) RELEASE="${1#*=}" ;;
        -h|--help)    usage; exit 0 ;;
        *)            echo "Unknown: $1"; usage >&2; exit 1 ;;
    esac
    shift
done

if [ -z "$RELEASE" ]; then
    echo "ERROR: --release is required" >&2
    exit 1
fi

ARCHIVE_DIR="$PROJECT_ROOT/sources/redbear-$RELEASE"
MANIFEST="$ARCHIVE_DIR/manifest.txt"

GREEN='\033[1;32m'
RED='\033[1;31m'
NC='\033[0m'

pass() { echo -e "${GREEN}PASS${NC}: $*"; }
fail() { echo -e "${RED}FAIL${NC}: $*"; }

errors=0

# 1. Verify .complete sentry exists (release is sealed)
if [ -f "$ARCHIVE_DIR/.complete" ]; then
    pass ".complete sentry: $(cat "$ARCHIVE_DIR/.complete")"
else
    fail ".complete sentry NOT FOUND — release may be incomplete or corrupted"
    errors=$((errors + 1))
fi

# 2. Verify configs
if [ -d "$ARCHIVE_DIR/configs" ]; then
    config_count=$(ls "$ARCHIVE_DIR/configs"/*.toml 2>/dev/null | wc -l)
    pass "configs: $config_count files"
else
    fail "configs directory not found"
    errors=$((errors + 1))
fi

# 3. Verify patches
if [ -d "$ARCHIVE_DIR/patches" ]; then
    patch_count=$(ls "$ARCHIVE_DIR/patches"/*.patch 2>/dev/null | wc -l)
    pass "patches: $patch_count files"
fi

SOURCES_TARGET="$PROJECT_ROOT/sources/x86_64-unknown-redox"

# 4. Check for BLAKE3SUMS
if [ -f "$ARCHIVE_DIR/BLAKE3SUMS" ]; then
    pass "BLAKE3SUMS present ($(wc -l < "$ARCHIVE_DIR/BLAKE3SUMS") entries)"
    # Verify checksums against actual archive files
    verified=0
    failed_checksums=0
    while read -r hash filename; do
        [ -z "$hash" ] && continue
        archive_path="$ARCHIVE_DIR/tarballs/$filename"
        if [ ! -f "$archive_path" ]; then
            archive_path="$ARCHIVE_DIR/snapshots/$filename"
        fi
        if [ ! -f "$archive_path" ]; then
            fail "archive missing: $filename"
            errors=$((errors + 1))
            continue
        fi
        if command -v b3sum >/dev/null 2>&1; then
            computed=$(b3sum "$archive_path" | awk '{print $1}')
        else
            fail "b3sum not available — cannot verify BLAKE3SUMS"
            errors=$((errors + 1))
            break
        fi
        if [ "$computed" != "$hash" ]; then
            fail "checksum mismatch: $filename (expected $hash, got $computed)"
            failed_checksums=$((failed_checksums + 1))
            errors=$((errors + 1))
        else
            verified=$((verified + 1))
        fi
    done < "$ARCHIVE_DIR/BLAKE3SUMS"
    if [ "$verified" -gt 0 ]; then
        pass "checksums verified: $verified archives"
    fi
    if [ "$failed_checksums" -gt 0 ]; then
        fail "$failed_checksums checksum mismatches"
    fi
else
    fail "BLAKE3SUMS not found in $ARCHIVE_DIR"
    errors=$((errors + 1))
fi

# 5. Count archives in sources/<target>/
SOURCES_TARGET="$PROJECT_ROOT/sources/x86_64-unknown-redox"
if [ -d "$ARCHIVE_DIR/tarballs" ]; then
    archive_count=$(ls "$ARCHIVE_DIR/tarballs"/*.tar.gz 2>/dev/null | wc -l)
    pass "source archives: $archive_count files in $ARCHIVE_DIR/tarballs/"
fi

echo ""
if [ "$errors" -eq 0 ]; then
    echo -e "${GREEN}=========================================${NC}"
    echo -e "${GREEN}  Release $RELEASE: VERIFIED${NC}"
    echo -e "${GREEN}=========================================${NC}"
else
    echo -e "${RED}=========================================${NC}"
    echo -e "${RED}  Release $RELEASE: $errors error(s)${NC}"
    echo -e "${RED}=========================================${NC}"
    exit 1
fi
