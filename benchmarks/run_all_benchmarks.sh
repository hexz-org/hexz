#!/usr/bin/env bash
#
# Run all competitor benchmarks and generate comparison report.
#
# Usage:
#   ./benchmarks/run_all_benchmarks.sh [--small]
#
# Options:
#   --small    Use small test dataset (1000 images) for quick testing

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Parse arguments
USE_SMALL=false
if [[ "${1:-}" == "--small" ]]; then
    USE_SMALL=true
    echo "Using small test dataset"
fi

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "======================================"
echo "  Hexz Competitor Benchmarks"
echo "======================================"
echo

# Step 1: Generate test data if needed
if $USE_SMALL; then
    DATA_DIR="$PROJECT_ROOT/benchmarks/data/test_small"
else
    DATA_DIR="$PROJECT_ROOT/benchmarks/data/imagenet_val_50k"
fi

if [ ! -d "$DATA_DIR" ]; then
    echo -e "${YELLOW}Step 1:${NC} Generating test data..."
    if $USE_SMALL; then
        python "$SCRIPT_DIR/generate_test_data.py" --small
    else
        python "$SCRIPT_DIR/generate_test_data.py"
    fi
    echo
else
    echo -e "${GREEN}Step 1:${NC} Test data already exists at $DATA_DIR"
    echo
fi

# Step 2: Run benchmarks
echo -e "${YELLOW}Step 2:${NC} Running benchmarks..."
echo

# Local files (baseline)
echo "Running: Local Files benchmark..."
python "$SCRIPT_DIR/competitors/local_files_benchmark.py" \
    --data-dir "$DATA_DIR" \
    --results-file "$SCRIPT_DIR/results/local_files_results.json"
echo

# WebDataset
echo "Running: WebDataset benchmark..."
python "$SCRIPT_DIR/competitors/webdataset_benchmark.py" \
    --data-dir "$DATA_DIR" \
    --output-dir "$PROJECT_ROOT/benchmarks/data/webdataset_shards" \
    --num-shards 10 \
    --results-file "$SCRIPT_DIR/results/webdataset_results.json"
echo

# HDF5
echo "Running: HDF5 benchmark..."
python "$SCRIPT_DIR/competitors/hdf5_benchmark.py" \
    --data-dir "$DATA_DIR" \
    --output-file "$PROJECT_ROOT/benchmarks/data/hdf5_dataset.h5" \
    --compression gzip \
    --compression-level 3 \
    --results-file "$SCRIPT_DIR/results/hdf5_results.json"
echo

# TODO: Add Hexz benchmark here
# echo "Running: Hexz benchmark..."
# cargo run --release --bin hexz -- pack ...

# Step 3: Generate comparison report
echo -e "${YELLOW}Step 3:${NC} Generating comparison report..."
python "$SCRIPT_DIR/compare_all.py" \
    --results-dir "$SCRIPT_DIR/results" \
    --output "$SCRIPT_DIR/results/COMPARISON.md"

echo
echo -e "${GREEN}All benchmarks complete!${NC}"
echo
echo "Results:"
echo "  - Individual results: $SCRIPT_DIR/results/*_results.json"
echo "  - Comparison report: $SCRIPT_DIR/results/COMPARISON.md"
echo
echo "View comparison:"
echo "  cat $SCRIPT_DIR/results/COMPARISON.md"
