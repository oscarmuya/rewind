#!/usr/bin/env sh
set -eu

REWIND_REPO="${REWIND_REPO:-oscarmuya/rewind}"
REWIND_VERSION="${REWIND_VERSION:-latest}"
REWIND_INSTALL_DIR="${REWIND_INSTALL_DIR:-}"

fail() {
    printf 'rewind install: %s\n' "$*" >&2
    exit 1
}

info() {
    printf 'rewind install: %s\n' "$*" >&2
}

need_cmd() {
    command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"
}

normalize_arch() {
    case "$1" in
        x86_64 | amd64)
            printf '%s\n' x86_64
            ;;
        arm64 | aarch64)
            printf '%s\n' aarch64
            ;;
        *)
            fail "unsupported CPU architecture: $1"
            ;;
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

    # Unknown distro (Arch, Void, NixOS, Gentoo, etc.): fall back to the
    # Debian build as a reasonable generic glibc binary. Alpine (musl) users
    # on unrecognised distros may have issues.
    info "unrecognised Linux distro '${os_id:-unknown}' (ID_LIKE='${os_like:-}'); falling back to Debian build"
    printf '%s\n' "linux-debian-12-$arch"
}

detect_asset() {
    os="$(uname -s)"
    arch="$(normalize_arch "$(uname -m)")"

    case "$os" in
        Darwin)
            case "$arch" in
                x86_64)  printf '%s\n' macos-x86_64  ;;
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

    checksum_line="$(grep "  ${asset}$" "$checksums")" || fail "checksum file does not contain an entry for $asset"

    if command -v sha256sum >/dev/null 2>&1; then
        (cd "$(dirname "$archive")" && printf '%s\n' "$checksum_line" | sha256sum -c -)
    elif command -v shasum >/dev/null 2>&1; then
        expected="$(printf '%s\n' "$checksum_line" | awk '{print $1}')"
        actual="$(shasum -a 256 "$archive" | awk '{print $1}')"
        [ "$expected" = "$actual" ] || fail "checksum mismatch for $asset"
    else
        info "skipping checksum verification: sha256sum and shasum not found"
    fi
}

need_cmd curl
need_cmd tar
need_cmd mktemp
need_cmd uname

asset_platform="$(detect_asset)"
asset="rewind-$asset_platform.tar.gz"
package="rewind-$asset_platform"
base_url="$(release_base_url)"
url="$base_url/$asset"
target_dir="$(install_dir)"
tmp="$(mktemp -d)"

cleanup() {
    rm -rf "$tmp"
}
trap cleanup EXIT INT TERM

info "detected $asset_platform"
info "downloading $url"

curl -fsSL "$url" -o "$tmp/$asset" || fail "download failed: $url"

if curl -fsSL "$base_url/SHA256SUMS" -o "$tmp/SHA256SUMS" 2>/dev/null; then
    verify_checksum "$tmp/SHA256SUMS" "$tmp/$asset" "$asset"
else
    info "SHA256SUMS not available; skipping checksum verification"
fi

tar -xzf "$tmp/$asset" -C "$tmp"

# GitHub's artifact system strips file permissions, so executability is set
# explicitly below rather than checked here.
[ -f "$tmp/$package/rw" ]        || fail "archive is missing rw"
[ -f "$tmp/$package/rw-daemon" ] || fail "archive is missing rw-daemon"

mkdir -p "$target_dir"
cp "$tmp/$package/rw"        "$target_dir/rw"
cp "$tmp/$package/rw-daemon" "$target_dir/rw-daemon"
chmod 755 "$target_dir/rw" "$target_dir/rw-daemon"

info "installed rw and rw-daemon to $target_dir"

case ":$PATH:" in
    *":$target_dir:"*) ;;
    *)
        info "$target_dir is not on PATH"
        info "add this to your shell config: export PATH=\"$target_dir:\$PATH\""
        ;;
esac

info "next: run 'rw init --install' to enable shell recording"
