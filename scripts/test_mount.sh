#!/usr/bin/env bash
#
# FUSE Mount Integration Test.
#
# This script verifies the end-to-end functionality of the FUSE daemon.
# It mounts a snapshot, performs file system operations, verifies content,
# and unmounts cleanly.
#
# Usage: ./scripts/test_mount.sh

set -e

# Constants
SRC_DATA="mount_test.data"
SNAP_FILE="mount_test.st"
MOUNT_DIR="mnt_test_point"
BINARY="./target/release/snapfs"

# Cleanup trap
cleanup() {
    echo ">>> Cleaning up..."
    if mountpoint -q "$MOUNT_DIR"; then
        $BINARY unmount "$MOUNT_DIR"
    fi
    rm -f "$SRC_DATA" "$SNAP_FILE"
    rmdir "$MOUNT_DIR" 2>/dev/null || true
}
trap cleanup EXIT

echo ">>> Starting FUSE Mount Test"

# Setup
echo ">>> Creating test data..."
echo "Hello SnapFS World" > "$SRC_DATA"
mkdir -p "$MOUNT_DIR"

# Build (ensure release binary is ready)
cargo build --release --workspace

# Create Snapshot
echo ">>> Creating snapshot..."
$BINARY create --disk "$SRC_DATA" --output "$SNAP_FILE"

# Mount
echo ">>> Mounting..."
# Run in background
$BINARY mount "$SNAP_FILE" "$MOUNT_DIR" &
MOUNT_PID=$!

# Wait for mount to stabilize
sleep 2

if ! mountpoint -q "$MOUNT_DIR"; then
    echo "!!! Mount failed."
    exit 1
fi

# Verify Content
echo ">>> Verifying content..."
# Note: In the new structure, the file is exposed as 'disk' inside the mount
CONTENT=$(cat "$MOUNT_DIR/disk")
EXPECTED="Hello SnapFS World"

if [ "$CONTENT" != "$EXPECTED" ]; then
    echo "!!! Content Mismatch"
    echo "Expected: $EXPECTED"
    echo "Got: $CONTENT"
    exit 1
fi

echo ">>> Read successful: $CONTENT"

# Unmount handled by trap
echo ">>> Test Passed."
