#!/usr/bin/env python3
"""
Example: Model Checkpoint Deduplication with Hexz

This script demonstrates the "Checkpoint Pivot" value proposition:
Storing multiple versions of a model fine-tuned on different data,
while only paying for the storage of the changed weights.
"""

import os
import time

import numpy as np

import hexz


def create_fake_weights(size_mb: int, pattern: float = 0.0):
    """Create synthetic weights with some structure."""
    size = size_mb * 1024 * 1024
    # Simulate float32 weights
    weights = np.random.normal(pattern, 1.0, size // 4).astype(np.float32)
    return weights.tobytes()


def main():
    print("Hexz Checkpoint Deduplication Demo")
    print("-" * 40)

    base_path = "base_model.hxz"
    ft_path = "finetuned_model.hxz"

    # 1. Create a Base Model (e.g., 100MB of weights)
    print("Phase 1: Saving Base Model (100MB)...")
    base_weights = create_fake_weights(100, pattern=0.0)

    start = time.time()
    with hexz.Writer(base_path, compression="lz4", dedup=True) as writer:
        writer.add_bytes(base_weights)
        writer.add_metadata(
            {"name": "base-model-v1", "framework": "pytorch", "type": "foundation"}
        )
    duration = time.time() - start

    base_size = os.path.getsize(base_path)
    print(f"Saved base model in {duration:.2f}s")
    print(f"Physical file size: {base_size / 1024 / 1024:.2f} MB")

    # 2. Simulate Fine-tuning
    # We take the base weights and change only 5% of them
    print("\nPhase 2: Simulating Fine-tuning (modifying 5% of weights)...")
    ft_weights = bytearray(base_weights)
    # Modify a 5MB slice in the middle
    # We use a distinct pattern to ensure the content hash changes
    start_idx = 40 * 1024 * 1024
    end_idx = 45 * 1024 * 1024
    ft_weights[start_idx:end_idx] = create_fake_weights(5, pattern=5.0)

    # 3. Save Fine-tuned Model using the Base as Parent
    print("Saving Fine-tuned Model (deduplicating against base)...")
    start = time.time()
    # By passing `parent=base_path`, the writer will automatically read the
    # base model's index and avoid storing any data chunks that are already
    # present in the base model.
    with hexz.Writer(ft_path, compression="lz4", parent=base_path) as writer:
        # Save the full fine-tuned weights. The writer will internally turn
        # any unmodified chunks into lightweight parent references.
        writer.add_bytes(bytes(ft_weights))
        writer.add_metadata({"name": "finetuned-step-1000", "framework": "pytorch"})
    duration = time.time() - start

    ft_size = os.path.getsize(ft_path)
    print(f"Saved fine-tuned model in {duration:.2f}s")
    print(f"Physical file size: {ft_size / 1024 / 1024:.2f} MB")

    # 4. Compare and Analyze
    print("Storage Analysis")
    print("-" * 20)
    total_raw = 200  # 100MB + 100MB
    total_hexz = (base_size + ft_size) / 1024 / 1024
    savings = (1 - (total_hexz / total_raw)) * 100

    print(f"Total uncompressed size: {total_raw:.2f} MB")
    print(f"Total Hexz storage:      {total_hexz:.2f} MB")
    print(f"Space Savings:           {savings:.1f}%")

    # 5. Fast Random Access
    print("\nPhase 3: Fast Random Access")
    with hexz.open(ft_path) as reader:
        # Read the modified layer only (5MB at offset 40MB)
        # This only fetches the changed blocks!
        layer = reader.read(5 * 1024 * 1024, offset=40 * 1024 * 1024)
        print(
            f"Read {len(layer) / 1024 / 1024:.1f}MB layer from S3-ready snapshot. Cursor: {reader.tell()}"
        )

    # Cleanup
    os.remove(base_path)
    os.remove(ft_path)


if __name__ == "__main__":
    main()
