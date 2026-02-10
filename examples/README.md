# Strata Examples

This directory contains modular examples demonstrating key features of the Strata Python API.

## 0. Quick Start (`examples/quickstart.py`)

**Run first.** Creates a tiny snapshot and reads it back — no CLI or extra data required.

```bash
# From repo root (after: cd crates/loader && maturin develop -E numpy)
python examples/quickstart.py
```

See [docs/quickstart.md](../docs/quickstart.md) for the full 5-minute guide (install, pack, read).

## 1. Machine Learning Training (`examples/ml_training/`)

Demonstrates how to use `strata.Dataset` with PyTorch for high-performance training loops.

- **Files:**
  - `create_dataset.py`: Generates a dummy dataset (`dataset.st`) with variable-length items and an index file (`dataset.idx`).
  - `train.py`: Loads the dataset using `strata.Dataset` (with caching & prefetching) and iterates via `torch.utils.data.DataLoader`.

- **Requires:** PyTorch (`pip install torch` or `pip install -e ".[torch]"` from `crates/loader`).

- **Run:**
  ```bash
  cd examples/ml_training
  python3 create_dataset.py
  python3 train.py
  ```

## 2. Compression Benchmarking (`examples/compression_bench/`)

Benchmarks different build profiles (`ml`, `eda`, `archival`) on real data.

- **Files:**
  - `bench.py`: Recursively builds snapshots from a source directory using different profiles and measures build time, file size, and read speed.

- **Run:**
  ```bash
  cd examples/compression_bench
  python3 bench.py
  ```

## 3. Advanced Build Configuration (`examples/advanced_build/`)

Demonstrates `strata.build` with custom overrides for fine-tuned control.

- **Files:**
  - `build_with_profiles.py`: Builds a snapshot using the `archival` profile but overrides `block_size` and verifies metadata.

- **Run:**
  ```bash
  cd examples/advanced_build
  python3 build_with_profiles.py
  ```

## 4. Benchmarks (`examples/benchmarks/`)

Performance testing and data generation scripts.

- **Files:**
  - `boot_performance.sh`: Measures VM boot times under various conditions.
  - `compression_ratio.sh`: Analyzes compression efficiency.
  - `large_scale.sh`: Runs large-scale system tests.
  - `gen_json_logs.py` / `gen_mixed_data.py`: Utilities for generating synthetic test data.

- **Run:**
  ```bash
  cd examples/benchmarks
  ./boot_performance.sh
  ```
