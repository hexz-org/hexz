"""
Template for competitor benchmarks.

Copy this file to <format>_benchmark.py and implement the benchmark
functions for your target format.
"""

import json
import time
import platform
from pathlib import Path
from typing import Dict, Any


class BenchmarkRunner:
    """Base class for format-specific benchmarks."""

    def __init__(self, format_name: str, test_data_path: Path):
        self.format_name = format_name
        self.test_data_path = test_data_path
        self.results = {
            "format": format_name,
            "test_data": test_data_path.name,
            "system": self._get_system_info(),
            "metrics": {},
        }

    def _get_system_info(self) -> Dict[str, str]:
        """Collect system information for reproducibility."""
        return {
            "cpu": platform.processor() or "Unknown",
            "platform": platform.platform(),
            "python_version": platform.python_version(),
        }

    def benchmark_write(self) -> float:
        """
        Benchmark write/pack performance.

        Returns:
            Throughput in GB/s
        """
        raise NotImplementedError("Implement write benchmark")

    def benchmark_sequential_read(self) -> float:
        """
        Benchmark sequential read throughput.

        Returns:
            Throughput in GB/s
        """
        raise NotImplementedError("Implement sequential read benchmark")

    def benchmark_random_access(self, num_samples: int = 1000) -> Dict[str, float]:
        """
        Benchmark random access latency.

        Args:
            num_samples: Number of random samples to access

        Returns:
            Dict with 'cold_us' and 'warm_us' latencies in microseconds
        """
        raise NotImplementedError("Implement random access benchmark")

    def measure_storage_efficiency(self) -> Dict[str, float]:
        """
        Measure storage efficiency metrics.

        Returns:
            Dict with 'compressed_size_gb' and 'compression_ratio'
        """
        raise NotImplementedError("Implement storage measurement")

    def run_all(self) -> Dict[str, Any]:
        """Run all benchmarks and return results."""
        print(f"Running {self.format_name} benchmarks...")

        # Write performance
        print("  - Write throughput...")
        self.results["metrics"]["write_throughput_gbps"] = self.benchmark_write()

        # Sequential read
        print("  - Sequential read...")
        self.results["metrics"]["sequential_read_gbps"] = (
            self.benchmark_sequential_read()
        )

        # Random access
        print("  - Random access...")
        random_results = self.benchmark_random_access()
        self.results["metrics"]["random_access_cold_us"] = random_results["cold_us"]
        self.results["metrics"]["random_access_warm_us"] = random_results["warm_us"]

        # Storage
        print("  - Storage efficiency...")
        storage_results = self.measure_storage_efficiency()
        self.results["metrics"]["compressed_size_gb"] = storage_results[
            "compressed_size_gb"
        ]
        self.results["metrics"]["compression_ratio"] = storage_results[
            "compression_ratio"
        ]

        # Add timestamp
        self.results["date"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())

        return self.results


def main():
    """Run benchmarks and output results."""
    # Configure
    test_data = Path("benchmarks/data/imagenet_val_50k")

    # Create benchmark runner (replace with actual implementation)
    runner = BenchmarkRunner("template", test_data)

    # Run benchmarks
    results = runner.run_all()

    # Output results
    print("\nResults:")
    print(json.dumps(results, indent=2))

    # Save to file
    output_file = Path(f"benchmarks/results/{runner.format_name}_results.json")
    output_file.parent.mkdir(parents=True, exist_ok=True)
    with open(output_file, "w") as f:
        json.dump(results, f, indent=2)

    print(f"\nResults saved to {output_file}")


if __name__ == "__main__":
    main()
