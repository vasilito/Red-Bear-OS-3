#!/bin/bash
# Validate all generated init service files from Red Bear OS config TOMLs.
# Checks for:
#   1. Valid TOML syntax
#   2. Required [unit] section in .service/.target files
#   3. Required [service] section with cmd field in .service files
#   4. Non-empty data
#
# Usage: ./local/scripts/validate-service-files.sh [config_dir ...]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RB_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CONFIG_DIR="${1:-$RB_ROOT/config}"

PASS=0
FAIL=0
ERRORS=""

# Use a Python helper to extract [[files]] entries from TOML
extract_service_files() {
    local toml_file="$1"
    python3 -c "
import sys
try:
    import tomllib
except ImportError:
    import tomli as tomllib

try:
    with open('${toml_file}', 'rb') as f:
        data = tomllib.load(f)
except Exception as e:
    print(f'PARSE_ERROR: {e}', file=sys.stderr)
    sys.exit(1)

for entry in data.get('files', []):
    path = entry.get('path', '')
    content = entry.get('data', '')
    if path.endswith('.service') or path.endswith('.target'):
        safe_content = content.replace('\\\\n', '\\\\\\\\n').replace(chr(10), '\\\\n')
        print(f'{path}\t{safe_content}')
" 2>&1
}

for toml_file in "$CONFIG_DIR"/redbear-*.toml; do
    [ -f "$toml_file" ] || continue

    BASENAME="$(basename "$toml_file")"

    while IFS=$'\t' read -r file_path file_data_escaped; do
        [ -n "$file_path" ] || continue

        # Handle TOML parse errors from Python extractor
        if [[ "$file_path" == PARSE_ERROR:* ]]; then
            ERRORS="${ERRORS}FAIL: $BASENAME: TOML parse error: ${file_path#PARSE_ERROR:}\n"
            FAIL=$((FAIL + 1))
            continue
        fi

        # Decode newlines
        file_data="$(echo "$file_data_escaped" | sed 's/\\n/\n/g')"

        # Only check .service and .target files
        case "$file_path" in
            *.service|*.target) ;;
            *) continue ;;
        esac

        # Check 1: No empty data (would cause parse errors)
        if [ -z "$file_data" ]; then
            ERRORS="${ERRORS}FAIL: $BASENAME → $file_path: empty data\n"
            FAIL=$((FAIL + 1))
            continue
        fi

        # Check 2: Must have [unit] section
        if ! echo "$file_data" | grep -q '\[unit\]'; then
            ERRORS="${ERRORS}FAIL: $BASENAME → $file_path: missing [unit] section\n"
            FAIL=$((FAIL + 1))
            continue
        fi

        # Check 3: .service files must have [service] section with cmd field
        case "$file_path" in
            *.service)
                if ! echo "$file_data" | grep -q '\[service\]'; then
                    ERRORS="${ERRORS}FAIL: $BASENAME → $file_path: missing [service] section\n"
                    FAIL=$((FAIL + 1))
                    continue
                fi

                if ! echo "$file_data" | grep -q 'cmd[[:space:]]*='; then
                    ERRORS="${ERRORS}FAIL: $BASENAME → $file_path: [service] missing cmd field\n"
                    FAIL=$((FAIL + 1))
                    continue
                fi
                ;;
        esac

        PASS=$((PASS + 1))
    done < <(extract_service_files "$toml_file")
done

echo ""
echo "=== Service File Validation ==="
echo "PASS: $PASS"
echo "FAIL: $FAIL"

if [ -n "$ERRORS" ]; then
    echo ""
    echo "Errors:"
    echo -e "$ERRORS"
    exit 1
fi

echo "All service files valid."
exit 0
