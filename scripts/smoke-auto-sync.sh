#!/usr/bin/env bash
# M2 auto-sync smokes. No daemon. Does not call `skl sync` on the mutate path.
#
# Binds to furnace SyncPrefs + auto_sync::maybe_run + verb hooks:
#   Piggyback maybe_run when due:
#     login (after token stored)
#     init (after import)
#     use / unuse — fail-soft (never fail the verb)
#     status — best-effort sync when due (not display-only)
#   No network sync:
#     doctor — display last_sync only
#   Optional: list (do not rely on it as the only trigger)
#   Due / prefs:
#     last_sync_at (LocalDb, existing)
#     config.toml [sync] auto=… frequency_secs=…  (default 900)
#     throttle last_auto_sync_attempt_at — min 15m between attempts
#     fail-soft: eprintln!("auto-sync ({reason}): {err} (ignored)")
#     background conflict: KeepRemote (no TTY)
#
# Sections:
#   1. Dual-HOME: A mutate → due → B verb → library without `skl sync`
#   2. Throttle: two rapid verbs within 15m → one network sync
#   3. Fail-soft: API down during use → link succeeds; status shows issue
#
# Usage (crate path crates/cli; package name skl):
#   cargo build -p skl
#   ./scripts/smoke-auto-sync.sh
#   START_API=1 ./scripts/smoke-auto-sync.sh
#   START_API=1 SKIP_DOCKER=1 ./scripts/smoke-auto-sync.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=smoke-lib.sh
source "$ROOT/scripts/smoke-lib.sh"

skl_smoke_defaults
TOKEN="${SKL_TOKEN:-dev:smoke-autosync-$$}"
TOKEN_A="${SKL_TOKEN_A:-$TOKEN}"
TOKEN_B="${SKL_TOKEN_B:-$TOKEN}"
SKILL_NAME="${SKL_SMOKE_SKILL:-autosync-$$}"
DEAD_API="${SKL_SMOKE_DEAD_API:-http://127.0.0.1:1}"
# > default frequency 900 and the 15m attempt floor.
AGE_SECS="${SKL_SMOKE_AGE_SECS:-901}"

MACHINE_A="$WORKDIR/machine-a"
MACHINE_B="$WORKDIR/machine-b"
PROJECT_B="$WORKDIR/project-b"
PROJECT_THROTTLE="$WORKDIR/project-throttle"
PROJECT_FAIL="$WORKDIR/project-fail"

trap skl_smoke_cleanup EXIT

seed_skill() {
  local home="$1"
  local name="$2"
  local body="$3"
  mkdir -p "$home/.claude/skills/$name"
  printf '%s' "$body" > "$home/.claude/skills/$name/SKILL.md"
}

dev_user() {
  local token="$1"
  printf '%s' "${token#dev:}"
}

run_home() {
  local home="$1"
  local token="$2"
  shift 2
  SKL_TOKEN="$token" skl_run "$home" "$@"
}

run_a() { run_home "$MACHINE_A" "$TOKEN_A" "$@"; }
run_b() { run_home "$MACHINE_B" "$TOKEN_B" "$@"; }

prepare_machine() {
  local home="$1"
  local token="$2"
  mkdir -p "$home/.claude/skills" "$home/.config/skl" "$home/.local/share/skl"
  run_home "$home" "$token" login --dev-user "$(dev_user "$token")" >/dev/null
  skl_write_sync_prefs "$home" true 900
}

# --- 1. Dual-HOME ----------------------------------------------------------

smoke_dual_home() {
  echo "==> [1] Dual-HOME: A mutate → due → B status → library (no skl sync)"

  seed_skill "$MACHINE_A" "$SKILL_NAME" "# ${SKILL_NAME}

hello from machine A v1
"
  prepare_machine "$MACHINE_A" "$TOKEN_A"

  echo "    A: init (import + due piggyback upload)"
  local a_init
  a_init="$(run_a init 2>&1)"
  echo "$a_init"
  skl_assert_contains "$a_init" "Imported 1 skill"
  skl_require_auto_sync_hooks "$a_init"
  skl_assert_contains "$a_init" "POST $API/v1/sync"

  echo "    B: init empty home (import 0 + due piggyback download)"
  prepare_machine "$MACHINE_B" "$TOKEN_B"
  local b_init
  b_init="$(run_b init 2>&1)"
  echo "$b_init"
  skl_assert_contains "$b_init" "Imported 0 skill"
  skl_assert_contains "$b_init" "POST $API/v1/sync"
  skl_assert_file_contains "$MACHINE_B/.claude/skills/${SKILL_NAME}/SKILL.md" \
    "hello from machine A v1"

  echo "    A: mutate library"
  printf '%s' "# ${SKILL_NAME}

hello from machine A v2
" > "$MACHINE_A/.claude/skills/${SKILL_NAME}/SKILL.md"

  echo "    A: age due, then init (refresh index + piggyback push)"
  skl_age_auto_sync "$MACHINE_A" "$AGE_SECS"
  local a_push
  a_push="$(run_a init 2>&1)"
  echo "$a_push"
  skl_assert_contains "$a_push" "POST $API/v1/sync"

  echo "    B: age due, then status (best-effort sync — not display-only)"
  skl_age_auto_sync "$MACHINE_B" "$AGE_SECS"
  local b_status
  b_status="$(run_b status 2>&1)"
  echo "$b_status"
  skl_assert_contains "$b_status" "POST $API/v1/sync"
  skl_assert_contains "$b_status" "last_sync"
  skl_assert_file_contains "$MACHINE_B/.claude/skills/${SKILL_NAME}/SKILL.md" \
    "hello from machine A v2"

  echo "    B: use links the pulled library (agents-only)"
  mkdir -p "$PROJECT_B"
  local b_use
  b_use="$(run_b use "$SKILL_NAME" --project "$PROJECT_B" 2>&1)"
  echo "$b_use"
  skl_assert_contains "$b_use" "using $SKILL_NAME"
  skl_assert_symlink_to \
    "$PROJECT_B/.agents/skills/${SKILL_NAME}" \
    "$MACHINE_B/.claude/skills/${SKILL_NAME}"
  if [[ -e "$PROJECT_B/.claude" || -e "$PROJECT_B/.cursor" ]]; then
    echo "default use must not create .claude/.cursor" >&2
    ls -la "$PROJECT_B" >&2 || true
    exit 1
  fi
  skl_assert_file_contains \
    "$PROJECT_B/.agents/skills/${SKILL_NAME}/SKILL.md" \
    "hello from machine A v2"

  echo "OK: Dual-HOME library updated without manual skl sync"
}

# --- 2. Throttle -----------------------------------------------------------

smoke_throttle() {
  echo "==> [2] Throttle: two rapid verbs within 15m → one network sync"

  local home="$WORKDIR/machine-throttle"
  local project="$PROJECT_THROTTLE"
  mkdir -p "$home" "$project"
  seed_skill "$home" "$SKILL_NAME" "# ${SKILL_NAME}

throttle seed
"
  prepare_machine "$home" "$TOKEN_A"

  echo "    init (due) → one POST"
  local first
  first="$(run_home "$home" "$TOKEN_A" init 2>&1)"
  echo "$first"
  skl_require_auto_sync_hooks "$first"
  local first_posts
  first_posts="$(skl_count_sync_posts "$first")"
  if [[ "$first_posts" -lt 1 ]]; then
    echo "expected first due verb to POST /v1/sync" >&2
    exit 1
  fi

  echo "    status immediately → no POST"
  local second
  second="$(run_home "$home" "$TOKEN_A" status 2>&1)"
  echo "$second"
  skl_assert_sync_posts "$second" 0
  skl_assert_contains "$second" "last_sync"

  echo "    use immediately → no POST (link still succeeds)"
  local third
  third="$(run_home "$home" "$TOKEN_A" use "$SKILL_NAME" --project "$project" 2>&1)"
  echo "$third"
  skl_assert_sync_posts "$third" 0
  skl_assert_contains "$third" "using $SKILL_NAME"
  skl_assert_symlink_to \
    "$project/.agents/skills/${SKILL_NAME}" \
    "$home/.claude/skills/${SKILL_NAME}"

  echo "    doctor → no POST /v1/sync; display last_sync only"
  local doctor
  doctor="$(run_home "$home" "$TOKEN_A" doctor 2>&1)"
  echo "$doctor"
  skl_assert_sync_posts "$doctor" 0
  skl_assert_contains "$doctor" "last_sync"
  if [[ "$doctor" == *"POST $API/v1/sync"* ]]; then
    echo "doctor must not piggyback hash sync" >&2
    exit 1
  fi

  echo "OK: throttle — only the first due verb hit the network"
}

# --- 3. Fail-soft ----------------------------------------------------------

smoke_fail_soft() {
  echo "==> [3] Fail-soft: API down during use → link ok; status shows issue"

  local home="$WORKDIR/machine-fail"
  local project="$PROJECT_FAIL"
  mkdir -p "$home" "$project"
  seed_skill "$home" "$SKILL_NAME" "# ${SKILL_NAME}

fail-soft local skill
"
  prepare_machine "$home" "$TOKEN_A"
  local primed
  primed="$(run_home "$home" "$TOKEN_A" init 2>&1)"
  echo "$primed"
  skl_require_auto_sync_hooks "$primed"
  skl_age_auto_sync "$home" "$AGE_SECS"

  echo "    use against dead API (due) — verb must succeed"
  local use_out use_status
  set +e
  use_out="$(
    API="$DEAD_API" SKL_TOKEN="$TOKEN_A" skl_run "$home" \
      --api-base "$DEAD_API" use "$SKILL_NAME" --project "$project" 2>&1
  )"
  use_status=$?
  set -e
  echo "$use_out"
  if [[ "$use_status" -ne 0 ]]; then
    echo "use must fail-soft when auto-sync cannot reach the API (exit $use_status)" >&2
    exit 1
  fi
  skl_assert_contains "$use_out" "using $SKILL_NAME"
  skl_assert_symlink_to \
    "$project/.agents/skills/${SKILL_NAME}" \
    "$home/.claude/skills/${SKILL_NAME}"
  skl_assert_file_contains \
    "$project/.agents/skills/${SKILL_NAME}/SKILL.md" \
    "fail-soft local skill"
  skl_assert_contains "$use_out" "auto-sync (use):"
  skl_assert_contains "$use_out" "(ignored)"

  echo "    age due again so status is not attempt-throttled"
  skl_age_auto_sync "$home" "$AGE_SECS"

  echo "    status against dead API — exit 0, last_sync + sync issue"
  local status_out status_rc
  set +e
  status_out="$(
    API="$DEAD_API" SKL_TOKEN="$TOKEN_A" skl_run "$home" \
      --api-base "$DEAD_API" status 2>&1
  )"
  status_rc=$?
  set -e
  echo "$status_out"
  if [[ "$status_rc" -ne 0 ]]; then
    echo "status must stay best-effort (exit $status_rc)" >&2
    exit 1
  fi
  skl_assert_contains "$status_out" "last_sync"
  skl_assert_sync_issue "${use_out}"$'\n'"${status_out}"

  echo "OK: fail-soft use linked; status reported the sync issue"
}

# --- run -------------------------------------------------------------------

skl_start_api
skl_require_bin
skl_wait_for_api

mkdir -p "$MACHINE_A" "$MACHINE_B" "$PROJECT_B" "$PROJECT_THROTTLE" "$PROJECT_FAIL"

smoke_dual_home
smoke_throttle
smoke_fail_soft

echo "OK: auto-sync Dual-HOME + throttle + fail-soft against $API"
