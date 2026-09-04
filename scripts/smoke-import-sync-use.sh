#!/usr/bin/env bash
# Two-machine happy path against cipher's API on localhost:8787:
#   import (skl init) → sync → skl use
#
# Same Clerk-dev user, two HOMEs (two devices). A clash is per-user, not
# cross-user — use the same SKL_TOKEN (or SKL_TOKEN_A/B both set to the
# same ALLOW_DEV_AUTH id). Distinct tokens are different users and will
# not share skills.
#
# Clash / keep-local / scrub coverage stays in scripts/smoke-clash.sh
# (conflict/scrub PR). This harness does not rewrite furnace sync/use.
#
# Prerequisites (or START_API=1 to boot them here):
#   - apps/api listening on :8787
#   - ALLOW_DEV_AUTH=true (or CLERK_SECRET_KEY unset)
#   - Postgres reachable as DATABASE_URL (see apps/api/.env.example)
#
# Usage:
#   cargo build -p skl
#   ./scripts/smoke-import-sync-use.sh
#   START_API=1 ./scripts/smoke-import-sync-use.sh
#   START_API=1 SKIP_DOCKER=1 ./scripts/smoke-import-sync-use.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=smoke-lib.sh
source "$ROOT/scripts/smoke-lib.sh"

skl_smoke_defaults
MACHINE_A="$WORKDIR/machine-a"
MACHINE_B="$WORKDIR/machine-b"
PROJECT_B="$WORKDIR/project-b"
# Same user on both machines. Override SKL_TOKEN_A / SKL_TOKEN_B only if
# they map to the same ALLOW_DEV_AUTH clerk id.
TOKEN_A="${SKL_TOKEN_A:-$TOKEN}"
TOKEN_B="${SKL_TOKEN_B:-$TOKEN}"
SKILL_BODY=$'# greeter\n\nhello from machine A\n'

trap skl_smoke_cleanup EXIT

seed_skill() {
  local home="$1"
  local name="$2"
  local body="$3"
  mkdir -p "$home/.claude/skills/$name"
  printf '%s' "$body" > "$home/.claude/skills/$name/SKILL.md"
}

run_a() {
  SKL_TOKEN="$TOKEN_A" skl_run "$MACHINE_A" "$@"
}

run_b() {
  SKL_TOKEN="$TOKEN_B" skl_run "$MACHINE_B" "$@"
}

skl_start_api
skl_require_bin
skl_wait_for_api

mkdir -p "$MACHINE_A" "$MACHINE_B" "$PROJECT_B"
# Machine B needs an existing pull root so sync writes into ~/.claude/skills.
mkdir -p "$MACHINE_B/.claude/skills"
seed_skill "$MACHINE_A" "greeter" "$SKILL_BODY"

echo "==> machine A: import (skl init)"
a_init="$(run_a init 2>&1)"
echo "$a_init"
skl_assert_contains "$a_init" "Imported 1 skill"
skl_assert_contains "$a_init" "greeter"

echo "==> machine A: sync (first upload)"
a_sync="$(run_a sync 2>&1)"
echo "$a_sync"
skl_assert_contains "$a_sync" "POST $API/v1/sync"
skl_assert_contains "$a_sync" "PUT /v1/blobs/"
skl_assert_contains "$a_sync" "PUT /v1/skills/greeter/tree"
skl_assert_contains "$a_sync" "sync done"
skl_assert_contains "$a_sync" "conflicts=0"

echo "==> machine A: list sees remote"
a_list="$(run_a list 2>&1)"
echo "$a_list"
skl_assert_contains "$a_list" "greeter"
skl_assert_contains "$a_list" "yes"

echo "==> machine B: import empty home, then sync (download)"
b_init="$(run_b init 2>&1)"
echo "$b_init"
skl_assert_contains "$b_init" "Imported 0 skill"

b_sync="$(run_b sync 2>&1)"
echo "$b_sync"
skl_assert_contains "$b_sync" "POST $API/v1/sync"
skl_assert_contains "$b_sync" "GET /v1/blobs/"
skl_assert_contains "$b_sync" "wrote skill greeter"
skl_assert_contains "$b_sync" "sync done"
skl_assert_file_contains "$MACHINE_B/.claude/skills/greeter/SKILL.md" "hello from machine A"

echo "==> machine B: list after pull"
b_list="$(run_b list 2>&1)"
echo "$b_list"
skl_assert_contains "$b_list" "greeter"
skl_assert_contains "$b_list" "yes"

echo "==> machine B: skl use greeter → project symlinks"
b_use="$(run_b use greeter --project "$PROJECT_B" 2>&1)"
echo "$b_use"
skl_assert_contains "$b_use" "using greeter"
skl_assert_contains "$b_use" "updated"

claude_link="$PROJECT_B/.claude/skills/greeter"
cursor_link="$PROJECT_B/.cursor/skills/greeter"
home_skill="$MACHINE_B/.claude/skills/greeter"
skl_assert_symlink_to "$claude_link" "$home_skill"
skl_assert_symlink_to "$cursor_link" "$home_skill"
skl_assert_file_contains "$claude_link/SKILL.md" "hello from machine A"
skl_assert_file_contains "$PROJECT_B/skills.toml" "greeter"
skl_assert_file_contains "$PROJECT_B/skills.toml" "symlink"

echo "==> machine B: skl use (list activated)"
b_used="$(run_b use --project "$PROJECT_B" 2>&1)"
echo "$b_used"
skl_assert_contains "$b_used" "greeter"
skl_assert_contains "$b_used" "symlink"

echo "==> machine B: skl unuse greeter"
b_unuse="$(run_b unuse greeter --project "$PROJECT_B" 2>&1)"
echo "$b_unuse"
if [[ -e "$claude_link" || -L "$claude_link" ]]; then
  echo "expected $claude_link to be removed" >&2
  ls -la "$PROJECT_B/.claude/skills" >&2 || true
  exit 1
fi
if [[ -e "$cursor_link" || -L "$cursor_link" ]]; then
  echo "expected $cursor_link to be removed" >&2
  exit 1
fi

echo "OK: import → sync → use against $API (A=$TOKEN_A B=$TOKEN_B)"
