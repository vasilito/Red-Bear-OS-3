#!/usr/bin/env bash
set -euo pipefail

if [ "$(uname -s)" != "Redox" ]; then
    echo "SKIP: Bluetooth runtime helper is guest-only and requires a Redox runtime"
    echo "Use the host checks for build/test verification, and run this script inside a Redox guest."
    exit 0
fi

exec redbear-bluetooth-battery-check
