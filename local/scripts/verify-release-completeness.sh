#!/usr/bin/env bash
# verify-release-completeness.sh — Run 7 mechanical completeness gates.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
RELEASE=""
USE_STAGING=0
FAIL_COUNT=0

declare -A ENTRY_PRESENT=()
declare -A ENTRY_FIELDS=()
declare -A CONFIG_VISITED=()
declare -A CONFIG_PACKAGES=()
declare -A RECIPE_CACHE=()
declare -A CLOSURE_RECIPE_KEYS=()
declare -a CONFIG_ORDER=()

usage() {
    cat <<EOF
Usage: $(basename "$0") --release=<ver> [--staging]

Run the 7 mechanical completeness gates for a Red Bear release directory.

Options:
  --release=<ver>   Release version (for example 0.2.0)
  --staging         Verify sources/.staging/redbear-<ver>
  -h, --help        Show this help
EOF
}

pass_gate() {
    printf 'PASS: %s — %s\n' "$1" "$2"
}

fail_gate() {
    FAIL_COUNT=$((FAIL_COUNT + 1))
    printf 'FAIL: %s — %s\n' "$1" "$2" >&2
}

trim() {
    local value="$1"
    value="${value#"${value%%[![:space:]]*}"}"
    value="${value%"${value##*[![:space:]]}"}"
    printf '%s' "$value"
}

json_unquote() {
    local value="$1"
    if [[ "$value" == '"'*'"' ]]; then
        value="${value:1:${#value}-2}"
    fi
    printf '%s' "$value"
}

json_tokenize() {
    local json_file="$1"

    awk '
    BEGIN {
        in_string = 0
        escape = 0
        token = ""
    }
    {
        line = $0 "\n"
        for (i = 1; i <= length(line); i++) {
            c = substr(line, i, 1)
            if (in_string) {
                token = token c
                if (escape) {
                    escape = 0
                    continue
                }
                if (c == "\\") {
                    escape = 1
                    continue
                }
                if (c == "\"") {
                    print token
                    token = ""
                    in_string = 0
                }
                continue
            }

            if (c ~ /[[:space:]]/) {
                continue
            }
            if (c == "\"") {
                in_string = 1
                escape = 0
                token = "\""
                continue
            }
            if (c ~ /[\{\}\[\]:,]/) {
                print c
                continue
            }

            token = c
            while (i + 1 <= length(line)) {
                next_c = substr(line, i + 1, 1)
                if (next_c ~ /[[:space:]\{\}\[\]:,]/) {
                    break
                }
                i++
                token = token next_c
            }
            print token
            token = ""
        }
    }
    END {
        if (in_string) {
            exit 1
        }
    }
    ' "$json_file"
}

declare -a TOKENS=()
TOKEN_INDEX=0
CURRENT_TOKEN=''

current_token() {
    if [ "$TOKEN_INDEX" -ge "${#TOKENS[@]}" ]; then
        CURRENT_TOKEN=''
        return
    fi
    CURRENT_TOKEN="${TOKENS[$TOKEN_INDEX]}"
}

consume_token() {
    current_token
    TOKEN_INDEX=$((TOKEN_INDEX + 1))
}

expect_token() {
    local expected="$1"
    consume_token
    if [ "$CURRENT_TOKEN" != "$expected" ]; then
        printf 'JSON parse error: expected %s but found %s\n' "$expected" "$CURRENT_TOKEN" >&2
        exit 1
    fi
}

skip_json_value() {
    current_token

    case "$CURRENT_TOKEN" in
        '{')
            consume_token >/dev/null
            current_token
            while [ "$CURRENT_TOKEN" != '}' ]; do
                consume_token >/dev/null
                expect_token ':'
                skip_json_value
                current_token
                if [ "$CURRENT_TOKEN" = ',' ]; then
                    consume_token >/dev/null
                fi
                current_token
            done
            expect_token '}'
            ;;
        '[')
            consume_token >/dev/null
            current_token
            while [ "$CURRENT_TOKEN" != ']' ]; do
                skip_json_value
                current_token
                if [ "$CURRENT_TOKEN" = ',' ]; then
                    consume_token >/dev/null
                fi
                current_token
            done
            expect_token ']'
            ;;
        *)
            consume_token >/dev/null
            ;;
    esac
}

store_entry_scalar() {
    local entry="$1"
    local key="$2"
    local raw="$3"
    local value="$raw"

    if [[ "$raw" == '"'*'"' ]]; then
        value="$(json_unquote "$raw")"
    fi

    ENTRY_FIELDS["$entry:$key"]="$value"
}

parse_entry_object() {
    local entry="$1"
    local prefix="$2"
    local field raw

    expect_token '{'
    current_token
    while [ "$CURRENT_TOKEN" != '}' ]; do
        consume_token
        field="$(json_unquote "$CURRENT_TOKEN")"
        expect_token ':'

        current_token
        case "$CURRENT_TOKEN" in
            '{')
                parse_entry_object "$entry" "${prefix}${field}."
                ;;
            '[')
                skip_json_value
                ;;
            *)
                consume_token
                raw="$CURRENT_TOKEN"
                store_entry_scalar "$entry" "${prefix}${field}" "$raw"
                ;;
        esac

        current_token
        if [ "$CURRENT_TOKEN" = ',' ]; then
            consume_token >/dev/null
        fi
        current_token
    done
    expect_token '}'
}

parse_entries_object() {
    local entry_name

    expect_token '{'
    current_token
    while [ "$CURRENT_TOKEN" != '}' ]; do
        consume_token
        entry_name="$(json_unquote "$CURRENT_TOKEN")"
        ENTRY_PRESENT["$entry_name"]=1
        expect_token ':'
        parse_entry_object "$entry_name" ""

        current_token
        if [ "$CURRENT_TOKEN" = ',' ]; then
            consume_token >/dev/null
        fi
        current_token
    done
    expect_token '}'
}

parse_manifest_json() {
    local manifest_json="$1"
    local key

    if ! mapfile -t TOKENS < <(json_tokenize "$manifest_json"); then
        printf 'failed to tokenize manifest JSON: %s\n' "$manifest_json" >&2
        exit 1
    fi

    TOKEN_INDEX=0
    expect_token '{'
    current_token
    while [ "$CURRENT_TOKEN" != '}' ]; do
        consume_token
        key="$(json_unquote "$CURRENT_TOKEN")"
        expect_token ':'
        if [ "$key" = 'entries' ]; then
            parse_entries_object
        else
            skip_json_value
        fi
        current_token
        if [ "$CURRENT_TOKEN" = ',' ]; then
            consume_token >/dev/null
        fi
        current_token
    done
    expect_token '}'
}

entry_field() {
    printf '%s' "${ENTRY_FIELDS["$1:$2"]-}"
}

first_nonempty_field() {
    local entry="$1"
    shift
    local field value

    for field in "$@"; do
        value="$(entry_field "$entry" "$field")"
        if [ -n "$value" ] && [ "$value" != 'null' ] && [ "$value" != 'false' ]; then
            printf '%s' "$value"
            return
        fi
    done

    printf ''
}

resolve_config_path() {
    local base_file="$1"
    local include_rel="$2"
    local base_dir resolved_dir

    base_dir="$(cd "$(dirname "$base_file")" && pwd)"
    resolved_dir="$(cd "$base_dir/$(dirname "$include_rel")" 2>/dev/null && pwd)" || return 1
    printf '%s/%s' "$resolved_dir" "$(basename "$include_rel")"
}

collect_config_closure() {
    local config_file="$1"
    local rel_path section line trimmed include_text include_rel matched_include package_name package_value resolved

    if [ ! -f "$config_file" ]; then
        printf 'missing config file in repo: %s\n' "$config_file" >&2
        exit 1
    fi

    rel_path="${config_file#"$PROJECT_ROOT/config/"}"
    if [ "${CONFIG_VISITED["$rel_path"]-}" = '1' ]; then
        return
    fi

    CONFIG_VISITED["$rel_path"]=1
    CONFIG_ORDER+=("$rel_path")
    section=''

    while IFS= read -r line || [ -n "$line" ]; do
        trimmed="$(trim "$line")"

        if [[ "$trimmed" =~ ^include[[:space:]]*=[[:space:]]*\[(.*)\][[:space:]]*$ ]]; then
            include_text="${BASH_REMATCH[1]}"
            while [[ "$include_text" =~ \"([^\"]+)\" ]]; do
                matched_include="${BASH_REMATCH[0]}"
                include_rel="${BASH_REMATCH[1]}"
                resolved="$(resolve_config_path "$config_file" "$include_rel")" || {
                    printf 'cannot resolve include %s from %s\n' "$include_rel" "$config_file" >&2
                    exit 1
                }
                collect_config_closure "$resolved"
                include_text=${include_text#*${matched_include}}
            done
            continue
        fi

        trimmed="${trimmed%%#*}"
        trimmed="$(trim "$trimmed")"
        [ -z "$trimmed" ] && continue

        if [[ "$trimmed" =~ ^\[(.+)\]$ ]]; then
            section="${BASH_REMATCH[1]}"
            continue
        fi

        if [ "$section" = 'packages' ] && [[ "$trimmed" =~ ^([A-Za-z0-9._+-]+)[[:space:]]*=[[:space:]]*(.+)$ ]]; then
            package_name="${BASH_REMATCH[1]}"
            package_value="$(trim "${BASH_REMATCH[2]}")"
            if [[ "$package_value" =~ ^\"ignore\"$ ]]; then
                CONFIG_PACKAGES["$package_name"]='ignore'
            else
                CONFIG_PACKAGES["$package_name"]='present'
            fi
        fi
    done < "$config_file"
}

resolve_recipe_key() {
    local package_name="$1"
    local recipe_file match rel_path recipe_key
    local -a matches=()

    if [ -n "${RECIPE_CACHE["$package_name"]-}" ]; then
        printf '%s' "${RECIPE_CACHE["$package_name"]}"
        return
    fi

    while IFS= read -r recipe_file; do
        [ -n "$recipe_file" ] || continue
        matches+=("$recipe_file")
    done < <(find -L "$PROJECT_ROOT/recipes" -path "*/${package_name}/recipe.toml" -not -path '*/source/*' -print 2>/dev/null | sort)

    if [ "${#matches[@]}" -eq 1 ]; then
        match="${matches[0]}"
        rel_path="${match#"$PROJECT_ROOT/recipes/"}"
        recipe_key="${rel_path%/recipe.toml}"
        RECIPE_CACHE["$package_name"]="$recipe_key"
        printf '%s' "$recipe_key"
        return
    fi

    if [ "${#matches[@]}" -eq 0 ]; then
        RECIPE_CACHE["$package_name"]=''
        printf ''
        return
    fi

    printf '__AMBIGUOUS__:'
    printf '%s' "${matches[0]#"$PROJECT_ROOT/recipes/"}"
    local index
    for ((index = 1; index < ${#matches[@]}; index++)); do
        printf ',%s' "${matches[$index]#"$PROJECT_ROOT/recipes/"}"
    done
}

verify_archive_file() {
    local entry="$1"
    local kind="$2"
    local directory="$3"
    local file_name hash_value archive_path computed_hash

    file_name="$(first_nonempty_field "$entry" "$kind" "$kind.path")"
    [ -n "$file_name" ] || return 0

    case "$kind" in
        archive)  hash_value="$(first_nonempty_field "$entry" 'archive.blake3' 'blake3')" ;;
        snapshot) hash_value="$(first_nonempty_field "$entry" 'snapshot.blake3' 'blake3')" ;;
        *)        hash_value="$(first_nonempty_field "$entry" 'blake3')" ;;
    esac

    archive_path="$directory/$file_name"
    if [ ! -f "$archive_path" ]; then
        printf '  - %s: missing %s file %s\n' "$entry" "$kind" "$archive_path" >&2
        return 1
    fi
    if [ -z "$hash_value" ]; then
        printf '  - %s: missing BLAKE3 for %s file %s\n' "$entry" "$kind" "$file_name" >&2
        return 1
    fi

    computed_hash="$(b3sum "$archive_path" | awk '{print $1}')"
    if [ "$computed_hash" != "$hash_value" ]; then
        printf '  - %s: checksum mismatch for %s (expected %s, got %s)\n' "$entry" "$file_name" "$hash_value" "$computed_hash" >&2
        return 1
    fi

    return 0
}

run_gate_closure_completeness() {
    local package_name recipe_key
    local closure_ok=0
    local -a closure_missing=() closure_ambiguous=()

    for package_name in "${CONFIG_PACKAGES_SORTED[@]}"; do
        recipe_key="$(resolve_recipe_key "$package_name")"
        if [ -z "$recipe_key" ]; then
            closure_missing+=("$package_name (no recipe path under recipes/)")
            continue
        fi
        if [[ "$recipe_key" == __AMBIGUOUS__:* ]]; then
            closure_ambiguous+=("$package_name (${recipe_key#__AMBIGUOUS__:})")
            continue
        fi

        CLOSURE_RECIPE_KEYS["$package_name"]="$recipe_key"
        if [ -n "${ENTRY_PRESENT["$recipe_key"]-}" ]; then
            closure_ok=$((closure_ok + 1))
        else
            closure_missing+=("$package_name ($recipe_key)")
        fi
    done

    if [ "${#closure_missing[@]}" -eq 0 ] && [ "${#closure_ambiguous[@]}" -eq 0 ]; then
        pass_gate '1/7 closure completeness' "$closure_ok closure packages all have manifest entries"
        return
    fi

    if [ "${#closure_missing[@]}" -gt 0 ]; then
        printf '  Missing closure entries:\n' >&2
        printf '    %s\n' "${closure_missing[@]}" >&2
    fi
    if [ "${#closure_ambiguous[@]}" -gt 0 ]; then
        printf '  Ambiguous recipe matches:\n' >&2
        printf '    %s\n' "${closure_ambiguous[@]}" >&2
    fi
    fail_gate '1/7 closure completeness' 'one or more closure packages could not be matched to a manifest entry'
}

run_gate_git_provenance() {
    local package_name recipe_key entry_type entry_rev
    local git_checked=0
    local -a blank_rev=()

    for package_name in "${CONFIG_PACKAGES_SORTED[@]}"; do
        recipe_key="${CLOSURE_RECIPE_KEYS["$package_name"]-}"
        [ -n "$recipe_key" ] || continue
        [ -n "${ENTRY_PRESENT["$recipe_key"]-}" ] || continue
        entry_type="$(first_nonempty_field "$recipe_key" 'type')"
        if [ "$entry_type" = 'git' ]; then
            git_checked=$((git_checked + 1))
            entry_rev="$(trim "$(first_nonempty_field "$recipe_key" 'rev')")"
            if [ -z "$entry_rev" ]; then
                blank_rev+=("$recipe_key")
            fi
        fi
    done

    if [ "${#blank_rev[@]}" -eq 0 ]; then
        pass_gate '2/7 git provenance' "$git_checked closure git entries have non-blank rev values"
        return
    fi

    printf '  Blank rev entries:\n' >&2
    printf '    %s\n' "${blank_rev[@]}" >&2
    fail_gate '2/7 git provenance' 'one or more closure git entries have a blank rev'
}

run_gate_archive_coverage() {
    local entry_name archive_name snapshot_name target_name meta_value
    local total_entries=0
    local -a coverage_missing=()

    while IFS= read -r entry_name; do
        [ -n "$entry_name" ] || continue
        total_entries=$((total_entries + 1))
        archive_name="$(first_nonempty_field "$entry_name" 'archive' 'archive.path')"
        snapshot_name="$(first_nonempty_field "$entry_name" 'snapshot' 'snapshot.path')"
        target_name="$(first_nonempty_field "$entry_name" 'target' 'same_as.target')"
        meta_value="$(first_nonempty_field "$entry_name" 'meta' 'meta.kind')"
        if [ -z "$archive_name" ] && [ -z "$snapshot_name" ] && [ -z "$target_name" ] && [ -z "$meta_value" ]; then
            coverage_missing+=("$entry_name")
        fi
    done < <(printf '%s\n' "${!ENTRY_PRESENT[@]}" | sort)

    if [ "${#coverage_missing[@]}" -eq 0 ]; then
        pass_gate '3/7 archive coverage' "$total_entries manifest entries all have archive, snapshot, target, or meta resolution"
        return
    fi

    printf '  Entries without resolution path:\n' >&2
    printf '    %s\n' "${coverage_missing[@]}" >&2
    fail_gate '3/7 archive coverage' 'one or more manifest entries have no resolution path'
}

run_gate_archive_integrity() {
    local entry_name archive_name snapshot_name
    local archive_checks=0
    local -a integrity_failures=()

    while IFS= read -r entry_name; do
        [ -n "$entry_name" ] || continue
        archive_name="$(first_nonempty_field "$entry_name" 'archive' 'archive.path')"
        snapshot_name="$(first_nonempty_field "$entry_name" 'snapshot' 'snapshot.path')"

        if [ -n "$archive_name" ]; then
            archive_checks=$((archive_checks + 1))
            if ! verify_archive_file "$entry_name" archive "$RELEASE_DIR/tarballs"; then
                integrity_failures+=("$entry_name")
            fi
        fi
        if [ -n "$snapshot_name" ]; then
            archive_checks=$((archive_checks + 1))
            if ! verify_archive_file "$entry_name" snapshot "$RELEASE_DIR/snapshots"; then
                integrity_failures+=("$entry_name")
            fi
        fi
    done < <(printf '%s\n' "${!ENTRY_PRESENT[@]}" | sort)

    if [ "${#integrity_failures[@]}" -eq 0 ]; then
        pass_gate '4/7 archive integrity' "$archive_checks archive or snapshot payloads exist and match their BLAKE3 hashes"
        return
    fi

    fail_gate '4/7 archive integrity' 'one or more archive or snapshot payloads are missing or have hash mismatches'
}

run_gate_same_as_validation() {
    local entry_name entry_type target_name next_target next_type seen cursor
    local same_as_checked=0
    local -a same_as_missing=() same_as_cycles=()

    while IFS= read -r entry_name; do
        [ -n "$entry_name" ] || continue
        entry_type="$(first_nonempty_field "$entry_name" 'type')"
        target_name="$(first_nonempty_field "$entry_name" 'target' 'same_as.target')"

        if [ "$entry_type" != 'same_as' ] && [ -z "$(entry_field "$entry_name" 'same_as.target')" ] && [ -z "$target_name" ]; then
            continue
        fi

        same_as_checked=$((same_as_checked + 1))
        if [ -z "$target_name" ]; then
            same_as_missing+=("$entry_name (blank target)")
            continue
        fi
        if [ -z "${ENTRY_PRESENT["$target_name"]-}" ]; then
            same_as_missing+=("$entry_name -> $target_name")
            continue
        fi

        seen="|$entry_name|"
        cursor="$target_name"
        while :; do
            next_target="$(first_nonempty_field "$cursor" 'target' 'same_as.target')"
            next_type="$(first_nonempty_field "$cursor" 'type')"
            if [ "$next_type" != 'same_as' ] && [ -z "$(entry_field "$cursor" 'same_as.target')" ]; then
                break
            fi
            if [ -z "$next_target" ]; then
                same_as_missing+=("$cursor (blank target)")
                break
            fi
            if [[ "$seen" == *"|$next_target|"* ]]; then
                same_as_cycles+=("$entry_name -> $next_target")
                break
            fi
            if [ -z "${ENTRY_PRESENT["$next_target"]-}" ]; then
                same_as_missing+=("$cursor -> $next_target")
                break
            fi
            seen+="$cursor|"
            cursor="$next_target"
        done
    done < <(printf '%s\n' "${!ENTRY_PRESENT[@]}" | sort)

    if [ "${#same_as_missing[@]}" -eq 0 ] && [ "${#same_as_cycles[@]}" -eq 0 ]; then
        pass_gate '5/7 same_as validation' "$same_as_checked same_as links resolve cleanly without cycles"
        return
    fi

    if [ "${#same_as_missing[@]}" -gt 0 ]; then
        printf '  Missing same_as targets:\n' >&2
        printf '    %s\n' "${same_as_missing[@]}" >&2
    fi
    if [ "${#same_as_cycles[@]}" -gt 0 ]; then
        printf '  same_as cycles:\n' >&2
        printf '    %s\n' "${same_as_cycles[@]}" >&2
    fi
    fail_gate '5/7 same_as validation' 'same_as target resolution failed or contains a cycle'
}

run_gate_config_closure() {
    local config_rel
    local -a missing_configs=()

    for config_rel in "${CONFIG_ORDER[@]}"; do
        if [ -f "$RELEASE_CONFIG_DIR/$config_rel" ] || [ -f "$RELEASE_CONFIG_DIR/$(basename "$config_rel")" ]; then
            continue
        fi
        missing_configs+=("$config_rel")
    done

    if [ "${#missing_configs[@]}" -eq 0 ]; then
        pass_gate '6/7 config closure' "${#CONFIG_ORDER[@]} reachable config files are present in configs/"
        return
    fi

    printf '  Missing archived configs:\n' >&2
    printf '    %s\n' "${missing_configs[@]}" >&2
    fail_gate '6/7 config closure' 'one or more reachable config files are missing from configs/'
}

run_gate_dirty_tree() {
    local package_name recipe_key entry_type source_dir
    local git_dirty_checked=0
    local -a dirty_recipes=()

    for package_name in "${CONFIG_PACKAGES_SORTED[@]}"; do
        recipe_key="${CLOSURE_RECIPE_KEYS["$package_name"]-}"
        [ -n "$recipe_key" ] || continue
        [ -n "${ENTRY_PRESENT["$recipe_key"]-}" ] || continue
        entry_type="$(first_nonempty_field "$recipe_key" 'type')"
        if [ "$entry_type" != 'git' ]; then
            continue
        fi

        git_dirty_checked=$((git_dirty_checked + 1))
        source_dir="$PROJECT_ROOT/recipes/$recipe_key/source"
        if ! git -C "$source_dir" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
            dirty_recipes+=("$recipe_key (source is not a git worktree: $source_dir)")
            continue
        fi
        if ! git -C "$source_dir" diff --quiet; then
            dirty_recipes+=("$recipe_key")
        fi
    done

    if [ "${#dirty_recipes[@]}" -eq 0 ]; then
        pass_gate '7/7 dirty-tree check' "$git_dirty_checked closure git source trees are clean"
        return
    fi

    printf '  Dirty git source trees:\n' >&2
    printf '    %s\n' "${dirty_recipes[@]}" >&2
    fail_gate '7/7 dirty-tree check' 'one or more closure git source trees have uncommitted changes'
}

while [ $# -gt 0 ]; do
    case "$1" in
        --release=*) RELEASE="${1#*=}" ;;
        --staging)   USE_STAGING=1 ;;
        -h|--help)   usage; exit 0 ;;
        *)           printf 'Unknown argument: %s\n' "$1" >&2; usage >&2; exit 1 ;;
    esac
    shift
done

if [ -z "$RELEASE" ]; then
    printf 'ERROR: --release is required\n' >&2
    usage >&2
    exit 1
fi

if ! command -v b3sum >/dev/null 2>&1; then
    printf 'ERROR: b3sum is required\n' >&2
    exit 1
fi
if ! command -v git >/dev/null 2>&1; then
    printf 'ERROR: git is required\n' >&2
    exit 1
fi

if [ "$USE_STAGING" -eq 1 ]; then
    RELEASE_DIR="$PROJECT_ROOT/sources/.staging/redbear-$RELEASE"
else
    RELEASE_DIR="$PROJECT_ROOT/sources/redbear-$RELEASE"
fi

MANIFEST_JSON="$RELEASE_DIR/manifest.json"
RELEASE_CONFIG_DIR="$RELEASE_DIR/configs"
ROOT_CONFIG="$PROJECT_ROOT/config/redbear-full.toml"

if [ ! -d "$RELEASE_DIR" ]; then
    printf 'ERROR: release directory not found: %s\n' "$RELEASE_DIR" >&2
    exit 1
fi

collect_config_closure "$ROOT_CONFIG"

CONFIG_PACKAGES_SORTED=()
while IFS= read -r package_name; do
    [ -n "$package_name" ] || continue
    if [ "${CONFIG_PACKAGES["$package_name"]}" = 'present' ]; then
        CONFIG_PACKAGES_SORTED+=("$package_name")
    fi
done < <(printf '%s\n' "${!CONFIG_PACKAGES[@]}" | sort)

if [ ! -f "$MANIFEST_JSON" ]; then
    fail_gate '1/7 closure completeness' 'manifest.json is missing, so manifest-backed checks cannot run'
    fail_gate '2/7 git provenance' 'manifest.json is missing, so git provenance cannot be verified'
    fail_gate '3/7 archive coverage' 'manifest.json is missing, so resolution paths cannot be verified'
    fail_gate '4/7 archive integrity' 'manifest.json is missing, so archive hashes cannot be verified'
    fail_gate '5/7 same_as validation' 'manifest.json is missing, so same_as targets cannot be verified'
    run_gate_config_closure
    fail_gate '7/7 dirty-tree check' 'manifest.json is missing, so closure git source trees cannot be verified'
else
    parse_manifest_json "$MANIFEST_JSON"
    run_gate_closure_completeness
    run_gate_git_provenance
    run_gate_archive_coverage
    run_gate_archive_integrity
    run_gate_same_as_validation
    run_gate_config_closure
    run_gate_dirty_tree
fi

printf '\n'
if [ "$FAIL_COUNT" -eq 0 ]; then
    printf 'Release completeness PASSED for %s\n' "$RELEASE_DIR"
    exit 0
fi

printf 'Release completeness FAILED for %s (%d gate(s) failed)\n' "$RELEASE_DIR" "$FAIL_COUNT" >&2
exit 1
