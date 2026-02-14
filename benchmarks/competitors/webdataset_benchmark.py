#!/usr/bin/env python3
"""
WebDataset benchmark implementation.

Benchmarks WebDataset against Hexz using identical test data and metrics.
"""

import argparse
import json
import tarfile
import time
from pathlib import Path
from typing import Dict

import numpy as np
import psutil
import webdataset as wds
from tqdm import tqdm


class WebDatasetBenchmark:
    """Benchmark WebDataset performance."""

    def __init__(self, data_dir: Path, output_dir: Path, num_shards: int = 100):
        self.data_dir = data_dir
        self.output_dir = output_dir
        self.num_shards = num_shards
        self.shard_pattern = str(output_dir / "shard_{0:06d}.tar")

        # Load metadata
        metadata_path = data_dir / "metadata.json"
        if metadata_path.exists():
            with open(metadata_path) as f:
                self.metadata = json.load(f)
        else:
            self.metadata = {"num_images": len(list(data_dir.glob("*.jpg")))}

    def create_shards(self) -> Dict[str, float]:
        """Create WebDataset tar shards from raw images."""
        print(f"Creating {self.num_shards} WebDataset shards...")

        self.output_dir.mkdir(parents=True, exist_ok=True)

        # Get all images
        images = sorted(self.data_dir.glob("*.jpg"))
        total_images = len(images)
        images_per_shard = (total_images + self.num_shards - 1) // self.num_shards

        start_time = time.time()
        total_bytes_written = 0

        for shard_idx in tqdm(range(self.num_shards)):
            shard_path = Path(self.shard_pattern.format(shard_idx))

            # Get images for this shard
            start_idx = shard_idx * images_per_shard
            end_idx = min(start_idx + images_per_shard, total_images)
            shard_images = images[start_idx:end_idx]

            if not shard_images:
                break

            # Create tar shard
            with tarfile.open(shard_path, "w") as tar:
                for img_path in shard_images:
                    # WebDataset naming convention
                    basename = img_path.stem
                    tar.add(img_path, arcname=f"{basename}.jpg")

            total_bytes_written += shard_path.stat().st_size

        elapsed = time.time() - start_time
        throughput_gbps = (total_bytes_written / (1024**3)) / elapsed

        return {
            "write_time_sec": elapsed,
            "write_throughput_gbps": throughput_gbps,
            "total_bytes": total_bytes_written,
            "total_gb": total_bytes_written / (1024**3),
            "num_shards": self.num_shards,
        }

    def benchmark_sequential_read(self, num_samples: int = None) -> Dict[str, float]:
        """Benchmark sequential read throughput."""
        print("Benchmarking sequential read...")

        if num_samples is None:
            num_samples = self.metadata["num_images"]

        # Create WebDataset
        dataset = wds.WebDataset(
            self.shard_pattern.format("*"),
            shardshuffle=False,
        ).decode("pil")

        start_time = time.time()
        bytes_read = 0
        samples_read = 0

        for sample in tqdm(dataset, total=num_samples, desc="Sequential read"):
            # Access the image data
            img = sample["jpg"]
            # Simulate minimal processing (convert to numpy)
            arr = np.array(img)
            bytes_read += arr.nbytes
            samples_read += 1

            if samples_read >= num_samples:
                break

        elapsed = time.time() - start_time
        throughput_gbps = (bytes_read / (1024**3)) / elapsed

        return {
            "sequential_read_throughput_gbps": throughput_gbps,
            "samples_read": samples_read,
            "time_sec": elapsed,
            "bytes_read": bytes_read,
        }

    def benchmark_random_access(
        self, num_samples: int = 1000, iterations: int = 3
    ) -> Dict[str, float]:
        """
        Benchmark random access latency.

        Note: WebDataset doesn't support true random access. This benchmarks
        the shard-level seeking which is the closest equivalent.
        """
        print("Benchmarking random access (shard-level)...")

        total_images = self.metadata["num_images"]
        images_per_shard = (total_images + self.num_shards - 1) // self.num_shards

        # Generate random sample indices
        rng = np.random.RandomState(42)
        sample_indices = rng.randint(0, total_images, size=num_samples)

        latencies_us = []

        for iteration in range(iterations):
            print(f"  Iteration {iteration + 1}/{iterations}")

            for idx in tqdm(sample_indices, desc="Random access"):
                # Determine which shard contains this sample
                shard_idx = idx // images_per_shard
                shard_offset = idx % images_per_shard

                # Open specific shard
                shard_path = Path(self.shard_pattern.format(shard_idx))

                start_time = time.perf_counter()

                # Read through shard until we hit the target offset
                with tarfile.open(shard_path, "r") as tar:
                    members = tar.getmembers()
                    if shard_offset < len(members):
                        # Extract specific file
                        target_member = members[shard_offset]
                        f = tar.extractfile(target_member)
                        if f:
                            f.close()

                latency_us = (time.perf_counter() - start_time) * 1e6
                latencies_us.append(latency_us)

        latencies = np.array(latencies_us)

        return {
            "random_access_mean_us": float(np.mean(latencies)),
            "random_access_median_us": float(np.median(latencies)),
            "random_access_p95_us": float(np.percentile(latencies, 95)),
            "random_access_p99_us": float(np.percentile(latencies, 99)),
            "num_samples": num_samples,
            "iterations": iterations,
        }

    def run_all(self) -> Dict:
        """Run all benchmarks."""
        results = {
            "format": "webdataset",
            "version": wds.__version__,
            "test_data": str(self.data_dir),
            "system": self._get_system_info(),
            "metrics": {},
        }

        # 1. Create shards (write benchmark)
        print("\n=== Write Performance ===")
        write_results = self.create_shards()
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
    parser = argparse.ArgumentParser(description="WebDataset benchmark")
    parser.add_argument(
        "--data-dir",
        type=Path,
        default=Path("benchmarks/data/imagenet_val_50k"),
        help="Directory containing raw images",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=Path("benchmarks/data/webdataset_shards"),
        help="Directory for WebDataset shards",
    )
    parser.add_argument(
        "--num-shards",
        type=int,
        default=100,
        help="Number of shards to create",
    )
    parser.add_argument(
        "--results-file",
        type=Path,
        default=Path("benchmarks/results/webdataset_results.json"),
        help="Output file for results",
    )

    args = parser.parse_args()

    # Run benchmarks
    benchmark = WebDatasetBenchmark(
        args.data_dir,
        args.output_dir,
        num_shards=args.num_shards,
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
    print("\nRandom Access (shard-level):")
    print(f"  Mean latency: {results['metrics']['random_access_mean_us']:.0f} µs")
    print(f"  Median latency: {results['metrics']['random_access_median_us']:.0f} µs")
    print(f"  P95 latency: {results['metrics']['random_access_p95_us']:.0f} µs")

    # Save results
    args.results_file.parent.mkdir(parents=True, exist_ok=True)
    with open(args.results_file, "w") as f:
        json.dump(results, f, indent=2)

    print(f"\nResults saved to: {args.results_file}")


if __name__ == "__main__":
    main()
