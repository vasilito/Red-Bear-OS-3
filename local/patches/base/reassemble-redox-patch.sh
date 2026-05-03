#!/bin/bash
# Reassemble redox.patch from split chunks (needed because the full patch
# exceeds GitHub's 100 MB file size limit).
#
# Usage:
#   cd local/patches/base/
#   bash reassemble-redox-patch.sh

set -euo pipefail
cd "$(dirname "$0")"
cat redox-patch-chunks/chunk_* > redox.patch
echo "Reassembled redox.patch ($(wc -c < redox.patch) bytes)"
