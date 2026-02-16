# Profiling and Flamegraph Workflow

This guide describes how to profile Hexz read/write hot paths using
flamegraphs, `perf`, and Criterion benchmark comparisons.

## Prerequisites

```bash
# Install flamegraph tool (wraps perf on Linux)
cargo install flamegraph

# Install critcmp for baseline comparisons
cargo install critcmp

# Linux: ensure perf is available
# Ubuntu/Debian: sudo apt install linux-tools-common linux-tools-$(uname -r)
# Arch: sudo pacman -S perf
# Fedora: sudo dnf install perf

# Allow perf for non-root users (required for flamegraph)
echo -1 | sudo tee /proc/sys/kernel/perf_event_paranoid
```

## Generating a Flamegraph

```bash
# Default: profiles the read_throughput benchmark
make bench-flamegraph

# Profile a specific benchmark binary
make bench-flamegraph read_throughput
make bench-flamegraph decompress_scaling
make bench-flamegraph cache_performance
```

This produces `flamegraph.svg` in the repo root. Open it in a browser
for an interactive visualization of where CPU time is spent.

## Identifying Hot Functions

1. **Generate flamegraph** with `make bench-flamegraph`
2. **Open `flamegraph.svg`** in a browser
3. **Look for wide bars** — these are the most expensive call stacks
4. **Click to zoom** into specific subtrees
5. **Common hot spots**:
   - `decompress` — block decompression (LZ4/Zstd)
   - `crc32` — checksum verification
   - `read_exact` — backend I/O
   - `Mutex::lock` — cache contention

## Running Targeted Benchmarks

```bash
# Run all benchmarks
make bench

# Run a specific benchmark binary
make bench read_throughput
make bench decompress_scaling

# List available benchmark categories
make bench-list
```

## Comparing Baselines

Use `critcmp` to measure the impact of your changes:

```bash
# 1. Save a baseline before your changes
make save-baseline before

# 2. Archive it for later comparison
make archive-baseline before

# 3. Make your changes, then save a new baseline
make save-baseline after
make archive-baseline after

# 4. Compare the two baselines
make compare-baseline before after

# Or: run benchmarks and compare to an archived baseline in one step
make bench-compare before
```

## Full Profiling Cycle

A typical performance investigation looks like:

```bash
# Step 1: Establish baseline
make save-baseline v0.1.4
make archive-baseline v0.1.4

# Step 2: Generate flamegraph to find hot spots
make bench-flamegraph read_throughput

# Step 3: Make targeted optimizations based on flamegraph

# Step 4: Run specific benchmarks to verify improvement
make bench read_throughput
make bench decompress_scaling

# Step 5: Compare to baseline
make bench-compare v0.1.4

# Step 6: Generate new flamegraph to verify hot spot is gone
make bench-flamegraph read_throughput
```

## Using perf Directly

For more detailed profiling (e.g., cache misses, branch mispredictions):

```bash
# Build benchmarks in release mode
cargo bench --package hexz --bench read_throughput --no-run

# Find the binary
BENCH_BIN=$(find target/release/deps -name 'read_throughput-*' -executable | head -1)

# Record with perf
perf record -g --call-graph dwarf $BENCH_BIN --bench

# View report
perf report

# Or generate a perf flamegraph
perf script | stackcollapse-perf.pl | flamegraph.pl > perf-flamegraph.svg
```
