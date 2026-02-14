"""
Base class for all benchmarks.

Provides common utilities and consistent interface.
"""

import json
import time
from abc import ABC, abstractmethod
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any, Dict, List, Optional

import numpy as np
import psutil


@dataclass
class BenchmarkResult:
    """Standard result format for all benchmarks."""

    format_name: str
    test_name: str
    throughput_mb_s: Optional[float] = None
    latency_us: Optional[float] = None
    samples_per_sec: Optional[float] = None
    total_time_s: Optional[float] = None
    cpu_percent: Optional[float] = None
    memory_mb: Optional[float] = None
    storage_mb: Optional[float] = None
    compression_ratio: Optional[float] = None
    pack_time_s: Optional[float] = None
    pack_throughput_mb_s: Optional[float] = None
    metadata: Optional[Dict[str, Any]] = None

    def to_dict(self) -> Dict:
        """Convert to dictionary, excluding None values."""
        return {k: v for k, v in asdict(self).items() if v is not None}


class BenchmarkBase(ABC):
    """Base class for format-specific benchmarks."""

    def __init__(self, data_dir: Path, results_dir: Path):
        self.data_dir = data_dir
        self.results_dir = results_dir
        self.results_dir.mkdir(parents=True, exist_ok=True)
        self.results: List[BenchmarkResult] = []

    @abstractmethod
    def format_name(self) -> str:
        """Return the name of the format being benchmarked."""
        pass

    @abstractmethod
    def prepare_dataset(self, dataset_name: str) -> Any:
        """
        Prepare dataset in the target format.

        Args:
            dataset_name: Name of the raw dataset to convert

        Returns:
            Handle or path to prepared dataset
        """
        pass

    def measure_time(self, func, *args, **kwargs):
        """Measure execution time and resource usage."""
        process = psutil.Process()
        cpu_before = process.cpu_percent()
        mem_before = process.memory_info().rss / 1024 / 1024  # MB

        start = time.perf_counter()
        result = func(*args, **kwargs)
        elapsed = time.perf_counter() - start

        cpu_after = process.cpu_percent()
        mem_after = process.memory_info().rss / 1024 / 1024  # MB

        return result, elapsed, (cpu_after + cpu_before) / 2, mem_after - mem_before

    def benchmark_sequential_read(
        self, dataset_handle: Any, num_samples: int, sample_size: int
    ) -> BenchmarkResult:
        """
        Benchmark sequential read throughput.

        Args:
            dataset_handle: Prepared dataset
            num_samples: Number of samples to read
            sample_size: Average size per sample

        Returns:
            BenchmarkResult with metrics
        """

        def read_all():
            for i in range(num_samples):
                _ = self._read_sample(dataset_handle, i)
            return num_samples

        count, elapsed, cpu, mem = self.measure_time(read_all)
        total_bytes = num_samples * sample_size
        throughput = (total_bytes / 1024 / 1024) / elapsed

        return BenchmarkResult(
            format_name=self.format_name(),
            test_name="sequential_read",
            throughput_mb_s=throughput,
            samples_per_sec=num_samples / elapsed,
            total_time_s=elapsed,
            cpu_percent=cpu,
            memory_mb=mem,
            metadata={
                "num_samples": num_samples,
                "sample_size": sample_size,
                "total_mb": total_bytes / 1024 / 1024,
            },
        )

    def benchmark_random_read(
        self,
        dataset_handle: Any,
        num_samples: int,
        sample_size: int,
        num_reads: int = 1000,
    ) -> BenchmarkResult:
        """
        Benchmark random access performance.

        Args:
            dataset_handle: Prepared dataset
            num_samples: Total samples in dataset
            sample_size: Average size per sample
            num_reads: Number of random reads to perform

        Returns:
            BenchmarkResult with metrics
        """
        # Generate random indices
        rng = np.random.RandomState(42)
        indices = rng.randint(0, num_samples, size=num_reads)

        def read_random():
            for idx in indices:
                _ = self._read_sample(dataset_handle, idx)
            return num_reads

        count, elapsed, cpu, mem = self.measure_time(read_random)
        latency_us = (elapsed / num_reads) * 1_000_000

        return BenchmarkResult(
            format_name=self.format_name(),
            test_name="random_read",
            latency_us=latency_us,
            samples_per_sec=num_reads / elapsed,
            total_time_s=elapsed,
            cpu_percent=cpu,
            memory_mb=mem,
            metadata={
                "num_samples": num_samples,
                "sample_size": sample_size,
                "num_reads": num_reads,
            },
        )

    def benchmark_shuffled_epoch(
        self, dataset_handle: Any, num_samples: int, sample_size: int
    ) -> BenchmarkResult:
        """
        Benchmark full shuffled epoch (training simulation).

        Args:
            dataset_handle: Prepared dataset
            num_samples: Number of samples
            sample_size: Average size per sample

        Returns:
            BenchmarkResult with metrics
        """
        # Generate shuffled indices
        rng = np.random.RandomState(42)
        indices = np.arange(num_samples)
        rng.shuffle(indices)

        def read_shuffled():
            for idx in indices:
                _ = self._read_sample(dataset_handle, idx)
            return num_samples

        count, elapsed, cpu, mem = self.measure_time(read_shuffled)
        total_bytes = num_samples * sample_size
        throughput = (total_bytes / 1024 / 1024) / elapsed

        return BenchmarkResult(
            format_name=self.format_name(),
            test_name="shuffled_epoch",
            throughput_mb_s=throughput,
            samples_per_sec=num_samples / elapsed,
            total_time_s=elapsed,
            cpu_percent=cpu,
            memory_mb=mem,
            metadata={
                "num_samples": num_samples,
                "sample_size": sample_size,
                "total_mb": total_bytes / 1024 / 1024,
            },
        )

    @abstractmethod
    def _read_sample(self, dataset_handle: Any, index: int) -> bytes:
        """
        Read a single sample by index.

        Args:
            dataset_handle: Prepared dataset
            index: Sample index

        Returns:
            Sample data as bytes
        """
        pass

    def benchmark_storage_efficiency(
        self, dataset_name: str, dataset_handle: Any, raw_size_mb: float
    ) -> BenchmarkResult:
        """
        Benchmark storage efficiency and compression.

        Args:
            dataset_name: Name of the dataset
            dataset_handle: Prepared dataset
            raw_size_mb: Original uncompressed size in MB

        Returns:
            BenchmarkResult with storage metrics
        """
        # Get storage size
        storage_mb = self._get_storage_size(dataset_name, dataset_handle)
        compression_ratio = raw_size_mb / storage_mb if storage_mb > 0 else 1.0

        return BenchmarkResult(
            format_name=self.format_name(),
            test_name="storage_efficiency",
            storage_mb=storage_mb,
            compression_ratio=compression_ratio,
            metadata={
                "raw_size_mb": raw_size_mb,
                "compressed_size_mb": storage_mb,
                "space_saved_mb": raw_size_mb - storage_mb,
                "space_saved_percent": ((raw_size_mb - storage_mb) / raw_size_mb * 100)
                if raw_size_mb > 0
                else 0,
            },
        )

    def _get_storage_size(self, dataset_name: str, dataset_handle: Any) -> float:
        """
        Get total storage size in MB for the prepared dataset.
        Override this in subclasses if needed.
        """
        # Default: try to find common file patterns
        patterns = [
            self.data_dir / f"{dataset_name}.hxz",
            self.data_dir / f"{dataset_name}.h5",
            self.data_dir / "webdataset" / dataset_name,
            self.data_dir / "raw" / dataset_name,
        ]

        total_size = 0
        for pattern in patterns:
            if pattern.is_file():
                total_size += pattern.stat().st_size
            elif pattern.is_dir():
                for file_path in pattern.rglob("*"):
                    if file_path.is_file():
                        total_size += file_path.stat().st_size

        return total_size / (1024 * 1024)  # Convert to MB

    def run_all_benchmarks(self, dataset_name: str = "cifar_like"):
        """
        Run all benchmarks for this format.

        Args:
            dataset_name: Name of dataset to use
        """
        print(f"\n{'=' * 60}")
        print(f"Benchmarking: {self.format_name()}")
        print(f"Dataset: {dataset_name}")
        print(f"{'=' * 60}\n")

        # Load dataset metadata
        metadata_path = self.data_dir / "datasets.json"
        with open(metadata_path) as f:
            datasets = json.load(f)

        if dataset_name not in datasets:
            raise ValueError(f"Dataset {dataset_name} not found")

        dataset_info = datasets[dataset_name]
        num_samples = dataset_info["num_samples"]
        sample_size = dataset_info["avg_size"]
        raw_size_mb = dataset_info["total_size"] / (1024 * 1024)

        # Prepare dataset in target format (also measures pack time)
        print(f"📦 Preparing dataset in {self.format_name()} format...")
        start_pack = time.perf_counter()
        dataset_handle = self.prepare_dataset(dataset_name)
        pack_time = time.perf_counter() - start_pack

        # Run benchmarks
        print("\n🏃 Running benchmarks...")

        # 1. Storage efficiency
        print("  • Storage efficiency...")
        result = self.benchmark_storage_efficiency(
            dataset_name, dataset_handle, raw_size_mb
        )
        result.pack_time_s = pack_time
        result.pack_throughput_mb_s = raw_size_mb / pack_time if pack_time > 0 else 0
        self.results.append(result)
        print(
            f"    → {result.storage_mb:.1f} MB ({result.compression_ratio:.2f}x compression)"
        )
        print(
            f"    → Packed in {pack_time:.2f}s ({result.pack_throughput_mb_s:.1f} MB/s)"
        )

        # 2. Sequential read
        print("  • Sequential read...")
        result = self.benchmark_sequential_read(
            dataset_handle, num_samples, sample_size
        )
        self.results.append(result)
        print(f"    → {result.throughput_mb_s:.1f} MB/s")

        # 3. Random read
        print("  • Random access (1000 reads)...")
        result = self.benchmark_random_read(dataset_handle, num_samples, sample_size)
        self.results.append(result)
        print(f"    → {result.latency_us:.1f} µs latency")

        # 4. Shuffled epoch
        print("  • Shuffled epoch...")
        result = self.benchmark_shuffled_epoch(dataset_handle, num_samples, sample_size)
        self.results.append(result)
        print(f"    → {result.throughput_mb_s:.1f} MB/s")

        # Save results
        self.save_results()

        print(f"\n✅ {self.format_name()} benchmarks complete!")

    def save_results(self):
        """Save results to JSON file."""
        output_file = self.results_dir / f"{self.format_name()}_results.json"
        results_dict = [r.to_dict() for r in self.results]

        with open(output_file, "w") as f:
            json.dump(results_dict, f, indent=2)

        print(f"\n💾 Results saved to: {output_file}")
