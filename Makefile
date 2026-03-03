# ═══════════════════════════════════════════════════════════════════════════════
#  Hexz — Build, Test, and Release
# ═══════════════════════════════════════════════════════════════════════════════
#  Run from repo root.  make help  for targets.
#  Override tools:  CARGO=cargo  or put them in .make.env
# ═══════════════════════════════════════════════════════════════════════════════

-include .make.env
SHELL           := /bin/bash
.DEFAULT_GOAL   := help

# ── Paths & tools ─────────────────────────────────────────────────────────────
BENCH_PACKAGE   := hexz
CARGO           ?= cargo

# ── Feature flags ─────────────────────────────────────────────────────────────
FEATURES        ?= default

# ── Pass-through args ─────────────────────────────────────────────────────────
ifneq (,$(filter test test-rust,$(firstword $(MAKECMDGOALS))))
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

# ── Colors ────────────────────────────────────────────────────────────────────
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
.PHONY: help build rust install run clean
.PHONY: test test-rust test-integration test-list test-nextest test-all test-quick test-stress
.PHONY: test-edge-cases test-property test-sanitize test-miri test-chaos test-concurrent test-memory-leak
.PHONY: test-exhaustive test-paranoid test-verify test-suite test-cov test-cov-rust
.PHONY: lint fmt fmt-check clippy deny check
.PHONY: bench bench-micro bench-macro bench-quick bench-list bench-compare save-baseline archive-baseline restore-baseline compare-baseline bench-flamegraph fuzz fuzz-long
.PHONY: perf-rust perf-clean
.PHONY: docker-dev docker-bench docs docs-serve setup setup-check setup-cross ci
.PHONY: pre-release _version-check _cross-aarch64 _cross-windows

# ═══════════════════════════════════════════════════════════════════════════════
#  Help
# ═══════════════════════════════════════════════════════════════════════════════
HELP_W := 35
help:
	@printf "\n$(BOLD)Hexz$(RESET) — general-purpose deduplicated archive engine\n\n"
	@printf "  $(CYAN)Build$(RESET)\n"
	@printf "    %-$(HELP_W)s  Build Rust workspace (release)\n" "make build"
	@printf "    %-$(HELP_W)s  Install hexz CLI locally\n" "make install"
	@printf "    %-$(HELP_W)s  Run CLI; e.g. make run pack\n" "make run [args]"
	@printf "\n  $(CYAN)Test$(RESET)\n"
	@printf "    %-$(HELP_W)s  All Rust tests; optional filter\n" "make test [filter]"
	@printf "    %-$(HELP_W)s  Quick tests (lib + bins, no integration)\n" "make test-quick"
	@printf "    %-$(HELP_W)s  Integration tests only\n" "make test-integration"
	@printf "    %-$(HELP_W)s  Full verification (lint + tests + fuzz)\n" "make test-verify"
	@printf "\n  $(CYAN)Quality$(RESET)\n"
	@printf "    %-$(HELP_W)s  Format check + clippy\n" "make lint"
	@printf "    %-$(HELP_W)s  Auto-format Rust code\n" "make fmt"
	@printf "\n  $(CYAN)Performance$(RESET)\n"
	@printf "    %-$(HELP_W)s  Run benchmarks; optional filter\n" "make bench [filter]"
	@printf "    %-$(HELP_W)s  Fuzz targets (cargo-fuzz)\n" "make fuzz"
	@printf "\n  $(CYAN)Housekeeping$(RESET)\n"
	@printf "    %-$(HELP_W)s  Remove build artifacts\n" "make clean"
	@printf "\n"

# ═══════════════════════════════════════════════════════════════════════════════
#  Build
# ═══════════════════════════════════════════════════════════════════════════════
build: rust

rust:
	@printf "$(GREEN)Building Rust workspace (release)…$(RESET)\n"
	$(CARGO) build --release --workspace

install:
	@printf "$(GREEN)Installing hexz CLI…$(RESET)\n"
	$(CARGO) install --path crates/cli

run:
	$(CARGO) run --package $(BENCH_PACKAGE) -- $(RUN_ARGS)

# ═══════════════════════════════════════════════════════════════════════════════
#  Test
# ═══════════════════════════════════════════════════════════════════════════════
test: test-rust

test-rust:
	@printf "$(GREEN)Running Rust tests…$(RESET)\n"
	@if [ -z "$(TEST_ARGS)" ]; then \
		$(CARGO) test --workspace --lib --bins --tests && \
		$(CARGO) test --workspace --doc; \
	else \
		$(CARGO) test --workspace --lib --bins --tests -- $(TEST_ARGS); \
	fi

test-integration:
	@printf "$(GREEN)Running integration tests…$(RESET)\n"
	$(CARGO) test --workspace -- --ignored

test-list:
	@cargo xtask test list

test-quick:
	@printf "$(GREEN)Running quick test suite (no integration tests)…$(RESET)\n"
	$(CARGO) test --workspace --lib --bins
	@printf "$(GREEN)Quick tests passed!$(RESET)\n"

test-verify: lint test-rust fuzz
	@printf "$(GREEN)Full verification passed! (lint + tests + fuzz)$(RESET)\n"

# ═══════════════════════════════════════════════════════════════════════════════
#  Quality
# ═══════════════════════════════════════════════════════════════════════════════
lint: fmt-check clippy

fmt:
	@printf "$(GREEN)Formatting…$(RESET)\n"
	$(CARGO) fmt --all

fmt-check:
	@printf "$(GREEN)Checking format…$(RESET)\n"
	$(CARGO) fmt --all -- --check

clippy:
	@printf "$(GREEN)Running clippy…$(RESET)\n"
	$(CARGO) clippy --workspace --all-targets -- -D warnings

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

fuzz:
	@printf "$(GREEN)Running fuzz targets ($(FUZZ_TIME)s each)…$(RESET)\n"
	@for target in $(FUZZ_TARGETS); do \
		printf "$(CYAN)▶ fuzzing $$target$(RESET)\n"; \
		(cd fuzz && $(CARGO) +nightly fuzz run $$target -- -max_total_time=$(FUZZ_TIME)); \
	done

# ── Profiling ────────────────────────────────────────────────────────────────
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

setup-check:
	@cargo xtask setup check

setup: setup-check
	@cargo xtask setup install

setup-cross:
	@cargo xtask setup cross

# ═══════════════════════════════════════════════════════════════════════════════
#  CI Pipeline
# ═══════════════════════════════════════════════════════════════════════════════
ci: lint test build
	@printf "$(GREEN)CI pipeline passed.$(RESET)\n"

# ═══════════════════════════════════════════════════════════════════════════════
#  Pre-release Validation
# ═══════════════════════════════════════════════════════════════════════════════
pre-release: _version-check lint test _cross-aarch64 _cross-windows
	@printf "\n$(BOLD)$(GREEN)Pre-release validation passed — ready to tag and release$(RESET)\n"

_version-check:
	@cargo xtask version-check

_cross-aarch64:
	@cargo xtask cross-check aarch64

_cross-windows:
	@cargo xtask cross-check windows

clean:
	@cargo xtask clean
