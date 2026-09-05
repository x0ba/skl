#!/usr/bin/env bash
# Hammer coverage for furnace portable `skills.toml` + restore verb
# `skl use --all`. Does not invent a second restore command.
#
#   1. Same committed skills.toml on two HOMEs → sync library on B →
#      `skl use --all` → project skills work
#   2. No $HOME / absolute home paths in committed skills.toml
#   3. Legacy absolute `path` still activates by name
#   4. `skl use` no longer writes absolute paths
#   5. doctor warns on absolute paths (warn only)
#
# Sync never auto-restores. `skl use` with no args still lists.
#
# Usage:
#   cargo build -p skl
#   ./scripts/smoke-portable-use-all.sh
#   START_API=1 ./scripts/smoke-portable-use-all.sh
#   START_API=1 SKIP_DOCKER=1 ./scripts/smoke-portable-use-all.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=smoke-lib.sh
source "$ROOT/scripts/smoke-lib.sh"

skl_smoke_defaults
TOKEN="${SKL_TOKEN:-dev:smoke-portable-$$}"
TOKEN_A="${SKL_TOKEN_A:-$TOKEN}"
TOKEN_B="${SKL_TOKEN_B:-$TOKEN}"
SKILL_NAME="${SKL_SMOKE_SKILL:-portable-$$}"
SKILL_BODY="# ${SKILL_NAME}

hello from machine A
"

MACHINE_A="$WORKDIR/machine-a"
MACHINE_B="$WORKDIR/machine-b"
HOME_LEGACY="$WORKDIR/home-legacy"
PROJECT_A="$WORKDIR/project-a"
PROJECT_B="$WORKDIR/project-b"
PROJECT_LEGACY="$WORKDIR/project-legacy"

trap skl_smoke_cleanup EXIT

run_home() {
  local home="$1"
  local token="$2"
  shift 2
  SKL_TOKEN="$token" skl_run "$home" "$@"
}

run_a() { run_home "$MACHINE_A" "$TOKEN_A" "$@"; }
run_b() { run_home "$MACHINE_B" "$TOKEN_B" "$@"; }

run_legacy() {
  SKL_TOKEN="$TOKEN" skl_run "$HOME_LEGACY" "$@"
}

doctor_in() {
  local home="$1"
  local cwd="$2"
  (
    cd "$cwd"
    env HOME="$home" \
      SKL_DATA_DIR="$home/.local/share/skl" \
      SKL_CONFIG_DIR="$home/.config/skl" \
      SKL_NO_PROMPT=1 \
      API_BASE="$API" \
      "$BIN" doctor
  )
}

seed_skill() {
  local home="$1"
  local name="$2"
  local body="$3"
  mkdir -p "$home/.claude/skills/$name"
  printf '%s' "$body" >"$home/.claude/skills/$name/SKILL.md"
}

plant_legacy_manifest() {
  local project="$1"
  local name="$2"
  mkdir -p "$project"
  cat >"$project/skills.toml" <<EOF
# planted legacy host-absolute path (not portable)

[[skills]]
name = "${name}"
source = "claude"
path = "/Users/other/.claude/skills/${name}"
mode = "symlink"
EOF
}

skl_start_api
skl_require_bin
skl_wait_for_api

echo "==> committed repo skills.toml has no \$HOME / absolute home paths"
skl_assert_portable_manifest "$ROOT/skills.toml"

mkdir -p "$MACHINE_A" "$MACHINE_B" "$HOME_LEGACY" "$PROJECT_A" "$PROJECT_B" "$PROJECT_LEGACY"
mkdir -p "$MACHINE_B/.agents/skills"
skl_write_sync_prefs "$MACHINE_A" false 900
skl_write_sync_prefs "$MACHINE_B" false 900
skl_write_sync_prefs "$HOME_LEGACY" false 900
seed_skill "$MACHINE_A" "$SKILL_NAME" "$SKILL_BODY"
seed_skill "$HOME_LEGACY" "$SKILL_NAME" "$SKILL_BODY"
plant_legacy_manifest "$PROJECT_LEGACY" "$SKILL_NAME"

echo "==> doctor warns on absolute paths (does not rewrite)"
doc="$(doctor_in "$HOME_LEGACY" "$PROJECT_LEGACY" 2>&1)"
echo "$doc"
skl_assert_contains "$doc" "absolute paths"
skl_assert_contains "$doc" "skl use --all"
skl_assert_file_contains "$PROJECT_LEGACY/skills.toml" "/Users/other/.claude/skills/${SKILL_NAME}"
if [[ -e "$PROJECT_LEGACY/.agents" ]]; then
  echo "doctor mutated legacy fixture (created .agents)" >&2
  exit 1
fi

echo "==> legacy absolute path still activates by name"
legacy_use="$(run_legacy use "$SKILL_NAME" --project "$PROJECT_LEGACY" 2>&1)"
echo "$legacy_use"
skl_assert_contains "$legacy_use" "using $SKILL_NAME"
skl_assert_symlink_to \
  "$PROJECT_LEGACY/.agents/skills/${SKILL_NAME}" \
  "$HOME_LEGACY/.claude/skills/${SKILL_NAME}"
skl_assert_file_contains "$PROJECT_LEGACY/.agents/skills/${SKILL_NAME}/SKILL.md" "hello from machine A"
skl_assert_portable_manifest "$PROJECT_LEGACY/skills.toml" \
  "$HOME_LEGACY" \
  "/Users/other/.claude/skills/${SKILL_NAME}"

echo "==> doctor warning clears after rewrite"
doc_after="$(doctor_in "$HOME_LEGACY" "$PROJECT_LEGACY" 2>&1)"
echo "$doc_after"
skl_assert_not_contains "$doc_after" "absolute paths"

echo "==> machine A: import → sync → use (writes portable manifest)"
a_init="$(run_a init 2>&1)"
echo "$a_init"
skl_assert_contains "$a_init" "Imported 1 skill"
skl_assert_contains "$a_init" "$SKILL_NAME"

a_sync="$(run_a sync 2>&1)"
echo "$a_sync"
skl_assert_contains "$a_sync" "sync done"
skl_assert_contains "$a_sync" "conflicts=0"

a_use="$(run_a use "$SKILL_NAME" --project "$PROJECT_A" 2>&1)"
echo "$a_use"
skl_assert_contains "$a_use" "using $SKILL_NAME"
skl_assert_symlink_to \
  "$PROJECT_A/.agents/skills/${SKILL_NAME}" \
  "$MACHINE_A/.claude/skills/${SKILL_NAME}"
skl_assert_portable_manifest "$PROJECT_A/skills.toml" "$MACHINE_A" '$HOME'

echo "==> clone committed skills.toml only onto project B (no dests)"
cp "$PROJECT_A/skills.toml" "$PROJECT_B/skills.toml"
skl_assert_portable_manifest "$PROJECT_B/skills.toml" "$MACHINE_A" "$MACHINE_B"
if [[ -e "$PROJECT_B/.agents" ]]; then
  echo "clone must not copy project dests" >&2
  exit 1
fi

echo "==> machine B: sync library (must not auto-restore project dests)"
b_init="$(run_b init 2>&1)"
echo "$b_init"
skl_assert_contains "$b_init" "Imported 0 skill"

b_sync="$(run_b sync 2>&1)"
echo "$b_sync"
skl_assert_contains "$b_sync" "wrote skill $SKILL_NAME"
skl_assert_contains "$b_sync" "sync done"
skl_assert_file_contains "$MACHINE_B/.agents/skills/${SKILL_NAME}/SKILL.md" "hello from machine A"
if [[ -e "$PROJECT_B/.agents" ]]; then
  echo "sync must not auto-restore project dests" >&2
  ls -la "$PROJECT_B" >&2 || true
  exit 1
fi

echo "==> machine B: skl use (no args) lists, does not restore"
b_list="$(run_b use --project "$PROJECT_B" 2>&1)"
echo "$b_list"
skl_assert_contains "$b_list" "$SKILL_NAME"
skl_assert_contains "$b_list" "symlink"
if [[ -e "$PROJECT_B/.agents" ]]; then
  echo "skl use with no args must not rematerialize dests" >&2
  exit 1
fi

echo "==> machine B: skl use --all rematerializes from this machine"
b_all="$(run_b use --all --project "$PROJECT_B" 2>&1)"
echo "$b_all"
skl_assert_contains "$b_all" "using $SKILL_NAME"
skl_assert_symlink_to \
  "$PROJECT_B/.agents/skills/${SKILL_NAME}" \
  "$MACHINE_B/.agents/skills/${SKILL_NAME}"
skl_assert_file_contains "$PROJECT_B/.agents/skills/${SKILL_NAME}/SKILL.md" "hello from machine A"
skl_assert_portable_manifest "$PROJECT_B/skills.toml" \
  "$MACHINE_A" \
  "$MACHINE_B" \
  "$HOME_LEGACY"

echo "==> machine B: use --all with a name is rejected"
if b_both="$(run_b use --all "$SKILL_NAME" --project "$PROJECT_B" 2>&1)"; then
  echo "expected skl use --all <skill> to fail" >&2
  echo "$b_both" >&2
  exit 1
else
  echo "$b_both"
  skl_assert_contains "$b_both" "use either"
fi

echo "OK: portable skills.toml + skl use --all (A=$TOKEN_A B=$TOKEN_B)"
