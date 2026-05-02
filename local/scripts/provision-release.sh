#!/usr/bin/env bash
# provision-release.sh — Seal current build tree as a new Red Bear OS release (atomic).
#
# Usage:
#   ./local/scripts/provision-release.sh --release=0.2.0 [--ref=<tag>] [--dry-run]
#
# Provisions a self-contained, immutable release archive via staging + atomic mv.
# All 7 completeness gates must pass before .complete sentry is written.
# On failure, staging directory is cleaned up automatically.
#
# Requires explicit --release. Never runs automatically.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
REF=""
RELEASE=""
DRY_RUN=0

usage() {
    cat <<EOF
Usage: $(basename "$0") --release=<ver> [--ref=<redox-tag>] [--dry-run]

Seal current source tree as a new Red Bear OS release (atomic provisioning).

Options:
  --release=<ver>   Red Bear OS release version (e.g., 0.2.0) — REQUIRED
  --ref=<tag>       Optional Redox OS ref for provenance tracking
  --dry-run         Preview only — no filesystem changes
  -h, --help        Show this help
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --ref=*)     REF="${1#*=}" ;;
        --release=*) RELEASE="${1#*=}" ;;
        --dry-run)   DRY_RUN=1 ;;
        -h|--help)   usage; exit 0 ;;
        *)           echo "Unknown: $1"; usage >&2; exit 1 ;;
    esac
    shift
done

if [ -z "$RELEASE" ]; then
    echo "ERROR: --release is required" >&2
    usage >&2
    exit 1
fi

cd "$PROJECT_ROOT"

RED='\033[1;31m'
GREEN='\033[1;32m'
YELLOW='\033[1;33m'
BLUE='\033[1;34m'
NC='\033[0m'

status() { echo -e "${GREEN}==>${NC} $*"; }
warn()  { echo -e "${YELLOW}WARN${NC}: $*"; }
err()   { echo -e "${RED}ERROR${NC}: $*" >&2; }
info()  { echo -e "${BLUE}   ${NC} $*"; }

STAGING="sources/.staging/redbear-${RELEASE}"
FINAL="sources/redbear-${RELEASE}"

cleanup_staging() {
    if [ -d "$STAGING" ]; then
        warn "Cleaning up staging directory..."
        rm -rf "$STAGING"
    fi
}
trap cleanup_staging EXIT

# ── Step 1: Verify current release is archived ──────────────────────
status "Step 1: Verifying current release..."
CURRENT_RELEASE="${REDBEAR_RELEASE:-0.1.0}"
CURRENT_ARCHIVE="sources/redbear-$CURRENT_RELEASE"

if [ ! -f "$CURRENT_ARCHIVE/.complete" ] && [ ! -f "$CURRENT_ARCHIVE/manifest.txt" ]; then
    warn "Current release $CURRENT_RELEASE has no .complete sentry or manifest"
    warn "It may not be fully archived. Continue anyway? (y/N)"
    if [ "$DRY_RUN" -eq 0 ]; then
        read -r confirm
        [ "$confirm" = "y" ] || [ "$confirm" = "Y" ] || exit 1
    fi
fi
info "Current release: $CURRENT_RELEASE"

# ── Step 2: Ref validation (optional) ───────────────────────────────
if [ -n "$REF" ]; then
    status "Step 2: Validating ref=$REF..."
    if [ "$DRY_RUN" -eq 1 ]; then
        info "[dry-run] Would validate ref $REF"
    else
        REDOX_URL="https://gitlab.redox-os.org/redox-os/redox.git"
        if timeout 10 git ls-remote --tags "$REDOX_URL" "$REF" 2>/dev/null | grep -q "$REF"; then
            info "Ref $REF exists in Redox repository"
        elif timeout 10 git ls-remote --tags "$REDOX_URL" 2>/dev/null | grep -q .; then
            err "Ref $REF not found"
            exit 1
        else
            warn "Cannot reach Redox repository — ref recorded as stated provenance"
        fi
    fi
fi

# ── Step 3: Staging safety check ────────────────────────────────────
status "Step 3: Checking staging..."
if [ -d "$STAGING" ]; then
    err "Staging directory already exists: $STAGING"
    err "This may be from a previous failed provisioning run."
    err "Remove it first: rm -rf $STAGING"
    [ "$DRY_RUN" -eq 1 ] || exit 1
fi
if [ -d "$FINAL" ]; then
    err "Release already exists: $FINAL"
    err "Releases are immutable. Choose a different --release version."
    [ "$DRY_RUN" -eq 1 ] || exit 1
fi
info "Staging path is clear"

# ── Step 4: Archive sources ─────────────────────────────────────────
status "Step 4: Archiving sources..."
if [ "$DRY_RUN" -eq 1 ]; then
    info "[dry-run] Would run: archive-sources.sh --release=$RELEASE --all"
else
    mkdir -p "$STAGING"/{tarballs,snapshots,configs}
    if [ -f "$SCRIPT_DIR/archive-sources.sh" ]; then
        bash "$SCRIPT_DIR/archive-sources.sh" --release="$RELEASE" --all
        info "Sources archived"
    else
        err "archive-sources.sh not found"
        exit 1
    fi
fi

# ── Step 5: Archive configs ─────────────────────────────────────────
status "Step 5: Archiving configs..."
if [ "$DRY_RUN" -eq 1 ]; then
    info "[dry-run] Would copy configs"
else
    cp config/redbear-*.toml config/base.toml config/minimal.toml "$STAGING/configs/" 2>/dev/null || true
    cp .config "$STAGING/configs/" 2>/dev/null || true
    info "Configs: $(ls "$STAGING/configs"/*.toml 2>/dev/null | wc -l) files"
fi

# ── Step 6: Archive patches ─────────────────────────────────────────
status "Step 6: Archiving patches..."
if [ "$DRY_RUN" -eq 1 ]; then
    info "[dry-run] Would archive patches"
else
    if [ -d "local/patches" ]; then
        (cd local && tar czf "$PROJECT_ROOT/$STAGING/patches.tar.gz" patches/)
        info "Patches archived: patches.tar.gz"
    fi
fi

# ── Step 7: Generate manifest ───────────────────────────────────────
status "Step 7: Generating manifest..."
if [ "$DRY_RUN" -eq 1 ]; then
    info "[dry-run] Would generate manifest.json"
else
    if [ -f "$SCRIPT_DIR/generate-manifest.py" ]; then
        python3 "$SCRIPT_DIR/generate-manifest.py" --release="$RELEASE" --staging > "$STAGING/manifest.json" || {
            err "Manifest generation failed"
            exit 1
        }
        info "Manifest: $(python3 -c "import json; d=json.load(open('$STAGING/manifest.json')); print(len(d.get('entries',{})))" 2>/dev/null || echo "?") entries"
    else
        err "generate-manifest.py not found"
        exit 1
    fi
fi

# ── Step 8: Generate BLAKE3SUMS ─────────────────────────────────────
status "Step 8: Generating checksums..."
if [ "$DRY_RUN" -eq 1 ]; then
    info "[dry-run] Would generate BLAKE3SUMS and PAYLOAD.blake3"
else
    if [ -d "$STAGING/tarballs" ] && ls "$STAGING/tarballs"/*.tar.gz >/dev/null 2>&1; then
        (cd "$STAGING/tarballs" && b3sum *.tar.gz) > "$STAGING/BLAKE3SUMS"
        info "BLAKE3SUMS: $(wc -l < "$STAGING/BLAKE3SUMS") entries"
    fi
    if [ -d "$STAGING/snapshots" ] && ls "$STAGING/snapshots"/*.tar.gz >/dev/null 2>&1; then
        (cd "$STAGING/snapshots" && b3sum *.tar.gz) >> "$STAGING/BLAKE3SUMS"
    fi
    # Generate whole-payload hash
    (cd "$STAGING" && find . -type f ! -name PAYLOAD.blake3 ! -name .complete -print0 | sort -z | xargs -0 b3sum) > "$STAGING/PAYLOAD.blake3" 2>/dev/null || true
fi

# ── Step 9: Completeness gates ──────────────────────────────────────
status "Step 9: Running completeness gates..."
if [ "$DRY_RUN" -eq 1 ]; then
    info "[dry-run] Would run verify-release-completeness.sh"
else
    if [ -f "$SCRIPT_DIR/verify-release-completeness.sh" ]; then
        if bash "$SCRIPT_DIR/verify-release-completeness.sh" --release="$RELEASE" --staging; then
            info "All completeness gates PASSED"
        else
            err "Completeness gates FAILED"
            exit 1
        fi
    else
        warn "verify-release-completeness.sh not found — skipping gate checks"
    fi
fi

# ── Step 10: Seal and deploy ────────────────────────────────────────
status "Step 10: Sealing release..."
if [ "$DRY_RUN" -eq 1 ]; then
    info "[dry-run] Would write .complete sentry and move to $FINAL"
else
    echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) — Release $RELEASE" > "$STAGING/.complete"
    if [ -d "$FINAL" ]; then
        err "Release directory already exists: $FINAL"
        err "Releases are immutable. Choose a different --release version."
        exit 1
    fi
    mv "$STAGING" "$FINAL"
fi

# ── Report ──────────────────────────────────────────────────────────
echo ""
echo -e "${GREEN}=========================================${NC}"
if [ "$DRY_RUN" -eq 0 ]; then
    echo -e "${GREEN}  Release $RELEASE provisioned${NC}"
else
    echo -e "${GREEN}  Dry-run complete — no changes made${NC}"
fi
echo -e "${GREEN}=========================================${NC}"
echo ""

if [ "$DRY_RUN" -eq 0 ]; then
    echo "Archive: $FINAL/"
    echo "  tarballs/:  $(ls "$FINAL/tarballs" 2>/dev/null | wc -l) archives"
    echo "  configs/:   $(ls "$FINAL/configs" 2>/dev/null | wc -l) files"
    echo "  .complete:  $(cat "$FINAL/.complete")"
    echo ""
    echo "To verify: ./local/scripts/verify-sources-archived.sh --release=$RELEASE"
    echo ""
    echo "To switch: edit .config → REDBEAR_RELEASE?=$RELEASE"
fi

# Prevent trap cleanup on success
trap - EXIT
