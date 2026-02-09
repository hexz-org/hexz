# Strata Examples

This directory contains modular examples demonstrating key features of the Strata Python API.

## 1. Machine Learning Training (`examples/ml_training/`)

Demonstrates how to use `strata.Dataset` with PyTorch for high-performance training loops.

- **Files:**
  - `create_dataset.py`: Generates a dummy dataset (`dataset.st`) with variable-length items and an index file (`dataset.idx`).
  - `train.py`: Loads the dataset using `strata.Dataset` (with caching & prefetching) and iterates via `torch.utils.data.DataLoader`.

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
