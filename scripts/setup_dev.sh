#!/usr/bin/env bash
# ──────────────────────────────────────────────────────────────────────────────
# Strata — One-command development environment setup
# ──────────────────────────────────────────────────────────────────────────────
# Usage:  ./scripts/setup_dev.sh
#
# Installs Rust toolchain, Python venv, development tools, and verifies
# that the workspace compiles. Safe to re-run (idempotent).
# ──────────────────────────────────────────────────────────────────────────────

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

info()  { printf '\033[36m[setup]\033[0m %s\n' "$*"; }
ok()    { printf '\033[32m[setup]\033[0m %s\n' "$*"; }
warn()  { printf '\033[33m[setup]\033[0m %s\n' "$*"; }
fail()  { printf '\033[31m[setup]\033[0m %s\n' "$*" >&2; exit 1; }

# ── Rust ─────────────────────────────────────────────────────────────────────
info "Checking Rust toolchain…"
if ! command -v rustup &>/dev/null; then
    info "Installing Rust via rustup…"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

rustup update stable
rustup default stable
rustup component add rustfmt clippy

info "Installing Cargo tools…"
cargo install --locked cargo-deny 2>/dev/null || warn "cargo-deny already installed"
cargo install --locked cargo-fuzz 2>/dev/null || warn "cargo-fuzz already installed"
cargo install --locked maturin    2>/dev/null || warn "maturin already installed"

# ── System dependencies ──────────────────────────────────────────────────────
info "Checking system dependencies…"
MISSING=()

command -v pkg-config &>/dev/null || MISSING+=("pkg-config")

if [[ "$(uname)" == "Linux" ]]; then
    if command -v pacman &>/dev/null; then
        if ! pacman -Q fuse3 &>/dev/null; then
            MISSING+=("fuse3")
        fi
    elif command -v dpkg &>/dev/null; then
        if ! dpkg -s libfuse-dev &>/dev/null 2>&1; then
            MISSING+=("libfuse-dev")
        fi
    else
        warn "Could not detect package manager (pacman/dpkg). Skipping detailed dependency checks."
    fi
elif [[ "$(uname)" == "Darwin" ]]; then
    if ! brew list macfuse &>/dev/null 2>&1; then
        warn "macFUSE not found — install from https://osxfuse.github.io"
    fi
fi

if [[ ${#MISSING[@]} -gt 0 ]]; then
    warn "Missing system packages: ${MISSING[*]}"
    if command -v pacman &>/dev/null; then
        info "Installing via pacman…"
        sudo pacman -S --needed --noconfirm "${MISSING[@]}"
    elif [[ "$(uname)" == "Linux" ]] && command -v apt-get &>/dev/null; then
        info "Installing via apt-get…"
        sudo apt-get update && sudo apt-get install -y "${MISSING[@]}"
    else
        fail "Please install manually: ${MISSING[*]}"
    fi
fi

# ── Python ───────────────────────────────────────────────────────────────────
info "Setting up Python virtual environment…"
cd "$ROOT_DIR"

if [[ ! -d .venv ]]; then
    python3 -m venv .venv
fi
source .venv/bin/activate

pip install --quiet --upgrade pip
pip install --quiet pytest pytest-asyncio numpy maturin torch
# Python API docs (Sphinx); optional deps for examples
pip install --quiet -r docs/requirements.txt

# ── Build check ──────────────────────────────────────────────────────────────
info "Verifying Rust workspace compiles…"
cd "$ROOT_DIR"
cargo check --workspace

info "Building Python extension…"
cd "$ROOT_DIR/crates/loader"
maturin develop 2>/dev/null || warn "Python extension build skipped (may need manual setup)"

# ── Done ─────────────────────────────────────────────────────────────────────
ok "Development environment ready!"
ok ""
ok "Quick start:"
ok "  source .venv/bin/activate"
ok "  make check        # type-check workspace"
ok "  make test         # run all tests"
ok "  make build        # release build"
ok "  make docs-python  # build Python API docs (docs/_build/html)"
