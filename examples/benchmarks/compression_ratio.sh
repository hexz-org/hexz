#!/usr/bin/env bash
set -e

# Load common library
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../../scripts/lib/common.sh"

PROJECT_ROOT="$(get_project_root)"
BIN="${BIN:-$PROJECT_ROOT/target/release/hexz}"

# Default size 100MB
SIZE_MB=${1:-100}

# Paths
DATA_DIR="$PROJECT_ROOT/data"
DATA_FILE="$DATA_DIR/bench_data_${SIZE_MB}MB.json"
SNAP_NODICT="$DATA_DIR/bench_${SIZE_MB}MB_nodict.hxz"
SNAP_DICT="$DATA_DIR/bench_${SIZE_MB}MB_dict.hxz"
MOUNT_DIR="$PROJECT_ROOT/mnt_bench"
BLOCK_SIZE=4096

# Cleanup
cleanup() {
    if mountpoint -q "$MOUNT_DIR"; then
        $BIN vm unmount "$MOUNT_DIR"
    fi
    rmdir "$MOUNT_DIR" 2>/dev/null || true
}
trap cleanup EXIT

info "=== Hexz Dictionary vs Standard Benchmark ==="
info "Target Size: ${SIZE_MB} MB"
info "Block Size:  ${BLOCK_SIZE} bytes"

# 1. Build
ensure_build "$BIN"

# 2. Generate Data
mkdir -p "$DATA_DIR"
if [[ -f "$DATA_FILE" ]]; then
    info "[2/6] Data file exists, skipping generation."
else
    info "[2/6] Generating complex JSON data..."
    # Run from project root so scripts/gen_data.py works if it assumes relative paths?
    # gen_data.py writes to data/complex_logs.json relative to CWD.
    cd "$PROJECT_ROOT"
    python3 "$SCRIPT_DIR/gen_json_logs.py"

    if [[ -f "data/complex_logs.json" ]]; then
        mv "data/complex_logs.json" "$DATA_FILE"
    fi
fi

# 3. Benchmark Standard
info "[3/6] Running STANDARD Compression (No Dict)..."
START=$(date +%s.%N)
$BIN data pack --disk "$DATA_FILE" --output "$SNAP_NODICT" --compression zstd --block-size $BLOCK_SIZE
END=$(date +%s.%N)
TIME_NODICT=$(echo "$END - $START" | bc)

# 4. Benchmark Dictionary
info "[4/6] Running DICTIONARY Compression..."
START=$(date +%s.%N)
$BIN data pack --disk "$DATA_FILE" --output "$SNAP_DICT" --compression zstd --block-size $BLOCK_SIZE --train-dict
END=$(date +%s.%N)
TIME_DICT=$(echo "$END - $START" | bc)

# 5. Stats
SIZE_ORIG=$(stat -c%s "$DATA_FILE")
SIZE_NODICT=$(stat -c%s "$SNAP_NODICT")
SIZE_DICT=$(stat -c%s "$SNAP_DICT")

RATIO_NODICT=$(echo "scale=2; $SIZE_ORIG / $SIZE_NODICT" | bc)
RATIO_DICT=$(echo "scale=2; $SIZE_ORIG / $SIZE_DICT" | bc)
SAVINGS=$(echo "scale=2; ($SIZE_NODICT - $SIZE_DICT) / 1024 / 1024" | bc)
PERCENT=$(echo "scale=2; 100 * ($SIZE_NODICT - $SIZE_DICT) / $SIZE_NODICT" | bc)

echo ""
info "RESULTS (Block Size: $BLOCK_SIZE)"
printf "%-15s | %-10s | %-10s | %-10s\n" "Method" "Time (s)" "Size (MB)" "Ratio"
echo "-------------------------------------------------------"
printf "%-15s | %-10.2f | %-10.2f | %-10.2fx\n" "Standard" $TIME_NODICT $(echo "scale=2; $SIZE_NODICT/1024/1024" | bc) $RATIO_NODICT
printf "%-15s | %-10.2f | %-10.2f | %-10.2fx\n" "Dictionary" $TIME_DICT $(echo "scale=2; $SIZE_DICT/1024/1024" | bc) $RATIO_DICT
echo "-------------------------------------------------------"
info "Storage Saved: ${SAVINGS} MB (${PERCENT}%)"

# 6. Verify
info "[6/6] Verifying Integrity..."
mkdir -p "$MOUNT_DIR"
$BIN vm mount "$SNAP_DICT" "$MOUNT_DIR" -d
sleep 1

if cmp -s "$DATA_FILE" "$MOUNT_DIR/disk"; then
    ok "SUCCESS: Integrity Verified."
else
    fail "FAILURE: Data corruption detected!"
fi

$BIN vm unmount "$MOUNT_DIR"
ok "Done."
