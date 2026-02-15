#!/usr/bin/env python3
"""CLI script to run all benchmarks."""

import argparse
import sys
from pathlib import Path

# Add src to path
sys.path.insert(0, str(Path(__file__).parent / "src"))

from benchmarks.formats import (
    HexzBenchmark,
    HDF5Benchmark,
    LocalFilesBenchmark,
    WebDatasetBenchmark,
)


def main():
    parser = argparse.ArgumentParser(description="Run format benchmarks")
    parser.add_argument(
        "--dataset",
        nargs="+",
        default=["tiny"],
        choices=["tiny", "cifar10", "stl10", "cifar100", "all"],
        help="Dataset(s) to benchmark (use 'all' for all datasets)",
    )
    parser.add_argument(
        "--formats",
        nargs="+",
        choices=["hexz", "hdf5", "local_files", "webdataset"],
        help="Specific formats to benchmark (default: all)",
    )
    parser.add_argument(
        "--skip",
        nargs="+",
        choices=["hexz", "hdf5", "local_files", "webdataset"],
        help="Formats to skip",
    )

    args = parser.parse_args()

    benchmarks_root = Path(__file__).parent
    data_dir = benchmarks_root / "data"
    results_dir = benchmarks_root / "results"

    if not (data_dir / "datasets.json").exists():
        print("❌ Test data not found!")
        print("\nPlease run: python generate_data.py")
        sys.exit(1)

    results_dir.mkdir(exist_ok=True)

    # Build benchmark list
    all_benchmarks = {
        "local_files": LocalFilesBenchmark,
        "hdf5": HDF5Benchmark,
        "webdataset": WebDatasetBenchmark,
        "hexz": HexzBenchmark,
    }

    # Filter benchmarks
    if args.formats:
        benchmarks = {k: v for k, v in all_benchmarks.items() if k in args.formats}
    else:
        benchmarks = all_benchmarks

    if args.skip:
        benchmarks = {k: v for k, v in benchmarks.items() if k not in args.skip}

    # Resolve dataset list
    datasets = args.dataset
    if "all" in datasets:
        datasets = ["tiny", "cifar10", "stl10", "cifar100"]

    print(
        f"\n🏃 Running {len(benchmarks)} benchmarks on {len(datasets)} dataset(s): {datasets}"
    )
    print(f"Results will be saved to: {results_dir}\n")

    # Run each benchmark on each dataset
    for dataset_name in datasets:
        for name, benchmark_class in benchmarks.items():
            try:
                print(f"{'=' * 60}")
                print(f"Running: {name} on {dataset_name}")
                print(f"{'=' * 60}\n")

                benchmark = benchmark_class(data_dir, results_dir)
                benchmark.run_all_benchmarks(dataset_name)

            except Exception as e:
                print(f"\n❌ Error running {name} on {dataset_name}: {e}")
                import traceback

                traceback.print_exc()

    print(f"\n{'=' * 60}")
    print("✅ All benchmarks complete!")
    print(f"{'=' * 60}")
    print(f"\nResults saved to: {results_dir}")
    print("\nTo analyze results:")
    print("  python analyze_results.py")


if __name__ == "__main__":
    main()
