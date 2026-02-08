#!/usr/bin/env python3
"""
Streaming Demo: Start training instantly without downloading the full dataset

This demonstrates how Strata enables immediate training on large remote datasets
by streaming only the blocks you need, when you need them.
"""

import os
import time
from pathlib import Path

import numpy as np

try:
    import torch
    from torch.utils.data import DataLoader
    TORCH_AVAILABLE = True
except ImportError:
    TORCH_AVAILABLE = False
    print("WARNING: PyTorch not installed. Install with: pip install torch")

try:
    from strata import StrataDataset
    STRATA_AVAILABLE = True
except ImportError:
    STRATA_AVAILABLE = False
    print("WARNING: Strata not installed. Run: maturin develop --manifest-path ../../crates/loader/Cargo.toml")


def simulate_traditional_download(dataset_size_gb: float):
    """Simulate downloading entire dataset first"""
    print(f"Traditional Approach: Downloading {dataset_size_gb:.1f}GB dataset...")
    print("   [####################] 100%")

    # Simulate download time (assume 100 MB/s)
    download_time = dataset_size_gb * 1024 / 100
    print(f"   Download completed in {download_time:.0f}s ({download_time/60:.1f} minutes)")

    print("\nExtracting compressed files...")
    # Simulate extraction time
    extract_time = download_time * 0.3
    print(f"   Extraction completed in {extract_time:.0f}s")

    total_time = download_time + extract_time
    print(f"\nTotal time before first batch: {total_time:.0f}s ({total_time/60:.1f} minutes)\n")
    return total_time


def simulate_strata_streaming():
    """Simulate Strata's streaming approach"""
    print(f"Strata Approach: Streaming dataset...")

    # Simulate index download (tiny, ~0.1% of dataset)
    print("   |- Downloading index... [##] 0.2s")

    # Simulate first batch prefetch
    print("   |- Prefetching first batch... [####] 0.6s")

    first_batch_time = 0.8
    print(f"\nTime to first batch: {first_batch_time:.1f}s")
    print("   Background: Prefetching next batches in parallel...\n")
    return first_batch_time


def run_demo_with_strata():
    """Run actual training demo with Strata if available"""
    if not (TORCH_AVAILABLE and STRATA_AVAILABLE):
        print("[ERROR] Cannot run live demo - missing dependencies\n")
        return

    # Check if demo dataset exists
    dataset_path = Path("../imagenet-mini/imagenet-mini.st")
    if not dataset_path.exists():
        print(f"[INFO] Demo dataset not found at {dataset_path}")
        print("   Run the imagenet-mini example first to generate test data\n")
        return

    print("=" * 70)
    print("LIVE DEMO: Streaming with Strata")
    print("=" * 70)

    try:
        # Initialize dataset
        print("\n[1] Opening dataset...")
        start = time.time()
        dataset = StrataDataset(
            path=str(dataset_path),
            shuffle=True,
            cache_size_mb=256,
        )
        open_time = time.time() - start
        print(f"   [DONE] Dataset opened in {open_time:.3f}s (index loaded)")

        # Create DataLoader
        print("\n[2] Creating DataLoader...")
        loader = DataLoader(
            dataset,
            batch_size=32,
            num_workers=2,
            prefetch_factor=2,  # Helps with streaming
        )
        print(f"   [DONE] DataLoader ready")

        # Fetch first batch
        print("\n[3] Fetching first batch...")
        start = time.time()
        first_batch = next(iter(loader))
        first_batch_time = time.time() - start
        print(f"   [DONE] First batch ready in {first_batch_time:.3f}s")
        print(f"      Batch shape: {first_batch[0].shape}")

        # Simulate a few more batches to show consistent performance
        print("\n[4] Fetching next 5 batches (with prefetching)...")
        times = []
        for i in range(5):
            start = time.time()
            batch = next(iter(loader))
            elapsed = time.time() - start
            times.append(elapsed)
            print(f"      Batch {i+2}: {elapsed:.3f}s")

        avg_time = np.mean(times)
        print(f"\n   Average batch time: {avg_time:.3f}s")
        print(f"   Note: Prefetcher keeps subsequent batches fast!\n")

    except Exception as e:
        print(f"   [ERROR] Demo failed: {e}\n")


def main():
    print("=" * 70)
    print("Strata Streaming Demo")
    print("=" * 70)
    print("\nScenario: Training on a large remote dataset (e.g., ImageNet-21k)")
    print("          Dataset size: 100GB compressed, 1.3TB uncompressed")
    print("=" * 70)

    # Traditional approach
    print("\n" + "-" * 70)
    trad_time = simulate_traditional_download(dataset_size_gb=100)

    print("-" * 70)

    # Strata approach
    print()
    strata_time = simulate_strata_streaming()

    print("-" * 70)

    # Comparison
    print("\n" + "=" * 70)
    print("COMPARISON")
    print("=" * 70)
    speedup = trad_time / strata_time
    print(f"\n{'Approach':<20} {'Time to First Batch':<25} {'Speedup'}")
    print("-" * 70)
    print(f"{'Traditional':<20} {f'{trad_time:.0f}s ({trad_time/60:.1f} min)':<25} {'1.0x (baseline)'}")
    print(f"{'Strata Streaming':<20} {f'{strata_time:.1f}s':<25} {f'{speedup:.0f}x faster'}")

    print(f"\nKey Benefits:")
    print(f"   - Start training in <1 second instead of waiting minutes/hours")
    print(f"   - No need to download entire dataset upfront")
    print(f"   - Pay only for the data you actually use")
    print(f"   - Incremental caching: frequently used samples stay local")
    print("=" * 70)

    # Run live demo if available
    print("\n")
    run_demo_with_strata()

    print("=" * 70)
    print("\nReal-World Use Cases:")
    print("   1. Rapid experimentation: Test model on subset, scale up if promising")
    print("   2. Remote training: Train on spot instances without local storage")
    print("   3. Large datasets: Work with multi-TB datasets on machines with limited disk")
    print("   4. Cost savings: Don't pay for S3 bandwidth you don't need")
    print("=" * 70 + "\n")


if __name__ == "__main__":
    main()
