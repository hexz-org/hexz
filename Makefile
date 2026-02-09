# ──────────────────────────────────────────────────────────────────────────────
# Strata — Build, Test, and Release Automation
# ──────────────────────────────────────────────────────────────────────────────
# Run from repository root.  `make help` for available targets.

SHELL      := /bin/bash
.DEFAULT_GOAL := help

# ── Paths ────────────────────────────────────────────────────────────────────
LOADER_CRATE := crates/loader
MATURIN      ?= maturin
CARGO        ?= cargo

# ── Colors (only when stdout is a terminal) ──────────────────────────────────
ifneq ($(TERM),)
  GREEN  := \033[32m
  CYAN   := \033[36m
  BOLD   := \033[1m
  RESET  := \033[0m
else
  GREEN  :=
  CYAN   :=
  BOLD   :=
  RESET  :=
endif

# ── Phony targets ────────────────────────────────────────────────────────────
.PHONY: help build rust python install clean \
        test test-rust test-python test-integration \
        lint fmt clippy deny check \
        bench fuzz \
        docker-dev docker-bench \
        docs setup ci

# ═══════════════════════════════════════════════════════════════════════════════
#  Help
# ═══════════════════════════════════════════════════════════════════════════════
help:
	@printf "$(BOLD)Strata$(RESET) — snapshot storage engine\n\n"
	@printf "$(CYAN)Build$(RESET)\n"
	@printf "  make build         Build Rust workspace + Python wheel (release)\n"
	@printf "  make rust          Build Rust workspace only (release)\n"
	@printf "  make python        Build Python extension wheel via Maturin\n"
	@printf "  make install       Install the strata CLI locally\n"
	@printf "\n$(CYAN)Test$(RESET)\n"
	@printf "  make test          Run all tests (Rust + Python)\n"
	@printf "  make test-rust     Run Rust tests only\n"
	@printf "  make test-python   Run Python tests only\n"
	@printf "\n$(CYAN)Quality$(RESET)\n"
	@printf "  make lint          Run clippy + fmt check + deny\n"
	@printf "  make fmt           Auto-format all Rust code\n"
	@printf "  make clippy        Run clippy with strict lints\n"
	@printf "  make deny          Run cargo-deny (licenses + advisories)\n"
	@printf "  make check         Fast workspace-wide type check\n"
	@printf "\n$(CYAN)Performance$(RESET)\n"
	@printf "  make bench         Run criterion benchmarks\n"
	@printf "  make fuzz          Run fuzz targets (requires cargo-fuzz)\n"
	@printf "\n$(CYAN)Infrastructure$(RESET)\n"
	@printf "  make docker-dev    Build the development Docker image\n"
	@printf "  make docker-bench  Build the benchmark Docker image\n"
	@printf "  make docs          Build rustdoc for the workspace\n"
	@printf "  make setup         Install required tools (rustfmt, clippy, etc.)\n"
	@printf "  make ci            Full CI pipeline (lint + test + build)\n"
	@printf "\n$(CYAN)Housekeeping$(RESET)\n"
	@printf "  make clean         Remove all build artifacts\n"

# ═══════════════════════════════════════════════════════════════════════════════
#  Build
# ═══════════════════════════════════════════════════════════════════════════════
build: rust python

rust:
	@printf "$(GREEN)Building Rust workspace (release)…$(RESET)\n"
	$(CARGO) build --release --workspace

python:
	@printf "$(GREEN)Building Python wheel…$(RESET)\n"
	$(MATURIN) build --release --manifest-path $(LOADER_CRATE)/Cargo.toml

install:
	@printf "$(GREEN)Installing strata CLI…$(RESET)\n"
	$(CARGO) install --path crates/cli

# ═══════════════════════════════════════════════════════════════════════════════
#  Test
# ═══════════════════════════════════════════════════════════════════════════════
test: test-rust test-python

test-rust:
	@printf "$(GREEN)Running Rust tests…$(RESET)\n"
	$(CARGO) test --workspace

test-python:
	@printf "$(GREEN)Running Python tests…$(RESET)\n"
	cd $(LOADER_CRATE) && $(MATURIN) develop -E test,numpy && .venv/bin/python -m pytest tests/ -v

test-integration:
	@printf "$(GREEN)Running integration tests…$(RESET)\n"
	$(CARGO) test --workspace -- --ignored

# ═══════════════════════════════════════════════════════════════════════════════
#  Quality
# ═══════════════════════════════════════════════════════════════════════════════
lint: fmt-check clippy deny

fmt:
	@printf "$(GREEN)Formatting…$(RESET)\n"
	$(CARGO) fmt --all

fmt-check:
	@printf "$(GREEN)Checking format…$(RESET)\n"
	$(CARGO) fmt --all -- --check

clippy:
	@printf "$(GREEN)Running clippy…$(RESET)\n"
	$(CARGO) clippy --workspace --all-targets -- -D warnings

deny:
	@printf "$(GREEN)Running cargo-deny…$(RESET)\n"
	$(CARGO) deny check

check:
	@printf "$(GREEN)Type-checking workspace…$(RESET)\n"
	$(CARGO) check --workspace --all-targets

# ═══════════════════════════════════════════════════════════════════════════════
#  Performance
# ═══════════════════════════════════════════════════════════════════════════════
bench:
	@printf "$(GREEN)Running benchmarks…$(RESET)\n"
	$(CARGO) bench --package strata

fuzz:
	@printf "$(GREEN)Running fuzz targets (60 s each)…$(RESET)\n"
	cd fuzz && $(CARGO) +nightly fuzz run decompress -- -max_total_time=60
	cd fuzz && $(CARGO) +nightly fuzz run index_parser -- -max_total_time=60

# ═══════════════════════════════════════════════════════════════════════════════
#  Infrastructure
# ═══════════════════════════════════════════════════════════════════════════════
docker-dev:
	@printf "$(GREEN)Building dev container…$(RESET)\n"
	docker build -f docker/dev.Dockerfile -t strata-dev .

docker-bench:
	@printf "$(GREEN)Building benchmark container…$(RESET)\n"
	docker build -f docker/bench.Dockerfile -t strata-bench .

docs:
	@printf "$(GREEN)Building documentation…$(RESET)\n"
	$(CARGO) doc --workspace --no-deps --document-private-items

setup:
	@printf "$(GREEN)Installing development tools…$(RESET)\n"
	rustup component add rustfmt clippy
	cargo install cargo-deny cargo-fuzz maturin
	@printf "$(GREEN)Done. Run 'make check' to verify.$(RESET)\n"

# ═══════════════════════════════════════════════════════════════════════════════
#  CI Pipeline (runs everything in sequence)
# ═══════════════════════════════════════════════════════════════════════════════
ci: lint test-rust build
	@printf "$(GREEN)CI pipeline passed.$(RESET)\n"

# ═══════════════════════════════════════════════════════════════════════════════
#  Clean
# ═══════════════════════════════════════════════════════════════════════════════
clean:
	@printf "$(GREEN)Cleaning…$(RESET)\n"
	$(CARGO) clean
	rm -rf $(LOADER_CRATE)/build $(LOADER_CRATE)/dist
	find . -type d -name __pycache__ -exec rm -rf {} + 2>/dev/null || true
	find . -type f -name "*.pyc" -delete 2>/dev/null || true
	find . -type f -name "*.pyo" -delete 2>/dev/null || true
	find . -type d -name "*.egg-info" -exec rm -rf {} + 2>/dev/null || true
	find . -type d -name ".eggs" -exec rm -rf {} + 2>/dev/null || true
	rm -rf target/wheels
	@printf "$(GREEN)Clean complete.$(RESET)\n"
