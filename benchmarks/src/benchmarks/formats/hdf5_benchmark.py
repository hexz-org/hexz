#!/usr/bin/env python3
"""HDF5 format benchmark."""

import json
import sys
from pathlib import Path

import numpy as np

try:
    import h5py
except ImportError:
    print("ERROR: h5py not installed. Run: pip install h5py")
    sys.exit(1)

from ..base import BenchmarkBase


class HDF5Benchmark(BenchmarkBase):
    """Benchmark for HDF5 format."""

    def format_name(self) -> str:
        return "hdf5"

    def prepare_dataset(self, dataset_name: str):
        """
        Convert raw dataset to HDF5 format with compression.
        """
        raw_dir = self.data_dir / "raw" / dataset_name
        output_file = self.data_dir / f"{dataset_name}.h5"

        # Check if already prepared
        if output_file.exists():
            print(f"  ✓ Using existing {output_file.name}")
            return h5py.File(str(output_file), "r")

        manifest_path = raw_dir / "manifest.json"
        with open(manifest_path) as f:
            manifest = json.load(f)

        num_samples = len(manifest)
        print(f"  • Creating HDF5 file with {num_samples} samples...")

        # Determine max sample size
        max_size = max(entry["size"] for entry in manifest)

        # Create HDF5 file with datasets
        with h5py.File(str(output_file), "w") as hf:
            # Create dataset with compression
            # Use variable-length if sizes vary significantly
            avg_size = sum(entry["size"] for entry in manifest) / num_samples
            size_variance = max(entry["size"] for entry in manifest) / avg_size

            if size_variance > 1.5:
                # Variable-length dataset (use uint8 arrays, not vlen strings,
                # to handle binary data with embedded nulls)
                dt = h5py.vlen_dtype(np.uint8)
                data_ds = hf.create_dataset(
                    "data",
                    (num_samples,),
                    dtype=dt,
                    compression="lzf",  # Fast compression similar to LZ4
                )
            else:
                # Fixed-length dataset
                data_ds = hf.create_dataset(
                    "data",
                    (num_samples, max_size),
                    dtype="uint8",
                    compression="lzf",
                    chunks=(1, max_size),  # Chunk by sample
                )

            # Labels dataset
            labels_ds = hf.create_dataset("labels", (num_samples,), dtype="int32")

            # Write samples
            for i, entry in enumerate(manifest):
                sample_path = self.data_dir / entry["path"]
                with open(sample_path, "rb") as f:
                    data = f.read()

                if size_variance > 1.5:
                    data_ds[i] = np.frombuffer(data, dtype=np.uint8)
                else:
                    # Pad to max_size
                    padded = data + b"\x00" * (max_size - len(data))
                    data_ds[i] = list(padded)

                labels_ds[i] = entry["label"]

        print(f"  ✓ Created {output_file.name}")

        return h5py.File(str(output_file), "r")

    def _get_storage_size(self, dataset_name: str, dataset_handle) -> float:
        """Return .h5 file size in MB."""
        h5_file = self.data_dir / f"{dataset_name}.h5"
        if h5_file.exists():
            return h5_file.stat().st_size / (1024 * 1024)
        return 0.0

    def _read_sample(self, dataset_handle, index: int) -> bytes:
        """Read a single sample from HDF5 file."""
        data = dataset_handle["data"][index]
        return bytes(data)


def main():
    benchmarks_root = Path(__file__).parent.parent.parent.parent
    data_dir = benchmarks_root / "data"
    results_dir = benchmarks_root / "results"

    benchmark = HDF5Benchmark(data_dir, results_dir)

    if not (data_dir / "datasets.json").exists():
        print("ERROR: Test data not found. Run: python -m benchmarks.generate_data")
        sys.exit(1)

    benchmark.run_all_benchmarks("tiny")


if __name__ == "__main__":
    main()
