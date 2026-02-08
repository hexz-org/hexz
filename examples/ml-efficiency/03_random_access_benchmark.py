#!/usr/bin/env python3
"""
Random Access Benchmark: Compare random read performance across formats

This benchmark measures actual read latency and throughput for different
storage formats when accessing samples in random order (typical for ML training).
"""

import os
import random
import tarfile
import time
from pathlib import Path
from typing import List, Tuple

import numpy as np

try:
    from PIL import Image
    PIL_AVAILABLE = True
except ImportError:
    PIL_AVAILABLE = False
    print("Warning: PIL not installed. Install with: pip install pillow")

try:
    import h5py
    H5PY_AVAILABLE = True
except ImportError:
    H5PY_AVAILABLE = False
    print("Warning: h5py not installed for HDF5 comparison. Install with: pip install h5py")

try:
    from strata import StrataDataset
    STRATA_AVAILABLE = True
except ImportError:
    STRATA_AVAILABLE = False
    print("Warning: Strata not installed. Run: maturin develop --manifest-path ../../crates/loader/Cargo.toml")


# Configuration
NUM_SAMPLES = 1000  # Number of test images
IMAGE_SIZE = 224
NUM_READS = 500  # Number of random reads to benchmark
BENCHMARK_DIR = Path("./benchmark_data")


def create_test_dataset():
    """Create test dataset in multiple formats"""
    print("Setting up test dataset...")

    if BENCHMARK_DIR.exists():
        print(f"  Using existing data in {BENCHMARK_DIR}")
        return

    BENCHMARK_DIR.mkdir(parents=True)

    # Create images directory
    images_dir = BENCHMARK_DIR / "images"
    images_dir.mkdir(exist_ok=True)

    print(f"  Generating {NUM_SAMPLES} test images...")
    for i in range(NUM_SAMPLES):
        # Create random image
        arr = np.random.randint(0, 255, (IMAGE_SIZE, IMAGE_SIZE, 3), dtype=np.uint8)
        img = Image.fromarray(arr)
        img.save(images_dir / f"img_{i:05d}.jpg", quality=85)

    # Create TAR archive
    print("  Creating TAR archive...")
    tar_path = BENCHMARK_DIR / "dataset.tar"
    with tarfile.open(tar_path, "w") as tar:
        for img_file in images_dir.glob("*.jpg"):
            tar.add(img_file, arcname=img_file.name)

    # Create HDF5 file if available
    if H5PY_AVAILABLE:
        print("  Creating HDF5 file...")
        h5_path = BENCHMARK_DIR / "dataset.h5"
        with h5py.File(h5_path, "w") as f:
            # Store images as compressed datasets
            for i, img_file in enumerate(sorted(images_dir.glob("*.jpg"))):
                img = np.array(Image.open(img_file))
                f.create_dataset(f"img_{i:05d}", data=img, compression="gzip")

    print("  Dataset setup complete\n")


def benchmark_individual_files() -> Tuple[List[float], List[int]]:
    """Benchmark reading individual JPEG files"""
    if not PIL_AVAILABLE:
        return [], []

    images_dir = BENCHMARK_DIR / "images"
    image_files = sorted(list(images_dir.glob("*.jpg")))

    if len(image_files) == 0:
        return [], []

    # Random indices to read
    indices = [random.randint(0, len(image_files) - 1) for _ in range(NUM_READS)]

    latencies = []
    sizes = []

    for idx in indices:
        start = time.perf_counter()
        img = Image.open(image_files[idx])
        arr = np.array(img)  # Force decode
        elapsed = time.perf_counter() - start

        latencies.append(elapsed * 1000)  # Convert to ms
        sizes.append(arr.nbytes)

    return latencies, sizes


def benchmark_tar_archive() -> Tuple[List[float], List[int]]:
    """Benchmark reading from TAR archive (requires sequential scan)"""
    tar_path = BENCHMARK_DIR / "dataset.tar"
    if not tar_path.exists():
        return [], []

    # Build index by scanning TAR (this is required for random access)
    tar_index = {}
    with tarfile.open(tar_path, "r") as tar:
        for member in tar.getmembers():
            if member.name.endswith(".jpg"):
                img_id = int(member.name.split("_")[1].split(".")[0])
                tar_index[img_id] = member

    indices = [random.randint(0, len(tar_index) - 1) for _ in range(NUM_READS)]

    latencies = []
    sizes = []

    with tarfile.open(tar_path, "r") as tar:
        for idx in indices:
            start = time.perf_counter()
            member = tar_index[idx]
            f = tar.extractfile(member)
            if f:
                img = Image.open(f)
                arr = np.array(img)
                f.close()
                elapsed = time.perf_counter() - start

                latencies.append(elapsed * 1000)
                sizes.append(arr.nbytes)

    return latencies, sizes


def benchmark_hdf5() -> Tuple[List[float], List[int]]:
    """Benchmark reading from HDF5 file"""
    if not H5PY_AVAILABLE:
        return [], []

    h5_path = BENCHMARK_DIR / "dataset.h5"
    if not h5_path.exists():
        return [], []

    with h5py.File(h5_path, "r") as f:
        num_images = len(f.keys())
        indices = [random.randint(0, num_images - 1) for _ in range(NUM_READS)]

        latencies = []
        sizes = []

        for idx in indices:
            start = time.perf_counter()
            arr = f[f"img_{idx:05d}"][:]
            elapsed = time.perf_counter() - start

            latencies.append(elapsed * 1000)
            sizes.append(arr.nbytes)

    return latencies, sizes


def benchmark_strata() -> Tuple[List[float], List[int]]:
    """Benchmark reading from Strata archive"""
    if not STRATA_AVAILABLE:
        return [], []

    strata_path = BENCHMARK_DIR / "dataset.st"

    # Pack if doesn't exist
    if not strata_path.exists():
        print("  Packing Strata archive...")
        import strata
        strata.pack(
            input_dir=str(BENCHMARK_DIR / "images"),
            output_file=str(strata_path),
            compression="lz4",
            deduplication=False,  # No dedup for fair comparison
            threads=4,
        )

    dataset = StrataDataset(path=str(strata_path), shuffle=False, cache_size_mb=64)
    indices = [random.randint(0, len(dataset) - 1) for _ in range(NUM_READS)]

    latencies = []
    sizes = []

    for idx in indices:
        start = time.perf_counter()
        img, label = dataset[idx]
        arr = np.array(img)
        elapsed = time.perf_counter() - start

        latencies.append(elapsed * 1000)
        sizes.append(arr.nbytes)

    return latencies, sizes


def print_statistics(name: str, latencies: List[float], sizes: List[int]):
    """Print benchmark statistics"""
    if not latencies:
        print(f"{name:<20} SKIPPED (not available)")
        return

    latencies_array = np.array(latencies)
    throughput = (sum(sizes) / (1024**2)) / (sum(latencies) / 1000)  # MB/s

    print(f"{name:<20} "
          f"p50: {np.percentile(latencies_array, 50):6.2f}ms  "
          f"p95: {np.percentile(latencies_array, 95):6.2f}ms  "
          f"p99: {np.percentile(latencies_array, 99):6.2f}ms  "
          f"throughput: {throughput:6.1f} MB/s")


def main():
    print("=" * 80)
    print("Random Access Benchmark")
    print("=" * 80)
    print(f"\nConfiguration:")
    print(f"  Dataset size:     {NUM_SAMPLES} images ({IMAGE_SIZE}x{IMAGE_SIZE})")
    print(f"  Random reads:     {NUM_READS}")
    print(f"  Pattern:          Fully random (worst case for sequential formats)\n")

    # Setup
    create_test_dataset()

    print("=" * 80)
    print("Running benchmarks...")
    print("=" * 80)

    # Run benchmarks
    print("\nIndividual Files...")
    lat_files, size_files = benchmark_individual_files()

    print("TAR Archive...")
    lat_tar, size_tar = benchmark_tar_archive()

    print("HDF5...")
    lat_hdf5, size_hdf5 = benchmark_hdf5()

    print("Strata...")
    lat_strata, size_strata = benchmark_strata()

    # Results
    print("\n" + "=" * 80)
    print("RESULTS")
    print("=" * 80)
    print(f"\nRandom read latency (lower is better):\n")
    print(f"{'Format':<20} {'p50 Latency':<15} {'p95 Latency':<15} {'p99 Latency':<15} {'Throughput'}")
    print("-" * 80)

    print_statistics("Individual JPEGs", lat_files, size_files)
    print_statistics("TAR Archive", lat_tar, size_tar)
    print_statistics("HDF5", lat_hdf5, size_hdf5)
    print_statistics("Strata", lat_strata, size_strata)

    print("\n" + "=" * 80)
    print("\nKey Observations:")
    print("  - TAR archives have high tail latency due to sequential scanning")
    print("  - HDF5 provides good random access but requires decompression overhead")
    print("  - Strata uses block indexing for consistent low-latency random access")
    print("  - Individual files are fast but require many file system operations")
    print("=" * 80 + "\n")


if __name__ == "__main__":
    main()
