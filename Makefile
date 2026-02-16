# ═══════════════════════════════════════════════════════════════════════════════
#  Hexz — Build, Test, and Release
# ═══════════════════════════════════════════════════════════════════════════════
#  Run from repo root.  make help  for targets.
#  Override tools:  CARGO=cargo, MATURIN=maturin  or put them in .make.env
# ═══════════════════════════════════════════════════════════════════════════════

-include .make.env
SHELL           := /bin/bash
.DEFAULT_GOAL   := help

# ── Paths & tools ─────────────────────────────────────────────────────────────
LOADER_CRATE    := crates/loader
BENCH_PACKAGE   := hexz

MATURIN         ?= maturin
CARGO           ?= cargo
RUFF            := $(shell [ -f .venv/bin/ruff ] && echo .venv/bin/ruff || echo ruff)
PYTHON          ?= $(shell [ -f .venv/bin/python3 ] && echo .venv/bin/python3 || ([ -f .venv/bin/python ] && echo .venv/bin/python || echo python3))

# ── Feature flags ─────────────────────────────────────────────────────────────
#  Override with: make develop FEATURES=full
#  Or:            make python FEATURES="s3 compression-zstd"
#  Or:            make build FEATURES=minimal
FEATURES        ?= default

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

# ── Baseline / benchmark-compare args ────────────────────────────────────────
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

# ── Profiling size ────────────────────────────────────────────────────────────
PERF_SIZE_MB    ?= 256

# ── Colors (only when stdout is a terminal) ───────────────────────────────────
ifneq ($(TERM),)
  GREEN  := \033[32m
  RED    := \033[31m
  CYAN   := \033[36m
  BOLD   := \033[1m
  RESET  := \033[0m
else
  GREEN  :=
  RED    :=
  CYAN   :=
  BOLD   :=
  RESET  :=
endif

# ── Fuzz configuration ───────────────────────────────────────────────────────
FUZZ_TARGETS    ?= decompress_lz4 decompress_zstd header_parse index_parse decrypt_arbitrary ed25519_verify header_validation master_index_load zstd_dictionary
FUZZ_TIME       ?= 60

# ── Phony ────────────────────────────────────────────────────────────────────
.PHONY: help build rust python develop install run clean
.PHONY: test test-rust test-python test-integration test-list test-nextest test-all test-quick test-stress
.PHONY: test-edge-cases test-property test-sanitize test-miri test-chaos test-concurrent test-memory-leak
.PHONY: test-exhaustive test-paranoid test-verify test-suite test-cov test-cov-rust test-cov-python
.PHONY: lint fmt fmt-check clippy deny check
.PHONY: bench bench-micro bench-macro bench-ai bench-quick bench-list bench-compare save-baseline archive-baseline restore-baseline compare-baseline bench-flamegraph fuzz fuzz-long
.PHONY: bench-boot bench-compression-ratio bench-large-scale
.PHONY: perf-python perf-rust perf-clean
.PHONY: bench-competitors bench-competitors-small
.PHONY: docker-dev docker-bench docs docs-serve setup setup-check setup-cross ci
.PHONY: pre-release _version-check _cross-aarch64 _cross-windows _wheel-check

# ═══════════════════════════════════════════════════════════════════════════════
#  Help
# ═══════════════════════════════════════════════════════════════════════════════
HELP_W := 35
help:
	@printf "\n$(BOLD)Hexz$(RESET) — snapshot storage engine\n\n"
	@printf "  $(CYAN)Build$(RESET)\n"
	@printf "    %-$(HELP_W)s  Build Rust workspace + Python wheel (release)\n" "make build"
	@printf "    %-$(HELP_W)s  Build Rust workspace only (release)\n" "make rust"
	@printf "    %-$(HELP_W)s  Build Python extension wheel (dist)\n" "make python"
	@printf "    %-$(HELP_W)s  Install Python extension (editable)\n" "make develop"
	@printf "    %-$(HELP_W)s  Install hexz CLI locally\n" "make install"
	@printf "    %-$(HELP_W)s  Run CLI; e.g. make run serve\n" "make run [args]"
	@printf "\n  $(CYAN)Feature Selection$(RESET) (for build/develop/python targets)\n"
	@printf "    %-$(HELP_W)s  Default features (s3, zstd, signing)\n" "make develop FEATURES=default"
	@printf "    %-$(HELP_W)s  All features (s3, zstd, encryption, signing)\n" "make develop FEATURES=full"
	@printf "    %-$(HELP_W)s  Minimal (no default features)\n" "make develop FEATURES=minimal"
	@printf "    %-$(HELP_W)s  Custom feature list\n" "make develop FEATURES=\"s3 signing\""
	@printf "\n  $(CYAN)Test$(RESET)\n"
	@printf "    %-$(HELP_W)s  All tests; optional filter (e.g. make test cache)\n" "make test [filter]"
	@printf "    %-$(HELP_W)s  Rust tests only\n" "make test-rust [filter]"
	@printf "    %-$(HELP_W)s  Python tests only (pytest -k)\n" "make test-python [filter]"
	@printf "    %-$(HELP_W)s  List test categories for filtering\n" "make test-list"
	@printf "    %-$(HELP_W)s  All tests (Rust + Python + integration)\n" "make test-all"
	@printf "    %-$(HELP_W)s  Quick tests (lib + bins, no integration)\n" "make test-quick"
	@printf "    %-$(HELP_W)s  Stress tests (release mode, ignored tests)\n" "make test-stress"
	@printf "    %-$(HELP_W)s  Edge case tests (boundary conditions)\n" "make test-edge-cases"
	@printf "    %-$(HELP_W)s  Property-based tests (1k cases)\n" "make test-property"
	@printf "    %-$(HELP_W)s  Property tests thorough (10k cases)\n" "make test-property-thorough"
	@printf "    %-$(HELP_W)s  Chaos tests (failures, corruption)\n" "make test-chaos"
	@printf "    %-$(HELP_W)s  Address sanitizer (memory issues)\n" "make test-sanitize"
	@printf "    %-$(HELP_W)s  Thread sanitizer (data races)\n" "make test-concurrent"
	@printf "    %-$(HELP_W)s  Memory leak detection\n" "make test-memory-leak"
	@printf "    %-$(HELP_W)s  Miri undefined behavior detection\n" "make test-miri"
	@printf "    %-$(HELP_W)s  Exhaustive testing (~30-60 min)\n" "make test-exhaustive"
	@printf "    %-$(HELP_W)s  PARANOID: Every test possible (~2 hrs)\n" "make test-paranoid"
	@printf "    %-$(HELP_W)s  Coverage report (Rust + Python)\n" "make test-cov"
	@printf "    %-$(HELP_W)s  Rust coverage only (cargo-llvm-cov)\n" "make test-cov-rust"
	@printf "    %-$(HELP_W)s  Python coverage only (pytest-cov)\n" "make test-cov-python"
	@printf "    %-$(HELP_W)s  Run tests via cargo-nextest\n" "make test-nextest"
	@printf "    %-$(HELP_W)s  Integration tests only\n" "make test-integration"
	@printf "    %-$(HELP_W)s  Full verification (lint + tests + fuzz)\n" "make test-verify"
	@printf "    $(BOLD)%-$(HELP_W)s  Comprehensive suite (recommended)$(RESET)\n" "make test-suite"
	@printf "\n  $(CYAN)Quality$(RESET)\n"
	@printf "    %-$(HELP_W)s  Format check + clippy\n" "make lint"
	@printf "    %-$(HELP_W)s  Auto-format Rust + Python\n" "make fmt"
	@printf "    %-$(HELP_W)s  Clippy with strict lints\n" "make clippy"
	@printf "    %-$(HELP_W)s  Licenses + advisories (optional)\n" "make deny"
	@printf "    %-$(HELP_W)s  Fast workspace type check\n" "make check"
	@printf "\n  $(CYAN)Performance$(RESET)\n"
	@printf "    %-$(HELP_W)s  Run benchmarks; optional filter\n" "make bench [filter]"
	@printf "    %-$(HELP_W)s  Run micro benchmarks only\n" "make bench-micro"
	@printf "    %-$(HELP_W)s  Run macro benchmarks only\n" "make bench-macro"
	@printf "    %-$(HELP_W)s  Run AI benchmarks only\n" "make bench-ai"
	@printf "    %-$(HELP_W)s  Run all benchmarks with quick profile\n" "make bench-quick"
	@printf "    %-$(HELP_W)s  List benchmark categories for filtering\n" "make bench-list"
	@printf "    %-$(HELP_W)s  Run benchmarks [filter], compare to archived baseline\n" "make bench-compare <name> [filter]"
	@printf "    %-$(HELP_W)s  Benchmark vs competitors (WebDataset, HDF5, full dataset)\n" "make bench-competitors"
	@printf "    %-$(HELP_W)s  Quick competitor benchmark (1K images, ~2 min)\n" "make bench-competitors-small"
	@printf "    %-$(HELP_W)s  Shell: boot performance timing\n" "make bench-boot"
	@printf "    %-$(HELP_W)s  Shell: compression ratio analysis\n" "make bench-compression-ratio"
	@printf "    %-$(HELP_W)s  Shell: large-scale stress test\n" "make bench-large-scale"
	@printf "    %-$(HELP_W)s  Save current run as baseline\n" "make save-baseline <name>"
	@printf "    %-$(HELP_W)s  Archive baseline to .criterion/\n" "make archive-baseline <name>"
	@printf "    %-$(HELP_W)s  Restore baseline from archive\n" "make restore-baseline <name>"
	@printf "    %-$(HELP_W)s  Compare two archived baselines (critcmp)\n" "make compare-baseline <a> <b>"
	@printf "    %-$(HELP_W)s  Generate flamegraph SVG from benchmarks\n" "make bench-flamegraph [filter]"
	@printf "    %-$(HELP_W)s  Fuzz targets (cargo-fuzz, $(FUZZ_TIME)s each)\n" "make fuzz"
	@printf "    %-$(HELP_W)s  Extended fuzz run (600s, all targets)\n" "make fuzz-long"
	@printf "\n  $(CYAN)Profiling$(RESET)  (samply → Firefox Profiler flamegraphs)\n"
	@printf "    %-$(HELP_W)s  Flamegraph: Python ML workload\n" "make perf-python"
	@printf "    %-$(HELP_W)s  Flamegraph: Rust CLI data pack\n" "make perf-rust"
	@printf "    %-$(HELP_W)s  Remove profiling artifacts\n" "make perf-clean"
	@printf "\n  $(CYAN)Infrastructure$(RESET)\n"
	@printf "    %-$(HELP_W)s  Build dev Docker image\n" "make docker-dev"
	@printf "    %-$(HELP_W)s  Build benchmark Docker image\n" "make docker-bench"
	@printf "    %-$(HELP_W)s  Build unified MkDocs + Rust documentation\n" "make docs"
	@printf "    %-$(HELP_W)s  Serve unified docs locally for viewing\n" "make docs-serve"
	@printf "    %-$(HELP_W)s  Dev tools + Python venv\n" "make setup"
	@printf "    %-$(HELP_W)s  Verify system deps\n" "make setup-check"
	@printf "    %-$(HELP_W)s  Full CI (lint + test + build)\n" "make ci"
	@printf "    %-$(HELP_W)s  Full release validation (all targets)\n" "make pre-release"
	@printf "    %-$(HELP_W)s  Install cross-compilation toolchains\n" "make setup-cross"
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
	@printf "$(GREEN)Building Python wheel (features: $(FEATURES))…$(RESET)\n"
ifeq ($(FEATURES),default)
	$(MATURIN) build --release --manifest-path $(LOADER_CRATE)/Cargo.toml
else ifeq ($(FEATURES),minimal)
	$(MATURIN) build --release --manifest-path $(LOADER_CRATE)/Cargo.toml --no-default-features
else ifeq ($(FEATURES),full)
	$(MATURIN) build --release --manifest-path $(LOADER_CRATE)/Cargo.toml --features full
else
	$(MATURIN) build --release --manifest-path $(LOADER_CRATE)/Cargo.toml --no-default-features --features "$(FEATURES)"
endif

develop:
	@printf "$(GREEN)Installing Python extension (editable, features: $(FEATURES))…$(RESET)\n"
ifeq ($(FEATURES),default)
	$(MATURIN) develop --release --manifest-path $(LOADER_CRATE)/Cargo.toml
else ifeq ($(FEATURES),minimal)
	$(MATURIN) develop --release --manifest-path $(LOADER_CRATE)/Cargo.toml --no-default-features
else ifeq ($(FEATURES),full)
	$(MATURIN) develop --release --manifest-path $(LOADER_CRATE)/Cargo.toml --features full
else
	$(MATURIN) develop --release --manifest-path $(LOADER_CRATE)/Cargo.toml --no-default-features --features "$(FEATURES)"
endif

install:
	@printf "$(GREEN)Installing hexz CLI…$(RESET)\n"
	$(CARGO) install --path crates/cli

run:
	$(CARGO) run --package $(BENCH_PACKAGE) -- $(RUN_ARGS)

# ═══════════════════════════════════════════════════════════════════════════════
#  Test
# ═══════════════════════════════════════════════════════════════════════════════
test: test-rust test-python

# When TEST_ARGS is set, pipe through awk to hide "running 0 tests" blocks so only matching tests are shown.
test-rust:
	@printf "$(GREEN)Running Rust tests…$(RESET)\n"
	@if [ -z "$(TEST_ARGS)" ]; then \
		$(CARGO) test --workspace --lib --bins --tests && \
		$(CARGO) test --workspace --doc; \
	else \
		$(CARGO) test --workspace --lib --bins --tests -- $(TEST_ARGS) 2>&1 | awk '\
/^[[:space:]]+Running/ { \
	if (buf != "") { if (!skip) printf "%s", buf; buf = "" } \
	skip = 0; buf = $$0 "\n"; next \
} \
{ buf = buf $$0 "\n"; if ($$0 ~ /running 0 tests/) skip = 1 } \
END { if (buf != "" && !skip) printf "%s", buf }'; \
		exit $${PIPESTATUS[0]}; \
	fi

test-python:
	@printf "$(GREEN)Running Python tests…$(RESET)\n"
	VIRTUAL_ENV=$(CURDIR)/.venv $(MATURIN) develop --manifest-path $(LOADER_CRATE)/Cargo.toml -E test,numpy && $(PYTHON) -m pytest $(LOADER_CRATE)/tests/ -v $(if $(TEST_ARGS),-k "$(TEST_ARGS)",)

test-integration:
	@printf "$(GREEN)Running integration tests…$(RESET)\n"
	$(CARGO) test --workspace -- --ignored

test-list:
	@cargo xtask test list

test-nextest:
	@command -v cargo-nextest >/dev/null 2>&1 || { \
		printf "$(BOLD)cargo-nextest not installed.$(RESET)\n"; \
		printf "Install with: $(CYAN)cargo install cargo-nextest$(RESET)\n"; \
		exit 1; \
	}
	@printf "$(GREEN)Running tests via nextest…$(RESET)\n"
	$(CARGO) nextest run --workspace

test-all: test-rust test-python test-integration
	@printf "$(GREEN)All tests passed!$(RESET)\n"

test-quick:
	@printf "$(GREEN)Running quick test suite (no integration tests)…$(RESET)\n"
	$(CARGO) test --workspace --lib --bins
	@printf "$(GREEN)Quick tests passed!$(RESET)\n"

test-stress:
	@printf "$(GREEN)Running stress tests with release optimizations…$(RESET)\n"
	$(CARGO) test --workspace --release -- --include-ignored --test-threads=1
	@printf "$(GREEN)Stress tests passed!$(RESET)\n"

test-edge-cases:
	@printf "$(GREEN)Running edge case tests (boundary conditions, limits)…$(RESET)\n"
	HEXZ_TEST_EDGE_CASES=1 $(CARGO) test --workspace -- edge_case
	@printf "$(GREEN)Edge case tests passed!$(RESET)\n"

test-property:
	@printf "$(GREEN)Running property-based tests (proptest)…$(RESET)\n"
	PROPTEST_CASES=1000 $(CARGO) test --workspace -- proptest
	@printf "$(GREEN)Property tests passed (1,000 cases per test)!$(RESET)\n"

test-property-thorough:
	@printf "$(GREEN)Running thorough property-based tests (proptest)…$(RESET)\n"
	@printf "$(CYAN)This will take 5-10 minutes…$(RESET)\n"
	PROPTEST_CASES=10000 $(CARGO) test --workspace -- proptest
	@printf "$(GREEN)Property tests passed (10,000 cases per test)!$(RESET)\n"

test-sanitize:
	@printf "$(GREEN)Running tests with address sanitizer (detects memory issues)…$(RESET)\n"
	@printf "$(CYAN)This requires nightly Rust and will be slow…$(RESET)\n"
	RUSTFLAGS="-Z sanitizer=address" $(CARGO) +nightly test --workspace --target x86_64-unknown-linux-gnu --lib --tests
	@printf "$(GREEN)Sanitizer tests passed!$(RESET)\n"

test-miri:
	@printf "$(GREEN)Running tests with Miri (detects undefined behavior)…$(RESET)\n"
	@printf "$(CYAN)This requires nightly Rust and will be very slow…$(RESET)\n"
	@command -v cargo-miri >/dev/null 2>&1 || { \
		printf "$(BOLD)cargo-miri not installed.$(RESET)\n"; \
		printf "Install with: $(CYAN)rustup +nightly component add miri$(RESET)\n"; \
		exit 1; \
	}
	$(CARGO) +nightly miri test --package hexz-core --lib
	@printf "$(GREEN)Miri tests passed!$(RESET)\n"

test-chaos:
	@printf "$(GREEN)Running chaos tests (random failures, timeouts, corruption)…$(RESET)\n"
	HEXZ_CHAOS_MODE=1 $(CARGO) test --workspace -- chaos --test-threads=1
	@printf "$(GREEN)Chaos tests passed!$(RESET)\n"

test-concurrent:
	@printf "$(GREEN)Running concurrency tests with thread sanitizer…$(RESET)\n"
	@printf "$(CYAN)Testing for data races and thread safety issues…$(RESET)\n"
	RUSTFLAGS="-Z sanitizer=thread" $(CARGO) +nightly test --workspace --target x86_64-unknown-linux-gnu -- concurrent --test-threads=1
	@printf "$(GREEN)Concurrency tests passed!$(RESET)\n"

test-memory-leak:
	@printf "$(GREEN)Running memory leak detection tests…$(RESET)\n"
	RUSTFLAGS="-Z sanitizer=leak" $(CARGO) +nightly test --workspace --target x86_64-unknown-linux-gnu --lib
	@printf "$(GREEN)Memory leak tests passed!$(RESET)\n"

test-exhaustive:
	@printf "$(GREEN)Running exhaustive test suite (property + edge + stress + sanitizer)…$(RESET)\n"
	@printf "$(CYAN)This will take 30-60 minutes…$(RESET)\n"
	@$(MAKE) test-property-thorough
	@$(MAKE) test-edge-cases
	@$(MAKE) test-stress
	@$(MAKE) test-sanitize
	@printf "$(GREEN)Exhaustive tests passed!$(RESET)\n"

test-paranoid: test-exhaustive test-miri test-chaos test-concurrent test-memory-leak fuzz-long
	@printf "$(BOLD)$(GREEN)PARANOID MODE: All extensive tests passed!$(RESET)\n"
	@printf "$(CYAN)This is the most thorough testing possible.$(RESET)\n"

test-verify: lint test-all fuzz
	@printf "$(GREEN)Full verification passed! (lint + all tests + fuzz)$(RESET)\n"

test-suite:
	@printf "$(BOLD)$(CYAN)Running comprehensive test suite...$(RESET)\n"
	@printf "$(CYAN)This will test: quick, edge-cases, property, stress, all, and verify$(RESET)\n"
	@printf "\n$(GREEN)1/6 Running quick tests...$(RESET)\n"
	@$(MAKE) test-quick
	@printf "\n$(GREEN)2/6 Running edge case tests...$(RESET)\n"
	@$(MAKE) test-edge-cases
	@printf "\n$(GREEN)3/6 Running property-based tests...$(RESET)\n"
	@$(MAKE) test-property
	@printf "\n$(GREEN)4/6 Running stress tests...$(RESET)\n"
	@$(MAKE) test-stress
	@printf "\n$(GREEN)5/6 Running all tests...$(RESET)\n"
	@$(MAKE) test-all
	@printf "\n$(GREEN)6/6 Running verification...$(RESET)\n"
	@$(MAKE) test-verify
	@printf "\n$(BOLD)$(GREEN)✅ ALL TEST SUITES PASSED!$(RESET)\n"
	@printf "$(CYAN)Your code is thoroughly tested and ready.$(RESET)\n"

# ═══════════════════════════════════════════════════════════════════════════════
#  Coverage
# ═══════════════════════════════════════════════════════════════════════════════
test-cov:
	@cargo xtask coverage all

test-cov-rust:
	@cargo xtask coverage rust

test-cov-python:
	@cargo xtask coverage python

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

bench-micro:
	@cargo xtask bench run --group micro

bench-macro:
	@cargo xtask bench run --group macro

bench-ai:
	@cargo xtask bench run --group ai

bench-quick:
	@cargo xtask bench run --group all --profile quick

bench-list:
	@cargo xtask bench list

save-baseline:
	@cargo xtask baseline save $(BASELINE_NAME)

archive-baseline:
	@cargo xtask baseline archive $(BASELINE_NAME)

restore-baseline:
	@cargo xtask baseline restore $(BASELINE_NAME)

compare-baseline:
	@cargo xtask baseline compare $(BASE_OLD) $(BASE_NEW)

bench-compare:
	@cargo xtask baseline bench-compare $(BENCH_CMP_ARCHIVED) $(BENCH_CMP_FILTER)

bench-competitors:
	@printf "$(GREEN)Running competitor benchmarks (WebDataset, HDF5, Local Files)…$(RESET)\n"
	@printf "$(CYAN)This will take 30-60 minutes with full dataset (50K images, ~6.3GB)$(RESET)\n"
	@command -v $(PYTHON) >/dev/null 2>&1 || (echo "Error: Python not found" && exit 1)
	@$(PYTHON) -c "import webdataset" 2>/dev/null || (echo "Error: Install dependencies: pip install -r benchmarks/requirements-competitors.txt" && exit 1)
	@$(PYTHON) benchmarks/run_benchmarks.py
	@printf "$(GREEN)Benchmark results: benchmarks/results/COMPARISON.md$(RESET)\n"

bench-competitors-small:
	@printf "$(GREEN)Running competitor benchmarks (small test dataset)…$(RESET)\n"
	@printf "$(CYAN)Using 1000 images (~130MB) for quick testing$(RESET)\n"
	@$(PYTHON) benchmarks/run_benchmarks.py --quick
	@printf "$(GREEN)Benchmark results: benchmarks/results/COMPARISON.md$(RESET)\n"

bench-flamegraph:
	@printf "$(GREEN)Generating flamegraph from benchmarks…$(RESET)\n"
	@command -v cargo-flamegraph >/dev/null 2>&1 || { \
		printf "$(BOLD)cargo-flamegraph not installed.$(RESET)\n"; \
		printf "Install with: $(CYAN)cargo install flamegraph$(RESET)\n"; \
		exit 1; \
	}
	@printf "$(CYAN)Running benchmarks under perf for flamegraph…$(RESET)\n"
	@if [ -n "$(BENCH_BIN)" ]; then \
		$(CARGO) flamegraph --package $(BENCH_PACKAGE) --bench $(BENCH_BIN) -o flamegraph.svg -- --bench; \
	else \
		$(CARGO) flamegraph --package $(BENCH_PACKAGE) --bench read_throughput -o flamegraph.svg -- --bench; \
	fi
	@printf "$(GREEN)Flamegraph written to flamegraph.svg$(RESET)\n"

bench-boot:
	@printf "$(GREEN)Running boot performance benchmark…$(RESET)\n"
	bash tools/bench/boot_performance.sh

bench-compression-ratio:
	@printf "$(GREEN)Running compression ratio benchmark…$(RESET)\n"
	bash tools/bench/compression_ratio.sh

bench-large-scale:
	@printf "$(GREEN)Running large-scale benchmark…$(RESET)\n"
	bash tools/bench/large_scale.sh

fuzz:
	@printf "$(GREEN)Running fuzz targets ($(FUZZ_TIME)s each)…$(RESET)\n"
	@for target in $(FUZZ_TARGETS); do \
		printf "$(CYAN)▶ fuzzing $$target$(RESET)\n"; \
		(cd fuzz && $(CARGO) +nightly fuzz run $$target -- -max_total_time=$(FUZZ_TIME)); \
	done

fuzz-long:
	@printf "$(GREEN)Running extended fuzz (600s, all targets)…$(RESET)\n"
	@for target in decompress_lz4 decompress_zstd header_parse index_parse cdc_chunking encryption_roundtrip decrypt_arbitrary ed25519_verify header_validation master_index_load zstd_dictionary; do \
		printf "$(CYAN)▶ fuzzing $$target$(RESET)\n"; \
		(cd fuzz && $(CARGO) +nightly fuzz run $$target -- -max_total_time=600); \
	done

# ── Profiling ────────────────────────────────────────────────────────────────
perf-python: develop
	@command -v samply >/dev/null 2>&1 || { \
		printf "$(BOLD)samply not found.$(RESET)\n"; \
		printf "Install with: $(CYAN)cargo install samply$(RESET)\n"; \
		exit 1; \
	}
	@printf "$(GREEN)Profiling Python ML workload (samply)…$(RESET)\n"
	samply record -- $(PYTHON) tools/perf/ml_workload.py
	@printf "\n$(CYAN)Tip: type $(BOLD)hexz$(RESET)$(CYAN) in the search box to highlight only hexz frames$(RESET)\n"

perf-rust:
	@cargo xtask perf rust --size-mb $(PERF_SIZE_MB)

perf-clean:
	@cargo xtask perf clean

# ═══════════════════════════════════════════════════════════════════════════════
#  Infrastructure
# ═══════════════════════════════════════════════════════════════════════════════
docker-dev:
	@printf "$(GREEN)Building dev container…$(RESET)\n"
	docker build -f docker/dev.Dockerfile -t hexz-dev .

docker-bench:
	@printf "$(GREEN)Building benchmark container…$(RESET)\n"
	docker build -f docker/bench.Dockerfile -t hexz-bench .

docs:
	@cargo xtask docs

docs-serve: docs
	@printf "$(GREEN)Serving unified documentation on port 8000…$(RESET)\n"
	$(PYTHON) -m http.server 8000 -d site

setup-check:
	@cargo xtask setup check

setup: setup-check
	@cargo xtask setup install

setup-cross:
	@cargo xtask setup cross

# ═══════════════════════════════════════════════════════════════════════════════
#  CI Pipeline (runs everything in sequence)
# ═══════════════════════════════════════════════════════════════════════════════
ci: lint test build
	@printf "$(GREEN)CI pipeline passed.$(RESET)\n"

# ═══════════════════════════════════════════════════════════════════════════════
#  Pre-release Validation
# ═══════════════════════════════════════════════════════════════════════════════
pre-release: _version-check lint test _cross-aarch64 _cross-windows _wheel-check
	@printf "\n$(BOLD)$(GREEN)Pre-release validation passed — ready to tag and release$(RESET)\n"

_version-check:
	@cargo xtask version-check

_cross-aarch64:
	@cargo xtask cross-check aarch64

_cross-windows:
	@cargo xtask cross-check windows

_wheel-check:
	@printf "\n$(GREEN)[wheel] Building native Python wheel…$(RESET)\n"
	$(MATURIN) build --release --manifest-path $(LOADER_CRATE)/Cargo.toml

clean:
	@cargo xtask clean
