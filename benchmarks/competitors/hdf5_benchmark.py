#!/usr/bin/env python3
"""
HDF5 benchmark implementation.

Benchmarks HDF5 against Hexz using identical test data and metrics.
"""

import argparse
import json
import time
from pathlib import Path
from typing import Dict

import h5py
import numpy as np
import psutil
from tqdm import tqdm


class HDF5Benchmark:
    """Benchmark HDF5 performance."""

    def __init__(
        self,
        data_dir: Path,
        output_file: Path,
        chunk_size: int = 64 * 1024,  # 64KB chunks to match Hexz
        compression: str = "gzip",
        compression_opts: int = 3,  # gzip level 3 (similar to Zstd-3)
    ):
        self.data_dir = data_dir
        self.output_file = output_file
        self.chunk_size = chunk_size
        self.compression = compression
        self.compression_opts = compression_opts

        # Load metadata
        metadata_path = data_dir / "metadata.json"
        if metadata_path.exists():
            with open(metadata_path) as f:
                self.metadata = json.load(f)
        else:
            self.metadata = {"num_images": len(list(data_dir.glob("*.jpg")))}

    def create_hdf5(self) -> Dict[str, float]:
        """Create HDF5 file from raw images."""
        print(f"Creating HDF5 file: {self.output_file}")

        self.output_file.parent.mkdir(parents=True, exist_ok=True)

        # Get all images
        images = sorted(self.data_dir.glob("*.jpg"))
        total_images = len(images)

        start_time = time.time()

        with h5py.File(self.output_file, "w") as f:
            # Create variable-length dataset for JPEG blobs
            # (More realistic than storing decoded arrays)
            dt = h5py.special_dtype(vlen=np.dtype("uint8"))

            dataset = f.create_dataset(
                "images",
                shape=(total_images,),
                dtype=dt,
                chunks=(1,),  # One chunk per image for random access
                compression=self.compression,
                compression_opts=self.compression_opts,
            )

            # Store images as JPEG bytes (realistic for image datasets)
            for i, img_path in enumerate(tqdm(images, desc="Writing images")):
                with open(img_path, "rb") as img_file:
                    img_bytes = img_file.read()
                    dataset[i] = np.frombuffer(img_bytes, dtype=np.uint8)

            # Store metadata
            f.attrs["num_images"] = total_images
            f.attrs["compression"] = self.compression
            f.attrs["chunk_size"] = self.chunk_size

        elapsed = time.time() - start_time
        file_size = self.output_file.stat().st_size
        throughput_gbps = (file_size / (1024**3)) / elapsed

        return {
            "write_time_sec": elapsed,
            "write_throughput_gbps": throughput_gbps,
            "total_bytes": file_size,
            "total_gb": file_size / (1024**3),
            "compression": self.compression,
            "compression_opts": self.compression_opts,
        }

    def benchmark_sequential_read(self, num_samples: int = None) -> Dict[str, float]:
        """Benchmark sequential read throughput."""
        print("Benchmarking sequential read...")

        if num_samples is None:
            num_samples = self.metadata["num_images"]

        start_time = time.time()
        bytes_read = 0

        with h5py.File(self.output_file, "r") as f:
            dataset = f["images"]

            for i in tqdm(
                range(min(num_samples, len(dataset))), desc="Sequential read"
            ):
                # Read JPEG bytes
                img_bytes = dataset[i]
                bytes_read += len(img_bytes)

                # Optionally decode (to match real usage)
                # img = Image.open(io.BytesIO(img_bytes.tobytes()))
                # arr = np.array(img)

        elapsed = time.time() - start_time
        throughput_gbps = (bytes_read / (1024**3)) / elapsed

        return {
            "sequential_read_throughput_gbps": throughput_gbps,
            "samples_read": num_samples,
            "time_sec": elapsed,
            "bytes_read": bytes_read,
        }

    def benchmark_random_access(
        self, num_samples: int = 1000, iterations: int = 3
    ) -> Dict[str, float]:
        """Benchmark random access latency."""
        print("Benchmarking random access...")

        total_images = self.metadata["num_images"]

        # Generate random sample indices
        rng = np.random.RandomState(42)
        sample_indices = rng.randint(0, total_images, size=num_samples)

        latencies_cold = []
        latencies_warm = []

        with h5py.File(self.output_file, "r") as f:
            dataset = f["images"]

            # Cold cache measurement (first iteration)
            print("  Cold cache measurement...")
            for idx in tqdm(sample_indices, desc="Cold access"):
                start_time = time.perf_counter()
                img_bytes = dataset[idx]
                _ = len(img_bytes)  # Force read
                latency_us = (time.perf_counter() - start_time) * 1e6
                latencies_cold.append(latency_us)

            # Warm cache measurements
            for iteration in range(iterations - 1):
                print(f"  Warm cache iteration {iteration + 1}/{iterations - 1}...")
                for idx in tqdm(sample_indices, desc="Warm access"):
                    start_time = time.perf_counter()
                    img_bytes = dataset[idx]
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
            "format": "hdf5",
            "version": h5py.__version__,
            "test_data": str(self.data_dir),
            "system": self._get_system_info(),
            "metrics": {},
        }

        # 1. Create HDF5 file (write benchmark)
        print("\n=== Write Performance ===")
        write_results = self.create_hdf5()
        results["metrics"].update(write_results)

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
    parser = argparse.ArgumentParser(description="HDF5 benchmark")
    parser.add_argument(
        "--data-dir",
        type=Path,
        default=Path("benchmarks/data/imagenet_val_50k"),
        help="Directory containing raw images",
    )
    parser.add_argument(
        "--output-file",
        type=Path,
        default=Path("benchmarks/data/hdf5_dataset.h5"),
        help="Output HDF5 file",
    )
    parser.add_argument(
        "--compression",
        choices=["gzip", "lzf", None],
        default="gzip",
        help="HDF5 compression (gzip similar to zstd)",
    )
    parser.add_argument(
        "--compression-level",
        type=int,
        default=3,
        help="Compression level (1-9 for gzip)",
    )
    parser.add_argument(
        "--results-file",
        type=Path,
        default=Path("benchmarks/results/hdf5_results.json"),
        help="Output file for results",
    )

    args = parser.parse_args()

    # Run benchmarks
    benchmark = HDF5Benchmark(
        args.data_dir,
        args.output_file,
        compression=args.compression,
        compression_opts=args.compression_level,
    )

    results = benchmark.run_all()

    # Print summary
    print("\n" + "=" * 60)
    print("RESULTS SUMMARY")
    print("=" * 60)
    print(f"Format: {results['format']} v{results['version']}")
    print("\nWrite Performance:")
    print(f"  Throughput: {results['metrics']['write_throughput_gbps']:.2f} GB/s")
    print(f"  Total size: {results['metrics']['total_gb']:.2f} GB")
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
