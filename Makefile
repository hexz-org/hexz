# ═══════════════════════════════════════════════════════════════════════════════
#  Hexz — Build, Test, and Release
# ═══════════════════════════════════════════════════════════════════════════════
#  Run from repo root.  make help  for targets.
# ═══════════════════════════════════════════════════════════════════════════════

SHELL           := bash
.DEFAULT_GOAL   := help

# ── Paths & tools ─────────────────────────────────────────────────────────────
PYTHON_CRATE    := crates/python
MATURIN         := .venv/bin/maturin
CARGO           := cargo
PYTHON          := .venv/bin/python

# ── Colors ────────────────────────────────────────────────────────────────────
GREEN  := \033[32m
CYAN   := \033[36m
BOLD   := \033[1m
RESET  := \033[0m

# ── Phony ────────────────────────────────────────────────────────────────────
.PHONY: help build rust python develop install clean clippy check fmt test

help:
	@printf "\n$(BOLD)Hexz$(RESET) — snapshot storage engine\n\n"
	@printf "  $(CYAN)Build$(RESET)\n"
	@printf "    %-35s  Build Rust workspace + Python wheel\n" "make build"
	@printf "    %-35s  Build Rust workspace only\n" "make rust"
	@printf "    %-35s  Build Python extension wheel\n" "make python"
	@printf "    %-35s  Install Python extension (editable)\n" "make develop"
	@printf "    %-35s  Install hexz CLI locally\n" "make install"
	@printf "    %-35s  Run all workspace tests\n" "make test"
	@printf "\n  $(CYAN)Quality$(RESET)\n"
	@printf "    %-35s  Run clippy with strict lints\n" "make clippy"
	@printf "    %-35s  Fast workspace type check\n" "make check"
	@printf "    %-35s  Auto-format Rust code\n" "make fmt"
	@printf "\n"

build: rust python

rust:
	@printf "$(GREEN)Building Rust workspace…$(RESET)\n"
	$(CARGO) build --release --workspace

python:
	@printf "$(GREEN)Building Python wheel…$(RESET)\n"
	$(MATURIN) build --release --manifest-path $(PYTHON_CRATE)/Cargo.toml

develop:
	@printf "$(GREEN)Installing Python extension (editable)…$(RESET)\n"
	$(MATURIN) develop --manifest-path $(PYTHON_CRATE)/Cargo.toml

install:
	@printf "$(GREEN)Installing hexz CLI…$(RESET)\n"
	$(CARGO) install --path crates/cli

clippy:
	@printf "$(GREEN)Running clippy…$(RESET)\n"
	$(CARGO) clippy --workspace --all-targets -- -D warnings

check:
	@printf "$(GREEN)Type-checking workspace…$(RESET)\n"
	$(CARGO) check --workspace --all-targets

fmt:
	@printf "$(GREEN)Formatting…$(RESET)\n"
	$(CARGO) fmt --all

test:
	@printf "$(GREEN)Running tests…$(RESET)\n"
	$(CARGO) test --workspace

clean:
	@printf "$(GREEN)Cleaning artifacts…$(RESET)\n"
	$(CARGO) clean
