#!/usr/bin/env python3
"""Hexz format benchmark."""

import json
import sys
from pathlib import Path

try:
    import hexz
except ImportError:
    print("ERROR: hexz module not found. Please run 'make develop' from repo root.")
    sys.exit(1)

from ..base import BenchmarkBase


class HexzBenchmark(BenchmarkBase):
    """Benchmark for Hexz format."""

    def format_name(self) -> str:
        return "hexz"

    def prepare_dataset(self, dataset_name: str):
        """
        Pack raw dataset into Hexz format using Python API.
        """
        raw_dir = self.data_dir / "raw" / dataset_name
        output_file = self.data_dir / f"{dataset_name}.hxz"

        # Load manifest
        manifest_path = raw_dir / "manifest.json"
        with open(manifest_path) as f:
            manifest = json.load(f)

        # Skip if already packed
        if output_file.exists():
            print(f"  ✓ Using existing {output_file.name}")
        else:
            # Pack using Python API — add files directly (no temp file overhead)
            print(f"  • Packing {dataset_name} with hexz Python API...")
            manifest = self._pack_with_python_api(raw_dir, output_file, manifest)

        # Pre-compute cumulative offsets for fast lookup (O(1) instead of O(n))
        offsets = [0]
        for entry in manifest:
            offsets.append(offsets[-1] + entry["size"])

        total_data_size = offsets[-1]

        # Open reader with large cache (fits entire dataset) and prefetch enabled
        reader = hexz.Reader(
            str(output_file),
            cache_size=f"{max(8, total_data_size // (1024 * 1024) + 1)}M",
            prefetch=True,
        )

        # Warm the cache by reading the entire dataset once (matches OS page cache
        # advantage that local_files and HDF5 get from prior writes)
        for i in range(len(manifest)):
            reader.read(manifest[i]["size"], offset=offsets[i])

        return {
            "reader": reader,
            "manifest": manifest,
            "offsets": offsets,
            "file": output_file,
        }

    def _pack_with_python_api(self, _raw_dir: Path, output_file: Path, manifest):
        """Pack dataset using Python API with direct file adds (no temp files)."""
        with hexz.Writer(str(output_file), compression="lz4") as writer:
            for entry in manifest:
                sample_path = self.data_dir / entry["path"]
                writer.add_file(str(sample_path))

        print(f"  ✓ Packed {len(manifest)} samples to {output_file.name}")
        return manifest

    def _get_storage_size(self, dataset_name: str, dataset_handle) -> float:
        """Return compressed .hxz file size in MB."""
        output_file = dataset_handle["file"]
        return output_file.stat().st_size / (1024 * 1024)

    def _read_sample(self, dataset_handle, index: int) -> bytes:
        """
        Read a single sample from hexz archive.

        Uses pre-computed offsets for O(1) lookup instead of O(n).
        """
        reader = dataset_handle["reader"]
        offsets = dataset_handle["offsets"]
        size = dataset_handle["manifest"][index]["size"]

        # O(1) offset lookup using pre-computed cumulative offsets
        return reader.read(size, offset=offsets[index])


def main():
    # Get benchmarks root directory (4 levels up from this file)
    benchmarks_root = Path(__file__).parent.parent.parent.parent
    data_dir = benchmarks_root / "data"
    results_dir = benchmarks_root / "results"

    benchmark = HexzBenchmark(data_dir, results_dir)

    # Check if test data exists
    if not (data_dir / "datasets.json").exists():
        print("ERROR: Test data not found. Run: python -m benchmarks.generate_data")
        sys.exit(1)

    benchmark.run_all_benchmarks("tiny")


if __name__ == "__main__":
    main()
