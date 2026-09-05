#!/usr/bin/env bash
# Hammer smoke for furnace #20 harness catalog.
#
# Always: consume vendored `crates/cli/data/agents-catalog.json` (no invented
# product catalog). Asserts universal/custom partition, sample custom paths,
# unique globals, and that cursor/codex are never extras.
#
# CLI half (`skl use` / `-a claude-code` / sticky migrate / init from a
# non-trio global) runs only after #20 wires catalog.rs + use/targets.
# Until then this script exits 0 after the JSON contract and prints GAPS.
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
PROJECT="$WORKDIR/proj"
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
mkdir -p "$HOME_DIR/.agents/skills/${SKILL_NAME}" "$PROJECT"
printf '# %s\n\nhello from ~/.agents/skills\n' "$SKILL_NAME" \
  >"$HOME_DIR/.agents/skills/${SKILL_NAME}/SKILL.md"
skl_write_sync_prefs "$HOME_DIR" false 900

run_home() {
  skl_run "$HOME_DIR" "$@"
}

cli_ready=0
if run_home use --help 2>&1 | grep -q -- "claude-code"; then
  cli_ready=1
fi

if [[ "$cli_ready" -eq 0 ]]; then
  echo "GAP: #20 has not wired catalog.rs / use -a claude-code yet"
  echo "     JSON contract is green. Re-run this smoke after furnace lands:"
  echo "     - use greeter alone → only .agents/skills"
  echo "     - -a claude-code → also .claude/skills"
  echo "     - init imports ~/.openclaw/skills (non-trio catalog global)"
  echo "     - sticky claude→claude-code; cursor/codex extras dropped with warn"
  echo "     - soft-prompt: non-TTY skip; cursor/codex never toggles; Universal locked"
  echo "OK: catalog JSON contract (CLI half waiting on #20)"
  exit 0
fi

echo "==> init imports non-trio catalog global (~/.openclaw/skills)"
mkdir -p "$HOME_DIR/.openclaw/skills/clawed"
printf '# clawed\n\nfrom openclaw global\n' >"$HOME_DIR/.openclaw/skills/clawed/SKILL.md"
init_out="$(run_home init 2>&1)"
echo "$init_out"
skl_assert_contains "$init_out" "clawed"
skl_assert_contains "$init_out" "openclaw"

echo "==> sticky claude migrates to claude-code; cursor/codex dropped"
mkdir -p "$HOME_DIR/.config/skl"
cat >"$HOME_DIR/.config/skl/config.toml" <<EOF
[sync]
auto = false
frequency_secs = 900

[targets]
extra = ["claude", "cursor", "codex"]
EOF
mig_out="$(run_home doctor 2>&1 || true)"
echo "$mig_out"
cfg="$HOME_DIR/.config/skl/config.toml"
skl_assert_file_contains "$cfg" "claude-code"
if grep -Eq 'cursor|codex|"claude"' "$cfg" && grep -q 'claude-code' "$cfg"; then
  :
fi
if grep -Eq '(^|[[:space:]",[])claude($|[[:space:]",\]])' "$cfg"; then
  if grep -q 'claude-code' "$cfg"; then
    echo "sticky still lists bare claude after migrate" >&2
    cat "$cfg" >&2
    exit 1
  fi
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
if [[ "$mig_out" != *"cursor"* || "$mig_out" != *"codex"* ]]; then
  echo "expected doctor/targets to warn about dropping cursor/codex" >&2
  echo "$mig_out" >&2
  exit 1
fi

echo "==> soft-prompt never offers cursor/codex (targets listing)"
tgt="$(run_home targets 2>&1)"
echo "$tgt"
if [[ "$tgt" == *"cursor"* || "$tgt" == *"codex"* ]]; then
  echo "targets listing must not offer cursor/codex extras" >&2
  exit 1
fi
skl_assert_contains "$tgt" "claude-code"

echo "==> use ${SKILL_NAME} alone → .agents/skills only"
use_out="$(run_home use "$SKILL_NAME" --project "$PROJECT" 2>&1)"
echo "$use_out"
skl_assert_contains "$use_out" "using $SKILL_NAME"
agents_link="$PROJECT/.agents/skills/${SKILL_NAME}"
home_skill="$HOME_DIR/.agents/skills/${SKILL_NAME}"
skl_assert_symlink_to "$agents_link" "$home_skill"
if [[ -e "$PROJECT/.claude" || -d "$PROJECT/.claude" ]]; then
  echo "default use must not create .claude" >&2
  ls -la "$PROJECT" >&2 || true
  exit 1
fi
if [[ -e "$PROJECT/.cursor" || -e "$PROJECT/.codex" ]]; then
  echo "default use must not create .cursor/.codex" >&2
  exit 1
fi

echo "==> use -a claude-code also writes .claude/skills"
use_a="$(run_home use "$SKILL_NAME" -a claude-code --project "$PROJECT" 2>&1)"
echo "$use_a"
skl_assert_symlink_to "$agents_link" "$home_skill"
skl_assert_symlink_to "$PROJECT/.claude/skills/${SKILL_NAME}" "$home_skill"
if [[ -e "$PROJECT/.cursor" || -e "$PROJECT/.codex" ]]; then
  echo "-a claude-code must not create .cursor/.codex" >&2
  exit 1
fi

echo "OK: catalog JSON + CLI harness smoke"
