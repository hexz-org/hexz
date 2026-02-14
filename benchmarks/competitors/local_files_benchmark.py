#!/usr/bin/env python3
"""
Local files benchmark implementation.

Benchmarks raw local file access as a baseline.
"""

import argparse
import json
import time
from pathlib import Path
from typing import Dict

import numpy as np
import psutil
from tqdm import tqdm


class LocalFilesBenchmark:
    """Benchmark raw local file performance (baseline)."""

    def __init__(self, data_dir: Path):
        self.data_dir = data_dir

        # Load metadata
        metadata_path = data_dir / "metadata.json"
        if metadata_path.exists():
            with open(metadata_path) as f:
                self.metadata = json.load(f)
        else:
            self.metadata = {"num_images": len(list(data_dir.glob("*.jpg")))}

    def measure_storage(self) -> Dict[str, float]:
        """Measure raw storage size."""
        print("Measuring storage...")

        images = list(self.data_dir.glob("*.jpg"))
        total_bytes = sum(img.stat().st_size for img in images)

        return {
            "total_bytes": total_bytes,
            "total_gb": total_bytes / (1024**3),
            "num_files": len(images),
            "avg_file_size_kb": (total_bytes / len(images)) / 1024 if images else 0,
        }

    def benchmark_sequential_read(self, num_samples: int = None) -> Dict[str, float]:
        """Benchmark sequential file reading."""
        print("Benchmarking sequential read...")

        images = sorted(self.data_dir.glob("*.jpg"))

        if num_samples is None:
            num_samples = len(images)
        else:
            images = images[:num_samples]

        start_time = time.time()
        bytes_read = 0

        for img_path in tqdm(images, desc="Sequential read"):
            # Read file
            with open(img_path, "rb") as f:
                img_bytes = f.read()
                bytes_read += len(img_bytes)

            # Optionally decode (to match real usage)
            # img = Image.open(img_path)
            # arr = np.array(img)

        elapsed = time.time() - start_time
        throughput_gbps = (bytes_read / (1024**3)) / elapsed

        return {
            "sequential_read_throughput_gbps": throughput_gbps,
            "samples_read": len(images),
            "time_sec": elapsed,
            "bytes_read": bytes_read,
        }

    def benchmark_random_access(
        self, num_samples: int = 1000, iterations: int = 3
    ) -> Dict[str, float]:
        """Benchmark random file access."""
        print("Benchmarking random access...")

        images = sorted(self.data_dir.glob("*.jpg"))
        total_images = len(images)

        # Generate random sample indices
        rng = np.random.RandomState(42)
        sample_indices = rng.randint(0, total_images, size=num_samples)

        latencies_cold = []
        latencies_warm = []

        # Cold cache measurement (first iteration)
        print("  Cold cache measurement...")

        # Clear OS page cache (requires sudo)
        # On Linux: sync && echo 3 > /proc/sys/vm/drop_caches
        # Since we can't do this in Python, we just measure first access

        for idx in tqdm(sample_indices, desc="Cold access"):
            img_path = images[idx]

            start_time = time.perf_counter()
            with open(img_path, "rb") as f:
                img_bytes = f.read()
                _ = len(img_bytes)
            latency_us = (time.perf_counter() - start_time) * 1e6
            latencies_cold.append(latency_us)

        # Warm cache measurements
        for iteration in range(iterations - 1):
            print(f"  Warm cache iteration {iteration + 1}/{iterations - 1}...")
            for idx in tqdm(sample_indices, desc="Warm access"):
                img_path = images[idx]

                start_time = time.perf_counter()
                with open(img_path, "rb") as f:
                    img_bytes = f.read()
                    _ = len(img_bytes)
                latency_us = (time.perf_counter() - start_time) * 1e6
                latencies_warm.append(latency_us)

        latencies_cold = np.array(latencies_cold)
        latencies_warm = np.array(latencies_warm)

        return {
            "random_access_cold_mean_us": float(np.mean(latencies_cold)),
            "random_access_cold_median_us": float(np.median(latencies_cold)),
            "random_access_cold_p95_us": float(np.percentile(latencies_cold, 95)),
            "random_access_warm_mean_us": float(np.mean(latencies_warm)),
            "random_access_warm_median_us": float(np.median(latencies_warm)),
            "random_access_warm_p95_us": float(np.percentile(latencies_warm, 95)),
            "num_samples": num_samples,
            "iterations": iterations,
        }

    def run_all(self) -> Dict:
        """Run all benchmarks."""
        results = {
            "format": "local_files",
            "version": "N/A",
            "test_data": str(self.data_dir),
            "system": self._get_system_info(),
            "metrics": {},
        }

        # 1. Storage measurement
        print("\n=== Storage ===")
        storage_results = self.measure_storage()
        results["metrics"].update(storage_results)

        # 2. Sequential read
        print("\n=== Sequential Read Performance ===")
        seq_results = self.benchmark_sequential_read()
        results["metrics"].update(seq_results)

        # 3. Random access
        print("\n=== Random Access Performance ===")
        random_results = self.benchmark_random_access(num_samples=1000)
        results["metrics"].update(random_results)

        # Add timestamp
        results["timestamp"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())

        return results

    def _get_system_info(self) -> Dict[str, str]:
        """Collect system information."""
        import platform

        return {
            "cpu": platform.processor() or "Unknown",
            "cpu_count": psutil.cpu_count(logical=True),
            "ram_gb": psutil.virtual_memory().total / (1024**3),
            "platform": platform.platform(),
            "python_version": platform.python_version(),
        }


def main():
    parser = argparse.ArgumentParser(description="Local files benchmark")
    parser.add_argument(
        "--data-dir",
        type=Path,
        default=Path("benchmarks/data/imagenet_val_50k"),
        help="Directory containing raw images",
    )
    parser.add_argument(
        "--results-file",
        type=Path,
        default=Path("benchmarks/results/local_files_results.json"),
        help="Output file for results",
    )

    args = parser.parse_args()

    # Run benchmarks
    benchmark = LocalFilesBenchmark(args.data_dir)

    results = benchmark.run_all()

    # Print summary
    print("\n" + "=" * 60)
    print("RESULTS SUMMARY")
    print("=" * 60)
    print(f"Format: {results['format']}")
    print("\nStorage:")
    print(f"  Total size: {results['metrics']['total_gb']:.2f} GB")
    print(f"  Num files: {results['metrics']['num_files']}")
    print("\nSequential Read:")
    print(
        f"  Throughput: {results['metrics']['sequential_read_throughput_gbps']:.2f} GB/s"
    )
    print("\nRandom Access:")
    print(f"  Cold mean: {results['metrics']['random_access_cold_mean_us']:.0f} µs")
    print(f"  Warm mean: {results['metrics']['random_access_warm_mean_us']:.0f} µs")

    # Save results
    args.results_file.parent.mkdir(parents=True, exist_ok=True)
    with open(args.results_file, "w") as f:
        json.dump(results, f, indent=2)

    print(f"\nResults saved to: {args.results_file}")


if __name__ == "__main__":
    main()
