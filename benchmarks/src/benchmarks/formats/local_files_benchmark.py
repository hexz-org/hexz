#!/usr/bin/env python3
"""Local files benchmark (baseline)."""

import json
import sys
from pathlib import Path

from ..base import BenchmarkBase


class LocalFilesBenchmark(BenchmarkBase):
    """Benchmark for local file system (baseline)."""

    def format_name(self) -> str:
        return "local_files"

    def prepare_dataset(self, dataset_name: str):
        """
        For local files, dataset is already prepared (raw files).

        Returns the directory path and manifest.
        """
        raw_dir = self.data_dir / "raw" / dataset_name
        manifest_path = raw_dir / "manifest.json"

        if not manifest_path.exists():
            raise FileNotFoundError(f"Manifest not found: {manifest_path}")

        with open(manifest_path) as f:
            manifest = json.load(f)

        return {"dir": raw_dir, "manifest": manifest}

    def _get_storage_size(self, dataset_name: str, dataset_handle) -> float:
        """Return total size of raw sample files in MB."""
        manifest = dataset_handle["manifest"]
        total = sum(entry["size"] for entry in manifest)
        return total / (1024 * 1024)

    def _read_sample(self, dataset_handle, index: int) -> bytes:
        """Read a single sample from local filesystem."""
        manifest = dataset_handle["manifest"]
        entry = manifest[index]

        sample_path = self.data_dir / entry["path"]
        with open(sample_path, "rb") as f:
            return f.read()


def main():
    benchmarks_root = Path(__file__).parent.parent.parent.parent
    data_dir = benchmarks_root / "data"
    results_dir = benchmarks_root / "results"

    benchmark = LocalFilesBenchmark(data_dir, results_dir)

    if not (data_dir / "datasets.json").exists():
        print("ERROR: Test data not found. Run: python -m benchmarks.generate_data")
        sys.exit(1)

    benchmark.run_all_benchmarks("tiny")


if __name__ == "__main__":
    main()
