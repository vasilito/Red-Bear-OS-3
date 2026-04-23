#!/usr/bin/env bash
# sync-upstream.sh — Update from upstream Redox and reapply Red Bear OS overlays.
#
# Usage:
#   ./local/scripts/sync-upstream.sh              # Rebase onto upstream master
#   ./local/scripts/sync-upstream.sh --dry-run    # Preview what would change
#   ./local/scripts/sync-upstream.sh --no-merge   # Only fetch + check for conflicts
#
# Strategy: git rebase (preserves Red Bear OS commits, replays on new upstream).
# Fallback: if rebase fails, patches in local/patches/build-system/ can be
#           applied from scratch via: ./local/scripts/apply-patches.sh --force
#
# IMPORTANT: upstream WIP recipes are not treated as durable shipping inputs by Red Bear.
# After upstream sync, Red Bear-owned WIP work still needs to come from local/recipes/ and
# local/patches/, not from trust in recipes/wip/ alone.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
UPSTREAM_URL="${UPSTREAM_URL:-https://github.com/redox-os/redox.git}"
UPSTREAM_REMOTE="upstream-redox"
UPSTREAM_BRANCH="${UPSTREAM_BRANCH:-master}"
DRY_RUN=0
NO_MERGE=0
FORCE=0

usage() {
    echo "Usage: $0 [--dry-run] [--no-merge] [--force]"
    echo "  --dry-run    Show what would happen without making changes"
    echo "  --no-merge   Only fetch and check patch conflicts"
    echo "  --force      Skip safety checks (uncommitted local/ changes)"
}

for arg in "$@"; do
    case "$arg" in
        --dry-run)   DRY_RUN=1 ;;
        --no-merge)  NO_MERGE=1 ;;
        --force)     FORCE=1 ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            echo "Unknown argument: $arg"
            usage >&2
            exit 1
            ;;
    esac
done

cd "$REPO_ROOT"

# ── 1. Ensure upstream remote ───────────────────────────────────────
if ! git remote get-url "$UPSTREAM_REMOTE" &>/dev/null; then
    echo "==> Adding upstream remote: $UPSTREAM_URL"
    [ "$DRY_RUN" = "0" ] && git remote add "$UPSTREAM_REMOTE" "$UPSTREAM_URL"
fi

echo "==> Fetching $UPSTREAM_REMOTE/$UPSTREAM_BRANCH..."
[ "$DRY_RUN" = "0" ] && git fetch "$UPSTREAM_REMOTE" "$UPSTREAM_BRANCH"

UPSTREAM_REF="${UPSTREAM_REMOTE}/${UPSTREAM_BRANCH}"

# ── 2. Check patch conflicts with upstream changes ──────────────────
MERGE_BASE=$(git merge-base HEAD "$UPSTREAM_REF" 2>/dev/null || echo "")
if [ -n "$MERGE_BASE" ]; then
    CHANGED_FILES=$(git diff --name-only "$MERGE_BASE" "$UPSTREAM_REF" 2>/dev/null || true)
    CHANGE_COUNT=$(echo "$CHANGED_FILES" | grep -c . 2>/dev/null || echo "0")
    echo "    $CHANGE_COUNT files changed upstream since common ancestor"

    if [ -n "$CHANGED_FILES" ] && [ -d local/patches ]; then
        echo ""
        echo "==> Checking patch conflict risks..."
        for patch_file in local/patches/build-system/[0-9]*.patch; do
            [ -f "$patch_file" ] || continue
            PATCH_NAME=$(basename "$patch_file")
            PATCHED_FILES=$(grep '^--- a/' "$patch_file" 2>/dev/null | sed 's|^--- a/||' | sort -u || true)
            for pf in $PATCHED_FILES; do
                if echo "$CHANGED_FILES" | grep -q "$pf" 2>/dev/null; then
                    echo "    ⚠ CONFLICT RISK: $PATCH_NAME modifies $pf (also changed upstream)"
                fi
            done
        done

        for patch_dir in local/patches/kernel local/patches/base; do
            [ -f "$patch_dir/redox.patch" ] || continue
            echo "    ℹ $patch_dir/redox.patch — check manually if kernel/base changed upstream"
        done
    fi
else
    echo "    WARNING: Could not find common ancestor with upstream"
fi

# ── 3. Summary ─────────────────────────────────────────────────────
AHEAD=$(git rev-list --count "$UPSTREAM_REF..HEAD" 2>/dev/null || echo "?")
BEHIND=$(git rev-list --count "HEAD..$UPSTREAM_REF" 2>/dev/null || echo "?")
echo ""
echo "=== Sync Summary ==="
echo "Upstream:  $UPSTREAM_REF"
echo "Local:     HEAD ($(git rev-parse --short HEAD))"
echo "Ahead:     $AHEAD Red Bear OS commits"
echo "Behind:    $BEHIND upstream commits"

if [ "$NO_MERGE" = 1 ]; then
    echo ""
    echo "To merge manually:"
    echo "  git rebase $UPSTREAM_REF"
    exit 0
fi

if [ "$DRY_RUN" = "1" ]; then
    echo ""
    echo "    [dry-run] Would rebase onto $UPSTREAM_REF"
    exit 0
fi

# ── 3.5. Check for uncommitted local/ changes ──────────────────────
if [ "$NO_MERGE" = "0" ] && [ "$DRY_RUN" = "0" ]; then
    LOCAL_CHANGES=""
    LOCAL_UNTRACKED=""
    if [ -d "local" ]; then
        LOCAL_CHANGES=$(cd local && git diff --name-only HEAD 2>/dev/null || true)
        LOCAL_UNTRACKED=$(cd local && git ls-files --others --exclude-standard 2>/dev/null || true)
    fi

    # Also check for uncommitted changes to tracked local/ files from repo root
    ROOT_LOCAL_CHANGES=$(git diff --name-only HEAD -- local/ 2>/dev/null || true)

    if [ -n "$LOCAL_CHANGES" ] || [ -n "$LOCAL_UNTRACKED" ] || [ -n "$ROOT_LOCAL_CHANGES" ]; then
        echo ""
        echo "!! WARNING: Uncommitted changes detected in local/"
        if [ -n "$ROOT_LOCAL_CHANGES" ]; then
            echo "   Modified files:"
            echo "$ROOT_LOCAL_CHANGES" | head -10 | while read -r f; do echo "     $f"; done
            TOTAL=$(echo "$ROOT_LOCAL_CHANGES" | grep -c .)
            [ "$TOTAL" -gt 10 ] && echo "     ... and $((TOTAL - 10)) more"
        fi
        if [ -n "$LOCAL_UNTRACKED" ]; then
            echo "   Untracked files (NOT protected by stash):"
            echo "$LOCAL_UNTRACKED" | head -5 | while read -r f; do echo "     $f"; done
            TOTAL=$(echo "$LOCAL_UNTRACKED" | grep -c .)
            [ "$TOTAL" -gt 5 ] && echo "     ... and $((TOTAL - 5)) more"
        fi
        echo ""
        echo "   git stash does NOT protect untracked files."
        echo "   Commit your local/ changes before syncing, or use --force to proceed anyway."

        if [ "$FORCE" = "0" ]; then
            echo ""
            echo "   ABORT: Uncommitted local/ changes detected."
            echo "   Commit your changes first: git add local/ && git commit -m 'WIP'"
            echo "   Or use --force if you understand the risks (untracked files will be LOST)."
            exit 1
        else
            # --force with untracked files requires explicit confirmation
            if [ -n "$LOCAL_UNTRACKED" ]; then
                echo ""
                echo "!! DANGER: --force with untracked files will DELETE them permanently. !!"
                echo "   git stash does NOT protect untracked files."
                echo "   Untracked files found:"
                echo "$LOCAL_UNTRACKED" | head -10 | while read -r f; do echo "     $f"; done
                TOTAL=$(echo "$LOCAL_UNTRACKED" | grep -c .)
                [ "$TOTAL" -gt 10 ] && echo "     ... and $((TOTAL - 10)) more"
                echo ""
                read -p "   Type 'YES_DELETE' to confirm destruction of untracked local/ files: " CONFIRM
                if [ "$CONFIRM" != "YES_DELETE" ]; then
                    echo "   Aborted. Your untracked files are safe."
                    exit 1
                fi
                echo "   Proceeding with --force — untracked files WILL be deleted..."
            else
                echo "   --force specified, proceeding (tracked changes will be stashed)..."
            fi
        fi
    fi
fi

# ── 4. Stash uncommitted changes ────────────────────────────────────
STASHED=0
if ! git diff --quiet 2>/dev/null || ! git diff --cached --quiet 2>/dev/null; then
    echo "==> Stashing uncommitted changes..."
    git stash push -u -m "redbear-sync-$(date +%Y%m%d-%H%M%S)"
    STASHED=1
fi

PREV_HEAD=$(git rev-parse HEAD)

# ── 4.5. Verify overlay integrity before rebase ────────────────────
echo "==> Verifying Red Bear overlay integrity before rebase..."
BROKEN_SYMLINKS=""
while IFS= read -r link; do
    if [ ! -e "$link" ]; then
        BROKEN_SYMLINKS="$BROKEN_SYMLINKS
  $link -> $(readlink "$link")"
    fi
done < <(find recipes -maxdepth 3 -type l 2>/dev/null)

if [ -n "$BROKEN_SYMLINKS" ]; then
    echo "!! WARNING: Broken symlinks detected in recipes/:"
    echo "$BROKEN_SYMLINKS" | head -20
    TOTAL=$(echo "$BROKEN_SYMLINKS" | grep -c .)
    [ "$TOTAL" -gt 20 ] && echo "  ... and $((TOTAL - 20)) more"
    echo ""
    echo "   These symlinks may break further during rebase."
    echo "   Run ./local/scripts/apply-patches.sh after rebase to recreate them."
fi

# Check that key local/patches exist
for patch_file in local/patches/kernel/redox.patch local/patches/base/redox.patch local/patches/relibc/redox.patch; do
    if [ ! -f "$patch_file" ]; then
        echo "!! CRITICAL: Missing patch file: $patch_file"
        echo "   Cannot recover from rebase failure without this patch."
        if [ "$FORCE" = "0" ]; then
            exit 1
        fi
    fi
done

# ── 5. Rebase ───────────────────────────────────────────────────────
echo ""
echo "==> Rebasing Red Bear OS commits onto $UPSTREAM_REF..."
echo "    (this replays our $AHEAD commits on top of updated upstream)"

if git rebase "$UPSTREAM_REF"; then
    echo ""
    echo "==> Rebase successful."
else
    echo ""
    echo "!! Rebase conflict. Options:"
    echo "   1. Resolve conflicts:  edit files, git add, git rebase --continue"
    echo "   2. Abort:              git rebase --abort"
    echo "   3. Nuclear option (DESTRUCTIVE — loses uncommitted work):"
    echo "      git rebase --abort"
    echo "      git reset --hard $UPSTREAM_REF"
    echo "      ./local/scripts/apply-patches.sh --force"
    echo ""
    echo "   Patches for recovery: local/patches/build-system/"
    echo "   Previous HEAD:        $PREV_HEAD"
    echo ""
    echo "   IMPORTANT: Before using the nuclear option, ensure all local/ changes"
    echo "   are committed. The nuclear option does NOT preserve uncommitted work."
    echo "   To recover to previous state: git reset --hard $PREV_HEAD"
    exit 1
fi

# ── 6. Restore stash ────────────────────────────────────────────────
if [ "$STASHED" = 1 ]; then
    echo "==> Restoring stashed changes..."
    if git stash pop; then
        echo "    Stash restored successfully."
    else
        echo "!! Stash pop had conflicts."
        echo "   Your changes are preserved in the stash."
        echo "   Options:"
        echo "     1. Resolve conflicts in the working tree"
        echo "     2. git checkout --theirs . && git stash drop"
        echo "     3. git reset --hard && git stash pop  (try again on clean tree)"
        echo "   List stashes: git stash list"
    fi
fi

# ── 7. Verify symlinks ─────────────────────────────────────────────
echo "==> Verifying recipe patch symlinks..."
if [ -f local/scripts/apply-patches.sh ]; then
    bash local/scripts/apply-patches.sh
else
    echo "    apply-patches.sh not found — verify symlinks manually"
    ls -la recipes/core/kernel/redox.patch recipes/core/base/redox.patch
fi

if [ -x local/scripts/verify-overlay-integrity.sh ]; then
    echo "==> Verifying overlay integrity..."
    local/scripts/verify-overlay-integrity.sh --repair
fi

echo ""
echo "==> Sync complete."
echo "    Previous HEAD: $PREV_HEAD"
echo "    New HEAD:      $(git rev-parse HEAD)"
echo ""
echo "Next: make all CONFIG_NAME=redbear-full"
