#!/usr/bin/env bash
# Shared helpers for two-HOME CLI smokes against cipher's API on :8787.
# CLI crate path is crates/cli; package/binary name is skl (`cargo build -p skl`).
#
# Used by scripts/smoke-import-sync-use.sh and
# scripts/smoke-portable-use-all.sh. The clash + scrub harness
# (scripts/smoke-clash.sh on the conflict/scrub PR) uses the same HOME /
# SKL_TOKEN / ALLOW_DEV_AUTH=true pattern.
#
# DAN-14: `skl login` persists to local `state.db` (no DBus / OS keyring).
# Prefer `skl_login_store` + `skl_run_store` so CI exercises that path.
# Belt-and-suspenders: SKL_SMOKE_TOKEN_ENV=1 (or skl_run) still exports
# SKL_TOKEN / SKL_TOKEN_FILE, which override the store.
#
# Env:
#   API_BASE      default http://localhost:8787
#   SKL_BIN       default $ROOT/target/debug/skl
#   SKL_TOKEN     default dev:alice  (ALLOW_DEV_AUTH user id)
#   SKL_TOKEN_FILE  if set, read token from this file (overrides empty SKL_TOKEN)
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
  if [[ -z "${SKL_TOKEN:-}" && -n "${SKL_TOKEN_FILE:-}" && -f "${SKL_TOKEN_FILE}" ]]; then
    SKL_TOKEN="$(tr -d '[:space:]' <"$SKL_TOKEN_FILE")"
    export SKL_TOKEN
  fi
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

# Strip an optional `dev:` prefix (`login --dev-user` accepts either).
skl_dev_user() {
  local token="${1:-}"
  token="${token#dev:}"
  printf '%s' "$token"
}

skl_state_db() {
  printf '%s' "$1/.local/share/skl/state.db"
}

# Drop Secret Service / DBus + token env so the local store is the path.
skl_clean_secret_service_env() {
  unset DBUS_SESSION_BUS_ADDRESS
  unset GNOME_KEYRING_CONTROL
  unset GNOME_KEYRING_PID
  unset KDE_FULL_SESSION
}

skl_meta_get() {
  local db="$1"
  local key="$2"
  if [[ ! -f "$db" ]]; then
    return 0
  fi
  python3 - "$db" "$key" <<'PY'
import sqlite3, sys

db, key = sys.argv[1], sys.argv[2]
con = sqlite3.connect(db)
try:
    row = con.execute("SELECT value FROM meta WHERE key = ?", (key,)).fetchone()
except sqlite3.OperationalError:
    row = None
print("" if row is None else row[0])
PY
}

skl_assert_state_token() {
  local home="$1"
  local expected="$2"
  local db got
  db="$(skl_state_db "$home")"
  if [[ ! -f "$db" ]]; then
    echo "missing state.db (login did not persist): $db" >&2
    exit 1
  fi
  got="$(skl_meta_get "$db" "device_token")"
  if [[ "$got" != "$expected" ]]; then
    echo "state.db device_token expected '$expected', got '$got'" >&2
    exit 1
  fi
}

skl_assert_no_state_token() {
  local home="$1"
  local db got
  db="$(skl_state_db "$home")"
  if [[ ! -f "$db" ]]; then
    return 0
  fi
  got="$(skl_meta_get "$db" "device_token")"
  if [[ -n "$got" ]]; then
    echo "expected no device_token in $db, got '$got'" >&2
    exit 1
  fi
}

skl_assert_unix_mode() {
  local path="$1"
  local expected="$2"
  local got
  if [[ ! -e "$path" ]]; then
    echo "missing path for mode check: $path" >&2
    exit 1
  fi
  got="$(stat -c '%a' "$path")"
  if [[ "$got" != "$expected" ]]; then
    echo "expected mode $expected on $path, got $got" >&2
    exit 1
  fi
}

# Seed `meta.device_token` the same way migrate-once / adopt writes it.
# Usage: skl_seed_local_token <home> <token>
skl_seed_local_token() {
  local home="$1"
  local token="$2"
  local db
  db="$(skl_state_db "$home")"
  mkdir -p "$(dirname "$db")"
  python3 - "$db" "$token" <<'PY'
import sqlite3, sys

db, token = sys.argv[1], sys.argv[2]
con = sqlite3.connect(db)
con.execute(
    "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)"
)
con.execute(
    "INSERT INTO meta (key, value) VALUES (?, ?) "
    "ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    ("device_token", token),
)
con.commit()
PY
}

# Plant a leftover native keyring token (linux-keyutils, no DBus).
# keyring 3 `linux-native` description is `keyring-rs:{user}@{service}`
# (`user` = account = device_token, `service` = skl).
# Prints the description that was written, or returns 1 if none worked.
skl_plant_legacy_keyring() {
  local token="$1"
  local desc
  if ! command -v keyctl >/dev/null 2>&1 && ! python3 -c 'import ctypes.util; raise SystemExit(0 if ctypes.util.find_library("keyutils") else 1)' 2>/dev/null; then
    return 1
  fi
  # Clear first so a stale description does not shadow the plant.
  skl_clear_legacy_keyring || true
  for desc in "keyring-rs:device_token@skl" "keyring:skl@device_token" "skl:device_token" "skl"; do
    if python3 - "$token" "$desc" <<'PY'
import ctypes
import ctypes.util
import sys

token, desc = sys.argv[1], sys.argv[2]
libname = ctypes.util.find_library("keyutils") or "libkeyutils.so.1"
try:
    lib = ctypes.CDLL(libname)
except OSError:
    raise SystemExit(2)
add_key = lib.add_key
add_key.argtypes = [
    ctypes.c_char_p,
    ctypes.c_char_p,
    ctypes.c_void_p,
    ctypes.c_size_t,
    ctypes.c_int,
]
add_key.restype = ctypes.c_int
# keyring-rs searches the session keyring (`@s` / KEY_SPEC_SESSION_KEYRING).
KEY_SPEC_SESSION_KEYRING = -3
KEY_SPEC_USER_SESSION_KEYRING = -5
payload = token.encode()
ok = False
for ring in (KEY_SPEC_SESSION_KEYRING, KEY_SPEC_USER_SESSION_KEYRING):
    serial = add_key(
        b"user",
        desc.encode(),
        payload,
        len(payload),
        ring,
    )
    if serial > 0:
        ok = True
        break
raise SystemExit(0 if ok else 1)
PY
    then
      printf '%s' "$desc"
      return 0
    fi
    if command -v keyctl >/dev/null 2>&1; then
      if keyctl add user "$desc" "$token" @s >/dev/null 2>&1 \
        || keyctl add user "$desc" "$token" @us >/dev/null 2>&1; then
        printf '%s' "$desc"
        return 0
      fi
    fi
  done
  return 1
}

skl_clear_legacy_keyring() {
  python3 - <<'PY' || true
import ctypes
import ctypes.util
import subprocess

descs = (
    "keyring-rs:device_token@skl",
    "keyring:skl@device_token",
    "skl:device_token",
    "skl",
)
libname = ctypes.util.find_library("keyutils")
if libname:
    try:
        lib = ctypes.CDLL(libname)
        request = lib.request_key
        request.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_char_p, ctypes.c_int]
        request.restype = ctypes.c_int
        invalidate = lib.keyctl_invalidate
        invalidate.argtypes = [ctypes.c_int]
        invalidate.restype = ctypes.c_int
        KEY_SPEC_SESSION_KEYRING = -3
        KEY_SPEC_USER_SESSION_KEYRING = -5
        KEY_SPEC_USER_KEYRING = -4
        for ring in (
            KEY_SPEC_SESSION_KEYRING,
            KEY_SPEC_USER_SESSION_KEYRING,
            KEY_SPEC_USER_KEYRING,
        ):
            for desc in descs:
                serial = request(b"user", desc.encode(), None, ring)
                if serial > 0:
                    invalidate(serial)
    except OSError:
        pass
for desc in descs:
    try:
        found = subprocess.run(
            ["keyctl", "search", "@u", "user", desc],
            check=False,
            capture_output=True,
            text=True,
        )
        serial = found.stdout.strip()
        if found.returncode == 0 and serial:
            subprocess.run(
                ["keyctl", "unlink", serial, "@u"],
                check=False,
                capture_output=True,
            )
    except FileNotFoundError:
        break
PY
}

# Run skl as one machine: isolated HOME + XDG dirs + SKL_TOKEN override.
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

# Run skl against the local `state.db` store (no SKL_TOKEN / no DBus secrets).
# Belt-and-suspenders: SKL_SMOKE_TOKEN_ENV=1 keeps the env override path.
# Usage: skl_run_store <home-dir> [skl args...]
skl_run_store() {
  local home="$1"
  shift
  if [[ "${SKL_SMOKE_TOKEN_ENV:-}" == "1" ]]; then
    skl_run "$home" "$@"
    return
  fi
  skl_clean_secret_service_env
  env -u SKL_TOKEN -u SKL_TOKEN_FILE \
    -u DBUS_SESSION_BUS_ADDRESS \
    -u GNOME_KEYRING_CONTROL \
    -u GNOME_KEYRING_PID \
    HOME="$home" \
    SKL_DATA_DIR="$home/.local/share/skl" \
    SKL_CONFIG_DIR="$home/.config/skl" \
    SKL_NO_PROMPT=1 \
    API_BASE="$API" \
    "$BIN" "$@"
}

# Persist a dev token into the local store (`skl login --dev-user`).
# Callers that want auto-sync should write `[sync] auto=false` before login
# so maybe_run does not consume the first due slot, then turn auto on after.
# Usage: skl_login_store <home-dir> [token]
skl_login_store() {
  local home="$1"
  local token="${2:-${SKL_TOKEN:-$TOKEN}}"
  local user out
  user="$(skl_dev_user "$token")"
  if [[ -z "${user// }" ]]; then
    echo "skl_login_store: empty token (set SKL_TOKEN or pass a token)" >&2
    exit 1
  fi
  mkdir -p "$home/.claude/skills" "$home/.config/skl" "$home/.local/share/skl"
  printf '%s\n' "dev:${user}" >"$home/.config/skl/ci-token"
  out="$(skl_run_store "$home" login --dev-user "$user" 2>&1)" || {
    echo "$out" >&2
    echo "skl_login_store failed for $home" >&2
    exit 1
  }
  echo "$out"
  skl_assert_contains "$out" "store"
  if echo "$out" | grep -qi 'keyring'; then
    echo "login must not require the OS keyring:" >&2
    echo "$out" >&2
    exit 1
  fi
  skl_assert_state_token "$home" "dev:${user}"
}

# Isolated HOME for a smoke machine. Writes a ci-token file only
# (env-override belt). Prefer skl_login_store for the local store path.
# Usage: skl_prepare_home <home-dir> [token]
skl_prepare_home() {
  local home="$1"
  local token="${2:-${SKL_TOKEN:-$TOKEN}}"
  if [[ -z "${token// }" ]]; then
    echo "skl_prepare_home: empty token (set SKL_TOKEN or SKL_TOKEN_FILE)" >&2
    exit 1
  fi
  mkdir -p "$home/.claude/skills" "$home/.config/skl" "$home/.local/share/skl"
  # File token next to XDG config so a later helper can re-read it.
  printf '%s\n' "$token" >"$home/.config/skl/ci-token"
}

# Write furnace `[sync]` prefs into one machine HOME.
# Usage: skl_write_sync_prefs <home> <auto:true|false> <frequency_secs>
skl_write_sync_prefs() {
  local home="$1"
  local auto="$2"
  local freq="$3"
  mkdir -p "$home/.config/skl"
  cat >"$home/.config/skl/config.toml" <<EOF
[sync]
auto = ${auto}
frequency_secs = ${freq}
EOF
}

# Backdate last_sync_at + last_auto_sync_attempt_at so maybe_run is due.
# Usage: skl_age_auto_sync <home> [age_secs]
skl_age_auto_sync() {
  local home="$1"
  local age="${2:-901}"
  local db="$home/.local/share/skl/state.db"
  if [[ ! -f "$db" ]]; then
    echo "missing $db (run init first)" >&2
    exit 1
  fi
  python3 - "$db" "$age" <<'PY'
import sqlite3, sys, time

db, age = sys.argv[1], int(sys.argv[2])
stamp = str(int(time.time()) - age)
con = sqlite3.connect(db)
con.execute(
    "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)"
)
for key in ("last_sync_at", "last_auto_sync_attempt_at"):
    con.execute(
        "INSERT INTO meta (key, value) VALUES (?, ?) "
        "ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        (key, stamp),
    )
con.commit()
PY
}

skl_count_sync_posts() {
  local haystack="$1"
  local api="${API:-http://localhost:8787}"
  printf '%s\n' "$haystack" | grep -F -c -- "POST ${api}/v1/sync" || true
}

skl_assert_sync_posts() {
  local haystack="$1"
  local expected="$2"
  local got
  got="$(skl_count_sync_posts "$haystack")"
  if [[ "$got" != "$expected" ]]; then
    echo "expected $expected POST /v1/sync, got $got" >&2
    echo "$haystack" >&2
    exit 1
  fi
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

skl_assert_not_contains() {
  local haystack="$1"
  local needle="$2"
  if [[ "$haystack" == *"$needle"* ]]; then
    echo "expected output not to contain: $needle" >&2
    echo "got:" >&2
    echo "$haystack" >&2
    exit 1
  fi
}

# Furnace portable manifest: names + mode, never host paths.
# Usage: skl_assert_portable_manifest <skills.toml> [forbidden-substring...]
skl_assert_portable_manifest() {
  local path="$1"
  shift
  if [[ ! -f "$path" ]]; then
    echo "missing manifest: $path" >&2
    exit 1
  fi
  local body
  body="$(cat "$path")"
  if grep -qE '^[[:space:]]*path[[:space:]]*=' "$path"; then
    echo "portable manifest must not write path=: $path" >&2
    echo "$body" >&2
    exit 1
  fi
  if [[ "$body" == *'$HOME'* ]]; then
    echo "portable manifest must not contain \$HOME: $path" >&2
    echo "$body" >&2
    exit 1
  fi
  if grep -qE '/Users/|/home/[A-Za-z0-9._-]+|[A-Za-z]:[\\/]' "$path"; then
    echo "portable manifest must not contain an absolute home path: $path" >&2
    echo "$body" >&2
    exit 1
  fi
  local needle
  for needle in "$@"; do
    if [[ -n "$needle" && "$body" == *"$needle"* ]]; then
      echo "portable manifest must not contain host path: $needle" >&2
      echo "$body" >&2
      exit 1
    fi
  done
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
