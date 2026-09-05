#!/usr/bin/env bash
# Capture smoke stacked on furnace `skl capture` (do not invent a second verb).
#
# Personal library is `{SKL_DATA_DIR}/skills/<name>/` (default
# `~/.local/share/skl/skills/`). `.agents/skills` is the project link dest —
# it is **not** the library.
#
#   1. write project skill → capture → sync machine B → skl use elsewhere
#   2. already-symlink to library → no-op
#   3. name clash fail / --force / --as
#   4. --keep-copy
#   5. fail-soft: API down still promotes+links
#   6. non-TTY: no prompts; clash needs --force / --as
#
# Usage:
#   cargo build -p skl
#   ./scripts/smoke-capture.sh
#   START_API=1 ./scripts/smoke-capture.sh
#   START_API=1 SKIP_DOCKER=1 ./scripts/smoke-capture.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=smoke-lib.sh
source "$ROOT/scripts/smoke-lib.sh"

skl_smoke_defaults
TOKEN="${SKL_TOKEN:-dev:smoke-capture-$$}"
TOKEN_A="${SKL_TOKEN_A:-$TOKEN}"
TOKEN_B="${SKL_TOKEN_B:-$TOKEN}"
SKILL_NAME="${SKL_SMOKE_SKILL:-cap-$$}"
DEAD_API="${SKL_SMOKE_DEAD_API:-http://127.0.0.1:1}"

MACHINE_A="$WORKDIR/machine-a"
MACHINE_B="$WORKDIR/machine-b"
PROJECT_A="$WORKDIR/project-a"
PROJECT_ELSEWHERE="$WORKDIR/project-elsewhere"

trap skl_smoke_cleanup EXIT

# --- helpers ---------------------------------------------------------------

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

login_home() {
  local home="$1"
  local token="$2"
  mkdir -p "$home/.config/skl" "$home/.local/share/skl"
  run_home "$home" "$token" login --dev-user "$(dev_user "$token")" >/dev/null
}

library_of() {
  local home="$1"
  local name="$2"
  printf '%s' "$home/.local/share/skl/skills/$name"
}

plant_project_skill() {
  local project="$1"
  local name="$2"
  local body="$3"
  mkdir -p "$project/.agents/skills/$name"
  printf '%s' "$body" >"$project/.agents/skills/$name/SKILL.md"
}

plant_library_skill() {
  local home="$1"
  local name="$2"
  local body="$3"
  local dest
  dest="$(library_of "$home" "$name")"
  mkdir -p "$dest"
  printf '%s' "$body" >"$dest/SKILL.md"
}

assert_not_contains() {
  local haystack="$1"
  local needle="$2"
  if [[ "$haystack" == *"$needle"* ]]; then
    echo "expected output not to contain: $needle" >&2
    echo "got:" >&2
    echo "$haystack" >&2
    exit 1
  fi
}

assert_no_prompt() {
  local haystack="$1"
  assert_not_contains "$haystack" "keep [l]ocal"
  assert_not_contains "$haystack" "overwrite? "
  assert_not_contains "$haystack" "[y/n]"
  assert_not_contains "$haystack" "Overwrite ("
}

assert_real_dir() {
  local path="$1"
  if [[ -L "$path" ]]; then
    echo "expected real directory, got symlink: $path -> $(readlink "$path")" >&2
    exit 1
  fi
  if [[ ! -d "$path" ]]; then
    echo "expected directory: $path" >&2
    ls -la "$(dirname "$path")" >&2 || true
    exit 1
  fi
}

assert_library_not_home_agents() {
  local home="$1"
  local name="$2"
  local lib
  lib="$(library_of "$home" "$name")"
  if [[ ! -d "$lib" ]]; then
    echo "expected personal library at $lib (SKL_DATA_DIR/skills)" >&2
    exit 1
  fi
  case "$lib" in
    */.agents/skills/*)
      echo "personal library must not be ~/.agents/skills: $lib" >&2
      exit 1
      ;;
  esac
  if [[ -e "$home/.agents/skills/$name" ]]; then
    echo "capture must not write the library under ~/.agents/skills ($home/.agents/skills/$name)" >&2
    ls -la "$home/.agents/skills" >&2 || true
    exit 1
  fi
}

# --- 1. Dual-HOME ----------------------------------------------------------

smoke_dual_home() {
  echo "==> [1] write project skill → capture → sync B → use elsewhere"

  mkdir -p "$MACHINE_A" "$MACHINE_B" "$PROJECT_A" "$PROJECT_ELSEWHERE"
  # Explicit `skl sync` — do not let capture's maybe_run steal the first PUT.
  skl_write_sync_prefs "$MACHINE_A" false 900
  skl_write_sync_prefs "$MACHINE_B" false 900
  login_home "$MACHINE_A" "$TOKEN_A"
  login_home "$MACHINE_B" "$TOKEN_B"
  # B pull root (furnace writes missing skills here; not the personal library).
  mkdir -p "$MACHINE_B/.claude/skills"

  plant_project_skill "$PROJECT_A" "$SKILL_NAME" "# ${SKILL_NAME}

hello from captured project
"

  echo "    A: skl capture (promote into ~/.local/share/skl/skills)"
  local a_cap
  a_cap="$(run_a capture ".agents/skills/${SKILL_NAME}" --project "$PROJECT_A" 2>&1)"
  echo "$a_cap"
  skl_assert_contains "$a_cap" "captured $SKILL_NAME"
  assert_no_prompt "$a_cap"

  local lib_a project_skill
  lib_a="$(library_of "$MACHINE_A" "$SKILL_NAME")"
  project_skill="$PROJECT_A/.agents/skills/${SKILL_NAME}"
  assert_library_not_home_agents "$MACHINE_A" "$SKILL_NAME"
  skl_assert_file_contains "$lib_a/SKILL.md" "hello from captured project"
  skl_assert_symlink_to "$project_skill" "$lib_a"
  skl_assert_file_contains "$project_skill/SKILL.md" "hello from captured project"

  echo "    A: sync (upload captured library skill)"
  local a_sync
  a_sync="$(run_a sync 2>&1)"
  echo "$a_sync"
  skl_assert_contains "$a_sync" "POST $API/v1/sync"
  skl_assert_contains "$a_sync" "PUT /v1/blobs/"
  skl_assert_contains "$a_sync" "PUT /v1/skills/${SKILL_NAME}/tree"
  skl_assert_contains "$a_sync" "sync done"
  skl_assert_contains "$a_sync" "conflicts=0"

  echo "    B: init empty, then sync (download)"
  local b_init
  b_init="$(run_b init 2>&1)"
  echo "$b_init"
  skl_assert_contains "$b_init" "Imported 0 skill"

  local b_sync
  b_sync="$(run_b sync 2>&1)"
  echo "$b_sync"
  skl_assert_contains "$b_sync" "POST $API/v1/sync"
  skl_assert_contains "$b_sync" "GET /v1/blobs/"
  skl_assert_contains "$b_sync" "wrote skill $SKILL_NAME"
  skl_assert_contains "$b_sync" "sync done"
  skl_assert_file_contains "$MACHINE_B/.claude/skills/${SKILL_NAME}/SKILL.md" \
    "hello from captured project"

  echo "    B: skl use in a different project"
  local b_use
  b_use="$(run_b use "$SKILL_NAME" --project "$PROJECT_ELSEWHERE" 2>&1)"
  echo "$b_use"
  skl_assert_contains "$b_use" "using $SKILL_NAME"
  skl_assert_symlink_to \
    "$PROJECT_ELSEWHERE/.agents/skills/${SKILL_NAME}" \
    "$MACHINE_B/.claude/skills/${SKILL_NAME}"
  if [[ -e "$PROJECT_ELSEWHERE/.claude" || -e "$PROJECT_ELSEWHERE/.cursor" ]]; then
    echo "default use must not create .claude/.cursor" >&2
    ls -la "$PROJECT_ELSEWHERE" >&2 || true
    exit 1
  fi
  skl_assert_file_contains \
    "$PROJECT_ELSEWHERE/.agents/skills/${SKILL_NAME}/SKILL.md" \
    "hello from captured project"

  echo "OK: capture → sync B → use elsewhere (library=$lib_a)"
}

# --- 2. Already-symlink no-op ----------------------------------------------

smoke_noop() {
  echo "==> [2] already-symlink to library → no-op"

  local home="$WORKDIR/machine-noop"
  local project="$WORKDIR/project-noop"
  local name="${SKILL_NAME}-noop"
  mkdir -p "$home" "$project"
  skl_write_sync_prefs "$home" false 900
  plant_project_skill "$project" "$name" "# ${name}

noop seed
"

  local first
  first="$(run_home "$home" "$TOKEN_A" capture ".agents/skills/${name}" --project "$project" 2>&1)"
  echo "$first"
  skl_assert_contains "$first" "captured $name"
  local lib project_skill
  lib="$(library_of "$home" "$name")"
  project_skill="$project/.agents/skills/${name}"
  assert_library_not_home_agents "$home" "$name"
  skl_assert_symlink_to "$project_skill" "$lib"

  local again
  again="$(run_home "$home" "$TOKEN_A" capture ".agents/skills/${name}" --project "$project" 2>&1)"
  echo "$again"
  skl_assert_contains "$again" "already linked"
  assert_no_prompt "$again"
  skl_assert_symlink_to "$project_skill" "$lib"
  skl_assert_file_contains "$lib/SKILL.md" "noop seed"
  assert_library_not_home_agents "$home" "$name"

  echo "OK: second capture was a no-op"
}

# --- 3. Clash / --force / --as ---------------------------------------------

smoke_clash_force_as() {
  echo "==> [3] name clash fail / --force / --as"

  local home="$WORKDIR/machine-clash"
  local project="$WORKDIR/project-clash"
  local name="${SKILL_NAME}-clash"
  mkdir -p "$home" "$project"
  skl_write_sync_prefs "$home" false 900
  plant_library_skill "$home" "$name" "# library

old library body
"
  plant_project_skill "$project" "$name" "# project

new project body
"

  echo "    clash without flags must fail"
  local clash_out clash_rc
  set +e
  clash_out="$(
    run_home "$home" "$TOKEN_A" capture ".agents/skills/${name}" --project "$project" 2>&1
  )"
  clash_rc=$?
  set -e
  echo "$clash_out"
  if [[ "$clash_rc" -eq 0 ]]; then
    echo "expected name clash without --force/--as to fail" >&2
    exit 1
  fi
  skl_assert_contains "$clash_out" "already exists"
  skl_assert_contains "$clash_out" "--force"
  skl_assert_contains "$clash_out" "--as"
  assert_no_prompt "$clash_out"
  skl_assert_file_contains "$(library_of "$home" "$name")/SKILL.md" "old library body"
  skl_assert_file_contains "$project/.agents/skills/${name}/SKILL.md" "new project body"
  assert_real_dir "$project/.agents/skills/${name}"

  echo "    --force overwrites the library and links"
  local force_out
  force_out="$(
    run_home "$home" "$TOKEN_A" capture ".agents/skills/${name}" --force --project "$project" 2>&1
  )"
  echo "$force_out"
  skl_assert_contains "$force_out" "captured $name"
  skl_assert_contains "$force_out" "overwrote library"
  assert_no_prompt "$force_out"
  skl_assert_file_contains "$(library_of "$home" "$name")/SKILL.md" "new project body"
  skl_assert_symlink_to "$project/.agents/skills/${name}" "$(library_of "$home" "$name")"
  assert_library_not_home_agents "$home" "$name"

  echo "    --as writes a new library name and leaves the clash name"
  local as_project="$WORKDIR/project-as"
  local as_name="${name}-notes"
  plant_library_skill "$home" "$name" "# library

clash again
"
  plant_project_skill "$as_project" "$name" "# notes

renamed body
"
  local as_out
  as_out="$(
    run_home "$home" "$TOKEN_A" capture ".agents/skills/${name}" --as "$as_name" --project "$as_project" 2>&1
  )"
  echo "$as_out"
  skl_assert_contains "$as_out" "captured $as_name"
  assert_no_prompt "$as_out"
  skl_assert_file_contains "$(library_of "$home" "$as_name")/SKILL.md" "renamed body"
  skl_assert_file_contains "$(library_of "$home" "$name")/SKILL.md" "clash again"
  skl_assert_symlink_to "$as_project/.agents/skills/${name}" "$(library_of "$home" "$as_name")"
  assert_library_not_home_agents "$home" "$as_name"

  echo "OK: clash errors; --force overwrites; --as renames"
}

# --- 4. --keep-copy --------------------------------------------------------

smoke_keep_copy() {
  echo "==> [4] --keep-copy leaves the project as a real directory"

  local home="$WORKDIR/machine-keep"
  local project="$WORKDIR/project-keep"
  local name="${SKILL_NAME}-keep"
  mkdir -p "$home" "$project"
  skl_write_sync_prefs "$home" false 900
  plant_project_skill "$project" "$name" "# ${name}

keep-copy body
"

  local out
  out="$(
    run_home "$home" "$TOKEN_A" capture ".agents/skills/${name}" --keep-copy --project "$project" 2>&1
  )"
  echo "$out"
  skl_assert_contains "$out" "captured $name"
  skl_assert_contains "$out" "kept project copy"
  assert_no_prompt "$out"

  local lib project_skill
  lib="$(library_of "$home" "$name")"
  project_skill="$project/.agents/skills/${name}"
  assert_library_not_home_agents "$home" "$name"
  assert_real_dir "$project_skill"
  skl_assert_file_contains "$project_skill/SKILL.md" "keep-copy body"
  skl_assert_file_contains "$lib/SKILL.md" "keep-copy body"

  echo "OK: --keep-copy promoted without replacing the project dir"
}

# --- 5. Fail-soft ----------------------------------------------------------

smoke_fail_soft() {
  echo "==> [5] fail-soft: API down still promotes+links"

  local home="$WORKDIR/machine-fail"
  local project="$WORKDIR/project-fail"
  local name="${SKILL_NAME}-fail"
  mkdir -p "$home" "$project"
  plant_project_skill "$project" "$name" "# ${name}

fail-soft capture
"
  login_home "$home" "$TOKEN_A"
  skl_write_sync_prefs "$home" true 900

  echo "    capture against dead API (due) — verb must succeed"
  local cap_out cap_rc
  set +e
  cap_out="$(
    API="$DEAD_API" SKL_TOKEN="$TOKEN_A" skl_run "$home" \
      --api-base "$DEAD_API" capture ".agents/skills/${name}" --project "$project" 2>&1
  )"
  cap_rc=$?
  set -e
  echo "$cap_out"
  if [[ "$cap_rc" -ne 0 ]]; then
    echo "capture must fail-soft when auto-sync cannot reach the API (exit $cap_rc)" >&2
    exit 1
  fi
  skl_assert_contains "$cap_out" "captured $name"
  skl_assert_contains "$cap_out" "auto-sync (capture):"
  skl_assert_contains "$cap_out" "(ignored)"
  assert_no_prompt "$cap_out"

  local lib project_skill
  lib="$(library_of "$home" "$name")"
  project_skill="$project/.agents/skills/${name}"
  assert_library_not_home_agents "$home" "$name"
  skl_assert_file_contains "$lib/SKILL.md" "fail-soft capture"
  skl_assert_symlink_to "$project_skill" "$lib"

  echo "OK: dead API did not block promote+link"
}

# --- 6. Non-TTY ------------------------------------------------------------

smoke_non_tty() {
  echo "==> [6] non-TTY: no prompts; clash needs flags"

  local home="$WORKDIR/machine-ntty"
  local project="$WORKDIR/project-ntty"
  local name="${SKILL_NAME}-ntty"
  mkdir -p "$home" "$project"
  skl_write_sync_prefs "$home" false 900
  plant_library_skill "$home" "$name" "# library

ntty library
"
  plant_project_skill "$project" "$name" "# project

ntty project
"

  local out rc
  set +e
  out="$(
    SKL_TOKEN="$TOKEN_A" skl_run "$home" \
      capture ".agents/skills/${name}" --project "$project" </dev/null 2>&1
  )"
  rc=$?
  set -e
  echo "$out"
  if [[ "$rc" -eq 0 ]]; then
    echo "non-TTY clash must fail without --force/--as" >&2
    exit 1
  fi
  skl_assert_contains "$out" "already exists"
  skl_assert_contains "$out" "--force"
  skl_assert_contains "$out" "--as"
  assert_no_prompt "$out"
  skl_assert_file_contains "$(library_of "$home" "$name")/SKILL.md" "ntty library"
  assert_real_dir "$project/.agents/skills/${name}"

  echo "OK: non-TTY clash failed cleanly (flags required, no prompt)"
}

# --- run -------------------------------------------------------------------

skl_start_api
skl_require_bin
skl_wait_for_api

mkdir -p "$WORKDIR"

smoke_dual_home
smoke_noop
smoke_clash_force_as
smoke_keep_copy
smoke_fail_soft
smoke_non_tty

echo "OK: capture Dual-HOME + no-op + clash/force/as + keep-copy + fail-soft + non-TTY against $API"
