# ═══════════════════════════════════════════════════════════════════════════════
#  Strata — Build, Test, and Release
# ═══════════════════════════════════════════════════════════════════════════════
#  Run from repo root.  make help  for targets.
#  Override tools:  CARGO=cargo, MATURIN=maturin  or put them in .make.env
# ═══════════════════════════════════════════════════════════════════════════════

-include .make.env
SHELL           := /bin/bash
.DEFAULT_GOAL   := help

# ── Paths & tools ─────────────────────────────────────────────────────────────
LOADER_CRATE    := crates/loader
BENCH_PACKAGE   := strata
CRITERION_DIR   := target/criterion
BENCH_STORE_DIR := .criterion
BENCH_CMP_TMP   := _cmp

MATURIN         ?= maturin
CARGO           ?= cargo
RUFF            := $(shell [ -f .venv/bin/ruff ] && echo .venv/bin/ruff || echo ruff)

# ── Pass-through args ─────────────────────────────────────────────────────────
#  make test cache            →  cargo test -- cache
#  make test-python writer    →  pytest -k writer
#  make bench <name>  →  cargo bench --bench <name> (only that binary); make bench  →  all
#  make run serve             →  cargo run -- serve
ifneq (,$(filter test test-rust test-python,$(firstword $(MAKECMDGOALS))))
  TEST_ARGS := $(wordlist 2,$(words $(MAKECMDGOALS)),$(MAKECMDGOALS))
  $(foreach a,$(TEST_ARGS),$(eval $a:;@:))
endif
ifneq (,$(filter bench,$(firstword $(MAKECMDGOALS))))
  BENCH_ARGS   := $(wordlist 2,$(words $(MAKECMDGOALS)),$(MAKECMDGOALS))
  BENCH_BIN    := $(firstword $(BENCH_ARGS))
  BENCH_EXTRA  := $(wordlist 2,$(words $(BENCH_ARGS)),$(BENCH_ARGS))
  $(foreach a,$(BENCH_ARGS),$(eval $a:;@:))
endif
ifneq (,$(filter run,$(firstword $(MAKECMDGOALS))))
  RUN_ARGS := $(wordlist 2,$(words $(MAKECMDGOALS)),$(MAKECMDGOALS))
  $(foreach a,$(RUN_ARGS),$(eval $a:;@:))
endif

# ── Baseline args ────────────────────────────────────────────────────────────
#  save/archive/restore-baseline <name>;  compare-baseline <a> <b>
#  bench-compare <archived> [filter]  (filter = substring of bench name, e.g. cache)
ifneq (,$(filter save-baseline archive-baseline restore-baseline,$(firstword $(MAKECMDGOALS))))
  BASELINE_NAME := $(word 2,$(MAKECMDGOALS))
  $(eval $(BASELINE_NAME):;@:)
endif
ifneq (,$(filter bench-compare,$(firstword $(MAKECMDGOALS))))
  BENCH_CMP_ARCHIVED := $(word 2,$(MAKECMDGOALS))
  BENCH_CMP_FILTER   := $(word 3,$(MAKECMDGOALS))
  $(eval $(BENCH_CMP_ARCHIVED):;@:)
  $(if $(BENCH_CMP_FILTER),$(eval $(BENCH_CMP_FILTER):;@:))
endif
ifneq (,$(filter compare-baseline,$(firstword $(MAKECMDGOALS))))
  BASE_OLD := $(word 2,$(MAKECMDGOALS))
  BASE_NEW := $(word 3,$(MAKECMDGOALS))
  $(eval $(BASE_OLD):;@:)
  $(eval $(BASE_NEW):;@:)
endif

# ── Colors (only when stdout is a terminal) ───────────────────────────────────
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

# ── Phony ────────────────────────────────────────────────────────────────────
.PHONY: help build rust python develop install run clean
.PHONY: test test-rust test-python test-integration test-list
.PHONY: lint fmt fmt-check clippy deny check
.PHONY: bench bench-list bench-compare save-baseline archive-baseline restore-baseline compare-baseline fuzz
.PHONY: docker-dev docker-bench docs docs-python setup setup-check ci

# ═══════════════════════════════════════════════════════════════════════════════
#  Help
# ═══════════════════════════════════════════════════════════════════════════════
HELP_W := 35
help:
	@printf "\n$(BOLD)Strata$(RESET) — snapshot storage engine\n\n"
	@printf "  $(CYAN)Build$(RESET)\n"
	@printf "    %-$(HELP_W)s  Build Rust workspace + Python wheel (release)\n" "make build"
	@printf "    %-$(HELP_W)s  Build Rust workspace only (release)\n" "make rust"
	@printf "    %-$(HELP_W)s  Build Python extension wheel (dist)\n" "make python"
	@printf "    %-$(HELP_W)s  Install Python extension (editable)\n" "make develop"
	@printf "    %-$(HELP_W)s  Install strata CLI locally\n" "make install"
	@printf "    %-$(HELP_W)s  Run CLI; e.g. make run serve\n" "make run [args]"
	@printf "\n  $(CYAN)Test$(RESET)\n"
	@printf "    %-$(HELP_W)s  All tests; optional filter (e.g. make test cache)\n" "make test [filter]"
	@printf "    %-$(HELP_W)s  Rust tests only\n" "make test-rust [filter]"
	@printf "    %-$(HELP_W)s  Python tests only (pytest -k)\n" "make test-python [filter]"
	@printf "    %-$(HELP_W)s  List test categories for filtering\n" "make test-list"
	@printf "\n  $(CYAN)Quality$(RESET)\n"
	@printf "    %-$(HELP_W)s  Format check + clippy\n" "make lint"
	@printf "    %-$(HELP_W)s  Auto-format Rust + Python\n" "make fmt"
	@printf "    %-$(HELP_W)s  Clippy with strict lints\n" "make clippy"
	@printf "    %-$(HELP_W)s  Licenses + advisories (optional)\n" "make deny"
	@printf "    %-$(HELP_W)s  Fast workspace type check\n" "make check"
	@printf "\n  $(CYAN)Performance$(RESET)\n"
	@printf "    %-$(HELP_W)s  Run benchmarks; optional filter\n" "make bench [filter]"
	@printf "    %-$(HELP_W)s  List benchmark categories for filtering\n" "make bench-list"
	@printf "    %-$(HELP_W)s  Run benchmarks [filter], compare to archived baseline\n" "make bench-compare <name> [filter]"
	@printf "    %-$(HELP_W)s  Save current run as baseline\n" "make save-baseline <name>"
	@printf "    %-$(HELP_W)s  Archive baseline to $(BENCH_STORE_DIR)/\n" "make archive-baseline <name>"
	@printf "    %-$(HELP_W)s  Restore baseline from archive\n" "make restore-baseline <name>"
	@printf "    %-$(HELP_W)s  Compare two archived baselines (critcmp)\n" "make compare-baseline <a> <b>"
	@printf "    %-$(HELP_W)s  Fuzz targets (cargo-fuzz)\n" "make fuzz"
	@printf "\n  $(CYAN)Infrastructure$(RESET)\n"
	@printf "    %-$(HELP_W)s  Build dev Docker image\n" "make docker-dev"
	@printf "    %-$(HELP_W)s  Build benchmark Docker image\n" "make docker-bench"
	@printf "    %-$(HELP_W)s  rustdoc for workspace\n" "make docs"
	@printf "    %-$(HELP_W)s  Sphinx API docs (docs/_build/html)\n" "make docs-python"
	@printf "    %-$(HELP_W)s  Dev tools + Python venv\n" "make setup"
	@printf "    %-$(HELP_W)s  Verify system deps\n" "make setup-check"
	@printf "    %-$(HELP_W)s  Full CI (lint + test + build)\n" "make ci"
	@printf "\n  $(CYAN)Housekeeping$(RESET)\n"
	@printf "    %-$(HELP_W)s  Remove build artifacts\n" "make clean"
	@printf "\n"

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

develop:
	@printf "$(GREEN)Installing Python extension (editable)…$(RESET)\n"
	$(MATURIN) develop --release --manifest-path $(LOADER_CRATE)/Cargo.toml

install:
	@printf "$(GREEN)Installing strata CLI…$(RESET)\n"
	$(CARGO) install --path crates/cli

run:
	$(CARGO) run --package $(BENCH_PACKAGE) -- $(RUN_ARGS)

# ═══════════════════════════════════════════════════════════════════════════════
#  Test
# ═══════════════════════════════════════════════════════════════════════════════
test: test-rust test-python

test-rust:
	@printf "$(GREEN)Running Rust tests…$(RESET)\n"
	$(CARGO) test --workspace -- $(TEST_ARGS)

test-python:
	@printf "$(GREEN)Running Python tests…$(RESET)\n"
	cd $(LOADER_CRATE) && $(MATURIN) develop -E test,numpy && .venv/bin/python -m pytest tests/ -v $(if $(TEST_ARGS),-k "$(TEST_ARGS)",)

test-integration:
	@printf "$(GREEN)Running integration tests…$(RESET)\n"
	$(CARGO) test --workspace -- --ignored

test-list:
	@TMP=$$(mktemp); \
	$(CARGO) test --workspace -- --list > "$$TMP" 2>/dev/null; \
	printf "$(GREEN)Rust test categories (use with make test <category>)…$(RESET)\n\n"; \
	sed -n 's/: test$$//p' "$$TMP" | grep '::' | cut -d: -f1 | sort -u; \
	rm -f "$$TMP"

# ═══════════════════════════════════════════════════════════════════════════════
#  Quality
# ═══════════════════════════════════════════════════════════════════════════════
lint: fmt-check clippy

fmt:
	@printf "$(GREEN)Formatting…$(RESET)\n"
	$(CARGO) fmt --all
	$(RUFF) format .
	$(RUFF) check --fix .

fmt-check:
	@printf "$(GREEN)Checking format…$(RESET)\n"
	$(CARGO) fmt --all -- --check
	$(RUFF) format --check .
	$(RUFF) check .

clippy:
	@printf "$(GREEN)Running clippy…$(RESET)\n"
	$(CARGO) clippy --workspace --all-targets -- -D warnings

deny:  # optional: licenses + RustSec; not required for make ci
	@printf "$(GREEN)Running cargo-deny…$(RESET)\n"
	@command -v cargo-deny >/dev/null 2>&1 || { printf "$(CYAN)Install with: make setup$(RESET)\n"; exit 1; }
	$(CARGO) deny check

check:
	@printf "$(GREEN)Type-checking workspace…$(RESET)\n"
	$(CARGO) check --workspace --all-targets

# ═══════════════════════════════════════════════════════════════════════════════
#  Performance
# ═══════════════════════════════════════════════════════════════════════════════
bench:
	@printf "$(GREEN)Running benchmarks…$(RESET)\n"
	@if [ -n "$(BENCH_BIN)" ]; then \
		$(CARGO) bench --package $(BENCH_PACKAGE) --bench $(BENCH_BIN) -- $(BENCH_EXTRA); \
	else \
		$(CARGO) bench --package $(BENCH_PACKAGE) -- $(BENCH_ARGS); \
	fi

bench-list:
	@BENCH_DIR=""; \
	if [ -d "$(CRITERION_DIR)" ]; then BENCH_DIR="$(CRITERION_DIR)"; \
	elif [ -d "$(BENCH_STORE_DIR)" ]; then for d in $(BENCH_STORE_DIR)/*/; do [ -d "$$d" ] && BENCH_DIR="$$d" && break; done; fi; \
	if [ -z "$$BENCH_DIR" ]; then \
	  printf "$(BOLD)No criterion data. Run 'make bench' or have a baseline in $(BENCH_STORE_DIR)/$(RESET)\n"; exit 1; \
	fi; \
	printf "$(GREEN)Benchmark categories (use with make bench <category> or bench-compare <baseline> <category>)…$(RESET)\n\n"; \
	find "$$BENCH_DIR" -name 'benchmark.json' 2>/dev/null | sed "s|$$BENCH_DIR/||;s|/[^/]*/benchmark.json||" | cut -d/ -f1 | sort -u

save-baseline:
	@if [ -z "$(BASELINE_NAME)" ]; then \
		echo "$(BOLD)Usage:$(RESET) make save-baseline <name>"; \
		exit 1; \
	fi
	@printf "$(GREEN)Running benchmarks and saving baseline '$(BASELINE_NAME)'…$(RESET)\n"
	$(CARGO) bench -p $(BENCH_PACKAGE) -- --save-baseline $(BASELINE_NAME)

archive-baseline:
	@if [ -z "$(BASELINE_NAME)" ]; then \
		echo "$(BOLD)Usage:$(RESET) make archive-baseline <name>"; \
		exit 1; \
	fi
	@printf "$(GREEN)Archiving baseline to $(BENCH_STORE_DIR)/$(BASELINE_NAME)...$(RESET)\n"
	@mkdir -p $(BENCH_STORE_DIR)/$(BASELINE_NAME)
	@if [ -d "$(CRITERION_DIR)" ]; then \
		cp -r $(CRITERION_DIR)/* $(BENCH_STORE_DIR)/$(BASELINE_NAME)/; \
		printf "$(CYAN)Baseline '$(BASELINE_NAME)' archived to $(BENCH_STORE_DIR)/$(BASELINE_NAME).$(RESET)\n"; \
	else \
		echo "$(BOLD)Error:$(RESET) No criterion directory found at $(CRITERION_DIR). Run 'make save-baseline <name>' first."; \
		exit 1; \
	fi

restore-baseline:
	@if [ -z "$(BASELINE_NAME)" ]; then \
		echo "$(BOLD)Usage:$(RESET) make restore-baseline <name>"; \
		exit 1; \
	fi
	@printf "$(GREEN)Restoring baseline '$(BASELINE_NAME)' from archive...$(RESET)\n"
	@if [ -d "$(BENCH_STORE_DIR)/$(BASELINE_NAME)" ]; then \
		rm -rf $(CRITERION_DIR); \
		mkdir -p $(CRITERION_DIR); \
		cp -r $(BENCH_STORE_DIR)/$(BASELINE_NAME)/* $(CRITERION_DIR)/; \
		printf "$(CYAN)Baseline '$(BASELINE_NAME)' restored to $(CRITERION_DIR).$(RESET)\n"; \
	else \
		echo "$(BOLD)Error:$(RESET) Baseline archive '$(BASELINE_NAME)' not found in $(BENCH_STORE_DIR)."; \
		exit 1; \
	fi

compare-baseline:
	@if [ -z "$(BASE_OLD)" ] || [ -z "$(BASE_NEW)" ]; then \
		echo "$(BOLD)Usage:$(RESET) make compare-baseline <old> <new>"; \
		exit 1; \
	fi
	@command -v critcmp >/dev/null 2>&1 || { echo "$(BOLD)Error:$(RESET) critcmp not found. Install with 'cargo install critcmp'."; exit 1; }
	@printf "$(GREEN)Preparing to compare '$(BASE_OLD)' vs '$(BASE_NEW)'...$(RESET)\n"
	@mkdir -p $(CRITERION_DIR)
	@if [ -d "$(BENCH_STORE_DIR)/$(BASE_OLD)" ]; then \
		cp -rn $(BENCH_STORE_DIR)/$(BASE_OLD)/* $(CRITERION_DIR)/ 2>/dev/null || true; \
	fi
	@if [ -d "$(BENCH_STORE_DIR)/$(BASE_NEW)" ]; then \
		cp -rn $(BENCH_STORE_DIR)/$(BASE_NEW)/* $(CRITERION_DIR)/ 2>/dev/null || true; \
	fi
	@printf "$(GREEN)Running critcmp...$(RESET)\n"
	critcmp $(BASE_OLD) $(BASE_NEW)

# Run current benchmarks (optionally filtered), then compare to an archived baseline.
#  make bench-compare v0.1.0-alpha         →  all benches, then critcmp
#  make bench-compare v0.1.0-alpha cache   →  only benches matching "cache", then critcmp
bench-compare:
	@if [ -z "$(BENCH_CMP_ARCHIVED)" ]; then \
		printf "$(BOLD)Usage:$(RESET) make bench-compare <archived> [filter]\n"; \
		printf "  Run benchmarks (optional filter, e.g. cache) then compare to archived baseline.\n"; \
		exit 1; \
	fi
	@command -v critcmp >/dev/null 2>&1 || { printf "$(BOLD)Error:$(RESET) critcmp not found. Install with 'cargo install critcmp'.\n"; exit 1; }
	@if [ ! -d "$(BENCH_STORE_DIR)/$(BENCH_CMP_ARCHIVED)" ]; then \
		printf "$(BOLD)Error:$(RESET) Baseline '$(BENCH_CMP_ARCHIVED)' not found in $(BENCH_STORE_DIR)/\n"; \
		exit 1; \
	fi
	@printf "$(GREEN)Running benchmarks$(if $(BENCH_CMP_FILTER), matching '$(BENCH_CMP_FILTER)') (saving as $(BENCH_CMP_TMP))…$(RESET)\n"
	$(CARGO) bench --package $(BENCH_PACKAGE) $(BENCH_CMP_FILTER) -- --save-baseline $(BENCH_CMP_TMP)
	@printf "$(GREEN)Comparing to archived baseline '$(BENCH_CMP_ARCHIVED)'…$(RESET)\n"
	@mkdir -p $(CRITERION_DIR) && cp -rn $(BENCH_STORE_DIR)/$(BENCH_CMP_ARCHIVED)/* $(CRITERION_DIR)/ 2>/dev/null || true
	critcmp $(BENCH_CMP_ARCHIVED) $(BENCH_CMP_TMP)$(if $(BENCH_CMP_FILTER), -f '$(BENCH_CMP_FILTER)',)

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

docs-python:
	@printf "$(GREEN)Building Python API docs (Sphinx)…$(RESET)\n"
	@command -v sphinx-build >/dev/null 2>&1 || (echo "Install sphinx: pip install sphinx" && exit 1)
	sphinx-build -b html docs/source docs/_build/html
	@printf "$(GREEN)Open docs/_build/html/index.html$(RESET)\n"

# Check required system packages; exit with clear install instructions if missing
setup-check:
	@MISSING=""; \
	command -v rustup >/dev/null 2>&1 || MISSING="$${MISSING:+$$MISSING }rustup (Rust toolchain)"; \
	command -v cargo >/dev/null 2>&1 || MISSING="$${MISSING:+$$MISSING }cargo"; \
	command -v pkg-config >/dev/null 2>&1 || MISSING="$${MISSING:+$$MISSING }pkg-config"; \
	command -v python3 >/dev/null 2>&1 || command -v python >/dev/null 2>&1 || MISSING="$${MISSING:+$$MISSING }python3"; \
	\
	FUSE_OK=""; \
	if pkg-config --exists fuse 2>/dev/null; then FUSE_OK=1; fi; \
	if [ -z "$$FUSE_OK" ] && [ "$$(uname)" = "Linux" ]; then \
	  if command -v dpkg >/dev/null 2>&1 && dpkg -s libfuse-dev >/dev/null 2>&1; then FUSE_OK=1; fi; \
	  if command -v pacman >/dev/null 2>&1 && pacman -Q fuse2 >/dev/null 2>&1; then FUSE_OK=1; fi; \
	  if command -v pacman >/dev/null 2>&1 && pacman -Q fuse3 >/dev/null 2>&1; then FUSE_OK=1; fi; \
	fi; \
	if [ -z "$$FUSE_OK" ] && [ "$$(uname)" = "Darwin" ]; then \
	  command -v brew >/dev/null 2>&1 && brew list macfuse >/dev/null 2>&1 && FUSE_OK=1; \
	fi; \
	[ -z "$$FUSE_OK" ] && MISSING="$${MISSING:+$$MISSING }libfuse (FUSE dev headers)"; \
	\
	if [ -n "$$MISSING" ]; then \
	  printf "$(BOLD)Missing required packages:$(RESET)\n  %s\n\n" "$$MISSING"; \
	  printf "Install them first, then run $(BOLD)make setup$(RESET) again.\n\n"; \
	  printf "$(CYAN)Examples:$(RESET)\n"; \
	  printf "  Rust:        https://rustup.rs  →  curl -sSf https://sh.rustup.rs | sh\n"; \
	  printf "  Ubuntu/Debian:  sudo apt-get update && sudo apt-get install -y pkg-config libfuse-dev python3 python3-venv\n"; \
	  printf "  Arch:        sudo pacman -S --needed base-devel fuse3 pkg-config python\n"; \
	  printf "  Fedora:      sudo dnf install pkg-config fuse-devel python3\n"; \
	  printf "  macOS:       brew install pkg-config macfuse; Rust from https://rustup.rs\n"; \
	  printf "\nOn Windows use WSL and follow the Ubuntu/Debian line.\n"; \
	  exit 1; \
	fi; \
	printf "$(GREEN)All required system packages found.$(RESET)\n"

setup: setup-check
	@printf "$(GREEN)Installing development tools…$(RESET)\n"
	rustup component add rustfmt clippy
	$(CARGO) install cargo-deny cargo-fuzz maturin critcmp
	@printf "$(GREEN)Creating Python venv (if missing)…$(RESET)\n"
	@if [ ! -d .venv ]; then python3 -m venv .venv 2>/dev/null || python -m venv .venv; fi; \
	if [ -f docs/requirements.txt ]; then \
	  ( [ -f .venv/Scripts/pip ] && .venv/Scripts/pip install -q -r docs/requirements.txt || .venv/bin/pip install -q -r docs/requirements.txt ) 2>/dev/null || true; \
	fi
	@printf "$(GREEN)Done. Run 'make check' to verify.$(RESET)\n"

# ═══════════════════════════════════════════════════════════════════════════════
#  CI Pipeline (runs everything in sequence)
# ═══════════════════════════════════════════════════════════════════════════════
ci: lint test build
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
