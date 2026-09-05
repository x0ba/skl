#!/usr/bin/env bash
# Atuin-style curl installer for the skl CLI. Served as a website asset:
#
#   curl -fsSL https://<site>/install.sh | bash
#   curl -fsSL https://<site>/install.sh | bash -s -- --non-interactive
#
# Never edits shell rc files (.bashrc / .zshrc / fish). Installs to ~/.local/bin/skl.
# Binaries still come from GitHub Releases (same asset names).

set -euo pipefail

# Download origin for release binaries + SHA256SUMS (not this script).
SKL_DOWNLOAD_BASE="${SKL_DOWNLOAD_BASE:-https://github.com/x0ba/skl/releases/latest/download}"

INSTALL_DIR="${SKL_INSTALL_DIR:-$HOME/.local/bin}"
BIN_NAME="skl"
NON_INTERACTIVE=0

log() { printf '==> %s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

usage() {
  cat <<EOF
Install skl to ${INSTALL_DIR}/${BIN_NAME} (no sudo).

Usage:
  install.sh [--non-interactive] [--print-target] [--help]

  --non-interactive   Binary only. Skip login / init / harness checklist.
  --print-target      Print the GitHub Release triple for this machine and exit.

Environment:
  SKL_DOWNLOAD_BASE   Asset origin (default: GitHub Releases latest/download)
  SKL_INSTALL_DIR     Install directory (default: ~/.local/bin)
  SKL_NON_INTERACTIVE Set to skip first-run prompts (same as --non-interactive)

Non-interactive / non-TTY (including curl | bash in CI) installs the binary only.
On a TTY, first-run asks login [Y/n] then init [Y/n]; init shows the harness
checklist (Universal .agents locked). This script never edits shell rc files.
EOF
}

parse_args() {
  local arg
  for arg in "$@"; do
    case "$arg" in
      --non-interactive|-n) NON_INTERACTIVE=1 ;;
      --print-target)
        detect_target
        exit 0
        ;;
      --help|-h)
        usage
        exit 0
        ;;
      *)
        die "unknown argument: $arg (try --help)"
        ;;
    esac
  done
  if [[ -n "${SKL_NON_INTERACTIVE:-}" ]]; then
    NON_INTERACTIVE=1
  fi
}

detect_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os" in
    Linux)
      case "$arch" in
        x86_64|amd64) echo "x86_64-unknown-linux-musl" ;;
        aarch64|arm64) echo "aarch64-unknown-linux-musl" ;;
        *) die "unsupported Linux architecture: $arch" ;;
      esac
      ;;
    Darwin)
      case "$arch" in
        x86_64) echo "x86_64-apple-darwin" ;;
        arm64|aarch64) echo "aarch64-apple-darwin" ;;
        *) die "unsupported macOS architecture: $arch" ;;
      esac
      ;;
    MINGW*|MSYS*|CYGWIN*|Windows_NT)
      case "$arch" in
        x86_64|amd64|i686|i386) echo "x86_64-pc-windows-gnu" ;;
        *) die "unsupported Windows architecture: $arch (need x86_64)" ;;
      esac
      ;;
    *)
      die "unsupported OS: $os (Linux, macOS, or Windows Git Bash)"
      ;;
  esac
}

asset_name_for() {
  local target="$1"
  case "$target" in
    *-pc-windows-*) echo "skl-${target}.exe" ;;
    *) echo "skl-${target}" ;;
  esac
}

installed_name_for() {
  local target="$1"
  case "$target" in
    *-pc-windows-*) echo "skl.exe" ;;
    *) echo "skl" ;;
  esac
}

have() { command -v "$1" >/dev/null 2>&1; }

checksum() {
  if have sha256sum; then
    sha256sum "$@"
  elif have shasum; then
    shasum -a 256 "$@"
  else
    return 1
  fi
}

# Verify $1 (basename in $2 dir) against downloaded SHA256SUMS when present.
verify_checksum() {
  local dest_dir="$1"
  local filename="$2"
  local sums tmp expected actual
  tmp="$(mktemp)"
  if ! curl -fsSL "${SKL_DOWNLOAD_BASE}/SHA256SUMS" -o "$tmp" 2>/dev/null; then
    rm -f "$tmp"
    warn "SHA256SUMS not available; skipping checksum verify"
    return 0
  fi
  expected="$(awk -v f="$filename" '$2 == f { print $1; exit }' "$tmp")"
  rm -f "$tmp"
  if [[ -z "$expected" ]]; then
    warn "no SHA256SUMS entry for $filename; skipping verify"
    return 0
  fi
  actual="$(checksum "$dest_dir/$filename" | awk '{ print $1 }')" || die "no sha256sum/shasum"
  if [[ "$actual" != "$expected" ]]; then
    die "checksum mismatch for $filename (got $actual, want $expected)"
  fi
  log "checksum ok ($filename)"
}

download_asset() {
  local url="$1"
  local dest="$2"
  have curl || die "curl is required"
  log "downloading $url"
  curl -fsSL "$url" -o "$dest" || die "download failed: $url"
}

path_contains() {
  local dir="$1"
  case ":${PATH}:" in
    *":${dir}:"*) return 0 ;;
    *) return 1 ;;
  esac
}

print_path_hint() {
  local dir="$1"
  local dest_name="$2"
  if path_contains "$dir"; then
    return 0
  fi
  cat <<EOF

skl is installed to ${dir}/${dest_name}.
That directory is not on PATH. For this session:

  export PATH="${dir}:\$PATH"

This installer never edits .bashrc, .zshrc, or fish config. Add the export
yourself if you want it to persist.
EOF
}

# curl | bash: stdin is the script pipe. Use /dev/tty when the user is at a TTY.
is_tty_first_run() {
  [[ "$NON_INTERACTIVE" != "1" ]] || return 1
  [[ -z "${CI:-}" ]] || return 1
  [[ -t 1 ]] && [[ -e /dev/tty ]] && [[ -r /dev/tty ]]
}

run_first_run() {
  local bin="$1"
  if ! is_tty_first_run; then
    log "non-interactive / non-TTY: binary only (skip login, init, checklist)"
    return 0
  fi
  log "first-run: invoking ${bin} setup (login / init / harness checklist)"
  # Reattach the controlling TTY so dialoguer + Y/n prompts work after curl | bash.
  "$bin" setup </dev/tty >/dev/tty 2>/dev/tty || warn "skl setup exited $?"
}

main() {
  parse_args "$@"

  local target asset dest_name dest url tmp
  target="$(detect_target)"
  asset="$(asset_name_for "$target")"
  dest_name="$(installed_name_for "$target")"
  dest="${INSTALL_DIR}/${dest_name}"
  url="${SKL_DOWNLOAD_BASE}/${asset}"

  mkdir -p "$INSTALL_DIR"
  tmp="$(mktemp -d)"
  # shellcheck disable=SC2064
  trap "rm -rf '$tmp'" EXIT

  download_asset "$url" "$tmp/$asset"
  verify_checksum "$tmp" "$asset"
  chmod +x "$tmp/$asset"
  mv "$tmp/$asset" "$dest"
  trap - EXIT
  rm -rf "$tmp"
  chmod +x "$dest"
  log "installed $dest"

  print_path_hint "$INSTALL_DIR" "$dest_name"
  run_first_run "$dest"
  log "done. try: skl --help"
}

main "$@"
