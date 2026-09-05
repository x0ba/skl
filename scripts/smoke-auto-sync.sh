#!/usr/bin/env bash
# Dual-HOME auto-sync smoke stacked on furnace `maybe_run` (no daemon).
# Never calls `skl sync` on the mutate / pull / use path.
#
#   1. Machine A mutates library (new skill) → age due → A init (push) →
#      B status (pull) → skill present in B library without manual `skl sync`.
#      Piggyback is KeepRemote: same-slug overwrites after upload conflict
#      and restore remote, so this path publishes *new* slugs.
#   2. Two rapid verbs within 15m → only one POST /v1/sync
#   3. API down during `use` → link succeeds; `status` shows sync_issue
#
# Usage:
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
# Default furnace frequency is 900s; age past that so maybe_run is due.
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
  printf '%s' "$body" >"$home/.claude/skills/$name/SKILL.md"
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
  echo "==> [1] Dual-HOME: A mutate → due → B verb → library (no skl sync)"

  # KeepRemote cannot push a same-slug edit after the first upload (API
  # treats local≠remote tree hash as a conflict and restores remote). A
  # new slug is missing_skills / local-only — piggyback can publish it.
  local skill_more="${SKILL_NAME}-more"

  seed_skill "$MACHINE_A" "$SKILL_NAME" "# ${SKILL_NAME}

hello from machine A
"
  prepare_machine "$MACHINE_A" "$TOKEN_A"

  echo "    A: init (import + due maybe_run upload)"
  local a_init
  a_init="$(run_a init 2>&1)"
  echo "$a_init"
  skl_assert_contains "$a_init" "Imported 1 skill"
  skl_assert_contains "$a_init" "POST $API/v1/sync"

  echo "    B: init empty home (import 0 + due maybe_run download)"
  prepare_machine "$MACHINE_B" "$TOKEN_B"
  local b_init
  b_init="$(run_b init 2>&1)"
  echo "$b_init"
  skl_assert_contains "$b_init" "Imported 0 skill"
  skl_assert_contains "$b_init" "POST $API/v1/sync"
  skl_assert_file_contains "$MACHINE_B/.claude/skills/${SKILL_NAME}/SKILL.md" \
    "hello from machine A"

  echo "    A: mutate library (new skill, not a same-slug overwrite)"
  seed_skill "$MACHINE_A" "$skill_more" "# ${skill_more}

second skill from machine A
"

  echo "    A: age due, then init (refresh index + piggyback push)"
  skl_age_auto_sync "$MACHINE_A" "$AGE_SECS"
  local a_push
  a_push="$(run_a init 2>&1)"
  echo "$a_push"
  skl_assert_contains "$a_push" "Imported 2 skill"
  skl_assert_contains "$a_push" "POST $API/v1/sync"

  echo "    B: age due, then status (best-effort sync — not display-only)"
  skl_age_auto_sync "$MACHINE_B" "$AGE_SECS"
  local b_status
  b_status="$(run_b status 2>&1)"
  echo "$b_status"
  skl_assert_contains "$b_status" "POST $API/v1/sync"
  skl_assert_contains "$b_status" "auto_sync    on"
  skl_assert_contains "$b_status" "sync_frequency 900s"
  skl_assert_contains "$b_status" "last_sync"
  skl_assert_file_contains "$MACHINE_B/.claude/skills/${SKILL_NAME}/SKILL.md" \
    "hello from machine A"
  skl_assert_file_contains "$MACHINE_B/.claude/skills/${skill_more}/SKILL.md" \
    "second skill from machine A"

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
    "hello from machine A"

  echo "OK: Dual-HOME library updated without manual skl sync"
}

# --- 2. Throttle -----------------------------------------------------------

smoke_throttle() {
  echo "==> [2] Throttle: two rapid verbs within 15m → one network sync"

  local home="$WORKDIR/machine-throttle"
  local project="$PROJECT_THROTTLE"
  mkdir -p "$home" "$project"
  local skill="${SKILL_NAME}-throttle"
  seed_skill "$home" "$skill" "# ${skill}

throttle seed
"
  prepare_machine "$home" "$TOKEN_A"

  echo "    init (due) → one POST"
  local first
  first="$(run_home "$home" "$TOKEN_A" init 2>&1)"
  echo "$first"
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
  skl_assert_contains "$second" "auto_sync    on"
  skl_assert_contains "$second" "sync_frequency 900s"

  echo "    use immediately → no POST (link still succeeds)"
  local third
  third="$(run_home "$home" "$TOKEN_A" use "$skill" --project "$project" 2>&1)"
  echo "$third"
  skl_assert_sync_posts "$third" 0
  skl_assert_contains "$third" "using $skill"
  skl_assert_symlink_to \
    "$project/.agents/skills/${skill}" \
    "$home/.claude/skills/${skill}"

  echo "    doctor → no POST /v1/sync; display last_sync only"
  local doctor
  doctor="$(run_home "$home" "$TOKEN_A" doctor 2>&1)"
  echo "$doctor"
  skl_assert_sync_posts "$doctor" 0
  skl_assert_contains "$doctor" "last_sync"

  echo "OK: throttle — only the first due verb hit the network"
}

# --- 3. Fail-soft ----------------------------------------------------------

smoke_fail_soft() {
  echo "==> [3] Fail-soft: API down during use → link ok; status shows issue"

  local home="$WORKDIR/machine-fail"
  local project="$PROJECT_FAIL"
  mkdir -p "$home" "$project"
  # Unique slug so KeepRemote does not restore a skill published earlier
  # in this run (same ALLOW_DEV_AUTH user / remote library).
  local skill="${SKILL_NAME}-fail"
  seed_skill "$home" "$skill" "# ${skill}

fail-soft local skill
"
  prepare_machine "$home" "$TOKEN_A"
  local primed
  primed="$(run_home "$home" "$TOKEN_A" init 2>&1)"
  echo "$primed"
  skl_age_auto_sync "$home" "$AGE_SECS"

  echo "    use against dead API (due) — verb must succeed"
  local use_out use_status
  set +e
  use_out="$(
    API="$DEAD_API" SKL_TOKEN="$TOKEN_A" skl_run "$home" \
      --api-base "$DEAD_API" use "$skill" --project "$project" 2>&1
  )"
  use_status=$?
  set -e
  echo "$use_out"
  if [[ "$use_status" -ne 0 ]]; then
    echo "use must fail-soft when auto-sync cannot reach the API (exit $use_status)" >&2
    exit 1
  fi
  skl_assert_contains "$use_out" "using $skill"
  skl_assert_contains "$use_out" "auto-sync (use):"
  skl_assert_contains "$use_out" "(ignored)"
  skl_assert_symlink_to \
    "$project/.agents/skills/${skill}" \
    "$home/.claude/skills/${skill}"
  skl_assert_file_contains \
    "$project/.agents/skills/${skill}/SKILL.md" \
    "fail-soft local skill"

  echo "    status against dead API — exit 0, last_sync + sync_issue"
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
  skl_assert_contains "$status_out" "auto_sync    on"
  skl_assert_contains "$status_out" "sync_frequency 900s"
  skl_assert_contains "$status_out" "sync_issue"

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
