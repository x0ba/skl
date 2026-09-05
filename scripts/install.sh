#!/usr/bin/env bash
# Compatibility shim. The installer is a website asset at apps/web/public/install.sh.
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/apps/web/public/install.sh" "$@"
