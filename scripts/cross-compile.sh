#!/usr/bin/env bash
# Build a single portable `skl` CLI binary (no brew, no installer).
#
# Default: host triple via `cargo build --release -p skl`, plus a static-ish
# Linux musl binary when zig + cargo-zigbuild (or musl-gcc) are available.
#
# macOS / Windows triples are built only when practical:
#   - windows-gnu from Linux/mac via cargo-zigbuild
#   - apple triples only when the host is already Darwin (no osxcross)
#
# Usage:
#   ./scripts/cross-compile.sh
#   TARGETS=x86_64-unknown-linux-musl ./scripts/cross-compile.sh
#   INSTALL_TOOLS=1 ./scripts/cross-compile.sh
#   OUT_DIR=dist ./scripts/cross-compile.sh
#
# Output: $OUT_DIR/skl-<target>[.exe]  and  $OUT_DIR/skl  (copy of host/musl)

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${OUT_DIR:-$ROOT/dist}"
HOST_TRIPLE="$(rustc -vV | awk '/^host:/{print $2}')"
ZIG_VERSION="${ZIG_VERSION:-0.14.1}"

log() { printf '==> %s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

host_os() {
  case "$HOST_TRIPLE" in
    *-apple-darwin*) echo darwin ;;
    *-pc-windows-*) echo windows ;;
    *) echo linux ;;
  esac
}

default_targets() {
  local targets=("$HOST_TRIPLE")
  case "$(host_os)" in
    linux)
      if [[ "$HOST_TRIPLE" == x86_64-* ]]; then
        targets+=("x86_64-unknown-linux-musl")
      elif [[ "$HOST_TRIPLE" == aarch64-* ]]; then
        targets+=("aarch64-unknown-linux-musl")
      fi
      # Practical from Linux: Windows GNU via zig. Apple needs an SDK.
      targets+=("x86_64-pc-windows-gnu")
      ;;
    darwin)
      if [[ "$HOST_TRIPLE" == aarch64-* ]]; then
        targets+=("x86_64-apple-darwin")
      fi
      targets+=("x86_64-unknown-linux-musl" "x86_64-pc-windows-gnu")
      ;;
    windows)
      targets+=("x86_64-pc-windows-gnu")
      ;;
  esac
  printf '%s\n' "${targets[@]}" | awk 'NF && !seen[$0]++'
}

have() { command -v "$1" >/dev/null 2>&1; }

ensure_tools() {
  if [[ "${INSTALL_TOOLS:-}" != "1" ]]; then
    return 0
  fi
  if ! have zig; then
    log "INSTALL_TOOLS=1: fetching zig $ZIG_VERSION"
    local uname_s uname_m zig_os zig_arch url tmp dest
    uname_s="$(uname -s)"
    uname_m="$(uname -m)"
    case "$uname_s" in
      Linux) zig_os=linux ;;
      Darwin) zig_os=macos ;;
      *) die "INSTALL_TOOLS cannot fetch zig on $uname_s" ;;
    esac
    case "$uname_m" in
      x86_64|amd64) zig_arch=x86_64 ;;
      aarch64|arm64) zig_arch=aarch64 ;;
      *) die "INSTALL_TOOLS cannot fetch zig for arch $uname_m" ;;
    esac
    url="https://ziglang.org/download/${ZIG_VERSION}/zig-${zig_os}-${zig_arch}-${ZIG_VERSION}.tar.xz"
    tmp="$(mktemp -d)"
    curl -fsSL "$url" | tar -xJ -C "$tmp"
    dest="$HOME/.local/share/zig-${ZIG_VERSION}"
    rm -rf "$dest"
    mkdir -p "$HOME/.local/share" "$HOME/.local/bin"
    mv "$tmp"/zig-"${zig_os}"-"${zig_arch}"-"${ZIG_VERSION}" "$dest"
    ln -sfn "$dest/zig" "$HOME/.local/bin/zig"
    rm -rf "$tmp"
    export PATH="$HOME/.local/bin:$PATH"
    log "zig → $HOME/.local/bin/zig"
  fi
  if ! have cargo-zigbuild; then
    log "INSTALL_TOOLS=1: cargo install cargo-zigbuild"
    cargo install cargo-zigbuild --locked
  fi
}

bin_name_for() {
  local target="$1"
  case "$target" in
    *-pc-windows-*) echo "skl.exe" ;;
    *) echo "skl" ;;
  esac
}

artifact_name_for() {
  local target="$1"
  case "$target" in
    *-pc-windows-*) echo "skl-${target}.exe" ;;
    *) echo "skl-${target}" ;;
  esac
}

# Prefer cargo-zigbuild (C deps: rusqlite bundled). Fall back to musl-gcc
# for linux-musl, then plain cargo for the host triple.
build_target() {
  local target="$1"
  local required="${2:-0}"
  local crate_bin artifact src
  crate_bin="$(bin_name_for "$target")"
  artifact="$(artifact_name_for "$target")"

  log "target $target"

  if [[ "$target" != "$HOST_TRIPLE" ]]; then
    rustup target add "$target" >/dev/null
  fi

  if have cargo-zigbuild && have zig && [[ "$target" != *apple-darwin* || "$(host_os)" == darwin ]]; then
    if [[ "$target" == *apple-darwin* && "$(host_os)" != darwin ]]; then
      :
    else
      (cd "$ROOT" && cargo zigbuild --release -p skl --target "$target")
      src="$ROOT/target/${target}/release/${crate_bin}"
      [[ -f "$src" ]] || die "zigbuild produced no $src"
      cp "$src" "$OUT_DIR/$artifact"
      log "wrote $OUT_DIR/$artifact"
      return 0
    fi
  fi

  if [[ "$target" == *-linux-musl ]]; then
    local musl_cc=""
    if have musl-gcc; then
      musl_cc=musl-gcc
    elif have x86_64-linux-musl-gcc && [[ "$target" == x86_64-* ]]; then
      musl_cc=x86_64-linux-musl-gcc
    elif have aarch64-linux-musl-gcc && [[ "$target" == aarch64-* ]]; then
      musl_cc=aarch64-linux-musl-gcc
    fi
    if [[ -n "$musl_cc" ]]; then
      (cd "$ROOT" && \
        CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="$musl_cc" \
        CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER="$musl_cc" \
        CC_x86_64_unknown_linux_musl="$musl_cc" \
        CC_aarch64_unknown_linux_musl="$musl_cc" \
        cargo build --release -p skl --target "$target")
      src="$ROOT/target/${target}/release/${crate_bin}"
      [[ -f "$src" ]] || die "musl build produced no $src"
      cp "$src" "$OUT_DIR/$artifact"
      log "wrote $OUT_DIR/$artifact (musl-gcc)"
      return 0
    fi
  fi

  if [[ "$target" == "$HOST_TRIPLE" ]]; then
    (cd "$ROOT" && cargo build --release -p skl)
    src="$ROOT/target/release/${crate_bin}"
    [[ -f "$src" ]] || die "host build produced no $src"
    cp "$src" "$OUT_DIR/$artifact"
    log "wrote $OUT_DIR/$artifact (host cargo)"
    return 0
  fi

  if [[ "$required" == "1" ]]; then
    die "cannot build $target (need cargo-zigbuild+zig, or musl-gcc for *-linux-musl)"
  fi
  warn "skip $target (install zig + cargo-zigbuild, or re-run with INSTALL_TOOLS=1)"
  return 0
}

pick_portable_copy() {
  local musl_x64="$OUT_DIR/skl-x86_64-unknown-linux-musl"
  local musl_arm="$OUT_DIR/skl-aarch64-unknown-linux-musl"
  local host="$OUT_DIR/$(artifact_name_for "$HOST_TRIPLE")"
  if [[ -f "$musl_x64" ]]; then
    cp "$musl_x64" "$OUT_DIR/skl"
  elif [[ -f "$musl_arm" ]]; then
    cp "$musl_arm" "$OUT_DIR/skl"
  elif [[ -f "$host" ]]; then
    cp "$host" "$OUT_DIR/skl"
  else
    return 0
  fi
  chmod +x "$OUT_DIR/skl" 2>/dev/null || true
  log "portable copy $OUT_DIR/skl"
}

checksum() {
  if have sha256sum; then
    sha256sum "$@"
  elif have shasum; then
    shasum -a 256 "$@"
  else
    warn "no sha256sum/shasum; skipping checksums"
    return 1
  fi
}

write_manifest() {
  local manifest="$OUT_DIR/SHA256SUMS"
  : > "$manifest"
  local f
  for f in "$OUT_DIR"/skl-*; do
    [[ -f "$f" ]] || continue
    (cd "$OUT_DIR" && checksum "$(basename "$f")") >> "$manifest" || true
  done
  if [[ -f "$OUT_DIR/skl" ]]; then
    (cd "$OUT_DIR" && checksum skl) >> "$manifest" || true
  fi
  if [[ -s "$manifest" ]]; then
    log "checksums $manifest"
    cat "$manifest"
  fi
}

main() {
  mkdir -p "$OUT_DIR"
  ensure_tools

  local -a targets=()
  if [[ -n "${TARGETS:-}" ]]; then
    IFS=',' read -r -a targets <<< "$TARGETS"
  else
    mapfile -t targets < <(default_targets)
  fi

  local required=0
  if [[ -n "${TARGETS:-}" ]]; then
    required=1
  fi

  log "cross-compile skl  host=$HOST_TRIPLE  targets=${targets[*]}"
  local t
  for t in "${targets[@]}"; do
    t="$(echo "$t" | tr -d '[:space:]')"
    [[ -n "$t" ]] || continue
    build_target "$t" "$required"
  done

  pick_portable_copy
  write_manifest

  local built=0
  shopt -s nullglob
  for _ in "$OUT_DIR"/skl "$OUT_DIR"/skl-*; do
    built=1
    break
  done
  shopt -u nullglob
  [[ "$built" == "1" ]] || die "no binaries written to $OUT_DIR"
  log "done. binaries in $OUT_DIR"
}

main "$@"
