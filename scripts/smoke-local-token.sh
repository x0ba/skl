#!/usr/bin/env bash
# Consumes `skl login --dev-user` / `logout` / `status` / `doctor` and the
# existing SKL_TOKEN / SKL_TOKEN_FILE overrides. Does not invent verbs.
#
#   1. Login persists to state.db without OS keyring / DBus secrets
#   2. Subsequent verbs use the stored token; logout clears it
#   3. SKL_TOKEN and SKL_TOKEN_FILE still override the store
#   4. Migrate-once from a leftover native keyring token (adopt into empty store)
#   5. Dual-HOME / capture / auto-sync stay on this store path (those scripts)
#
# Usage:
#   cargo build -p skl
#   ./scripts/smoke-local-token.sh
#   START_API=1 ./scripts/smoke-local-token.sh
#   START_API=1 SKIP_DOCKER=1 ./scripts/smoke-local-token.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=smoke-lib.sh
source "$ROOT/scripts/smoke-lib.sh"

skl_smoke_defaults
TOKEN="${SKL_TOKEN:-dev:smoke-local-$$}"
TOKEN_A="${SKL_TOKEN_A:-$TOKEN}"
TOKEN_B="${SKL_TOKEN_B:-$TOKEN}"
SKILL_NAME="${SKL_SMOKE_SKILL:-localtok-$$}"
HOME_A="$WORKDIR/machine-a"
HOME_B="$WORKDIR/machine-b"
HOME_OVERRIDE="$WORKDIR/machine-override"
HOME_MIGRATE="$WORKDIR/machine-migrate"
HOME_ADOPT="$WORKDIR/machine-adopt"

# This smoke must exercise the store, not the env override.
unset SKL_TOKEN SKL_TOKEN_FILE SKL_SMOKE_TOKEN_ENV
skl_clean_secret_service_env

trap 'skl_clear_legacy_keyring || true; skl_smoke_cleanup' EXIT

user_of() {
  skl_dev_user "$1"
}

skl_start_api
skl_require_bin
if [[ "${START_API:-}" == "1" ]]; then
  skl_wait_for_api
fi

mkdir -p "$HOME_A" "$HOME_B" "$HOME_OVERRIDE" "$HOME_MIGRATE" "$HOME_ADOPT"
skl_write_sync_prefs "$HOME_A" false 900
skl_write_sync_prefs "$HOME_B" false 900

echo "==> [1] login persists without OS keyring / DBus secrets"
login_out="$(skl_login_store "$HOME_A" "$TOKEN_A")"
echo "$login_out"
db_a="$(skl_state_db "$HOME_A")"
skl_assert_unix_mode "$HOME_A/.local/share/skl" "700"
skl_assert_unix_mode "$db_a" "600"

echo "==> [2] subsequent verbs use the stored token; logout clears"
status_a="$(skl_run_store "$HOME_A" status 2>&1)"
echo "$status_a"
skl_assert_contains "$status_a" "token        yes (dev:$(user_of "$TOKEN_A"))"
skl_assert_contains "$status_a" "state.db"

doctor_a="$(skl_run_store "$HOME_A" doctor 2>&1)"
echo "$doctor_a"
skl_assert_contains "$doctor_a" "token        present"
skl_assert_contains "$doctor_a" "token_store  local state.db"
skl_assert_not_contains "$doctor_a" "keyring      "

if [[ "${START_API:-}" == "1" ]]; then
  mkdir -p "$HOME_A/.claude/skills/${SKILL_NAME}"
  printf '# %s\n\nhello from local store\n' "$SKILL_NAME" \
    >"$HOME_A/.claude/skills/${SKILL_NAME}/SKILL.md"
  echo "    A: init + sync without SKL_TOKEN (API verb uses store)"
  a_init="$(skl_run_store "$HOME_A" init 2>&1)"
  echo "$a_init"
  skl_assert_contains "$a_init" "Imported 1 skill"
  a_sync="$(skl_run_store "$HOME_A" sync 2>&1)"
  echo "$a_sync"
  skl_assert_contains "$a_sync" "POST $API/v1/sync"
  skl_assert_contains "$a_sync" "sync done"

  echo "    B: login + sync download without SKL_TOKEN"
  mkdir -p "$HOME_B/.agents/skills"
  skl_login_store "$HOME_B" "$TOKEN_B" >/dev/null
  b_init="$(skl_run_store "$HOME_B" init 2>&1)"
  echo "$b_init"
  b_sync="$(skl_run_store "$HOME_B" sync 2>&1)"
  echo "$b_sync"
  skl_assert_contains "$b_sync" "wrote skill $SKILL_NAME"
  skl_assert_file_contains \
    "$(skl_library_of "$HOME_B" "$SKILL_NAME")/SKILL.md" \
    "hello from local store"
fi

logout_out="$(skl_run_store "$HOME_A" logout 2>&1)"
echo "$logout_out"
skl_assert_no_state_token "$HOME_A"
after="$(skl_run_store "$HOME_A" status 2>&1)"
echo "$after"
skl_assert_contains "$after" "token        no"

echo "==> [3] SKL_TOKEN and SKL_TOKEN_FILE still override the store"
skl_login_store "$HOME_OVERRIDE" "$TOKEN_A" >/dev/null
skl_assert_state_token "$HOME_OVERRIDE" "dev:$(user_of "$TOKEN_A")"

env_out="$(
  env -u SKL_TOKEN_FILE \
    SKL_TOKEN="dev:from-env" \
    HOME="$HOME_OVERRIDE" \
    SKL_DATA_DIR="$HOME_OVERRIDE/.local/share/skl" \
    SKL_CONFIG_DIR="$HOME_OVERRIDE/.config/skl" \
    SKL_NO_PROMPT=1 \
    API_BASE="$API" \
    "$BIN" status 2>&1
)"
echo "$env_out"
skl_assert_contains "$env_out" "dev:from-env"
# Override must not wipe the local store.
skl_assert_state_token "$HOME_OVERRIDE" "dev:$(user_of "$TOKEN_A")"
local_again="$(skl_run_store "$HOME_OVERRIDE" status 2>&1)"
skl_assert_contains "$local_again" "dev:$(user_of "$TOKEN_A")"

token_file="$HOME_OVERRIDE/token-file"
printf 'dev:from-file\n' >"$token_file"
file_out="$(
  env -u SKL_TOKEN SKL_TOKEN_FILE="$token_file" \
    HOME="$HOME_OVERRIDE" \
    SKL_DATA_DIR="$HOME_OVERRIDE/.local/share/skl" \
    SKL_CONFIG_DIR="$HOME_OVERRIDE/.config/skl" \
    SKL_NO_PROMPT=1 \
    API_BASE="$API" \
    "$BIN" status 2>&1
)"
echo "$file_out"
skl_assert_contains "$file_out" "dev:from-file"
skl_assert_state_token "$HOME_OVERRIDE" "dev:$(user_of "$TOKEN_A")"

both_out="$(
  env SKL_TOKEN="dev:from-env" SKL_TOKEN_FILE="$token_file" \
    HOME="$HOME_OVERRIDE" \
    SKL_DATA_DIR="$HOME_OVERRIDE/.local/share/skl" \
    SKL_CONFIG_DIR="$HOME_OVERRIDE/.config/skl" \
    SKL_NO_PROMPT=1 \
    API_BASE="$API" \
    "$BIN" status 2>&1
)"
echo "$both_out"
skl_assert_contains "$both_out" "dev:from-env"

echo "==> [4] migrate-once from leftover keyring / adopt into empty store"
skl_write_sync_prefs "$HOME_ADOPT" false 900
skl_write_sync_prefs "$HOME_MIGRATE" false 900
# 4a. Same meta key migrate-once writes (adopt when local empty).
skl_seed_local_token "$HOME_ADOPT" "dev:from-adopt"
adopt_status="$(skl_run_store "$HOME_ADOPT" status 2>&1)"
echo "$adopt_status"
skl_assert_contains "$adopt_status" "dev:from-adopt"
skl_assert_state_token "$HOME_ADOPT" "dev:from-adopt"
# Adopt-once: a second leftover must not overwrite an existing local token.
skl_seed_local_token "$HOME_ADOPT" "dev:from-adopt"
skl_assert_state_token "$HOME_ADOPT" "dev:from-adopt"

# 4b. Native keyutils leftover (no DBus / Secret Service).
skl_clear_legacy_keyring || true
if desc="$(skl_plant_legacy_keyring "dev:from-keyring")"; then
  echo "    planted leftover keyring desc=$desc"
  migrate_status="$(skl_run_store "$HOME_MIGRATE" status 2>&1)"
  echo "$migrate_status"
  skl_assert_contains "$migrate_status" "dev:from-keyring"
  skl_assert_state_token "$HOME_MIGRATE" "dev:from-keyring"
  # Leftover is unused after migrate: changing it must not overwrite local.
  skl_clear_legacy_keyring || true
  if skl_plant_legacy_keyring "dev:from-keyring-2" >/dev/null; then
    again_status="$(skl_run_store "$HOME_MIGRATE" status 2>&1)"
    echo "$again_status"
    skl_assert_contains "$again_status" "dev:from-keyring"
    skl_assert_not_contains "$again_status" "dev:from-keyring-2"
    skl_assert_state_token "$HOME_MIGRATE" "dev:from-keyring"
  fi
  skl_clear_legacy_keyring || true
else
  echo "    native keyutils plant unavailable — migrate-once adopt path (4a) + cargo test cover it"
fi

echo "OK: DAN-14 local token store (login/logout/overrides/migrate) against store=$db_a"
