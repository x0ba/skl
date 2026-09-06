#!/usr/bin/env bash
# Cloud Agent start: per-boot runtime initialization. Ensures PostgreSQL is
# running and the skl role/database exist before the dev servers (declared as
# terminals) start. Must be idempotent and tolerate restarts.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

echo "==> Starting PostgreSQL cluster"
sudo pg_ctlcluster 16 main start || true
for _ in $(seq 1 30); do
  if sudo -u postgres pg_isready -q; then break; fi
  sleep 1
done

echo "==> Ensuring skl role/database exist"
sudo -u postgres psql -tc "SELECT 1 FROM pg_roles WHERE rolname='skl'" | grep -q 1 \
  || sudo -u postgres psql -c "CREATE ROLE skl LOGIN PASSWORD 'skl'"
sudo -u postgres psql -tc "SELECT 1 FROM pg_database WHERE datname='skl'" | grep -q 1 \
  || sudo -u postgres createdb -O skl skl

echo "==> Applying any pending migrations"
(cd apps/api && pnpm migrate) || true

echo "==> Start complete"
