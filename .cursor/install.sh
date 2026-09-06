#!/usr/bin/env bash
# Cloud Agent install: idempotent repository setup run after source checkout.
# The default base image already provides Rust, Node, and pnpm; PostgreSQL and
# Docker are not present, so we install PostgreSQL directly (the repo's
# docker-compose Postgres is replaced by a local cluster here).
set -euo pipefail

# Non-interactive: let pnpm proceed without TTY prompts (e.g. modules purge).
export CI=true

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

echo "==> Installing PostgreSQL (system package)"
if ! command -v pg_ctlcluster >/dev/null 2>&1; then
  sudo apt-get update -qq
  sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq postgresql postgresql-client
fi

echo "==> Ensuring PostgreSQL cluster is running with the skl role/database"
sudo pg_ctlcluster 16 main start || true
for _ in $(seq 1 30); do
  if sudo -u postgres pg_isready -q; then break; fi
  sleep 1
done
sudo -u postgres psql -tc "SELECT 1 FROM pg_roles WHERE rolname='skl'" | grep -q 1 \
  || sudo -u postgres psql -c "CREATE ROLE skl LOGIN PASSWORD 'skl'"
sudo -u postgres psql -tc "SELECT 1 FROM pg_database WHERE datname='skl'" | grep -q 1 \
  || sudo -u postgres createdb -O skl skl

echo "==> Building the Rust CLI (skl)"
cargo build -p skl

echo "==> Installing Node dependencies for apps (api, web, docs)"
for app in api web docs; do
  (cd "apps/$app" && pnpm install)
done

echo "==> Seeding local env files (safe defaults; do not overwrite existing)"
[ -f apps/api/.env ] || cp apps/api/.env.example apps/api/.env
[ -f apps/web/.env.local ] || cp apps/web/.env.example apps/web/.env.local

echo "==> Applying database migrations"
(cd apps/api && pnpm migrate)

echo "==> Install complete"
