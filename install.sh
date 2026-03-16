#!/bin/sh
# Hexz installer — downloads the latest release binary from GitHub.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/hexz-org/hexz/main/install.sh | sh
#
# Environment:
#   HEXZ_INSTALL_DIR  — where to place the binary (default: ~/.local/bin)
#   HEXZ_VERSION      — specific version to install (default: latest)

set -eu

REPO="hexz-org/hexz"
INSTALL_DIR="${HEXZ_INSTALL_DIR:-$HOME/.local/bin}"

# --- Detect platform ---

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)  os="unknown-linux-gnu" ;;
    Darwin) os="apple-darwin" ;;
    *)      echo "Error: unsupported OS: $OS"; exit 1 ;;
esac

case "$ARCH" in
    x86_64)  arch="x86_64" ;;
    aarch64|arm64) arch="aarch64" ;;
    *)       echo "Error: unsupported architecture: $ARCH"; exit 1 ;;
esac

TARGET="${arch}-${os}"

# --- Resolve version ---

if [ -n "${HEXZ_VERSION:-}" ]; then
    VERSION="$HEXZ_VERSION"
else
    VERSION="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
        | grep '"tag_name"' | head -1 | cut -d '"' -f 4)"
    if [ -z "$VERSION" ]; then
        echo "Error: could not determine latest version."
        exit 1
    fi
fi

# --- Download ---

BINARY_NAME="hexz-${TARGET}"
URL="https://github.com/${REPO}/releases/download/${VERSION}/${BINARY_NAME}"

echo "Installing hexz ${VERSION} for ${TARGET}..."
echo "  ${URL}"

mkdir -p "$INSTALL_DIR"

if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$URL" -o "${INSTALL_DIR}/hexz"
elif command -v wget >/dev/null 2>&1; then
    wget -q "$URL" -O "${INSTALL_DIR}/hexz"
else
    echo "Error: curl or wget required."
    exit 1
fi

chmod +x "${INSTALL_DIR}/hexz"

echo "Installed hexz to ${INSTALL_DIR}/hexz"

# --- Check PATH ---

case ":${PATH}:" in
    *":${INSTALL_DIR}:"*) ;;
    *)
        echo ""
        echo "Note: ${INSTALL_DIR} is not in your PATH. Add it with:"
        echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
        ;;
esac
