#!/usr/bin/env bash
# Deliberate M0-style fixture: skills only under .claude/.cursor, no .agents.
# Asserts doctor warns without mutating, then `skl migrate targets`.
# No API required.
#
# Usage (crate path crates/cli; package name skl):
#   cargo build -p skl
#   ./scripts/smoke-migrate-targets.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=smoke-lib.sh
source "$ROOT/scripts/smoke-lib.sh"

skl_smoke_defaults
HOME_DIR="$WORKDIR/home"
KEEP_PROJ="$WORKDIR/m0-keep"
PRUNE_PROJ="$WORKDIR/m0-prune"
SKILL_NAME="${SKL_SMOKE_SKILL:-greeter}"
SKILL_BODY="# ${SKILL_NAME}

hello from M0 fixture
"

trap skl_smoke_cleanup EXIT

plant_m0() {
  local project="$1"
  local skill_src="$HOME_DIR/.claude/skills/${SKILL_NAME}"
  mkdir -p "$project/.claude/skills" "$project/.cursor/skills"
  ln -s "$skill_src" "$project/.claude/skills/${SKILL_NAME}"
  ln -s "$skill_src" "$project/.cursor/skills/${SKILL_NAME}"
  cat >"$project/skills.toml" <<EOF
# planted M0 manifest (no .agents)

[[skills]]
name = "${SKILL_NAME}"
source = "claude"
path = "${skill_src}"
mode = "symlink"
EOF
}

run_home() {
  local cwd="$1"
  shift
  env HOME="$HOME_DIR" \
    SKL_DATA_DIR="$HOME_DIR/.local/share/skl" \
    SKL_CONFIG_DIR="$HOME_DIR/.config/skl" \
    SKL_NO_PROMPT=1 \
    API_BASE="$API" \
    "$BIN" "$@" --project "$cwd"
}

doctor_in() {
  local cwd="$1"
  (
    cd "$cwd"
    env HOME="$HOME_DIR" \
      SKL_DATA_DIR="$HOME_DIR/.local/share/skl" \
      SKL_CONFIG_DIR="$HOME_DIR/.config/skl" \
      SKL_NO_PROMPT=1 \
      API_BASE="$API" \
      "$BIN" doctor
  )
}

skl_require_bin
mkdir -p "$HOME_DIR/.claude/skills/${SKILL_NAME}" "$KEEP_PROJ" "$PRUNE_PROJ"
printf '%s' "$SKILL_BODY" >"$HOME_DIR/.claude/skills/${SKILL_NAME}/SKILL.md"
plant_m0 "$KEEP_PROJ"
plant_m0 "$PRUNE_PROJ"

echo "==> doctor on M0 fixture (warn only, no mutate)"
doc="$(doctor_in "$KEEP_PROJ" 2>&1)"
echo "$doc"
skl_assert_contains "$doc" "skl migrate targets"
if [[ -e "$KEEP_PROJ/.agents" ]]; then
  echo "doctor mutated M0 fixture (created .agents)" >&2
  exit 1
fi
if [[ ! -L "$KEEP_PROJ/.claude/skills/${SKILL_NAME}" ]]; then
  echo "doctor removed M0 claude link" >&2
  exit 1
fi

echo "==> migrate targets (keep old links)"
keep_out="$(run_home "$KEEP_PROJ" migrate targets 2>&1)"
echo "$keep_out"
skl_assert_contains "$keep_out" "migrating M0 targets"
skl_assert_contains "$keep_out" "agents"

agents_link="$KEEP_PROJ/.agents/skills/${SKILL_NAME}"
claude_link="$KEEP_PROJ/.claude/skills/${SKILL_NAME}"
cursor_link="$KEEP_PROJ/.cursor/skills/${SKILL_NAME}"
home_skill="$HOME_DIR/.claude/skills/${SKILL_NAME}"
skl_assert_symlink_to "$agents_link" "$home_skill"
skl_assert_symlink_to "$claude_link" "$home_skill"
skl_assert_symlink_to "$cursor_link" "$home_skill"
skl_assert_file_contains "$KEEP_PROJ/skills.toml" "canonical"
skl_assert_file_contains "$KEEP_PROJ/skills.toml" "agents"
skl_assert_file_contains "$KEEP_PROJ/skills.toml" "claude"
skl_assert_file_contains "$agents_link/SKILL.md" "hello from M0 fixture"

echo "==> doctor after migrate (no M0 warn)"
doc_after="$(doctor_in "$KEEP_PROJ" 2>&1)"
echo "$doc_after"
if [[ "$doc_after" == *"M0 layout"* ]]; then
  echo "expected M0 warning to clear after migrate" >&2
  echo "$doc_after" >&2
  exit 1
fi

echo "==> migrate targets --prune-old"
prune_out="$(run_home "$PRUNE_PROJ" migrate targets --prune-old 2>&1)"
echo "$prune_out"
skl_assert_contains "$prune_out" "pruned"
skl_assert_symlink_to "$PRUNE_PROJ/.agents/skills/${SKILL_NAME}" "$home_skill"
if [[ -e "$PRUNE_PROJ/.claude/skills/${SKILL_NAME}" || -L "$PRUNE_PROJ/.claude/skills/${SKILL_NAME}" ]]; then
  echo "expected pruned claude link to be gone" >&2
  exit 1
fi
if [[ -e "$PRUNE_PROJ/.cursor/skills/${SKILL_NAME}" || -L "$PRUNE_PROJ/.cursor/skills/${SKILL_NAME}" ]]; then
  echo "expected pruned cursor link to be gone" >&2
  exit 1
fi

echo "==> migrate is idempotent"
again="$(run_home "$KEEP_PROJ" migrate targets 2>&1)"
echo "$again"
skl_assert_contains "$again" "already using canonical .agents/skills"

echo "OK: migrate targets on deliberate M0 fixture"
