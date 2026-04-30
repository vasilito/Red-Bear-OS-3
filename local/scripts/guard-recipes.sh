#!/usr/bin/env bash
# Red Bear OS — Recipe Durability Guard
#
# PROBLEM: The build system ("cargo cook", "make distclean", "sync-upstream.sh")
# can delete or overwrite recipe.toml files in recipes/*/. This script
# ensures ALL custom recipes are backed in local/recipes/ and symlinked
# into the recipes/ tree properly.
#
# USAGE:
#   ./local/scripts/guard-recipes.sh              # Verify all recipes
#   ./local/scripts/guard-recipes.sh --fix        # Fix broken symlinks
#   ./local/scripts/guard-recipes.sh --save-all   # Save ALL recipe.toml files to local/
#   ./local/scripts/guard-recipes.sh --restore    # Restore all symlinks from local/
#
# RECOMMENDED: Run --fix before every build, --restore after every sync-upstream.

set -euo pipefail

REDOX_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
LOCAL_RECIPES="$REDOX_ROOT/local/recipes"
MAIN_RECIPES="$REDOX_ROOT/recipes"

MODE="${1:-}"

if [ -z "$MODE" ]; then
    echo "Usage: $0 [--fix|--save-all|--restore|--verify]"
    exit 1
fi

fix_symlink() {
    local local_path="$1"
    local target_recipe="$2"

    local rel_path="${local_path#$LOCAL_RECIPES/}"
    local recipe_name="$(dirname "$rel_path")"
    # Remove leading category/name/recipe.toml to get just category/name
    local package_dir="$(dirname "$rel_path")"

    local main_recipe="$MAIN_RECIPES/$package_dir/recipe.toml"

    if [ ! -d "$(dirname "$main_recipe")" ]; then
        echo "  SKIP: $main_recipe — target dir does not exist"
        return
    fi

    if [ -L "$main_recipe" ]; then
        local existing_target="$(readlink "$main_recipe")"
        if [ "$existing_target" == "$target_recipe" ]; then
            # echo "  OK: $main_recipe"
            return
        fi
    fi

    if [ "$MODE" == "--fix" ]; then
        rm -f "$main_recipe"
        ln -sf "$target_recipe" "$main_recipe"
        echo "  FIXED: $main_recipe → $target_recipe"
    else
        echo "  BROKEN: $main_recipe (would fix with --fix)"
    fi
}

echo "=== Red Bear OS Recipe Durability Guard ==="
echo "Local recipes: $LOCAL_RECIPES"
echo "Main recipes:  $MAIN_RECIPES"
echo "Mode: $MODE"
echo ""

case "$MODE" in
    --verify|--fix)
        echo "Checking all local recipes..."
        BROKEN=0
        FIXED=0
        find "$LOCAL_RECIPES" -name "recipe.toml" -type f | while read -r local_recipe; do
            rel="${local_recipe#$LOCAL_RECIPES/}"
            # Compute relative symlink path
            depth=$(echo "$rel" | tr -cd '/' | wc -c)
            up=""
            for ((i=0; i<depth; i++)); do up="../$up"; done
            target="$up$local_recipe"
            main="$MAIN_RECIPES/${rel%/*}/recipe.toml"

            if [ ! -L "$main" ] || [ "$(readlink "$main")" != "$target" ]; then
                if [ "$MODE" == "--fix" ]; then
                    mkdir -p "$(dirname "$main")"
                    rm -f "$main"
                    ln -sf "$target" "$main"
                    echo "  FIXED: $main"
                    FIXED=$((FIXED+1))
                else
                    echo "  BROKEN: $main → should link to $target"
                    BROKEN=$((BROKEN+1))
                fi
            fi
        done
        echo ""
        if [ "$MODE" == "--fix" ]; then
            echo "Symlinks fixed. Run after every sync-upstream."
        else
            echo "$BROKEN broken symlink(s). Run with --fix to repair."
        fi
        ;;

    --save-all)
        echo "Saving ALL recipe.toml files from recipes/ to local/..."
        SAVED=0
        find "$MAIN_RECIPES" -name "recipe.toml" -type f -not -path "*/source/*" | while read -r recipe; do
            rel="${recipe#$MAIN_RECIPES/}"
            local_dest="$LOCAL_RECIPES/$rel"

            # Skip if already in local/
            if [ "$recipe" == "$local_dest" ]; then
                continue
            fi

            if [ -f "$local_dest" ]; then
                continue  # Already saved
            fi

            if [ -L "$recipe" ]; then
                # It's a symlink — already backed
                continue
            fi

            mkdir -p "$(dirname "$local_dest")"
            cp "$recipe" "$local_dest"
            echo "  SAVED: $rel"
            SAVED=$((SAVED+1))
        done
        echo ""
        echo "$SAVED recipe(s) saved to local/recipes/."
        echo "Now run: $0 --fix to replace with symlinks."
        ;;

    --restore)
        echo "Restoring all symlinks from local/recipes/..."
        find "$LOCAL_RECIPES" -name "recipe.toml" -type f | while read -r local_recipe; do
            rel="${local_recipe#$LOCAL_RECIPES/}"
            depth=$(echo "$rel" | tr -cd '/' | wc -c)
            up=""
            for ((i=0; i<depth; i++)); do up="../$up"; done
            target="$up$local_recipe"
            main="$MAIN_RECIPES/${rel%/*}/recipe.toml"

            mkdir -p "$(dirname "$main")"
            rm -f "$main"
            ln -sf "$target" "$main"
            echo "  RESTORED: $main"
        done
        echo "All symlinks restored."
        ;;

    *)
        echo "Unknown mode: $MODE"
        echo "Use: --verify, --fix, --save-all, or --restore"
        exit 1
        ;;
esac
