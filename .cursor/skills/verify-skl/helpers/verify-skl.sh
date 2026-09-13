#!/usr/bin/env bash
# Isolated skl verification helper. Never points at ~/.local/share/skl or this repo's skills.toml.
#
#   helpers/verify-skl.sh launch
#   helpers/verify-skl.sh doctor
#   helpers/verify-skl.sh cli -- create verify-greeter
#   helpers/verify-skl.sh tui-start
#   helpers/verify-skl.sh tui-capture
#   helpers/verify-skl.sh tui-stop
#   helpers/verify-skl.sh evidence use-skill
#   helpers/verify-skl.sh cleanup
set -euo pipefail

HELPER_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SKILL_DIR="$(cd "$HELPER_DIR/.." && pwd)"

# Repo root is three levels up from helpers/ (.cursor/skills/verify-skl/helpers).
default_repo_root() {
  cd "$SKILL_DIR/../../.." && pwd
}

REPO_ROOT="${VERIFY_SKL_REPO:-$(default_repo_root)}"
RUN_ID="${VERIFY_SKL_RUN_ID:-}"
ROOT="${VERIFY_SKL_ROOT:-}"
EVIDENCE="${VERIFY_SKL_EVIDENCE:-}"
SESSION_NAME=""
TMUX_CONF="/exec-daemon/tmux.portal.conf"

tmux_bin() {
  if [[ -f "$TMUX_CONF" ]]; then
    tmux -f "$TMUX_CONF"
  else
    tmux
  fi
}

die() {
  echo "verify-skl: $*" >&2
  exit 1
}

resolve_run() {
  if [[ -n "${VERIFY_SKL_SESSION:-}" && -f "$VERIFY_SKL_SESSION" ]]; then
    # shellcheck disable=SC1090
    source "$VERIFY_SKL_SESSION"
  elif [[ -n "$RUN_ID" && -f "/tmp/skl-verify-${RUN_ID}/session.env" ]]; then
    # shellcheck disable=SC1090
    source "/tmp/skl-verify-${RUN_ID}/session.env"
  elif [[ -n "$ROOT" && -f "$ROOT/session.env" ]]; then
    # shellcheck disable=SC1090
    source "$ROOT/session.env"
  fi
  RUN_ID="${VERIFY_SKL_RUN_ID:-${RUN_ID:-}}"
  ROOT="${VERIFY_SKL_ROOT:-${ROOT:-}}"
  EVIDENCE="${VERIFY_SKL_EVIDENCE:-${EVIDENCE:-}}"
  SESSION_NAME="${VERIFY_SKL_TMUX:-}"
}

require_launch() {
  resolve_run
  [[ -n "$RUN_ID" && -n "$ROOT" ]] || die "no launch session. Run: helpers/verify-skl.sh launch"
  [[ -f "$ROOT/session.env" ]] || die "missing $ROOT/session.env. Re-run launch."
  # shellcheck disable=SC1091
  source "$ROOT/session.env"
  [[ -x "${VERIFY_SKL_BIN:-}" ]] || die "skl binary missing at ${VERIFY_SKL_BIN:-unset}"
}

export_isolate() {
  export SKL_DATA_DIR="$ROOT/data"
  export SKL_CONFIG_DIR="$ROOT/config"
  export HOME="$ROOT/home"
  export SKL_NO_TUI=1
  export SKL_NO_PROMPT=1
  export CI=1
  export VISUAL="${VERIFY_SKL_EDITOR:-true}"
  export EDITOR="${VERIFY_SKL_EDITOR:-true}"
  export API_BASE="${VERIFY_SKL_API_BASE:-http://127.0.0.1:1}"
  unset SKL_TOKEN SKL_TOKEN_FILE
}

skl_cmd() {
  export_isolate
  "$VERIFY_SKL_BIN" --api-base "$API_BASE" "$@"
}

cmd_launch() {
  command -v cargo >/dev/null || die "cargo not on PATH"
  [[ -f "$REPO_ROOT/crates/cli/Cargo.toml" ]] || die "not the skl repo: $REPO_ROOT"

  RUN_ID="${VERIFY_SKL_RUN_ID:-$(date +%Y%m%d%H%M%S)-$$}"
  ROOT="${VERIFY_SKL_ROOT:-/tmp/skl-verify-$RUN_ID}"
  EVIDENCE="${VERIFY_SKL_EVIDENCE:-/tmp/skl-verify-evidence/$RUN_ID}"
  SESSION_NAME="skl-verify-$RUN_ID"

  mkdir -p "$ROOT/data" "$ROOT/config" "$ROOT/home" "$ROOT/project" "$ROOT/logs" "$EVIDENCE"
  mkdir -p "$ROOT/home/.agents/skills" "$ROOT/home/.claude/skills"

  cat >"$ROOT/config/config.toml" <<'EOF'
api_base = "http://127.0.0.1:1"

[sync]
auto = false
frequency_secs = 900
EOF

  echo "==> building skl (cargo build -p skl)"
  (cd "$REPO_ROOT" && cargo build -p skl) | tee "$ROOT/logs/build.txt"
  local bin="$REPO_ROOT/target/debug/skl"
  [[ -x "$bin" ]] || die "build did not produce $bin"

  local version
  version="$("$bin" --version)"
  [[ "$version" == "skl 0.4.2" ]] || die "unexpected version: $version (want skl 0.4.2)"

  cat >"$ROOT/session.env" <<EOF
export VERIFY_SKL_RUN_ID="$RUN_ID"
export VERIFY_SKL_ROOT="$ROOT"
export VERIFY_SKL_EVIDENCE="$EVIDENCE"
export VERIFY_SKL_BIN="$bin"
export VERIFY_SKL_REPO="$REPO_ROOT"
export VERIFY_SKL_PROJECT="$ROOT/project"
export VERIFY_SKL_TMUX="$SESSION_NAME"
export VERIFY_SKL_API_BASE="http://127.0.0.1:1"
export VERIFY_SKL_SESSION="$ROOT/session.env"
EOF

  # shellcheck disable=SC1091
  source "$ROOT/session.env"
  export_isolate

  echo "ready  version=$version"
  echo "root   $ROOT"
  echo "data   $SKL_DATA_DIR"
  echo "config $SKL_CONFIG_DIR"
  echo "home   $HOME"
  echo "project $VERIFY_SKL_PROJECT"
  echo "evidence $EVIDENCE"
  echo "bin    $VERIFY_SKL_BIN"
}

cmd_doctor() {
  require_launch
  export_isolate
  local project="${VERIFY_SKL_PROJECT:?}"
  [[ -d "$project" ]] || die "isolated project missing: $project"

  local version
  version="$("$VERIFY_SKL_BIN" --version)"
  [[ "$version" == "skl 0.4.2" ]] || die "wrong binary: $version"

  [[ "$SKL_DATA_DIR" == "$ROOT/data" ]] || die "SKL_DATA_DIR is not isolated: $SKL_DATA_DIR"
  [[ "$SKL_CONFIG_DIR" == "$ROOT/config" ]] || die "SKL_CONFIG_DIR is not isolated: $SKL_CONFIG_DIR"
  [[ "$HOME" == "$ROOT/home" ]] || die "HOME is not isolated: $HOME"
  [[ "$SKL_DATA_DIR" != "$HOME/.local/share/skl" ]] || die "refusing to drive the default library"

  local status_out doctor_out
  status_out="$(mktemp)"
  doctor_out="$(mktemp)"
  # doctor / status inspect cwd for skills.toml — always run from the isolate project.
  (
    cd "$project"
    skl_cmd status
  ) | tee "$status_out" | tee "$ROOT/logs/status.txt"
  (
    cd "$project"
    skl_cmd doctor
  ) | tee "$doctor_out" | tee "$ROOT/logs/doctor.txt"

  grep -q "^state.db     $ROOT/data/state.db" "$status_out" || die "status state.db is not the isolate"
  grep -q "^config       $ROOT/config/config.toml" "$status_out" || die "status config is not the isolate"
  grep -q "^api_base     http://127.0.0.1:1" "$status_out" || die "status api_base is not the isolate stub"
  grep -q "^auto_sync    off" "$status_out" || die "status auto_sync should be off"
  grep -q "^== API$" "$doctor_out" || die "doctor missing == API"
  grep -q "^== Auth$" "$doctor_out" || die "doctor missing == Auth"
  grep -q "^== State$" "$doctor_out" || die "doctor missing == State"
  grep -q "^== Agent skill roots$" "$doctor_out" || die "doctor missing == Agent skill roots"
  grep -q "^== Linking$" "$doctor_out" || die "doctor missing == Linking"
  grep -q "state.db     $ROOT/data/state.db" "$doctor_out" || die "doctor state.db is not the isolate"
  grep -q "personal library is canonical" "$doctor_out" || die "doctor missing linking roles line"
  if grep -q "/workspace/skills.toml" "$doctor_out"; then
    die "doctor is looking at the skl repo checkout, not the isolate project"
  fi

  rm -f "$status_out" "$doctor_out"
  echo "doctor ok  isolate=$ROOT  version=$version"
}

cmd_cli() {
  require_launch
  export_isolate
  local log="$ROOT/logs/cli-last.txt"
  local rc=0
  set +e
  (
    cd "${VERIFY_SKL_PROJECT:?}"
    skl_cmd "$@"
  ) >"$log" 2>&1
  rc=$?
  set -e
  cat "$log"
  echo "exit $rc" | tee -a "$log"
  return "$rc"
}

cmd_tui_start() {
  require_launch
  command -v tmux >/dev/null || die "tmux not on PATH"
  SESSION_NAME="${VERIFY_SKL_TMUX:-skl-verify-$RUN_ID}"
  if tmux_bin has-session -t "=$SESSION_NAME" 2>/dev/null; then
    die "tmux session $SESSION_NAME already exists. tui-stop first."
  fi
  # TUI needs a TTY and must not inherit SKL_NO_TUI.
  tmux_bin new-session -d -s "$SESSION_NAME" -c "${VERIFY_SKL_PROJECT:?}" -- \
    env -u SKL_NO_TUI \
        SKL_DATA_DIR="$ROOT/data" \
        SKL_CONFIG_DIR="$ROOT/config" \
        HOME="$ROOT/home" \
        SKL_NO_PROMPT=1 \
        CI=1 \
        VISUAL="${VERIFY_SKL_EDITOR:-true}" \
        EDITOR="${VERIFY_SKL_EDITOR:-true}" \
        API_BASE="${VERIFY_SKL_API_BASE:-http://127.0.0.1:1}" \
        "$VERIFY_SKL_BIN" --api-base "${VERIFY_SKL_API_BASE:-http://127.0.0.1:1}" tui
  echo "tui session $SESSION_NAME  cwd=$VERIFY_SKL_PROJECT"
}

cmd_tui_capture() {
  require_launch
  SESSION_NAME="${VERIFY_SKL_TMUX:-skl-verify-$RUN_ID}"
  tmux_bin has-session -t "=$SESSION_NAME" 2>/dev/null || die "no tui session $SESSION_NAME"
  local dest="${1:-$ROOT/logs/tui-pane.txt}"
  tmux_bin capture-pane -t "$SESSION_NAME:0.0" -p -e -J >"$dest"
  echo "tui pane -> $dest"
}

cmd_tui_stop() {
  resolve_run
  SESSION_NAME="${VERIFY_SKL_TMUX:-skl-verify-${RUN_ID:-}}"
  [[ -n "$SESSION_NAME" ]] || die "no tmux session name"
  if tmux_bin has-session -t "=$SESSION_NAME" 2>/dev/null; then
    local pane_pid
    pane_pid="$(tmux_bin list-panes -t "$SESSION_NAME:0.0" -F "#{pane_pid}" 2>/dev/null || true)"
    tmux_bin kill-session -t "$SESSION_NAME"
    if [[ -n "${pane_pid:-}" ]] && kill -0 "$pane_pid" 2>/dev/null; then
      kill "$pane_pid" || true
    fi
    echo "stopped tui session $SESSION_NAME"
  else
    echo "tui session $SESSION_NAME already gone"
  fi
}

cmd_evidence() {
  require_launch
  export_isolate
  local feature="${1:-unspecified}"
  local dest="$EVIDENCE/$feature"
  mkdir -p "$dest"
  local project="${VERIFY_SKL_PROJECT:?}"

  {
    echo "feature=$feature"
    echo "run_id=$RUN_ID"
    echo "root=$ROOT"
    echo "evidence=$dest"
    echo "bin=$VERIFY_SKL_BIN"
    echo "version=$("$VERIFY_SKL_BIN" --version)"
    echo "captured_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  } >"$dest/meta.txt"

  cp -f "$ROOT/logs/status.txt" "$dest/status.txt" 2>/dev/null || true
  cp -f "$ROOT/logs/doctor.txt" "$dest/doctor.txt" 2>/dev/null || true
  cp -f "$ROOT/logs/cli-last.txt" "$dest/cli-last.txt" 2>/dev/null || true
  cp -f "$ROOT/logs/tui-pane.txt" "$dest/tui-pane.txt" 2>/dev/null || true
  cp -f "$ROOT/config/config.toml" "$dest/config.toml"
  [[ -f "$project/skills.toml" ]] && cp -f "$project/skills.toml" "$dest/skills.toml"

  {
    echo "== library ($ROOT/data/skills)"
    find "$ROOT/data/skills" -type f -print 2>/dev/null | sort || true
    echo
    echo "== project ($project)"
    find "$project" \( -type f -o -type l \) -print 2>/dev/null | sort || true
    echo
    echo "== dest file types"
    if [[ -e "$project/.agents/skills" ]]; then
      find "$project/.agents/skills" -mindepth 1 -maxdepth 2 -printf '%p %y\n' 2>/dev/null | sort || true
    fi
  } >"$dest/tree.txt"

  if [[ -d "$ROOT/data/skills" ]]; then
    mkdir -p "$dest/library"
    cp -a "$ROOT/data/skills/." "$dest/library/" 2>/dev/null || true
  fi
  if [[ -d "$project/.agents/skills" ]]; then
    mkdir -p "$dest/project-agents"
    cp -a "$project/.agents/skills/." "$dest/project-agents/" 2>/dev/null || true
  fi

  echo "evidence $dest"
}

cmd_cleanup() {
  resolve_run
  [[ -n "$ROOT" ]] || die "nothing to clean (no launch session)"
  SESSION_NAME="${VERIFY_SKL_TMUX:-skl-verify-${RUN_ID:-}}"
  if [[ -n "$SESSION_NAME" ]] && tmux_bin has-session -t "=$SESSION_NAME" 2>/dev/null; then
    cmd_tui_stop
  fi
  if [[ -n "$EVIDENCE" && -d "$EVIDENCE" ]]; then
    echo "keeping evidence $EVIDENCE"
  fi
  # Never delete evidence. Never delete paths outside /tmp/skl-verify-*.
  case "$ROOT" in
    /tmp/skl-verify-*)
      rm -rf "$ROOT"
      echo "removed isolate $ROOT"
      ;;
    *)
      die "refusing to delete unexpected root: $ROOT"
      ;;
  esac
}

usage() {
  cat <<'EOF'
verify-skl.sh launch|doctor|cli|tui-start|tui-capture|tui-stop|evidence|cleanup

launch        build crates/cli, create /tmp/skl-verify-$RUN_ID isolate
doctor        read-only check: version, isolate paths, skl status + skl doctor
cli -- ARGS   run skl ARGS inside the isolate (cwd = isolate project)
tui-start     tmux session with `skl tui` in the isolate project
tui-capture   write the tmux pane to logs/tui-pane.txt
tui-stop      kill the session this run started
evidence ID   copy transcripts + trees into /tmp/skl-verify-evidence/$RUN_ID/ID
cleanup       remove isolate + tmux; keep evidence
EOF
}

main() {
  local cmd="${1:-}"
  shift || true
  case "$cmd" in
    launch) cmd_launch ;;
    doctor) cmd_doctor ;;
    cli)
      if [[ "${1:-}" == "--" ]]; then shift; fi
      cmd_cli "$@"
      ;;
    tui-start) cmd_tui_start ;;
    tui-capture) cmd_tui_capture "${1:-}" ;;
    tui-stop) cmd_tui_stop ;;
    evidence) cmd_evidence "${1:-unspecified}" ;;
    cleanup) cmd_cleanup ;;
    -h|--help|help|"") usage ;;
    *) die "unknown command: $cmd" ;;
  esac
}

main "$@"
