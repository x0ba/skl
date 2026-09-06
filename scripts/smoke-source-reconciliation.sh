#!/usr/bin/env bash
# Hammer coverage for furnace DAN-15 source reconciliation.
# Consumes the locked model — no new product verbs.
#
#   1. init → skill in the personal library only (foreign home stays)
#   2. use → project link points at the library
#   3. Mutate library → sync B → use --all restores the projection
#      (no agent-dir merge UI)
#   4. Divergent real project copy → doctor warns (exit 0);
#      capture imports; use re-projects
#   5. Names-only skills.toml + sync + use --all on B still works
#   6. Sync never treats ~/.agents/skills / harness homes as peers
#
# Usage:
#   cargo build -p skl
#   ./scripts/smoke-source-reconciliation.sh
#   START_API=1 ./scripts/smoke-source-reconciliation.sh
#   START_API=1 SKIP_DOCKER=1 ./scripts/smoke-source-reconciliation.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=smoke-lib.sh
source "$ROOT/scripts/smoke-lib.sh"

skl_smoke_defaults
TOKEN="${SKL_TOKEN:-dev:smoke-dan15-$$}"
TOKEN_A="${SKL_TOKEN_A:-$TOKEN}"
TOKEN_B="${SKL_TOKEN_B:-$TOKEN}"
SKILL_NAME="${SKL_SMOKE_SKILL:-recon-$$}"
SKILL_BODY="# ${SKILL_NAME}

hello from machine A
"
MUTATED_BODY="# ${SKILL_NAME}

mutated on A library
"
DIVERGENT_BODY="# ${SKILL_NAME}

divergent project copy
"

MACHINE_A="$WORKDIR/machine-a"
MACHINE_B="$WORKDIR/machine-b"
PROJECT_A="$WORKDIR/project-a"
PROJECT_B="$WORKDIR/project-b"

trap skl_smoke_cleanup EXIT

run_home() {
  local home="$1"
  local token="$2"
  shift 2
  SKL_TOKEN="$token" skl_run_store "$home" "$@"
}

run_a() { run_home "$MACHINE_A" "$TOKEN_A" "$@"; }
run_b() { run_home "$MACHINE_B" "$TOKEN_B" "$@"; }

assert_harness_not_sync_peer() {
  local home="$1"
  local name="$2"
  skl_assert_absent "$home/.agents/skills/$name"
  skl_assert_absent "$home/.claude/skills/$name"
  skl_assert_absent "$home/.cursor/skills/$name"
  skl_assert_absent "$home/.config/agents/skills/$name"
}

seed_foreign() {
  local home="$1"
  local name="$2"
  local body="$3"
  mkdir -p "$home/.claude/skills/$name"
  printf '%s' "$body" >"$home/.claude/skills/$name/SKILL.md"
}

skl_start_api
skl_require_bin
skl_wait_for_api

mkdir -p "$MACHINE_A" "$MACHINE_B" "$PROJECT_A" "$PROJECT_B"
# Plant empty harness homes on B so a regression that writes peers is visible.
mkdir -p "$MACHINE_B/.agents/skills" "$MACHINE_B/.claude/skills"
skl_write_sync_prefs "$MACHINE_A" false 900
skl_write_sync_prefs "$MACHINE_B" false 900
skl_login_store "$MACHINE_A" "$TOKEN_A" >/dev/null
skl_login_store "$MACHINE_B" "$TOKEN_B" >/dev/null
seed_foreign "$MACHINE_A" "$SKILL_NAME" "$SKILL_BODY"

echo "==> [1] init → skill in library only"
a_init="$(run_a init 2>&1)"
echo "$a_init"
skl_assert_contains "$a_init" "Imported 1 skill"
skl_assert_contains "$a_init" "$SKILL_NAME"
skl_assert_contains "$a_init" "personal library only"
skl_assert_library_only "$MACHINE_A" "$SKILL_NAME"
skl_assert_file_contains "$(skl_library_of "$MACHINE_A" "$SKILL_NAME")/SKILL.md" \
  "hello from machine A"
# Foreign tree stays (importer, not a projection / not the library).
skl_assert_file_contains "$MACHINE_A/.claude/skills/${SKILL_NAME}/SKILL.md" \
  "hello from machine A"
skl_assert_no_merge_ui "$a_init"

echo "==> [2] use → link points at library"
a_use="$(run_a use "$SKILL_NAME" --project "$PROJECT_A" 2>&1)"
echo "$a_use"
skl_assert_contains "$a_use" "using $SKILL_NAME"
skl_assert_symlink_to \
  "$PROJECT_A/.agents/skills/${SKILL_NAME}" \
  "$(skl_library_of "$MACHINE_A" "$SKILL_NAME")"
skl_assert_portable_manifest "$PROJECT_A/skills.toml" "$MACHINE_A" '$HOME'
skl_assert_no_merge_ui "$a_use"
if [[ -e "$PROJECT_A/.claude" || -e "$PROJECT_A/.cursor" ]]; then
  echo "default use must not create .claude/.cursor" >&2
  exit 1
fi

echo "==> [5 prelude] clone names-only skills.toml onto project B"
cp "$PROJECT_A/skills.toml" "$PROJECT_B/skills.toml"
skl_assert_portable_manifest "$PROJECT_B/skills.toml" "$MACHINE_A" "$MACHINE_B"
skl_assert_absent "$PROJECT_B/.agents"

echo "==> [3] mutate library → sync B → use --all restores projection"
printf '%s' "$MUTATED_BODY" >"$(skl_library_of "$MACHINE_A" "$SKILL_NAME")/SKILL.md"
a_sync="$(run_a sync 2>&1)"
echo "$a_sync"
skl_assert_contains "$a_sync" "sync done"
skl_assert_contains "$a_sync" "conflicts=0"
skl_assert_no_merge_ui "$a_sync"

b_init="$(run_b init 2>&1)"
echo "$b_init"
skl_assert_contains "$b_init" "Imported 0 skill"

b_sync="$(run_b sync 2>&1)"
echo "$b_sync"
skl_assert_contains "$b_sync" "wrote skill $SKILL_NAME"
skl_assert_contains "$b_sync" "sync done"
skl_assert_file_contains "$(skl_library_of "$MACHINE_B" "$SKILL_NAME")/SKILL.md" \
  "mutated on A library"
skl_assert_library_only "$MACHINE_B" "$SKILL_NAME"
assert_harness_not_sync_peer "$MACHINE_B" "$SKILL_NAME"
skl_assert_absent "$PROJECT_B/.agents"
skl_assert_no_merge_ui "$b_sync"
if [[ "$b_sync" == *"$MACHINE_B/.agents/skills"* ]]; then
  echo "sync wrote/mentioned harness home as a peer: $b_sync" >&2
  exit 1
fi

echo "==> [5] names-only toml + use --all on B"
b_all="$(run_b use --all --project "$PROJECT_B" 2>&1)"
echo "$b_all"
skl_assert_contains "$b_all" "using $SKILL_NAME"
skl_assert_symlink_to \
  "$PROJECT_B/.agents/skills/${SKILL_NAME}" \
  "$(skl_library_of "$MACHINE_B" "$SKILL_NAME")"
skl_assert_file_contains \
  "$PROJECT_B/.agents/skills/${SKILL_NAME}/SKILL.md" \
  "mutated on A library"
skl_assert_portable_manifest "$PROJECT_B/skills.toml" \
  "$MACHINE_A" "$MACHINE_B" '$HOME'
skl_assert_no_merge_ui "$b_all"

echo "==> [4] divergent real project copy → doctor / capture / use"
rm -rf "$PROJECT_A/.agents/skills/${SKILL_NAME}"
mkdir -p "$PROJECT_A/.agents/skills/${SKILL_NAME}"
printf '%s' "$DIVERGENT_BODY" >"$PROJECT_A/.agents/skills/${SKILL_NAME}/SKILL.md"
if [[ -L "$PROJECT_A/.agents/skills/${SKILL_NAME}" ]]; then
  echo "expected a real project copy, still a symlink" >&2
  exit 1
fi

set +e
doc="$(skl_doctor_in "$MACHINE_A" "$PROJECT_A" 2>&1)"
doc_rc=$?
set -e
echo "$doc"
if [[ "$doc_rc" -ne 0 ]]; then
  echo "doctor must be warn-only (exit 0), got $doc_rc" >&2
  exit 1
fi
skl_assert_contains "$doc" "not a sync peer"
skl_assert_contains "$doc" "skl capture"
skl_assert_contains "$doc" "personal library is canonical"
# Doctor must not mutate the divergent copy.
skl_assert_file_contains "$PROJECT_A/.agents/skills/${SKILL_NAME}/SKILL.md" \
  "divergent project copy"
if [[ -L "$PROJECT_A/.agents/skills/${SKILL_NAME}" ]]; then
  echo "doctor replaced the real copy with a symlink" >&2
  exit 1
fi
skl_assert_no_merge_ui "$doc"

a_cap="$(run_a capture ".agents/skills/${SKILL_NAME}" --force --project "$PROJECT_A" 2>&1)"
echo "$a_cap"
skl_assert_contains "$a_cap" "captured $SKILL_NAME"
skl_assert_contains "$a_cap" "overwrote library"
skl_assert_file_contains "$(skl_library_of "$MACHINE_A" "$SKILL_NAME")/SKILL.md" \
  "divergent project copy"
skl_assert_no_merge_ui "$a_cap"

a_re="$(run_a use "$SKILL_NAME" --project "$PROJECT_A" 2>&1)"
echo "$a_re"
skl_assert_contains "$a_re" "using $SKILL_NAME"
skl_assert_symlink_to \
  "$PROJECT_A/.agents/skills/${SKILL_NAME}" \
  "$(skl_library_of "$MACHINE_A" "$SKILL_NAME")"
skl_assert_file_contains \
  "$PROJECT_A/.agents/skills/${SKILL_NAME}/SKILL.md" \
  "divergent project copy"
skl_assert_no_merge_ui "$a_re"

doc_after="$(skl_doctor_in "$MACHINE_A" "$PROJECT_A" 2>&1)"
echo "$doc_after"
skl_assert_not_contains "$doc_after" "not a sync peer"

echo "==> [6] harness homes are not sync peers (A+B)"
assert_harness_not_sync_peer "$MACHINE_B" "$SKILL_NAME"
skl_assert_library_only "$MACHINE_A" "$SKILL_NAME"
skl_assert_library_only "$MACHINE_B" "$SKILL_NAME"
# A's foreign .claude tree is the original import source, not a sync dest.
skl_assert_file_contains "$MACHINE_A/.claude/skills/${SKILL_NAME}/SKILL.md" \
  "hello from machine A"

echo "OK: DAN-15 source reconciliation (A=$TOKEN_A B=$TOKEN_B)"
