#!/bin/bash
# Test wheel building locally before pushing to GitHub

set -e

# Get to repo root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$REPO_ROOT"

echo "🧪 Testing Python wheel build locally..."
echo

# Activate venv if it exists
if [ -d ".venv" ]; then
    echo "📦 Using .venv"
    source .venv/bin/activate
else
    echo "⚠️  No .venv found. Run 'make setup' first!"
    exit 1
fi

# Check if maturin is installed
if ! command -v maturin &> /dev/null; then
    echo "❌ maturin not found. Installing..."
    pip install 'maturin[patchelf]'
fi

# Determine platform
if [[ "$OSTYPE" == "darwin"* ]]; then
    echo "🍎 Detected macOS"
    # Check if OpenSSL is installed
    if ! brew list openssl@3 &>/dev/null; then
        echo "Installing OpenSSL via Homebrew..."
        brew install openssl@3
    fi
    export OPENSSL_DIR=$(brew --prefix openssl@3)
else
    echo "🐧 Detected Linux"
    # Use system OpenSSL
    export OPENSSL_NO_VENDOR=1
fi

# Build the wheel
echo
echo "📦 Building wheel..."
cd crates/loader

maturin build --release

echo
echo "✅ Wheel build successful!"
echo
echo "To install locally:"
echo "  pip install ../../target/wheels/*.whl"
echo
echo "To test:"
echo "  python -c 'import hexz; print(hexz.__version__)'"
