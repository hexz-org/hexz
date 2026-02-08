#!/usr/bin/env bash
set -e

# Helper script to run the snapfs CLI.
# Automatically builds the release binary if needed.

# Build quietly
cargo build --release --quiet

# Execute
exec ./target/release/snapfs "$@"
