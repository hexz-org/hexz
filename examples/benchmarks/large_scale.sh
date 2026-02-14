#!/usr/bin/env bash
set -e

# Load common library
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../../scripts/lib/common.sh"

PROJECT_ROOT="$(get_project_root)"
BIN="${BIN:-$PROJECT_ROOT/target/release/hexz}"

# Default size 1GB
SIZE_GB=${1:-1}

DATA_FILE="test_dump_${SIZE_GB}GB.bin"
SNAP_FILE="test_dump_${SIZE_GB}GB.hxz"
MOUNT_DIR="mnt_large_test"
RESTORE_FILE="test_dump_${SIZE_GB}GB.restored"

# Cleanup
cleanup() {
    if [[ -n "$MOUNT_PID" ]]; then
        kill "$MOUNT_PID" 2>/dev/null || true
    fi
    if mountpoint -q "$MOUNT_DIR"; then
        $BIN vm unmount "$MOUNT_DIR"
    fi
    rmdir "$MOUNT_DIR" 2>/dev/null || true
}
trap cleanup EXIT

info "=== Hexz Large Scale Benchmark ==="
info "Target Size: ${SIZE_GB} GB"

# 1. Build
ensure_build "$BIN"

# 2. Generate Data
if [[ -f "$DATA_FILE" ]]; then
    info "[2/5] Data file $DATA_FILE exists, skipping generation."
else
    info "[2/5] Generating mixed data file..."
    python3 "$SCRIPT_DIR/gen_mixed_data.py" "$DATA_FILE" "$SIZE_GB"
fi

# 3. Benchmark Create
info "[3/5] Benchmarking Compression (Create)..."
time $BIN data pack --disk "$DATA_FILE" --output "$SNAP_FILE"

# Stats
ORIG_SIZE=$(stat -c%s "$DATA_FILE")
SNAP_SIZE=$(stat -c%s "$SNAP_FILE")
RATIO=$(echo "scale=2; $ORIG_SIZE / $SNAP_SIZE" | bc)
info "Compression Ratio: ${RATIO}x"

# 4. Benchmark Restore
info "[4/5] Benchmarking Decompression (via Mount)..."
mkdir -p "$MOUNT_DIR"
$BIN vm mount "$SNAP_FILE" "$MOUNT_DIR" &
MOUNT_PID=$!
sleep 2

info "Reading back file..."
time dd if="$MOUNT_DIR/disk" of="$RESTORE_FILE" bs=1M status=progress

# 5. Verify
info "[5/5] Verifying Integrity..."
if cmp -s "$DATA_FILE" "$RESTORE_FILE"; then
    ok "SUCCESS: Restored file matches original exactly."
else
    fail "FAILURE: Restored file differs from original!"
fi

# Cleanup handled by trap
ok "Done."
