#!/bin/bash
set -e

# Hexz CLI Demo Script
# This script demonstrates:
# 1. Creating sample data and packing it (V1).
# 2. Creating a second version (V2) with small changes.
# 3. Packing V2 as a "Thin Archive" using V1 as a base.
# 4. Verifying that V2 is tiny because it only stores the delta.

# Use a hidden data directory inside scripts/
DATA_DIR="scripts/.data"
mkdir -p "$DATA_DIR"

SAMPLE_DATA="$DATA_DIR/sample_data"
ARCHIVE_V1="$DATA_DIR/archive_v1.hxz"
ARCHIVE_V2="$DATA_DIR/archive_v2.hxz"
RESTORED_V2="$DATA_DIR/restored_v2"

# Clean up previous runs
echo "Cleaning up..."
rm -rf "$SAMPLE_DATA" "$RESTORED_V2"

echo "=== 1. Creating Sample Data (10MB) ==="
mkdir -p "$SAMPLE_DATA/docs"
echo "Original text." > "$SAMPLE_DATA/docs/file.txt"
perl -e 'print "\xAA" x (1024 * 1024 * 10)' > "$SAMPLE_DATA/large.bin"

echo "=== 2. Packing V1 ==="
# Pack with DCAM to find optimal parameters
cargo run --package hexz-cli --release -- pack "$SAMPLE_DATA" "$ARCHIVE_V1" --compression zstd --dcam

echo ""
echo "=== 3. Creating Modified Data for V2 ==="
# We add a tiny file and modify one existing file.
# The 10MB large.bin remains identical.
echo "New file added in V2" > "$SAMPLE_DATA/new_file.txt"
echo "Modified existing file." >> "$SAMPLE_DATA/docs/file.txt"

echo ""
echo "=== 4. Packing V2 (Thin Archive) ==="
# By passing --base, Hexz will:
# 1. Inherit the CDC parameters from V1 (ensuring block alignment matches).
# 2. Only store the blocks that actually changed.
cargo run --package hexz-cli --release -- pack "$SAMPLE_DATA" "$ARCHIVE_V2" --base "$ARCHIVE_V1" --compression zstd

echo ""
echo "=== 5. Comparing V1 and V2 ==="
cargo run --package hexz-cli --release -- diff "$ARCHIVE_V1" "$ARCHIVE_V2"

echo ""
echo "=== 6. Verifying Results ==="
# Show sizes
SIZE_V1=$(ls -lh "$ARCHIVE_V1" | awk '{print $5}')
SIZE_V2=$(ls -lh "$ARCHIVE_V2" | awk '{print $5}')

echo "Archive V1 (Full): $SIZE_V1"
echo "Archive V2 (Thin): $SIZE_V2"

# Extract V2 to ensure it can reconstruct the full state using V1
cargo run --package hexz-cli --release -- extract "$ARCHIVE_V2" "$RESTORED_V2"

if diff -rq "$SAMPLE_DATA" "$RESTORED_V2"; then
    echo "SUCCESS: Thin archive V2 correctly reconstructed all files!"
else
    echo "FAILURE: Data mismatch detected."
    exit 1
fi

echo ""
echo "Demo complete."
