#!/usr/bin/env bash
# Live clash + scrub smoke against cipher's API on localhost:8787.
#
# Same Clerk-dev user on two machines (a clash is per-user, not cross-user).
# Prerequisites (or START_API=1 to boot them here):
#   - apps/api listening on :8787
#   - ALLOW_DEV_AUTH=true (or CLERK_SECRET_KEY unset)
#   - Postgres reachable as DATABASE_URL (see apps/api/.env.example)
#
# Usage:
#   cargo build -p skl
#   ./scripts/smoke-clash.sh
#   START_API=1 ./scripts/smoke-clash.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${SKL_BIN:-$ROOT/target/debug/skl}"
API="${API_BASE:-http://localhost:8787}"
WORKDIR="${TMPDIR:-/tmp}/skl-smoke-$$"
MACHINE_A="$WORKDIR/machine-a"
MACHINE_B="$WORKDIR/machine-b"
# Same user: two devices, one skill name, divergent trees.
TOKEN="${SKL_TOKEN:-dev:alice}"
API_PID=""

cleanup() {
  if [[ -n "$API_PID" ]] && kill -0 "$API_PID" 2>/dev/null; then
    kill "$API_PID" 2>/dev/null || true
    wait "$API_PID" 2>/dev/null || true
  fi
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

start_api() {
  if [[ "${START_API:-}" != "1" ]]; then
    return 0
  fi
  echo "==> START_API=1: postgres + apps/api (ALLOW_DEV_AUTH=true)"
  if ! command -v docker >/dev/null 2>&1; then
    echo "docker is required for START_API=1" >&2
    exit 1
  fi
  docker compose -f "$ROOT/docker-compose.yml" up -d postgres
  for _ in $(seq 1 40); do
    if docker compose -f "$ROOT/docker-compose.yml" exec -T postgres pg_isready -U skl >/dev/null 2>&1; then
      break
    fi
    sleep 1
  done
  (
    cd "$ROOT/apps/api"
    if [[ ! -f .env ]]; then
      cp .env.example .env
    fi
    if [[ ! -d node_modules ]]; then
      pnpm install
    fi
    pnpm migrate
    # Override .env.example's ALLOW_DEV_AUTH=false (dot-env loader skips set keys).
    ALLOW_DEV_AUTH=true pnpm start
  ) &
  API_PID=$!
}

wait_for_api() {
  echo "==> waiting for $API/v1/health"
  for _ in $(seq 1 60); do
    if curl -fsS "$API/v1/health" >/dev/null 2>&1; then
      echo "    api up"
      return 0
    fi
    sleep 1
  done
  echo "API not reachable at $API/v1/health" >&2
  echo "Start cipher's API with ALLOW_DEV_AUTH=true, or re-run with START_API=1." >&2
  exit 1
}

require_bin() {
  if [[ ! -x "$BIN" ]]; then
    echo "==> building skl"
    (cd "$ROOT" && cargo build -p skl)
  fi
  if [[ ! -x "$BIN" ]]; then
    echo "missing $BIN" >&2
    exit 1
  fi
}

seed_skill() {
  local home="$1"
  local body="$2"
  mkdir -p "$home/.claude/skills/clash-demo"
  printf '%s' "$body" > "$home/.claude/skills/clash-demo/SKILL.md"
}

run_skl() {
  local home="$1"
  shift
  env HOME="$home" \
    SKL_DATA_DIR="$home/.local/share/skl" \
    SKL_CONFIG_DIR="$home/.config/skl" \
    SKL_TOKEN="$TOKEN" \
    API_BASE="$API" \
    "$BIN" "$@"
}

assert_contains() {
  local haystack="$1"
  local needle="$2"
  if [[ "$haystack" != *"$needle"* ]]; then
    echo "expected output to contain: $needle" >&2
    echo "got:" >&2
    echo "$haystack" >&2
    exit 1
  fi
}

start_api
require_bin
wait_for_api

mkdir -p "$MACHINE_A" "$MACHINE_B"
seed_skill "$MACHINE_A" $'# Clash demo\n\nMachine A version of clash-demo.\n'
seed_skill "$MACHINE_B" $'# Clash demo\n\nMachine B version of clash-demo.\n'

echo "==> machine A: init + sync (first upload)"
run_skl "$MACHINE_A" init
a_out="$(run_skl "$MACHINE_A" sync 2>&1)"
echo "$a_out"
assert_contains "$a_out" "POST $API/v1/sync"
assert_contains "$a_out" "sync done"

echo "==> machine B: init + sync (non-interactive clash must fail)"
run_skl "$MACHINE_B" init
set +e
b_clash="$(run_skl "$MACHINE_B" sync 2>&1)"
b_status=$?
set -e
echo "$b_clash"
if [[ $b_status -eq 0 ]]; then
  echo "expected non-interactive clash to fail without --keep-local/--keep-remote" >&2
  exit 1
fi
assert_contains "$b_clash" "conflict:"
assert_contains "$b_clash" "non-interactive sync needs --keep-local or --keep-remote"

echo "==> machine B: --keep-local (PUT local tree, re-POST /v1/sync)"
b_keep="$(run_skl "$MACHINE_B" sync --keep-local 2>&1)"
echo "$b_keep"
assert_contains "$b_keep" "keep-local: clash-demo"
assert_contains "$b_keep" "PUT /v1/skills/clash-demo/tree"
assert_contains "$b_keep" "re-POST $API/v1/sync"
assert_contains "$b_keep" "re-POST conflicts: 0"

echo "==> machine A: --keep-remote (overwrite local from remote, re-POST /v1/sync)"
a_keep="$(run_skl "$MACHINE_A" sync --keep-remote 2>&1)"
echo "$a_keep"
assert_contains "$a_keep" "keep-remote: clash-demo"
assert_contains "$a_keep" "re-POST conflicts: 0"
if ! grep -q 'Machine B version of clash-demo' "$MACHINE_A/.claude/skills/clash-demo/SKILL.md"; then
  echo "machine A local file was not overwritten by keep-remote" >&2
  cat "$MACHINE_A/.claude/skills/clash-demo/SKILL.md" >&2
  exit 1
fi

echo "==> scrub: dirty blob is not PUT"
# Split so casual secret scanners do not flag this script.
printf '%s\n' "-----BEGIN OPENSSH PRIVATE KEY-----" "fake" "-----END OPENSSH PRIVATE KEY-----" \
  > "$MACHINE_B/.claude/skills/clash-demo/secret.env"
run_skl "$MACHINE_B" init
set +e
scrub_out="$(run_skl "$MACHINE_B" sync --keep-local 2>&1)"
scrub_status=$?
set -e
echo "$scrub_out"
if [[ $scrub_status -eq 0 ]]; then
  echo "expected scrub to block the dirty blob" >&2
  exit 1
fi
assert_contains "$scrub_out" "upload blocked:"
assert_contains "$scrub_out" "blocked"

echo "OK: clash keep-local / keep-remote + scrub against $API"
