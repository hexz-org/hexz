#!/bin/bash
set -e

# Default size 1GB, pass argument to override (e.g. ./scripts/large_scale_test.sh 32)
SIZE_GB=${1:-1}

DATA_FILE="test_dump_${SIZE_GB}GB.bin"
SNAP_FILE="test_dump_${SIZE_GB}GB.st"
MOUNT_DIR="mnt_large_test"
RESTORE_FILE="test_dump_${SIZE_GB}GB.restored"
BINARY="./target/release/snapfs"

echo "=== SnapFS Large Scale Benchmark ==="
echo "Target Size: ${SIZE_GB} GB"

# 1. Build Release CLI
echo "[1/5] Building Release CLI..."
cargo build --release -p snapfs

# 2. Generate Data
if [ -f "$DATA_FILE" ]; then
    echo "[2/5] Data file $DATA_FILE exists, skipping generation."
else
    echo "[2/5] Generating mixed data file..."
    python3 scripts/generate_mixed_data.py "$DATA_FILE" "$SIZE_GB"
fi

# 3. Benchmark Create
echo "[3/5] Benchmarking Compression (Create)..."
# Using 'time' to measure the duration
time $BINARY create --disk "$DATA_FILE" --output "$SNAP_FILE"

# Get sizes
ORIG_SIZE=$(stat -c%s "$DATA_FILE")
SNAP_SIZE=$(stat -c%s "$SNAP_FILE")
RATIO=$(echo "scale=2; $ORIG_SIZE / $SNAP_SIZE" | bc)
echo "Compression Ratio: ${RATIO}x"

# 4. Benchmark Restore (via Mount)
echo "[4/5] Benchmarking Decompression (via Mount)..."
mkdir -p "$MOUNT_DIR"
$BINARY mount "$SNAP_FILE" "$MOUNT_DIR" &
MOUNT_PID=$!
sleep 2

# Read back the file using dd to show progress
# bs=1M is optimal for throughput
time dd if="$MOUNT_DIR/disk" of="$RESTORE_FILE" bs=1M status=progress

# 5. Verify Integrity
echo "[5/5] Verifying Integrity..."
if cmp -s "$DATA_FILE" "$RESTORE_FILE"; then
    echo "SUCCESS: Restored file matches original exactly."
else
    echo "FAILURE: Restored file differs from original!"
    exit 1
fi

# Cleanup
$BINARY unmount "$MOUNT_DIR"
# rm "$DATA_FILE" "$SNAP_FILE" "$RESTORE_FILE"
rmdir "$MOUNT_DIR"
echo "Done."
