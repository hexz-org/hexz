#!/usr/bin/env bash
# Run Rust tests for Strata

set -e

# Load common library
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../lib/common.sh"

PROJECT_ROOT="$(get_project_root)"

info "Running Rust tests..."
cd "$PROJECT_ROOT"
make test-rust

ok "Rust tests passed."
