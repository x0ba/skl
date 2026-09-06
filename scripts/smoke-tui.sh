#!/usr/bin/env bash
# Hammer coverage stacked on furnace DAN-13 TUI. Does not invent keys or verbs.
#
#   1. Non-TTY / piped / CI never enters fullscreen (prints help)
#   2. SKL_NO_TUI=1 / --no-tui forces help *on a TTY*
#   3. `u` from TUI writes the same skills.toml + dests as `skl use`
#   4. `q` restores cooked mode (ICANON+ECHO) — no leftover raw mode
#   5. Existing dialoguer smokes are untouched (run them separately)
#
# No API. Browse is local-only.
#
# Usage:
#   cargo build -p skl
#   ./scripts/smoke-tui.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=smoke-lib.sh
source "$ROOT/scripts/smoke-lib.sh"

skl_smoke_defaults
TOKEN="${SKL_TOKEN:-dev:smoke-tui-$$}"
SKILL_NAME="${SKL_SMOKE_SKILL:-greeter}"
HOME_DIR="$WORKDIR/home"
PROJECT_TUI="$WORKDIR/project-tui"
PROJECT_CLI="$WORKDIR/project-cli"
PTY_DIR="$WORKDIR/pty"
export SKL_TOKEN="$TOKEN"

trap skl_smoke_cleanup EXIT

# Drive $BIN on a real PTY. Inherits the current env (HOME / SKL_*).
# Usage: tui_pty <prefix> <keys-or-empty> [skl args...]
tui_pty() {
  local prefix="$1"
  shift
  local keys="$1"
  shift
  mkdir -p "$PTY_DIR"
  python3 "$ROOT/scripts/tui-pty.py" "$BIN" "$prefix" "$keys" "$@"
}

pty_meta() {
  python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))[sys.argv[2]])' "$1.meta.json" "$2"
}

assert_help_transcript() {
  local prefix="$1"
  local label="$2"
  if ! grep -a -q "Usage:" "$prefix.out"; then
    echo "$label: expected clap help (Usage:)" >&2
    echo "meta: $(cat "$prefix.meta.json")" >&2
    echo "transcript:" >&2
    cat -v "$prefix.out" >&2 || true
    exit 1
  fi
  if python3 -c 'import sys; d=open(sys.argv[1],"rb").read(); sys.exit(0 if b"\x1b[?1049" in d else 1)' "$prefix.out"; then
    echo "$label: entered alternate screen (fullscreen)" >&2
    cat -v "$prefix.out" >&2 || true
    exit 1
  fi
  if [[ "$(pty_meta "$prefix" exit)" != "0" ]]; then
    echo "$label: exit $(pty_meta "$prefix" exit)" >&2
    exit 1
  fi
  if [[ "$(pty_meta "$prefix" entered_alt)" != "False" ]]; then
    echo "$label: meta says entered_alt" >&2
    exit 1
  fi
}

skl_require_bin
mkdir -p "$HOME_DIR/.claude/skills" "$HOME_DIR/.config/skl" "$HOME_DIR/.local/share/skl/skills/$SKILL_NAME" \
  "$PROJECT_TUI" "$PROJECT_CLI" "$PTY_DIR"
printf '# %s\n\nhello from library\n' "$SKILL_NAME" >"$HOME_DIR/.local/share/skl/skills/$SKILL_NAME/SKILL.md"
skl_write_sync_prefs "$HOME_DIR" false 900
skl_prepare_home "$HOME_DIR" "$TOKEN"

export HOME="$HOME_DIR"
export SKL_DATA_DIR="$HOME_DIR/.local/share/skl"
export SKL_CONFIG_DIR="$HOME_DIR/.config/skl"
export SKL_NO_PROMPT=1
export API_BASE="${API:-http://127.0.0.1:1}"
# This environment may export TERM=dumb; furnace degrades instead of Enter.
export TERM=xterm-256color

echo "==> 1. non-TTY / piped / CI never enters fullscreen"
piped="$(
  env HOME="$HOME_DIR" SKL_DATA_DIR="$SKL_DATA_DIR" SKL_CONFIG_DIR="$SKL_CONFIG_DIR" \
    SKL_TOKEN="$TOKEN" SKL_NO_PROMPT=1 API_BASE="$API_BASE" \
    "$BIN" </dev/null 2>&1 | cat
)"
echo "$piped"
skl_assert_contains "$piped" "Usage:"
if [[ "$piped" == *'?1049'* ]]; then
  echo "piped bare skl entered alternate screen" >&2
  exit 1
fi

ci_out="$(
  env CI=true HOME="$HOME_DIR" SKL_DATA_DIR="$SKL_DATA_DIR" SKL_CONFIG_DIR="$SKL_CONFIG_DIR" \
    SKL_TOKEN="$TOKEN" SKL_NO_PROMPT=1 API_BASE="$API_BASE" \
    "$BIN" tui </dev/null 2>&1 | cat
)"
skl_assert_contains "$ci_out" "Usage:"
if [[ "$ci_out" == *'?1049'* ]]; then
  echo "CI=true skl tui entered alternate screen" >&2
  exit 1
fi

echo "==> 2. SKL_NO_TUI=1 / --no-tui force help on a TTY"
SKL_NO_TUI=1 tui_pty "$PTY_DIR/env-no-tui" ""
assert_help_transcript "$PTY_DIR/env-no-tui" "SKL_NO_TUI=1 on TTY"
if [[ "$(pty_meta "$PTY_DIR/env-no-tui" cooked)" != "True" ]]; then
  echo "SKL_NO_TUI=1 on TTY left raw mode" >&2
  cat "$PTY_DIR/env-no-tui.meta.json" >&2
  exit 1
fi

tui_pty "$PTY_DIR/flag-no-tui" "" --no-tui
assert_help_transcript "$PTY_DIR/flag-no-tui" "--no-tui on TTY"
if [[ "$(pty_meta "$PTY_DIR/flag-no-tui" cooked)" != "True" ]]; then
  echo "--no-tui on TTY left raw mode" >&2
  exit 1
fi

echo "==> 3. TUI u matches skl use (skills.toml + links)"
# CLI side first — same HOME / library skill, sibling project.
cli_use="$(
  cd "$PROJECT_CLI"
  env HOME="$HOME_DIR" SKL_DATA_DIR="$SKL_DATA_DIR" SKL_CONFIG_DIR="$SKL_CONFIG_DIR" \
    SKL_TOKEN="$TOKEN" SKL_NO_PROMPT=1 API_BASE="$API_BASE" \
    "$BIN" use "$SKILL_NAME" --project "$PROJECT_CLI" 2>&1
)"
echo "$cli_use"
skl_assert_contains "$cli_use" "using $SKILL_NAME"
skl_assert_portable_manifest "$PROJECT_CLI/skills.toml"

# Furnace TUI `u` activates cwd only (no --project). Drive it from project-tui.
(
  cd "$PROJECT_TUI"
  tui_pty "$PTY_DIR/use-q" "uq" tui
)

if [[ "$(pty_meta "$PTY_DIR/use-q" exit)" != "0" ]]; then
  echo "skl tui u/q exited $(pty_meta "$PTY_DIR/use-q" exit)" >&2
  echo "meta: $(cat "$PTY_DIR/use-q.meta.json")" >&2
  cat -v "$PTY_DIR/use-q.out" >&2 || true
  exit 1
fi
if [[ ! -f "$PROJECT_TUI/skills.toml" ]]; then
  echo "TUI u did not write skills.toml" >&2
  cat -v "$PTY_DIR/use-q.out" >&2 || true
  exit 1
fi
skl_assert_portable_manifest "$PROJECT_TUI/skills.toml"
if ! cmp -s "$PROJECT_TUI/skills.toml" "$PROJECT_CLI/skills.toml"; then
  echo "TUI u skills.toml != skl use skills.toml" >&2
  echo "--- TUI ---" >&2
  cat "$PROJECT_TUI/skills.toml" >&2
  echo "--- CLI ---" >&2
  cat "$PROJECT_CLI/skills.toml" >&2
  exit 1
fi
skl_assert_symlink_to \
  "$PROJECT_TUI/.agents/skills/${SKILL_NAME}" \
  "$HOME_DIR/.local/share/skl/skills/${SKILL_NAME}"
skl_assert_symlink_to \
  "$PROJECT_CLI/.agents/skills/${SKILL_NAME}" \
  "$HOME_DIR/.local/share/skl/skills/${SKILL_NAME}"
skl_assert_file_contains "$PROJECT_TUI/.agents/skills/${SKILL_NAME}/SKILL.md" "hello from library"

echo "==> 4. q restores cooked terminal (no leftover raw mode)"
if [[ "$(pty_meta "$PTY_DIR/use-q" cooked)" != "True" ]]; then
  echo "after q, PTY is still raw (ICANON/ECHO cleared)" >&2
  cat "$PTY_DIR/use-q.meta.json" >&2
  exit 1
fi
# Quit-only: enter TUI then q, dests already exist (idempotent).
(
  cd "$PROJECT_TUI"
  tui_pty "$PTY_DIR/quit" "q" tui
)
if [[ "$(pty_meta "$PTY_DIR/quit" exit)" != "0" ]]; then
  echo "skl tui q exited $(pty_meta "$PTY_DIR/quit" exit)" >&2
  cat -v "$PTY_DIR/quit.out" >&2 || true
  exit 1
fi
if [[ "$(pty_meta "$PTY_DIR/quit" cooked)" != "True" ]]; then
  echo "q-only left raw mode" >&2
  cat "$PTY_DIR/quit.meta.json" >&2
  exit 1
fi
if [[ "$(pty_meta "$PTY_DIR/quit" entered_alt)" != "True" ]]; then
  echo "expected TUI to enter alt screen before q (nothing to restore otherwise)" >&2
  cat "$PTY_DIR/quit.meta.json" >&2
  cat -v "$PTY_DIR/quit.out" >&2 || true
  exit 1
fi

echo "OK: TUI launch / u parity / q restore (skill=$SKILL_NAME)"
