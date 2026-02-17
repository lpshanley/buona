#!/bin/sh
# Install buona from GitHub Releases.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/lpshanley/buona/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/lpshanley/buona/main/install.sh | sh -s -- v0.1.5
#
# Environment variables:
#   BUONA_INSTALL_DIR  Override install directory (default: ~/.local/bin)

set -e

REPO="lpshanley/buona"
INSTALL_DIR="${BUONA_INSTALL_DIR:-$HOME/.local/bin}"

main() {
    need_cmd uname
    need_cmd mktemp
    need_cmd chmod

    local _target
    _target="$(detect_target)" || exit 1

    local _tag
    if [ -n "${1:-}" ]; then
        _tag="$1"
        # Normalize: "0.1.5" → "v0.1.5"
        case "$_tag" in
            v*) ;;
            *)  _tag="v$_tag" ;;
        esac
    else
        _tag="$(fetch_latest_tag)" || exit 1
    fi

    echo "  Installing buona ${_tag} for ${_target}"
    echo ""

    local _archive="buona-${_tag}-${_target}.tar.gz"
    local _url="https://github.com/${REPO}/releases/download/${_tag}/${_archive}"
    local _checksum_url="${_url}.sha256"

    # Create temp directory with cleanup trap
    local _tmpdir
    _tmpdir="$(mktemp -d)" || { echo "Error: failed to create temp directory"; exit 1; }
    trap "rm -rf '$_tmpdir'" EXIT

    # Download archive
    echo "  Downloading ${_archive} ..."
    download "$_url" "$_tmpdir/${_archive}"

    # Download and verify checksum
    if download "$_checksum_url" "$_tmpdir/${_archive}.sha256" 2>/dev/null; then
        echo "  Verifying checksum ..."
        verify_checksum "$_tmpdir/${_archive}" "$_tmpdir/${_archive}.sha256"
    fi

    # Extract binary
    echo "  Extracting ..."
    tar -xzf "$_tmpdir/${_archive}" -C "$_tmpdir"

    # Install
    mkdir -p "$INSTALL_DIR"
    mv "$_tmpdir/buona" "$INSTALL_DIR/buona"
    chmod +x "$INSTALL_DIR/buona"

    echo ""
    echo "  Installed buona to ${INSTALL_DIR}/buona"

    # Check if install dir is in PATH
    case ":$PATH:" in
        *":${INSTALL_DIR}:"*) ;;
        *)
            echo ""
            echo "  Warning: ${INSTALL_DIR} is not in your PATH."
            echo "  Add it to your shell profile:"
            echo ""
            echo "    export PATH=\"${INSTALL_DIR}:\$PATH\""
            echo ""
            ;;
    esac
}

detect_target() {
    local _os _arch _target

    _os="$(uname -s)"
    _arch="$(uname -m)"

    case "$_os" in
        Linux)  _os="unknown-linux-gnu" ;;
        Darwin) _os="apple-darwin" ;;
        *)      echo "Error: unsupported OS: $_os"; exit 1 ;;
    esac

    case "$_arch" in
        x86_64|amd64)   _arch="x86_64" ;;
        aarch64|arm64)  _arch="aarch64" ;;
        *)              echo "Error: unsupported architecture: $_arch"; exit 1 ;;
    esac

    _target="${_arch}-${_os}"
    echo "$_target"
}

fetch_latest_tag() {
    local _url="https://api.github.com/repos/${REPO}/releases/latest"
    local _response

    if check_cmd curl; then
        _response="$(curl -fsSL "$_url")" || {
            echo "Error: could not fetch latest release from GitHub"
            exit 1
        }
    elif check_cmd wget; then
        _response="$(wget -qO- "$_url")" || {
            echo "Error: could not fetch latest release from GitHub"
            exit 1
        }
    else
        echo "Error: need curl or wget to fetch releases"
        exit 1
    fi

    # Parse tag_name from JSON (portable, no jq dependency)
    echo "$_response" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -1
}

download() {
    local _url="$1"
    local _dest="$2"

    if check_cmd curl; then
        curl -fsSL "$_url" -o "$_dest" || {
            echo "Error: failed to download $_url"
            exit 1
        }
    elif check_cmd wget; then
        wget -q "$_url" -O "$_dest" || {
            echo "Error: failed to download $_url"
            exit 1
        }
    else
        echo "Error: need curl or wget"
        exit 1
    fi
}

verify_checksum() {
    local _file="$1"
    local _checksum_file="$2"
    local _expected _actual

    _expected="$(cut -d ' ' -f 1 < "$_checksum_file")"

    if check_cmd sha256sum; then
        _actual="$(sha256sum "$_file" | cut -d ' ' -f 1)"
    elif check_cmd shasum; then
        _actual="$(shasum -a 256 "$_file" | cut -d ' ' -f 1)"
    else
        echo "  Warning: cannot verify checksum (no sha256sum or shasum found)"
        return 0
    fi

    if [ "$_actual" != "$_expected" ]; then
        echo "Error: checksum mismatch!"
        echo "  expected: $_expected"
        echo "  actual:   $_actual"
        exit 1
    fi

    echo "  Checksum verified"
}

check_cmd() {
    command -v "$1" > /dev/null 2>&1
}

need_cmd() {
    if ! check_cmd "$1"; then
        echo "Error: need '$1' (command not found)"
        exit 1
    fi
}

main "$@"
