"""Example: Zero-Copy Performance Comparison.

This example benchmarks Hexz against standard Python pickle for loading
large arrays, demonstrating the performance benefits of zero-copy
buffer protocol integration.
"""

import os
import numpy as np
import hexz
import pickle
import time
from pathlib import Path

_DATA_DIR = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))), ".data", "arrays"
)


def run_example():
    os.makedirs(_DATA_DIR, exist_ok=True)

    # 1. Create a large array (100MB)
    size_mb = 100
    print(f"Generating {size_mb}MB test array...")
    data = np.random.randn(size_mb * 1024 * 1024 // 8).astype(np.float64)

    hexz_path = os.path.join(_DATA_DIR, "bench.hxz")
    pickle_path = os.path.join(_DATA_DIR, "bench.pkl")

    # 2. Save both
    print("Saving data...")
    hexz.write_array(hexz_path, data)
    with open(pickle_path, "wb") as f:
        pickle.dump(data, f)

    # 3. Benchmark Loading
    print("\nBenchmark: Loading 100MB array")
    print("-" * 30)

    # Pickle
    start = time.perf_counter()
    with open(pickle_path, "rb") as f:
        data_pickle = pickle.load(f)
    pickle_duration = time.perf_counter() - start
    print(f"Pickle: {pickle_duration:.4f}s (loaded {len(data_pickle)} elements)")

    # Hexz (Standard Read)
    start = time.perf_counter()
    data_hexz = hexz.read_array(hexz_path, shape=data.shape, dtype=data.dtype)
    hexz_duration = time.perf_counter() - start
    print(f"Hexz:   {hexz_duration:.4f}s (loaded {data_hexz.size} elements)")

    # Hexz (Zero-Copy with ArrayView)
    # This doesn't actually copy the data into Python memory until accessed
    start = time.perf_counter()
    with hexz.ArrayView(hexz_path, shape=data.shape, dtype=data.dtype) as view:
        # Just accessing properties is near-instant
        _ = view.shape
        # Materializing a small slice is also fast
        _ = view[0:100]
    view_duration = time.perf_counter() - start
    print(f"Hexz (ArrayView): {view_duration:.4f}s  <-- near instant metadata access")

    speedup = pickle_duration / hexz_duration
    print("-" * 30)
    print(f"Hexz is {speedup:.1f}x faster than Pickle for this load.")

    # Clean up
    Path(hexz_path).unlink()
    Path(pickle_path).unlink()


if __name__ == "__main__":
    run_example()
