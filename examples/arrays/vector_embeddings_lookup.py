"""Example: Fast Vector Embeddings Lookup.

This example demonstrates using Hexz as a lightweight, high-performance
vector store. We store 100,000 embeddings and pull specific vectors
instantly by ID.
"""

import os
import numpy as np
import hexz
import time
from pathlib import Path

_DATA_DIR = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))), ".data", "arrays"
)


def run_example():
    os.makedirs(_DATA_DIR, exist_ok=True)

    num_vectors = 100_000
    dims = 768  # Standard dimension for many BERT/Transformer models

    path = os.path.join(_DATA_DIR, "embeddings.hxz")
    print(f"Creating vector store: {num_vectors} vectors, {dims} dimensions...")

    # 1. Create and save embeddings
    # We use float16 to save space
    embeddings = np.random.randn(num_vectors, dims).astype(np.float16)
    hexz.write_array(path, embeddings)

    # 2. Random Access Lookup
    print("\nPerforming random lookups...")

    # IDs we want to fetch
    test_ids = [0, 99_999, 42_000, 7]

    with hexz.ArrayView(path, shape=(num_vectors, dims), dtype=np.float16) as store:
        for vid in test_ids:
            start = time.perf_counter()
            vector = store[vid]
            duration = (time.perf_counter() - start) * 1000

            print(
                f"  Fetched Vector ID {vid:6} in {duration:6.2f}ms | Mean: {vector.mean():.4f}"
            )

            # Verify data integrity
            assert np.allclose(vector, embeddings[vid], atol=1e-3)

    print(
        "\n✓ Successfully retrieved random embeddings without loading the full 150MB file."
    )

    # Clean up
    Path(path).unlink()


if __name__ == "__main__":
    run_example()
