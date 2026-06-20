#!/usr/bin/env sh
# Rewind installer script
# Wrap in main() to protect against partial downloads when executed via `curl | sh`
main() {
  set -eu

  REWIND_REPO="${REWIND_REPO:-oscarmuya/rewind}"
  REWIND_VERSION="${REWIND_VERSION:-latest}"
  REWIND_INSTALL_DIR="${REWIND_INSTALL_DIR:-}"

  # ── Colors ────────────────────────────────────────────────────────────────────
  RED="" GREEN="" YELLOW="" CYAN="" BOLD="" DIM="" RESET=""
  if [ -t 2 ] && command -v tput >/dev/null 2>&1; then
    _ncolors=$(tput colors 2>/dev/null || echo 0)
    if [ "${_ncolors:-0}" -gt 0 ] 2>/dev/null; then
      _red="$(tput setaf 1 2>/dev/null || true)"
      _grn="$(tput setaf 2 2>/dev/null || true)"
      _yel="$(tput setaf 3 2>/dev/null || true)"
      _cyn="$(tput setaf 6 2>/dev/null || true)"
      _bld="$(tput bold 2>/dev/null || true)"
      _dim="$(tput dim 2>/dev/null || true)"
      _rst="$(tput sgr0 2>/dev/null || true)"
      if [ -n "$_red" ] && [ -n "$_grn" ] && [ -n "$_rst" ]; then
        RED="$_red" GREEN="$_grn" YELLOW="$_yel" CYAN="$_cyn"
        BOLD="$_bld" DIM="$_dim" RESET="$_rst"
      fi
    fi
  fi

  # ── Helpers ───────────────────────────────────────────────────────────────────
  fail() {
    printf '%s%s error:%s %s\n' "$RED" "$BOLD" "$RESET" "$*" >&2
    exit 1
  }

  info() {
    printf '%s::%s %s\n' "$CYAN" "$RESET" "$*" >&2
  }

  success() {
    printf '%s✓%s %s\n' "$GREEN" "$RESET" "$*" >&2
  }

  warn() {
    printf '%s%s warn:%s %s\n' "$YELLOW" "$BOLD" "$RESET" "$*" >&2
  }

  step() {
    printf '\n%s==>%s %s%s%s\n' "$BOLD$GREEN" "$RESET" "$BOLD" "$*" "$RESET" >&2
  }

  need_cmd() {
    command -v "$1" >/dev/null 2>&1 || fail "missing required command: ${BOLD}$1${RESET}"
  }

  # ── Dependency check ──────────────────────────────────────────────────────────
  step "Checking dependencies"
  for cmd in curl tar mktemp uname; do
    if command -v "$cmd" >/dev/null 2>&1; then
      success "$cmd"
    else
      fail "missing required command: $cmd"
    fi
  done

  # ── Platform detection ────────────────────────────────────────────────────────
  normalize_arch() {
    case "$1" in
    x86_64 | amd64) printf '%s\n' x86_64 ;;
    arm64 | aarch64) printf '%s\n' aarch64 ;;
    *) fail "unsupported CPU architecture: $1" ;;
    esac
  }

  linux_distro_asset() {
    arch="$1"
    os_id=""
    os_like=""

    if [ -r /etc/os-release ]; then
      # shellcheck disable=SC1091
      . /etc/os-release
      os_id="${ID:-}"
      os_like="${ID_LIKE:-}"
    fi

    case "$os_id" in
    ubuntu | pop | linuxmint | elementary)
      printf '%s\n' "linux-ubuntu-24.04-$arch"
      return
      ;;
    debian)
      printf '%s\n' "linux-debian-12-$arch"
      return
      ;;
    fedora | rhel | centos | rocky | almalinux)
      printf '%s\n' "linux-fedora-$arch"
      return
      ;;
    alpine)
      printf '%s\n' "linux-alpine-$arch"
      return
      ;;
    esac

    case " $os_like " in
    *" ubuntu "*)
      printf '%s\n' "linux-ubuntu-24.04-$arch"
      return
      ;;
    *" debian "*)
      printf '%s\n' "linux-debian-12-$arch"
      return
      ;;
    *" fedora "* | *" rhel "*)
      printf '%s\n' "linux-fedora-$arch"
      return
      ;;
    esac

    warn "unrecognised Linux distro '${os_id:-unknown}' (ID_LIKE='${os_like:-}'); falling back to Debian build"
    printf '%s\n' "linux-debian-12-$arch"
  }

  detect_asset() {
    os="$(uname -s)"
    arch="$(normalize_arch "$(uname -m)")"

    case "$os" in
    Darwin)
      case "$arch" in
      x86_64) printf '%s\n' macos-x86_64 ;;
      aarch64) printf '%s\n' macos-aarch64 ;;
      esac
      ;;
    Linux)
      linux_distro_asset "$arch"
      ;;
    *)
      fail "unsupported operating system: $os"
      ;;
    esac
  }

  install_dir() {
    if [ -n "$REWIND_INSTALL_DIR" ]; then
      printf '%s\n' "$REWIND_INSTALL_DIR"
    elif [ -d /usr/local/bin ] && [ -w /usr/local/bin ]; then
      printf '%s\n' /usr/local/bin
    else
      [ -n "${HOME:-}" ] || fail "HOME is not set; set REWIND_INSTALL_DIR explicitly"
      printf '%s\n' "$HOME/.local/bin"
    fi
  }

  release_base_url() {
    if [ "$REWIND_VERSION" = "latest" ]; then
      printf 'https://github.com/%s/releases/latest/download\n' "$REWIND_REPO"
    else
      printf 'https://github.com/%s/releases/download/%s\n' "$REWIND_REPO" "$REWIND_VERSION"
    fi
  }

  verify_checksum() {
    checksums="$1"
    archive="$2"
    asset="$3"

    checksum_line="$(grep "  ${asset}$" "$checksums")" ||
      fail "checksum file does not contain an entry for $asset"

    if command -v sha256sum >/dev/null 2>&1; then
      (cd "$(dirname "$archive")" && printf '%s\n' "$checksum_line" | sha256sum -c - >/dev/null 2>&1) ||
        fail "checksum mismatch for $asset"
    elif command -v shasum >/dev/null 2>&1; then
      expected="$(printf '%s\n' "$checksum_line" | awk '{print $1}')"
      actual="$(shasum -a 256 "$archive" | awk '{print $1}')"
      [ "$expected" = "$actual" ] || fail "checksum mismatch for $asset"
    else
      warn "skipping checksum verification: sha256sum and shasum not found"
    fi
  }

  download_with_progress() {
    url="$1"
    dest="$2"

    # curl's --progress-bar writes directly to stderr -- no pipe is needed and
    # none is used here. Piping curl through `while read` in POSIX sh reports
    # the exit status of the last pipeline stage (the while), not curl, so a
    # failed download would not be caught. Keeping curl as a simple foreground
    # command preserves its exit status under set -e.
    if [ -t 2 ]; then
      curl -fSL --progress-bar "$url" -o "$dest"
    else
      curl -fsSL "$url" -o "$dest"
    fi || fail "download failed: $url"
  }

  # ── Resolve platform ──────────────────────────────────────────────────────────
  step "Detecting platform"
  asset_platform="$(detect_asset)"
  asset="rewind-$asset_platform.tar.gz"
  package="rewind-$asset_platform"
  base_url="$(release_base_url)"
  url="$base_url/$asset"
  target_dir="$(install_dir)"

  info "platform : ${BOLD}$asset_platform${RESET}"
  info "version  : ${BOLD}$REWIND_VERSION${RESET}"
  info "install  : ${BOLD}$target_dir${RESET}"

  # ── Download ──────────────────────────────────────────────────────────────────
  step "Downloading rewind"
  info "source: ${DIM}$url${RESET}"

  tmp="$(mktemp -d)"
  cleanup() { rm -rf "$tmp"; }
  trap cleanup EXIT INT TERM

  download_with_progress "$url" "$tmp/$asset" "rewind"
  success "archive downloaded"

  # ── Verify checksum ───────────────────────────────────────────────────────────
  step "Verifying checksum"
  if curl -fsSL "$base_url/SHA256SUMS" -o "$tmp/SHA256SUMS" 2>/dev/null; then
    verify_checksum "$tmp/SHA256SUMS" "$tmp/$asset" "$asset"
    success "checksum verified"
  else
    warn "SHA256SUMS not available; skipping checksum verification"
  fi

  # ── Extract ───────────────────────────────────────────────────────────────────
  step "Extracting archive"
  tar -xzf "$tmp/$asset" -C "$tmp"
  success "extracted $asset"

  [ -f "$tmp/$package/rw" ] || fail "archive is missing: rw"
  [ -f "$tmp/$package/rw-daemon" ] || fail "archive is missing: rw-daemon"

  # ── Install ───────────────────────────────────────────────────────────────────
  step "Installing binaries"

  # Kill the daemon if running to avoid "Text file busy" on overwrite
  if [ -f "$target_dir/rw-daemon" ]; then
    info "stopping running rw-daemon"
    pkill -x rw-daemon 2>/dev/null || true
  fi

  mkdir -p "$target_dir"
  # Remove before copy to avoid "Text file busy"
  rm -f "$target_dir/rw" "$target_dir/rw-daemon"
  cp "$tmp/$package/rw" "$target_dir/rw"
  cp "$tmp/$package/rw-daemon" "$target_dir/rw-daemon"
  chmod 755 "$target_dir/rw" "$target_dir/rw-daemon"

  success "rw        -> $target_dir/rw"
  success "rw-daemon -> $target_dir/rw-daemon"

  # ── Start daemon ──────────────────────────────────────────────────────────────
  step "Starting daemon"
  ("$target_dir/rw-daemon" >/dev/null 2>&1 </dev/null &)
  success "rw-daemon started"

  # ── PATH check ────────────────────────────────────────────────────────────────
  case ":$PATH:" in
  *":$target_dir:"*) ;;
  *)
    printf '\n%s%s note:%s %s is not on PATH\n' "$YELLOW" "$BOLD" "$RESET" "$target_dir" >&2
    printf '       add this to your shell config:\n\n' >&2
    printf '         %sexport PATH="%s:$PATH"%s\n\n' "$CYAN" "$target_dir" "$RESET" >&2
    ;;
  esac

  # ── Done ──────────────────────────────────────────────────────────────────────
  printf '\n%s%s rewind installed successfully%s\n' "$GREEN" "$BOLD" "$RESET" >&2
  printf '%s next:%s run %srw init --install%s to enable shell recording\n\n' \
    "$BOLD" "$RESET" "$CYAN" "$RESET" >&2
}

main "$@"
