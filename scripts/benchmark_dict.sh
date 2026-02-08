#!/bin/bash
set -e

# Default size 100MB (0.1GB), pass argument to override
# We use 100MB by default because 1GB takes a while to generate/compress
SIZE_MB=${1:-100}

# Paths
DATA_FILE="data/bench_data_${SIZE_MB}MB.json"
SNAP_NODICT="data/bench_${SIZE_MB}MB_nodict.st"
SNAP_DICT="data/bench_${SIZE_MB}MB_dict.st"
MOUNT_DIR="mnt_bench"
RESTORE_FILE="data/bench_restore.tmp"
BINARY="./target/release/strata"
BLOCK_SIZE=4096 # 4KB blocks highlight dictionary benefits best

echo "=== SnapFS Dictionary vs Standard Benchmark ==="
echo "Target Size: ${SIZE_MB} MB"
echo "Block Size:  ${BLOCK_SIZE} bytes"

# 1. Build Release
echo -e "\n[1/6] Building Release CLI..."
cargo build --release -p snapfs --quiet

# 2. Generate Data
# Ensure directory exists
mkdir -p data
if [ -f "$DATA_FILE" ]; then
    echo "[2/6] Data file exists, skipping generation."
else
    echo "[2/6] Generating complex JSON data..."
    # Assuming you moved the python script to scripts/
    # If not, adjust path to ./gen_complex_data.py
    python3 scripts/gen_data.py

    # Rename/Move the output of the python script to our target variable if names differ
    # The python script defaults to data/complex_logs.json
    if [ -f "data/complex_logs.json" ]; then
        mv "data/complex_logs.json" "$DATA_FILE"
    fi
fi

# 3. Benchmark Standard (No Dict)
echo -e "\n[3/6] Running STANDARD Compression (No Dict)..."
START=$(date +%s.%N)
$BINARY create --disk "$DATA_FILE" --output "$SNAP_NODICT" --compression zstd --block-size $BLOCK_SIZE
END=$(date +%s.%N)
TIME_NODICT=$(echo "$END - $START" | bc)

# 4. Benchmark Dictionary (With Dict)
echo -e "\n[4/6] Running DICTIONARY Compression..."
START=$(date +%s.%N)
$BINARY create --disk "$DATA_FILE" --output "$SNAP_DICT" --compression zstd --block-size $BLOCK_SIZE --train-dict
END=$(date +%s.%N)
TIME_DICT=$(echo "$END - $START" | bc)

# 5. Calculate Stats
SIZE_ORIG=$(stat -c%s "$DATA_FILE")
SIZE_NODICT=$(stat -c%s "$SNAP_NODICT")
SIZE_DICT=$(stat -c%s "$SNAP_DICT")

RATIO_NODICT=$(echo "scale=2; $SIZE_ORIG / $SIZE_NODICT" | bc)
RATIO_DICT=$(echo "scale=2; $SIZE_ORIG / $SIZE_DICT" | bc)
SAVINGS=$(echo "scale=2; ($SIZE_NODICT - $SIZE_DICT) / 1024 / 1024" | bc)
PERCENT=$(echo "scale=2; 100 * ($SIZE_NODICT - $SIZE_DICT) / $SIZE_NODICT" | bc)

echo -e "\n======================================================="
echo "               RESULTS (Block Size: $BLOCK_SIZE)"
echo "======================================================="
printf "%-15s | %-10s | %-10s | %-10s\n" "Method" "Time (s)" "Size (MB)" "Ratio"
echo "-------------------------------------------------------"
printf "%-15s | %-10.2f | %-10.2f | %-10.2fx\n" "Standard" $TIME_NODICT $(echo "scale=2; $SIZE_NODICT/1024/1024" | bc) $RATIO_NODICT
printf "%-15s | %-10.2f | %-10.2f | %-10.2fx\n" "Dictionary" $TIME_DICT $(echo "scale=2; $SIZE_DICT/1024/1024" | bc) $RATIO_DICT
echo "-------------------------------------------------------"
echo "Storage Saved: ${SAVINGS} MB (${PERCENT}%)"
echo "======================================================="

# 6. Verify Integrity (Only checking the Dict version as it's the complex one)
echo -e "\n[6/6] Verifying Integrity of Dictionary Snapshot..."
mkdir -p "$MOUNT_DIR"
$BINARY mount "$SNAP_DICT" "$MOUNT_DIR" --daemon
sleep 1 # Wait for mount

if cmp -s "$DATA_FILE" "$MOUNT_DIR/disk"; then
    echo "SUCCESS: Integrity Verified."
else
    echo "FAILURE: Data corruption detected!"
    $BINARY unmount "$MOUNT_DIR"
    exit 1
fi

$BINARY unmount "$MOUNT_DIR"
rmdir "$MOUNT_DIR"
# Optional: cleanup files
# rm "$SNAP_NODICT" "$SNAP_DICT" "$RESTORE_FILE"
echo "Done."
