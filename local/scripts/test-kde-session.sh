#!/usr/bin/env bash
# test-kde-session.sh — bounded KDE session assembly proof inside a Red Bear runtime.

set -euo pipefail

usage() {
    cat <<'USAGE'
Usage: test-kde-session.sh

Launch a bounded virtual KDE session through redbear-session-launch, then verify
the session environment, compositor process, and optional Plasma helpers.
USAGE
}

for arg in "$@"; do
    case "$arg" in
        --help|-h|help)
            usage
            exit 0
            ;;
        *)
            printf 'ERROR: unsupported argument %s\n' "$arg" >&2
            usage >&2
            exit 1
            ;;
    esac
done

state_dir="${REDBEAR_KDE_SESSION_STATE_DIR:-/tmp/run/redbear-kde-session-test}"
runtime_dir="${REDBEAR_KDE_SESSION_RUNTIME_DIR:-/tmp/run/redbear-kde-session-runtime}"
display_name="${REDBEAR_KDE_SESSION_DISPLAY:-wayland-kde-test}"
session_pid=""

cleanup() {
    local status=$?

    trap - EXIT INT TERM

    if [[ -n "$session_pid" ]] && kill -0 "$session_pid" 2>/dev/null; then
        kill "$session_pid" 2>/dev/null || true
        wait "$session_pid" 2>/dev/null || true
    fi

    exit "$status"
}

trap cleanup EXIT INT TERM

require_binary() {
    local program="$1"
    if command -v "$program" >/dev/null 2>&1; then
        printf 'KDE_SESSION_BINARY_%s=ok\n' "$program"
    else
        printf 'KDE_SESSION_BINARY_%s=missing\n' "$program" >&2
        exit 1
    fi
}

wait_for_file() {
    local target="$1"
    local attempts="$2"
    local count=0

    while (( count < attempts )); do
        if [[ -e "$target" ]]; then
            return 0
        fi
        count=$((count + 1))
        sleep 1
    done

    return 1
}

wait_for_process_pattern() {
    local pattern="$1"
    local attempts="$2"
    local count=0

    while (( count < attempts )); do
        if ps | grep -Eq "$pattern"; then
            return 0
        fi
        count=$((count + 1))
        sleep 1
    done

    return 1
}

require_env_value() {
    local file="$1"
    local key="$2"
    local expected="$3"

    if grep -Eq "^${key}=${expected}$" "$file"; then
        printf 'KDE_SESSION_ENV_%s=ok\n' "$key"
    else
        printf 'KDE_SESSION_ENV_%s=unexpected\n' "$key" >&2
        exit 1
    fi
}

require_process_pattern() {
    local pattern="$1"
    local label="$2"

    if wait_for_process_pattern "$pattern" 15; then
        printf 'KDE_SESSION_PROCESS_%s=ok\n' "$label"
    else
        printf 'KDE_SESSION_PROCESS_%s=missing\n' "$label" >&2
        exit 1
    fi
}

check_optional_process() {
    local binary="$1"
    local pattern="$2"
    local label="$3"

    if command -v "$binary" >/dev/null 2>&1; then
        require_process_pattern "$pattern" "$label"
    else
        printf 'KDE_SESSION_PROCESS_%s=skipped_missing_binary\n' "$label"
    fi
}

require_binary redbear-session-launch
require_binary redbear-kde-session
require_binary kwin_wayland_wrapper

rm -rf "$state_dir" "$runtime_dir"
mkdir -p "$state_dir" "$runtime_dir"
chmod 700 "$state_dir" "$runtime_dir" 2>/dev/null || true

env \
    QT_PLUGIN_PATH=/usr/plugins \
    QT_QPA_PLATFORM_PLUGIN_PATH=/usr/plugins/platforms \
    QML2_IMPORT_PATH=/usr/qml \
    XCURSOR_THEME=Pop \
    XKB_CONFIG_ROOT=/usr/share/X11/xkb \
    REDBEAR_KDE_SESSION_BACKEND=virtual \
    REDBEAR_KDE_SESSION_STATE_DIR="$state_dir" \
    redbear-session-launch \
        --username root \
        --mode session \
        --session kde-wayland \
        --vt 4 \
        --runtime-dir "$runtime_dir" \
        --wayland-display "$display_name" &
session_pid=$!

ready_file="$state_dir/redbear-kde-session.ready"
env_file="$state_dir/redbear-kde-session.env"
panel_ready_file="$state_dir/redbear-kde-session.panel-ready"

if wait_for_file "$ready_file" 40; then
    printf 'KDE_SESSION_START=ok\n'
else
    printf 'KDE_SESSION_START=timeout\n' >&2
    exit 1
fi

if [[ ! -f "$env_file" ]]; then
    printf 'KDE_SESSION_ENV_FILE=missing\n' >&2
    exit 1
fi
printf 'KDE_SESSION_ENV_FILE=%s\n' "$env_file"

require_env_value "$env_file" XDG_SESSION_TYPE wayland
require_env_value "$env_file" XDG_CURRENT_DESKTOP KDE
require_env_value "$env_file" KDE_FULL_SESSION true
require_env_value "$env_file" KWIN_MODE virtual

require_process_pattern '(kwin_wayland_wrapper|redbear-compositor)' COMPOSITOR
check_optional_process kded6 'kded6' KDED6
check_optional_process plasmashell 'plasmashell' PLASMASHELL

if command -v plasmashell >/dev/null 2>&1; then
    if wait_for_file "$panel_ready_file" 15; then
        printf 'KDE_SESSION_PANEL_READY=ok\n'
    else
        printf 'KDE_SESSION_PANEL_READY=missing\n' >&2
        exit 1
    fi
else
    printf 'KDE_SESSION_PANEL_READY=skipped_missing_plasmashell\n'
fi
