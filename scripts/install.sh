#!/bin/sh
# Compatibility shim. The installer is a website asset at apps/web/public/install.sh.
exec "$(cd "$(dirname "$0")/.." && pwd)/apps/web/public/install.sh" "$@"
