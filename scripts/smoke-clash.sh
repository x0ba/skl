#!/usr/bin/env bash
# Live clash + scrub smoke against cipher's API on localhost:8787.
#
# Same Clerk-dev user on two machines (a clash is per-user, not cross-user).
# Shares helpers with scripts/smoke-import-sync-use.sh via smoke-lib.sh.
# Prerequisites (or START_API=1 to boot them here):
#   - apps/api listening on :8787
#   - ALLOW_DEV_AUTH=true (or CLERK_SECRET_KEY unset)
#   - Postgres reachable as DATABASE_URL (see apps/api/.env.example)
#
# Usage (crate path crates/cli; package name skl):
#   cargo build -p skl
#   ./scripts/smoke-clash.sh
#   START_API=1 ./scripts/smoke-clash.sh
#   START_API=1 SKIP_DOCKER=1 ./scripts/smoke-clash.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=smoke-lib.sh
source "$ROOT/scripts/smoke-lib.sh"

skl_smoke_defaults
MACHINE_A="$WORKDIR/machine-a"
MACHINE_B="$WORKDIR/machine-b"
# Same user: two devices, one skill name, divergent trees.
TOKEN="${SKL_TOKEN:-dev:alice}"
# Unique name so leftover clash-demo trees from a prior run do not collide.
SKILL_NAME="${SKL_SMOKE_SKILL:-clash-$$}"

trap skl_smoke_cleanup EXIT

seed_skill() {
  local home="$1"
  local body="$2"
  mkdir -p "$home/.claude/skills/$SKILL_NAME"
  printf '%s' "$body" > "$home/.claude/skills/$SKILL_NAME/SKILL.md"
}

skl_start_api
skl_require_bin
skl_wait_for_api

mkdir -p "$MACHINE_A" "$MACHINE_B"
seed_skill "$MACHINE_A" $'# Clash demo\n\nMachine A version of clash-demo.\n'
seed_skill "$MACHINE_B" $'# Clash demo\n\nMachine B version of clash-demo.\n'

echo "==> machine A: init + sync (first upload)"
skl_run "$MACHINE_A" init
a_out="$(skl_run "$MACHINE_A" sync 2>&1)"
echo "$a_out"
skl_assert_contains "$a_out" "POST $API/v1/sync"
skl_assert_contains "$a_out" "sync done"

echo "==> machine B: init + sync (non-interactive clash must fail)"
skl_run "$MACHINE_B" init
set +e
b_clash="$(skl_run "$MACHINE_B" sync 2>&1)"
b_status=$?
set -e
echo "$b_clash"
if [[ $b_status -eq 0 ]]; then
  echo "expected non-interactive clash to fail without --keep-local/--keep-remote" >&2
  exit 1
fi
skl_assert_contains "$b_clash" "conflict:"
skl_assert_contains "$b_clash" "non-interactive sync needs --keep-local or --keep-remote"

echo "==> machine B: --keep-local (PUT local tree, re-POST /v1/sync)"
b_keep="$(skl_run "$MACHINE_B" sync --keep-local 2>&1)"
echo "$b_keep"
skl_assert_contains "$b_keep" "keep-local: $SKILL_NAME"
skl_assert_contains "$b_keep" "PUT /v1/skills/${SKILL_NAME}/tree"
skl_assert_contains "$b_keep" "re-POST $API/v1/sync"
skl_assert_contains "$b_keep" "re-POST conflicts: 0"

echo "==> machine A: --keep-remote (overwrite local from remote, re-POST /v1/sync)"
a_keep="$(skl_run "$MACHINE_A" sync --keep-remote 2>&1)"
echo "$a_keep"
skl_assert_contains "$a_keep" "keep-remote: $SKILL_NAME"
skl_assert_contains "$a_keep" "re-POST conflicts: 0"
if ! grep -q 'Machine B version of clash-demo' "$MACHINE_A/.claude/skills/${SKILL_NAME}/SKILL.md"; then
  echo "machine A local file was not overwritten by keep-remote" >&2
  cat "$MACHINE_A/.claude/skills/${SKILL_NAME}/SKILL.md" >&2
  exit 1
fi

echo "==> scrub: dirty blob is not PUT"
# Split so casual secret scanners do not flag this script.
printf '%s\n' "-----BEGIN OPENSSH PRIVATE KEY-----" "fake" "-----END OPENSSH PRIVATE KEY-----" \
  > "$MACHINE_B/.claude/skills/${SKILL_NAME}/secret.env"
skl_run "$MACHINE_B" init
set +e
scrub_out="$(skl_run "$MACHINE_B" sync --keep-local 2>&1)"
scrub_status=$?
set -e
echo "$scrub_out"
if [[ $scrub_status -eq 0 ]]; then
  echo "expected scrub to block the dirty blob" >&2
  exit 1
fi
skl_assert_contains "$scrub_out" "upload blocked:"
skl_assert_contains "$scrub_out" "blocked"

echo "OK: clash keep-local / keep-remote + scrub against $API"
