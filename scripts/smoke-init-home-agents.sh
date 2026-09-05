#!/usr/bin/env bash
# Prove `skl init` imports furnace home roots (same list as doctor):
#   ~/.agents/skills
#   ~/.config/agents/skills
# No API required. Does not change harness roots or `skl use` dests.
#
# Usage (crate path crates/cli; package name skl):
#   cargo build -p skl
#   ./scripts/smoke-init-home-agents.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=smoke-lib.sh
source "$ROOT/scripts/smoke-lib.sh"

skl_smoke_defaults
HOME_DIR="$WORKDIR/home"
AGENTS_NAME="${SKL_SMOKE_AGENTS_SKILL:-greeter}"
XDG_NAME="${SKL_SMOKE_XDG_SKILL:-notes}"

trap skl_smoke_cleanup EXIT

run_home() {
  skl_run "$HOME_DIR" "$@"
}

skl_require_bin
mkdir -p "$HOME_DIR/.agents/skills/${AGENTS_NAME}" \
  "$HOME_DIR/.config/agents/skills/${XDG_NAME}"
printf '%s' "# ${AGENTS_NAME}

hello from ~/.agents/skills
" >"$HOME_DIR/.agents/skills/${AGENTS_NAME}/SKILL.md"
printf '%s' "# ${XDG_NAME}

hello from ~/.config/agents/skills
" >"$HOME_DIR/.config/agents/skills/${XDG_NAME}/SKILL.md"
# Explicit init — do not let furnace maybe_run try the API.
skl_write_sync_prefs "$HOME_DIR" false 900

echo "==> skl init imports ~/.agents/skills + ~/.config/agents/skills"
init_out="$(run_home init 2>&1)"
echo "$init_out"
skl_assert_contains "$init_out" "Imported 2 skill"
# `{:<8} {:<24}` — agents pads; xdg-agents (10) overflows the source column.
skl_assert_contains "$init_out" "agents   ${AGENTS_NAME}"
skl_assert_contains "$init_out" "xdg-agents ${XDG_NAME}"
# Harness roots were not planted — init must not invent them.
if [[ "$init_out" == *"claude"* ]]; then
  echo "init listed a claude skill that was never planted" >&2
  exit 1
fi

echo "==> skl list shows imported sources"
list_out="$(run_home list 2>&1)"
echo "$list_out"
skl_assert_contains "$list_out" "$AGENTS_NAME"
skl_assert_contains "$list_out" "agents"
skl_assert_contains "$list_out" "$XDG_NAME"
skl_assert_contains "$list_out" "xdg-agents"

echo "==> skl doctor reports both furnace home roots"
doc_out="$(run_home doctor 2>&1)"
echo "$doc_out"
skl_assert_contains "$doc_out" "agents"
skl_assert_contains "$doc_out" "xdg-agents"
skl_assert_contains "$doc_out" "skills=2"

echo "OK: skl init imported ${AGENTS_NAME} (agents) + ${XDG_NAME} (xdg-agents)"
