#!/usr/bin/env bash
# Shared helpers for two-HOME CLI smokes against cipher's API on :8787.
# CLI crate path is crates/cli; package/binary name is skl (`cargo build -p skl`).
#
# Used by scripts/smoke-import-sync-use.sh. The clash + scrub harness
# (scripts/smoke-clash.sh on the conflict/scrub PR) uses the same HOME /
# SKL_TOKEN / ALLOW_DEV_AUTH=true pattern.
#
# Env:
#   API_BASE      default http://localhost:8787
#   SKL_BIN       default $ROOT/target/debug/skl
#   SKL_TOKEN     default dev:alice  (ALLOW_DEV_AUTH user id)
#   START_API=1   boot postgres (docker compose, unless SKIP_DOCKER=1) + apps/api
#   SKIP_DOCKER=1 assume Postgres is already on DATABASE_URL

skl_smoke_root() {
  local here
  here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  echo "$here"
}

skl_smoke_defaults() {
  ROOT="${ROOT:-$(skl_smoke_root)}"
  BIN="${SKL_BIN:-$ROOT/target/debug/skl}"
  API="${API_BASE:-http://localhost:8787}"
  TOKEN="${SKL_TOKEN:-dev:alice}"
  WORKDIR="${WORKDIR:-${TMPDIR:-/tmp}/skl-smoke-$$}"
  API_PID="${API_PID:-}"
}

skl_smoke_cleanup() {
  if [[ -n "${API_PID:-}" ]] && kill -0 "$API_PID" 2>/dev/null; then
    kill "$API_PID" 2>/dev/null || true
    wait "$API_PID" 2>/dev/null || true
  fi
  if [[ -n "${WORKDIR:-}" && -d "${WORKDIR:-}" ]]; then
    rm -rf "$WORKDIR"
  fi
}

skl_postgres_ready() {
  if command -v pg_isready >/dev/null 2>&1; then
    pg_isready -h "${PGHOST:-localhost}" -p "${PGPORT:-5432}" -U "${PGUSER:-skl}" -d "${PGDATABASE:-skl}" >/dev/null 2>&1
    return $?
  fi
  if command -v docker >/dev/null 2>&1; then
    docker compose -f "$ROOT/docker-compose.yml" exec -T postgres pg_isready -U skl >/dev/null 2>&1
    return $?
  fi
  return 1
}

skl_start_api() {
  if [[ "${START_API:-}" != "1" ]]; then
    return 0
  fi
  echo "==> START_API=1: postgres + apps/api (ALLOW_DEV_AUTH=true)"
  if [[ "${SKIP_DOCKER:-}" != "1" ]]; then
    if command -v docker >/dev/null 2>&1; then
      docker compose -f "$ROOT/docker-compose.yml" up -d postgres
      for _ in $(seq 1 40); do
        if docker compose -f "$ROOT/docker-compose.yml" exec -T postgres pg_isready -U skl >/dev/null 2>&1; then
          break
        fi
        sleep 1
      done
    elif ! skl_postgres_ready; then
      echo "docker is required for START_API=1 unless Postgres is already up (or set SKIP_DOCKER=1)." >&2
      exit 1
    fi
  else
    # CI often has a healthy Postgres service but no pg_isready on the runner.
    if ! skl_postgres_ready; then
      if ! bash -c "echo >/dev/tcp/${PGHOST:-localhost}/${PGPORT:-5432}" 2>/dev/null; then
        echo "SKIP_DOCKER=1 but Postgres is not reachable at ${PGHOST:-localhost}:${PGPORT:-5432}" >&2
        echo "Start Postgres or point DATABASE_URL at a running instance." >&2
        exit 1
      fi
    fi
  fi
  mkdir -p "$WORKDIR"
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
  ) >"$WORKDIR/api.log" 2>&1 &
  API_PID=$!
}

skl_wait_for_api() {
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

skl_require_bin() {
  echo "==> building skl"
  (cd "$ROOT" && cargo build -p skl)
  if [[ ! -x "$BIN" ]]; then
    echo "missing $BIN" >&2
    exit 1
  fi
}

# Run skl as one machine: isolated HOME + XDG dirs + SKL_TOKEN.
# Usage: skl_run <home-dir> [skl args...]
skl_run() {
  local home="$1"
  shift
  local token="${SKL_TOKEN:-$TOKEN}"
  env HOME="$home" \
    SKL_DATA_DIR="$home/.local/share/skl" \
    SKL_CONFIG_DIR="$home/.config/skl" \
    SKL_TOKEN="$token" \
    SKL_NO_PROMPT=1 \
    API_BASE="$API" \
    "$BIN" "$@"
}

skl_assert_contains() {
  local haystack="$1"
  local needle="$2"
  if [[ "$haystack" != *"$needle"* ]]; then
    echo "expected output to contain: $needle" >&2
    echo "got:" >&2
    echo "$haystack" >&2
    exit 1
  fi
}

skl_assert_file_contains() {
  local path="$1"
  local needle="$2"
  if [[ ! -f "$path" ]]; then
    echo "missing file: $path" >&2
    exit 1
  fi
  if ! grep -q -- "$needle" "$path"; then
    echo "expected $path to contain: $needle" >&2
    echo "got:" >&2
    cat "$path" >&2
    exit 1
  fi
}

skl_assert_symlink_to() {
  local link="$1"
  local expected="$2"
  if [[ ! -L "$link" ]]; then
    echo "expected symlink: $link" >&2
    ls -la "$(dirname "$link")" >&2 || true
    exit 1
  fi
  local dest
  dest="$(readlink "$link")"
  if [[ "$dest" != "$expected" ]]; then
    # Compare canonical paths — linker writes absolute dests.
    local dest_real expected_real
    dest_real="$(cd "$(dirname "$link")" && realpath -m "$dest")"
    expected_real="$(realpath -m "$expected")"
    if [[ "$dest_real" != "$expected_real" ]]; then
      echo "symlink $link -> $dest (expected $expected)" >&2
      exit 1
    fi
  fi
}
