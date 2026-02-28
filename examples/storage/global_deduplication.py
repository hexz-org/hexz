#!/usr/bin/env python3
"""
Example: Many-to-One Model Deduplication

This script demonstrates deduplicating a new model against multiple
different base models simultaneously. This is useful when you have
a library of foundation models and want to ensure a new fine-tune
only stores unique data not present in ANY of them.
"""

import os
import time

import numpy as np

import hexz

_DATA_DIR = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))), ".data", "storage"
)


def create_weights(size_mb, seed):
    """Create deterministic weights based on a seed."""
    np.random.seed(seed)
    # Simulate float32 weights
    return np.random.randn(size_mb * 1024 * 1024 // 4).astype(np.float32).tobytes()


def main():
    os.makedirs(_DATA_DIR, exist_ok=True)

    print("Hexz Multi-Parent Deduplication Demo")
    print("-" * 40)

    # 1. Create two different foundation models
    # Model A: 50MB, Seed 1
    # Model B: 50MB, Seed 2
    path_a = os.path.join(_DATA_DIR, "model_a.hxz")
    path_b = os.path.join(_DATA_DIR, "model_b.hxz")

    print("\nCreating two foundation models...")
    weights_a = create_weights(50, seed=1)
    weights_b = create_weights(50, seed=2)

    with hexz.Writer(path_a) as w:
        w.add_bytes(weights_a)

    with hexz.Writer(path_b) as w:
        w.add_bytes(weights_b)

    size_a = os.path.getsize(path_a)
    size_b = os.path.getsize(path_b)
    print(
        f"Created Model A ({size_a / 1024 / 1024:.1f} MB) and Model B ({size_b / 1024 / 1024:.1f} MB)"
    )

    # 2. Create a "Hybrid" model
    # This model contains:
    # - First 25MB of Model A
    # - 10MB of completely NEW weights
    # - Last 25MB of Model B
    print("\nConstructing a Hybrid model (A + New + B)...")
    hybrid_weights = (
        weights_a[: 25 * 1024 * 1024]
        + create_weights(10, seed=3)
        + weights_b[25 * 1024 * 1024 :]
    )
    path_hybrid = os.path.join(_DATA_DIR, "model_hybrid.hxz")

    # 3. Save with MULTIPLE parents
    print("Saving Hybrid model with [Model A, Model B] as parents...")
    start = time.time()
    # hexz will look into both A and B's indices to find matching blocks
    with hexz.Writer(path_hybrid, parent=[path_a, path_b]) as writer:
        writer.add_bytes(hybrid_weights)
    duration = time.time() - start

    hybrid_file_size = os.path.getsize(path_hybrid)
    print(f"Saved hybrid model in {duration:.2f}s")
    print(f"Physical file size: {hybrid_file_size / 1024 / 1024:.2f} MB")

    # 4. Analysis
    print("\nEfficiency Analysis")
    print("-" * 20)
    # Uncompressed, this is 25+10+25 = 60MB.
    # It should only physically store the 10MB of new weights (plus some CDC overhead).
    expected_raw = 60.0
    actual_disk = hybrid_file_size / 1024 / 1024

    print(f"Logical model size:  {expected_raw:.1f} MB")
    print(f"Actual disk usage:   {actual_disk:.1f} MB")

    savings = (1 - (actual_disk / expected_raw)) * 100
    print(f"Cross-file savings:  {savings:.1f}%")

    if actual_disk < 15:  # 10MB + some metadata/index
        print("\n✨ Success! Deduplicated against multiple unrelated files.")
    else:
        print("\n❌ Deduplication did not work as expected.")

    # Cleanup
    for p in [path_a, path_b, path_hybrid]:
        if os.path.exists(p):
            os.remove(p)


if __name__ == "__main__":
    main()
