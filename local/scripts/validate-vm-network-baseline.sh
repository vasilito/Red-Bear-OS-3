#!/usr/bin/env bash
#
# validate-vm-network-baseline.sh - Validate the Red Bear OS Phase 2 VM network baseline
#
# This is a repo-level validation helper for the minimal boot networking chain:
#   pcid-spawner -> smolnetd -> dhcpd -> netctl --boot
#
# It verifies the config and init surfaces that must be present for the default
# wired DHCP profile to come up in a QEMU/virtio-net baseline image.

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"

fail() {
    printf '❌ %s\n' "$1" >&2
    exit 1
}

pass() {
    printf '✅ %s\n' "$1"
}

require_file() {
    local path="$1"
    [[ -f "$ROOT/$path" ]] || fail "missing file: $path"
}

require_pattern() {
    local path="$1"
    local pattern="$2"
    local message="$3"
    grep -Eq "$pattern" "$ROOT/$path" || fail "$message ($path)"
}

printf '=== Red Bear OS VM Network Baseline Validation ===\n'
printf 'Root: %s\n\n' "$ROOT"

require_file "config/redbear-minimal.toml"
require_file "config/redbear-netctl.toml"
require_file "recipes/core/base/source/init.d/00_pcid-spawner.service"
require_file "recipes/core/base/source/init.d/10_smolnetd.service"
require_file "recipes/core/base/source/init.d/10_dhcpd.service"
require_file "recipes/core/base/recipe.toml"

require_pattern "config/redbear-minimal.toml" 'path = "/etc/netctl/active"' \
    'redbear-minimal must install /etc/netctl/active'
require_pattern "config/redbear-minimal.toml" 'data = "wired-dhcp\\n"' \
    'redbear-minimal must enable wired-dhcp by default'
pass 'redbear-minimal enables the wired-dhcp profile by default'

require_pattern "config/redbear-netctl.toml" 'path = "/usr/lib/init.d/12_netctl.service"' \
    'redbear-netctl config must install the boot service'
require_pattern "config/redbear-netctl.toml" '"10_smolnetd.service"' \
    'netctl boot service must reference smolnetd'
require_pattern "config/redbear-netctl.toml" '"10_dhcpd.service"' \
    'netctl boot service must reference dhcpd'
require_pattern "config/redbear-netctl.toml" 'args = \["--boot"\]' \
    'netctl boot service must run netctl --boot'
pass 'netctl boot service wiring is present'

require_pattern "recipes/core/base/source/init.d/10_smolnetd.service" '"00_pcid-spawner.service"' \
    'smolnetd must start after pcid-spawner'
require_pattern "recipes/core/base/source/init.d/10_dhcpd.service" '"10_smolnetd.service"' \
    'dhcpd must start after smolnetd'
pass 'base init ordering links pcid-spawner -> smolnetd -> dhcpd'

require_pattern "recipes/core/base/recipe.toml" 'virtio-netd' \
    'base recipe must build virtio-netd for VM networking'
pass 'base recipe includes virtio-netd'

printf '\nValidation chain:\n'
printf '  pcid-spawner -> smolnetd -> dhcpd -> netctl --boot -> wired-dhcp\n'
printf '\nAll required Phase 2 VM-network baseline surfaces are present.\n'
