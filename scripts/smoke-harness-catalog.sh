#!/usr/bin/env bash
# Hammer smoke stacked on furnace #20 harness catalog.
# Consumes vendored `crates/cli/data/agents-catalog.json` only.
#
# Usage:
#   cargo build -p skl
#   ./scripts/smoke-harness-catalog.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=smoke-lib.sh
source "$ROOT/scripts/smoke-lib.sh"

skl_smoke_defaults
CATALOG="$ROOT/crates/cli/data/agents-catalog.json"
HOME_DIR="$WORKDIR/home"
SKILL_NAME="${SKL_SMOKE_SKILL:-greeter}"

trap skl_smoke_cleanup EXIT

if [[ ! -f "$CATALOG" ]]; then
  echo "missing furnace catalog: $CATALOG" >&2
  exit 1
fi

echo "==> catalog JSON contract (furnace vendored file)"
python3 - "$CATALOG" <<'PY'
import json, sys

path = sys.argv[1]
data = json.load(open(path))
agents = data["agents"]
if len(agents) < 70:
    raise SystemExit(f"expected ~77 agents, got {len(agents)}")

by_id = {a["id"]: a for a in agents}
for required in ("claude-code", "continue", "openclaw", "cursor", "codex"):
    if required not in by_id:
        raise SystemExit(f"catalog missing {required}")

def universal(a):
    return a["project_skills_dir"] == ".agents/skills"

for blocked in ("cursor", "codex"):
    if not universal(by_id[blocked]):
        raise SystemExit(
            f"{blocked} must be universal (.agents/skills), got {by_id[blocked]['project_skills_dir']}"
        )

samples = {
    "claude-code": (".claude/skills", "{home}/.claude/skills"),
    "continue": (".continue/skills", "{home}/.continue/skills"),
    "openclaw": ("skills", "{home}/.openclaw/skills"),
}
for id_, (proj, glob) in samples.items():
    a = by_id[id_]
    if universal(a):
        raise SystemExit(f"{id_} must be catalog-custom")
    if a["project_skills_dir"] != proj:
        raise SystemExit(f"{id_} project_skills_dir={a['project_skills_dir']} want {proj}")
    if a.get("global_skills_dir") != glob:
        raise SystemExit(f"{id_} global_skills_dir={a.get('global_skills_dir')} want {glob}")

customs = [a["id"] for a in agents if not universal(a)]
if "cursor" in customs or "codex" in customs:
    raise SystemExit("cursor/codex must never be catalog-custom extras")
if "claude-code" not in customs:
    raise SystemExit("claude-code must be custom")

print(f"    {len(agents)} agents; {len(customs)} custom; cursor/codex universal")
print("    samples: claude-code .claude/skills, continue .continue/skills, openclaw ~/.openclaw/skills")
PY

skl_require_bin
mkdir -p "$HOME_DIR/.agents/skills/${SKILL_NAME}" \
  "$HOME_DIR/.openclaw/skills/clawed"
printf '# %s\n\nhello from ~/.agents/skills\n' "$SKILL_NAME" \
  >"$HOME_DIR/.agents/skills/${SKILL_NAME}/SKILL.md"
printf '# clawed\n\nfrom openclaw global\n' \
  >"$HOME_DIR/.openclaw/skills/clawed/SKILL.md"
skl_write_sync_prefs "$HOME_DIR" false 900

run_home() {
  skl_run "$HOME_DIR" "$@"
}

help_out="$(run_home use --help 2>&1)"
if [[ "$help_out" != *"claude-code"* ]]; then
  echo "expected skl use --help to mention claude-code" >&2
  echo "$help_out" >&2
  exit 1
fi

echo "==> init imports non-trio catalog global (~/.openclaw/skills)"
init_out="$(run_home init 2>&1)"
echo "$init_out"
skl_assert_contains "$init_out" "clawed"
skl_assert_contains "$init_out" "openclaw"
skl_assert_contains "$init_out" "$SKILL_NAME"

echo "==> sticky claude migrates to claude-code; cursor/codex dropped"
mkdir -p "$HOME_DIR/.config/skl"
cat >"$HOME_DIR/.config/skl/config.toml" <<EOF
[sync]
auto = false
frequency_secs = 900

[targets]
extra = ["claude", "cursor", "codex"]
EOF
mig_out="$(run_home targets 2>&1)"
echo "$mig_out"
cfg="$HOME_DIR/.config/skl/config.toml"
skl_assert_file_contains "$cfg" "claude-code"
if grep -qE '(^|[[:space:]",\[])claude($|[[:space:]",\]])' "$cfg"; then
  echo "sticky still lists bare claude after migrate" >&2
  cat "$cfg" >&2
  exit 1
fi
if grep -q 'cursor' "$cfg"; then
  echo "sticky still lists cursor" >&2
  cat "$cfg" >&2
  exit 1
fi
if grep -q 'codex' "$cfg"; then
  echo "sticky still lists codex" >&2
  cat "$cfg" >&2
  exit 1
fi
skl_assert_contains "$mig_out" "cursor"
skl_assert_contains "$mig_out" "codex"
skl_assert_contains "$mig_out" "claude-code"

echo "==> soft-prompt / targets never offer cursor/codex extras"
tgt="$(run_home targets 2>&1)"
echo "$tgt"
if [[ "$tgt" == *"cursor"* || "$tgt" == *"codex"* ]]; then
  echo "targets listing must not offer cursor/codex extras" >&2
  exit 1
fi
skl_assert_contains "$tgt" "claude-code"

# Migrate leaves sticky `claude-code`. Clear extras so default `use` is
# agents-only; `-a` is a separate project so sticky cannot leak dests.
cat >"$cfg" <<EOF
[sync]
auto = false
frequency_secs = 900

[targets]
extra = []
EOF

PROJ_ALONE="$WORKDIR/proj-alone"
PROJ_CLAUDE="$WORKDIR/proj-claude"
mkdir -p "$PROJ_ALONE" "$PROJ_CLAUDE"
home_skill="$HOME_DIR/.agents/skills/${SKILL_NAME}"

echo "==> use ${SKILL_NAME} alone → .agents/skills only"
use_out="$(run_home use "$SKILL_NAME" --project "$PROJ_ALONE" 2>&1)"
echo "$use_out"
skl_assert_contains "$use_out" "using $SKILL_NAME"
agents_link="$PROJ_ALONE/.agents/skills/${SKILL_NAME}"
skl_assert_symlink_to "$agents_link" "$home_skill"
if [[ -e "$PROJ_ALONE/.claude" || -d "$PROJ_ALONE/.claude" ]]; then
  echo "default use must not create .claude" >&2
  ls -la "$PROJ_ALONE" >&2 || true
  exit 1
fi
if [[ -e "$PROJ_ALONE/.cursor" || -e "$PROJ_ALONE/.codex" ]]; then
  echo "default use must not create .cursor/.codex" >&2
  exit 1
fi

echo "==> use -a claude-code also writes .claude/skills"
use_a="$(run_home use "$SKILL_NAME" -a claude-code --project "$PROJ_CLAUDE" 2>&1)"
echo "$use_a"
skl_assert_symlink_to "$PROJ_CLAUDE/.agents/skills/${SKILL_NAME}" "$home_skill"
skl_assert_symlink_to "$PROJ_CLAUDE/.claude/skills/${SKILL_NAME}" "$home_skill"
if [[ -e "$PROJ_CLAUDE/.cursor" || -e "$PROJ_CLAUDE/.codex" ]]; then
  echo "-a claude-code must not create .cursor/.codex" >&2
  exit 1
fi

echo "OK: catalog JSON + CLI harness smoke"
