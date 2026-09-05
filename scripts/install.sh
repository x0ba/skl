#!/usr/bin/env bash
# skl curl installer (WIP — DAN-11 / M4).
# Swap SKL_DOWNLOAD_BASE to https://setup.skl.sh later.
set -euo pipefail

SKL_DOWNLOAD_BASE="${SKL_DOWNLOAD_BASE:-https://github.com/x0ba/skl/releases/latest/download}"

echo "skl install (WIP)"
echo "download base: $SKL_DOWNLOAD_BASE"
echo "hero: curl -fsSL ${SKL_DOWNLOAD_BASE}/install.sh | bash"
