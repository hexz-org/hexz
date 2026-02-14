#!/usr/bin/env python3
"""WebDataset format benchmark."""

import json
import sys
import tarfile
from pathlib import Path

try:
    import webdataset  # noqa: F401
except ImportError:
    print("ERROR: webdataset not installed. Run: pip install webdataset")
    sys.exit(1)

from ..base import BenchmarkBase


class WebDatasetBenchmark(BenchmarkBase):
    """Benchmark for WebDataset format."""

    def format_name(self) -> str:
        return "webdataset"

    def prepare_dataset(self, dataset_name: str):
        """
        Convert raw dataset to WebDataset (tar) format.

        Creates shards for better shuffling (though still shard-limited).
        """
        raw_dir = self.data_dir / "raw" / dataset_name
        output_dir = self.data_dir / "webdataset" / dataset_name
        output_dir.mkdir(parents=True, exist_ok=True)

        manifest_path = raw_dir / "manifest.json"
        with open(manifest_path) as f:
            manifest = json.load(f)

        num_samples = len(manifest)
        samples_per_shard = 1000  # WebDataset convention
        num_shards = (num_samples + samples_per_shard - 1) // samples_per_shard

        # Check if already prepared
        shard_pattern = str(output_dir / "shard-%04d.tar")
        if (output_dir / "shard-0000.tar").exists():
            print("  ✓ Using existing WebDataset shards")
            return {
                "pattern": shard_pattern,
                "num_shards": num_shards,
                "manifest": manifest,
            }

        print(f"  • Creating {num_shards} shards...")

        # Create shards
        for shard_idx in range(num_shards):
            start_idx = shard_idx * samples_per_shard
            end_idx = min(start_idx + samples_per_shard, num_samples)

            shard_path = output_dir / f"shard-{shard_idx:04d}.tar"

            with tarfile.open(shard_path, "w") as tar:
                for i in range(start_idx, end_idx):
                    entry = manifest[i]
                    sample_path = self.data_dir / entry["path"]

                    # WebDataset uses .bin extension for binary data
                    arcname = f"{i:06d}.bin"

                    tar.add(sample_path, arcname=arcname)

        print(f"  ✓ Created {num_shards} shards")

        return {
            "pattern": shard_pattern,
            "num_shards": num_shards,
            "manifest": manifest,
        }

    def _get_storage_size(self, dataset_name: str, dataset_handle) -> float:
        """Return total size of WebDataset shards in MB."""
        shard_dir = self.data_dir / "webdataset" / dataset_name
        if shard_dir.is_dir():
            total = sum(f.stat().st_size for f in shard_dir.glob("*.tar"))
            return total / (1024 * 1024)
        return 0.0

    def _read_sample(self, dataset_handle, index: int) -> bytes:
        """
        Read a single sample by index.

        Note: WebDataset is optimized for sequential access, not random access.
        This implementation is intentionally inefficient to demonstrate the limitation.
        """
        samples_per_shard = 1000

        # Determine which shard contains this sample
        shard_idx = index // samples_per_shard

        shard_path = dataset_handle["pattern"] % shard_idx

        # Open shard and find sample
        # This is inefficient by design - WebDataset isn't meant for random access
        with tarfile.open(shard_path, "r") as tar:
            target_name = f"{index:06d}.bin"
            try:
                member = tar.getmember(target_name)
                f = tar.extractfile(member)
                if f:
                    return f.read()
            except KeyError:
                # Sample not found
                pass

        return b"\x00" * 4096  # Return dummy data if not found


def main():
    benchmarks_root = Path(__file__).parent.parent.parent.parent
    data_dir = benchmarks_root / "data"
    results_dir = benchmarks_root / "results"

    benchmark = WebDatasetBenchmark(data_dir, results_dir)

    if not (data_dir / "datasets.json").exists():
        print("ERROR: Test data not found. Run: python -m benchmarks.generate_data")
        sys.exit(1)

    benchmark.run_all_benchmarks("tiny")


if __name__ == "__main__":
    main()
