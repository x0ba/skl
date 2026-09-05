#!/usr/bin/env bash
# Install smoke stacked on furnace `apps/web/public/install.sh` (do not invent a second installer).
#
# Clean env: after a binary exists, the curl | bash path never calls cargo/rustc.
#
#   1. fake GitHub Release over HTTP (skl-<triple> + SHA256SUMS + install.sh)
#   2. curl install.sh | bash -s -- --non-interactive  → ~/.local/bin/skl
#   3. skl --help
#   4. SKL_NON_INTERACTIVE=1 and non-TTY (no flags) also skip first-run
#   5. skl setup --non-interactive skips login / init / checklist
#   6. never edits .bashrc / .zshrc / fish config
#   7. checksum mismatch refuses the binary
#
# Usage:
#   cargo build -p skl && ./scripts/smoke-install.sh
#   SKL_BIN=./dist/skl-x86_64-unknown-linux-musl ./scripts/smoke-install.sh
#   SKL_SMOKE_ALLOW_BUILD=0 SKL_BIN=./dist/skl ./scripts/smoke-install.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=smoke-lib.sh
source "$ROOT/scripts/smoke-lib.sh"

WORKDIR="${WORKDIR:-${TMPDIR:-/tmp}/skl-smoke-install-$$}"
skl_smoke_defaults
SERVER_PID=""
INSTALL_RC=0

cleanup() {
  if [[ -n "${SERVER_PID:-}" ]] && kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  skl_smoke_cleanup
}
trap cleanup EXIT

assert_no_prompt() {
  local haystack="$1"
  local label="$2"
  local needle
  for needle in "Log in now?" "Import existing skills" "[Y/n]" "first-run: invoking"; do
    if [[ "$haystack" == *"$needle"* ]]; then
      echo "expected no first-run prompts ($label), found: $needle" >&2
      echo "$haystack" >&2
      exit 1
    fi
  done
}

assert_rc_untouched() {
  local home="$1"
  skl_assert_file_contains "$home/.bashrc" "keep-bashrc"
  skl_assert_file_contains "$home/.zshrc" "keep-zshrc"
  skl_assert_file_contains "$home/.config/fish/config.fish" "keep-fish"
  if grep -Eiq 'skl|PATH=|\.local/bin' \
    "$home/.bashrc" "$home/.zshrc" "$home/.config/fish/config.fish"
  then
    echo "installer edited a shell rc file" >&2
    cat "$home/.bashrc" "$home/.zshrc" "$home/.config/fish/config.fish" >&2
    exit 1
  fi
}

plant_rc() {
  local home="$1"
  mkdir -p "$home/.config/fish"
  printf 'keep-bashrc\n' >"$home/.bashrc"
  printf 'keep-zshrc\n' >"$home/.zshrc"
  printf 'keep-fish\n' >"$home/.config/fish/config.fish"
}

# PATH with curl/bash/coreutils but no rustc/cargo — the install path is curl-only.
clean_path() {
  local dir rustc_path cargo_path
  rustc_path="$(command -v rustc 2>/dev/null || true)"
  cargo_path="$(command -v cargo 2>/dev/null || true)"
  local -a keep=()
  IFS=':' read -r -a parts <<<"$PATH"
  for dir in "${parts[@]}" /usr/bin /bin /usr/local/bin; do
    [[ -n "$dir" && -d "$dir" ]] || continue
    if [[ -n "$rustc_path" && "$rustc_path" == "$dir/"* ]]; then
      continue
    fi
    if [[ -n "$cargo_path" && "$cargo_path" == "$dir/"* ]]; then
      continue
    fi
    if [[ -x "$dir/rustc" || -x "$dir/cargo" ]]; then
      continue
    fi
    keep+=("$dir")
  done
  printf '%s\n' "${keep[@]}" | awk 'NF && !seen[$0]++ { p = p ? p ":" $0 : $0 } END { print p }'
}

resolve_bin() {
  if [[ -n "${SKL_BIN:-}" ]]; then
    [[ -x "$SKL_BIN" ]] || {
      echo "SKL_BIN is not executable: $SKL_BIN" >&2
      exit 1
    }
    BIN="$SKL_BIN"
    return
  fi
  local target candidate
  target="$(bash "$ROOT/scripts/install.sh" --print-target)"
  for candidate in \
    "$ROOT/dist/skl-${target}" \
    "$ROOT/dist/skl" \
    "$ROOT/target/release/skl" \
    "$ROOT/target/debug/skl"
  do
    if [[ -x "$candidate" ]]; then
      BIN="$candidate"
      return
    fi
  done
  if [[ "${SKL_SMOKE_ALLOW_BUILD:-1}" == "1" ]] && command -v cargo >/dev/null 2>&1; then
    echo "==> no prebuilt binary; cargo build -p skl (install path itself still curl-only)"
    (cd "$ROOT" && cargo build -p skl)
    BIN="$ROOT/target/debug/skl"
    [[ -x "$BIN" ]] || {
      echo "missing $BIN after cargo build" >&2
      exit 1
    }
    return
  fi
  echo "no skl binary (set SKL_BIN, or cargo build -p skl)" >&2
  exit 1
}

wait_http() {
  local url="$1"
  local i
  for i in $(seq 1 50); do
    if curl -fsS "$url" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.1
  done
  echo "fake release server did not come up at $url" >&2
  exit 1
}

start_release_server() {
  local dir="$1"
  local port
  port="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
  python3 -m http.server "$port" --bind 127.0.0.1 --directory "$dir" >/dev/null 2>&1 &
  SERVER_PID=$!
  RELEASE_URL="http://127.0.0.1:${port}"
  wait_http "${RELEASE_URL}/install.sh"
}

run_install() {
  local home="$1"
  shift
  local log="$1"
  shift
  # Remaining args are extra env assignments (KEY=VAL) then optional bash -s args.
  local -a env_vars=(
    "HOME=$home"
    "SKL_DOWNLOAD_BASE=$RELEASE_URL"
    "PATH=$CLEAN_PATH"
    "TERM=dumb"
  )
  local -a bash_args=()
  local item
  for item in "$@"; do
    if [[ "$item" == *"="* && "$item" != --* ]]; then
      env_vars+=("$item")
    else
      bash_args+=("$item")
    fi
  done

  plant_rc "$home"
  mkdir -p "$home" "$WORKDIR/tmp"
  env_vars+=("TMPDIR=$WORKDIR/tmp")

  # Hero path: curl install.sh | bash. PATH has no rustc/cargo.
  local bash_bin
  bash_bin="$(command -v bash)"
  set +e
  if [[ ${#bash_args[@]} -gt 0 ]]; then
    curl -fsSL "${RELEASE_URL}/install.sh" | env -i "${env_vars[@]}" "$bash_bin" -s -- "${bash_args[@]}" >"$log" 2>&1
  else
    curl -fsSL "${RELEASE_URL}/install.sh" | env -i "${env_vars[@]}" "$bash_bin" >"$log" 2>&1
  fi
  INSTALL_RC=$?
  set -e
}

echo "==> print-target"
TARGET="$(bash "$ROOT/scripts/install.sh" --print-target)"
case "$TARGET" in
  aarch64-apple-darwin | x86_64-apple-darwin | \
  x86_64-unknown-linux-musl | aarch64-unknown-linux-musl | \
  x86_64-pc-windows-gnu) ;;
  *)
    echo "print-target outside release matrix: $TARGET" >&2
    exit 1
    ;;
esac
echo "    target=$TARGET"

resolve_bin
echo "==> binary $BIN"

CLEAN_PATH="$(clean_path)"
if PATH="$CLEAN_PATH" command -v rustc >/dev/null 2>&1 \
  || PATH="$CLEAN_PATH" command -v cargo >/dev/null 2>&1; then
  echo "clean PATH still has rustc/cargo:" >&2
  PATH="$CLEAN_PATH" command -v rustc || true
  PATH="$CLEAN_PATH" command -v cargo || true
  exit 1
fi
PATH="$CLEAN_PATH" command -v curl >/dev/null || {
  echo "curl missing from clean PATH" >&2
  exit 1
}
echo "    clean PATH (no rustc/cargo)"

mkdir -p "$WORKDIR/release"
ASSET="skl-${TARGET}"
if [[ "$TARGET" == *-pc-windows-* ]]; then
  ASSET="skl-${TARGET}.exe"
fi
cp "$BIN" "$WORKDIR/release/$ASSET"
chmod +x "$WORKDIR/release/$ASSET"
cp "$ROOT/apps/web/public/install.sh" "$WORKDIR/release/install.sh"
(
  cd "$WORKDIR/release"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$ASSET" install.sh >SHA256SUMS
  else
    shasum -a 256 "$ASSET" install.sh >SHA256SUMS
  fi
)

start_release_server "$WORKDIR/release"
echo "==> fake release $RELEASE_URL"

# --- 1. --non-interactive (hero CI path) ----------------------------------
echo "==> curl install.sh | bash -s -- --non-interactive"
HOME_A="$WORKDIR/home-a"
LOG_A="$WORKDIR/install-a.log"
run_install "$HOME_A" "$LOG_A" --non-interactive
[[ "$INSTALL_RC" -eq 0 ]] || {
  echo "--non-interactive install failed ($INSTALL_RC)" >&2
  cat "$LOG_A" >&2
  exit 1
}
skl_assert_contains "$(cat "$LOG_A")" "installed ${HOME_A}/.local/bin/skl"
skl_assert_contains "$(cat "$LOG_A")" "checksum ok ($ASSET)"
skl_assert_contains "$(cat "$LOG_A")" "non-interactive / non-TTY: binary only"
skl_assert_contains "$(cat "$LOG_A")" "That directory is not on PATH"
skl_assert_contains "$(cat "$LOG_A")" "never edits .bashrc, .zshrc, or fish"
assert_no_prompt "$(cat "$LOG_A")" "--non-interactive"
assert_rc_untouched "$HOME_A"
[[ -x "$HOME_A/.local/bin/skl" ]] || {
  echo "missing installed binary" >&2
  exit 1
}

echo "==> skl --help (installed, no Rust)"
HELP="$(PATH="$HOME_A/.local/bin:$CLEAN_PATH" HOME="$HOME_A" env -u CARGO_HOME -u RUSTUP_HOME \
  "$HOME_A/.local/bin/skl" --help)"
skl_assert_contains "$HELP" "Personal agent skill sync"
skl_assert_contains "$HELP" "setup"

echo "==> skl setup --non-interactive (no prompts)"
SETUP="$(PATH="$HOME_A/.local/bin:$CLEAN_PATH" HOME="$HOME_A" \
  "$HOME_A/.local/bin/skl" setup --non-interactive 2>&1 </dev/null)" || {
  echo "skl setup --non-interactive failed" >&2
  echo "$SETUP" >&2
  exit 1
}
skl_assert_contains "$SETUP" "Skipping first-run prompts"
assert_no_prompt "$SETUP" "skl setup --non-interactive"

# --- 2. SKL_NON_INTERACTIVE=1 ---------------------------------------------
echo "==> curl install.sh | SKL_NON_INTERACTIVE=1 bash"
HOME_B="$WORKDIR/home-b"
LOG_B="$WORKDIR/install-b.log"
run_install "$HOME_B" "$LOG_B" SKL_NON_INTERACTIVE=1
[[ "$INSTALL_RC" -eq 0 ]] || {
  echo "SKL_NON_INTERACTIVE=1 install failed ($INSTALL_RC)" >&2
  cat "$LOG_B" >&2
  exit 1
}
skl_assert_contains "$(cat "$LOG_B")" "installed ${HOME_B}/.local/bin/skl"
skl_assert_contains "$(cat "$LOG_B")" "non-interactive / non-TTY: binary only"
assert_no_prompt "$(cat "$LOG_B")" "SKL_NON_INTERACTIVE=1"
assert_rc_untouched "$HOME_B"

# --- 3. non-TTY, no flags (curl | bash without a terminal) -----------------
echo "==> curl install.sh | bash  (stdout redirected, no flags)"
HOME_C="$WORKDIR/home-c"
LOG_C="$WORKDIR/install-c.log"
# Unset CI so we exercise the non-TTY branch, not the CI short-circuit.
run_install "$HOME_C" "$LOG_C"
[[ "$INSTALL_RC" -eq 0 ]] || {
  echo "non-TTY curl|bash install failed ($INSTALL_RC)" >&2
  cat "$LOG_C" >&2
  exit 1
}
skl_assert_contains "$(cat "$LOG_C")" "installed ${HOME_C}/.local/bin/skl"
skl_assert_contains "$(cat "$LOG_C")" "non-interactive / non-TTY: binary only"
assert_no_prompt "$(cat "$LOG_C")" "non-TTY curl|bash"
assert_rc_untouched "$HOME_C"
PATH="$HOME_C/.local/bin:$CLEAN_PATH" HOME="$HOME_C" \
  "$HOME_C/.local/bin/skl" --help >/dev/null

# --- 4. checksum mismatch -------------------------------------------------
echo "==> checksum mismatch refuses install"
BAD="$WORKDIR/bad-release"
cp -a "$WORKDIR/release/." "$BAD/"
printf '0000000000000000000000000000000000000000000000000000000000000000  %s\n' "$ASSET" >"$BAD/SHA256SUMS"
if [[ -n "${SERVER_PID:-}" ]] && kill -0 "$SERVER_PID" 2>/dev/null; then
  kill "$SERVER_PID" 2>/dev/null || true
  wait "$SERVER_PID" 2>/dev/null || true
  SERVER_PID=""
fi
start_release_server "$BAD"
HOME_D="$WORKDIR/home-d"
LOG_D="$WORKDIR/install-d.log"
run_install "$HOME_D" "$LOG_D" --non-interactive
if [[ "$INSTALL_RC" -eq 0 ]]; then
  echo "expected checksum mismatch to fail" >&2
  cat "$LOG_D" >&2
  exit 1
fi
skl_assert_contains "$(cat "$LOG_D")" "checksum mismatch"
[[ ! -e "$HOME_D/.local/bin/skl" ]] || {
  echo "mismatch must not leave an installed binary" >&2
  exit 1
}

echo "OK: curl install.sh → skl --help (clean PATH, non-interactive, no rc edits)"
