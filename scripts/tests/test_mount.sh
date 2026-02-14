#!/usr/bin/env bash
#
# FUSE Mount Integration Test.
#
# This script verifies the end-to-end functionality of the FUSE daemon.

set -e

# Load common library
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../lib/common.sh"

PROJECT_ROOT="$(get_project_root)"
BIN="${BIN:-$PROJECT_ROOT/target/release/hexz}"

# Constants
SRC_DATA="mount_test.data"
SNAP_FILE="mount_test.hxz"
MOUNT_DIR="mnt_test_point"

# Cleanup trap
cleanup() {
    info "Cleaning up..."
    if [[ -n "$MOUNT_PID" ]]; then
        kill "$MOUNT_PID" 2>/dev/null || true
    fi
    if mountpoint -q "$MOUNT_DIR"; then
        $BIN vm unmount "$MOUNT_DIR"
    fi
    rm -f "$SRC_DATA" "$SNAP_FILE"
    rmdir "$MOUNT_DIR" 2>/dev/null || true
}
trap cleanup EXIT

info ">>> Starting FUSE Mount Test"

# Setup
info "Creating test data..."
echo "Hello Hexz World" > "$SRC_DATA"
mkdir -p "$MOUNT_DIR"

# Build
ensure_build "$BIN"

# Create Snapshot
info "Creating snapshot..."
$BIN data pack --disk "$SRC_DATA" --output "$SNAP_FILE"

# Mount
info "Mounting..."
$BIN vm mount "$SNAP_FILE" "$MOUNT_DIR" &
MOUNT_PID=$!
sleep 2

if ! mountpoint -q "$MOUNT_DIR"; then
    fail "Mount failed."
fi

# Verify Content
info "Verifying content..."
# Note: In the new structure, the file is exposed as 'disk' inside the mount
CONTENT=$(cat "$MOUNT_DIR/disk")
EXPECTED="Hello Hexz World"

if [[ "$CONTENT" != "$EXPECTED" ]]; then
    info "Expected: $EXPECTED"
    info "Got: $CONTENT"
    fail "Content Mismatch"
fi

ok "Read successful: $CONTENT"

# Unmount handled by trap
ok "Test Passed."
