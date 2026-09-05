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

# Drive $BIN on a real PTY. Writes transcript + meta JSON next to $1 prefix.
# Usage: tui_pty <prefix> <keys-or-empty> [-- extra env KEY=VAL ...] -- [skl args...]
tui_pty() {
  local prefix="$1"
  shift
  local keys="$1"
  shift
  mkdir -p "$PTY_DIR"
  python3 - "$BIN" "$prefix" "$keys" "$@" <<'PY'
import json, os, select, struct, sys, termios, time, errno, fcntl, pty

bin_path, prefix, keys = sys.argv[1], sys.argv[2], sys.argv[3]
rest = sys.argv[4:]
env_extra = {}
cmd = None
if "--" in rest:
    i = rest.index("--")
    for item in rest[:i]:
        if "=" in item:
            k, v = item.split("=", 1)
            env_extra[k] = v
    cmd = rest[i + 1 :]
else:
    cmd = rest
if not cmd:
    cmd = []

argv = [bin_path] + cmd
env = os.environ.copy()
env.update(env_extra)
env.setdefault("TERM", "xterm-256color")
env.setdefault("SKL_NO_PROMPT", "1")

master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))

pid = os.fork()
if pid == 0:
    os.close(master)
    os.setsid()
    os.dup2(slave, 0)
    os.dup2(slave, 1)
    os.dup2(slave, 2)
    if slave > 2:
        os.close(slave)
    os.execvpe(argv[0], argv, env)

deadline = time.time() + float(env.get("SKL_TUI_PTY_TIMEOUT", "12"))
chunks = []
status = None


def drain(budget):
    end = time.time() + budget
    got = b""
    while time.time() < end:
        remain = end - time.time()
        r, _, _ = select.select([master], [], [], max(0.0, remain))
        if not r:
            continue
        try:
            data = os.read(master, 65536)
        except OSError as exc:
            if exc.errno == errno.EIO:
                break
            raise
        if not data:
            break
        got += data
        end = time.time() + 0.05
    return got


# Wait for first paint (alt screen) before sending keys, or settle if help-only.
keys_b = keys.encode("utf-8") if keys else b""
entered = False
while time.time() < deadline:
    chunks.append(drain(0.15))
    blob = b"".join(chunks)
    if b"\x1b[?1049h" in blob or b"\x1b[?1049" in blob:
        entered = True
        break
    wpid, st = os.waitpid(pid, os.WNOHANG)
    if wpid != 0:
        status = st
        pid = None
        break

if keys_b and pid is not None:
    os.write(master, keys_b)

status = None
while True:
    if pid is None:
        break
    chunks.append(drain(0.2))
    wpid, status = os.waitpid(pid, os.WNOHANG)
    if wpid != 0:
        chunks.append(drain(0.2))
        break
    if time.time() >= deadline:
        os.kill(pid, 9)
        _, status = os.waitpid(pid, 0)
        meta = {
            "exit": 124,
            "cooked": False,
            "entered_alt": entered,
            "timeout": True,
        }
        open(prefix + ".meta.json", "w").write(json.dumps(meta) + "\n")
        open(prefix + ".out", "wb").write(b"".join(chunks))
        sys.stderr.write("tui-pty: timeout waiting for %s\n" % argv)
        sys.exit(124)

if status is None:
    exit_code = 0
elif os.WIFEXITED(status):
    exit_code = os.WEXITSTATUS(status)
else:
    exit_code = 1

lflag = termios.tcgetattr(slave)[3]
cooked = bool(lflag & termios.ICANON) and bool(lflag & termios.ECHO)
blob = b"".join(chunks)
if b"\x1b[?1049h" in blob or b"\x1b[?1049" in blob:
    entered = True

os.close(master)
os.close(slave)

open(prefix + ".out", "wb").write(blob)
meta = {
    "exit": exit_code,
    "cooked": cooked,
    "entered_alt": entered,
    "timeout": False,
    "bytes": len(blob),
}
open(prefix + ".meta.json", "w").write(json.dumps(meta) + "\n")
sys.exit(0)
PY
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
  if grep -a -q $'\033[?1049h' "$prefix.out" || grep -a -q $'\033[?1049' "$prefix.out"; then
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
export TERM="${TERM:-xterm-256color}"

echo "==> 1. non-TTY / piped / CI never enters fullscreen"
piped="$(
  env HOME="$HOME_DIR" SKL_DATA_DIR="$SKL_DATA_DIR" SKL_CONFIG_DIR="$SKL_CONFIG_DIR" \
    SKL_TOKEN="$TOKEN" SKL_NO_PROMPT=1 API_BASE="$API_BASE" \
    "$BIN" </dev/null 2>&1 | cat
)"
echo "$piped"
skl_assert_contains "$piped" "Usage:"
if [[ "$piped" == *$'[''?1049'* ]]; then
  echo "piped bare skl entered alternate screen" >&2
  exit 1
fi

ci_out="$(
  env CI=true HOME="$HOME_DIR" SKL_DATA_DIR="$SKL_DATA_DIR" SKL_CONFIG_DIR="$SKL_CONFIG_DIR" \
    SKL_TOKEN="$TOKEN" SKL_NO_PROMPT=1 API_BASE="$API_BASE" \
    "$BIN" tui </dev/null 2>&1 | cat
)"
skl_assert_contains "$ci_out" "Usage:"
if [[ "$ci_out" == *$'[''?1049'* ]]; then
  echo "CI=true skl tui entered alternate screen" >&2
  exit 1
fi

echo "==> 2. SKL_NO_TUI=1 / --no-tui force help on a TTY"
tui_pty "$PTY_DIR/env-no-tui" "" -- "HOME=$HOME_DIR" "SKL_DATA_DIR=$SKL_DATA_DIR" \
  "SKL_CONFIG_DIR=$SKL_CONFIG_DIR" "SKL_TOKEN=$TOKEN" "SKL_NO_TUI=1" --
assert_help_transcript "$PTY_DIR/env-no-tui" "SKL_NO_TUI=1 on TTY"
if [[ "$(pty_meta "$PTY_DIR/env-no-tui" cooked)" != "True" ]]; then
  echo "SKL_NO_TUI=1 on TTY left raw mode" >&2
  cat "$PTY_DIR/env-no-tui.meta.json" >&2
  exit 1
fi

tui_pty "$PTY_DIR/flag-no-tui" "" -- "HOME=$HOME_DIR" "SKL_DATA_DIR=$SKL_DATA_DIR" \
  "SKL_CONFIG_DIR=$SKL_CONFIG_DIR" "SKL_TOKEN=$TOKEN" -- --no-tui
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
  tui_pty "$PTY_DIR/use-q" "uq" -- "HOME=$HOME_DIR" "SKL_DATA_DIR=$SKL_DATA_DIR" \
    "SKL_CONFIG_DIR=$SKL_CONFIG_DIR" "SKL_TOKEN=$TOKEN" -- tui
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
  tui_pty "$PTY_DIR/quit" "q" -- "HOME=$HOME_DIR" "SKL_DATA_DIR=$SKL_DATA_DIR" \
    "SKL_CONFIG_DIR=$SKL_CONFIG_DIR" "SKL_TOKEN=$TOKEN" -- tui
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
