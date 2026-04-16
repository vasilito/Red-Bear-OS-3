#!/usr/bin/env bash
# Build redbear-wifictl for the Redox target using the repo-provided cross toolchain.

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: build-redbear-wifictl-redox.sh [cargo build args...]

This helper ensures the repository's Redox cross-linker directory is on PATH before invoking
`cargo build --target x86_64-unknown-redox` for `redbear-wifictl`.

Examples:
  ./local/scripts/build-redbear-wifictl-redox.sh
  ./local/scripts/build-redbear-wifictl-redox.sh --release
EOF
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
toolchain_bin="$repo_root/prefix/x86_64-unknown-redox/sysroot/bin"
crate_dir="$repo_root/local/recipes/system/redbear-wifictl/source"

if [[ ! -d "$toolchain_bin" ]]; then
  echo "ERROR: missing Redox toolchain bin dir: $toolchain_bin" >&2
  echo "Build the prefix/sysroot first before using this helper." >&2
  exit 1
fi

if [[ ! -x "$toolchain_bin/x86_64-unknown-redox-gcc" ]]; then
  echo "ERROR: missing executable linker: $toolchain_bin/x86_64-unknown-redox-gcc" >&2
  exit 1
fi

echo "Using Redox toolchain from: $toolchain_bin"
PATH="$toolchain_bin:$PATH" cargo build --target x86_64-unknown-redox "$@"
