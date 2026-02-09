"""
Benchmark different Strata build profiles.
"""

import time
import os
import strata
from pathlib import Path


def benchmark_profiles(source_dir: str):
    print(f"Benchmarking compression profiles on {source_dir}...")

    profiles = ["ml", "eda", "archival"]
    results = []

    for profile in profiles:
        output_file = f"bench_{profile}.st"
        print(f"\nTesting profile: {profile}")

        # Measure build time
        start_time = time.time()
        try:
            meta = strata.build(source_dir, output_file, profile=profile)
        except Exception as e:
            print(f"Build failed: {e}")
            continue
        build_time = time.time() - start_time

        # Measure read time (sequential scan)
        start_read = time.time()
        with strata.Reader(output_file) as reader:
            # Read in 1MB chunks
            for _ in reader.iter_chunks(1024 * 1024):
                pass
        read_time = time.time() - start_read

        # Calculate stats
        original_size = meta.disk_size  # Uncompressed size
        compressed_size = meta.size_compressed
        ratio = original_size / compressed_size if compressed_size > 0 else 0
        write_speed = original_size / build_time / (1024 * 1024)  # MB/s
        read_speed = original_size / read_time / (1024 * 1024)  # MB/s

        results.append(
            {
                "profile": profile,
                "build_time": build_time,
                "read_time": read_time,
                "ratio": ratio,
                "write_mb_s": write_speed,
                "read_mb_s": read_speed,
                "size_mb": compressed_size / (1024 * 1024),
            }
        )

        # Cleanup
        os.remove(output_file)

    # Print table
    print("\nResults:")
    print(
        f"{'Profile':<10} | {'Build (s)':<10} | {'Read (s)':<10} | {'Size (MB)':<10} | {'Ratio':<10} | {'Write MB/s':<10} | {'Read MB/s':<10}"
    )
    print("-" * 90)
    for r in results:
        print(
            f"{r['profile']:<10} | {r['build_time']:<10.2f} | {r['read_time']:<10.2f} | {r['size_mb']:<10.2f} | {r['ratio']:<10.2f} | {r['write_mb_s']:<10.1f} | {r['read_mb_s']:<10.1f}"
        )


if __name__ == "__main__":
    # Use dedup_data/base as source
    # First check project root for real data
    project_root = Path(__file__).parent.parent.parent
    source_dir = project_root / "dedup_data/base"

    if not source_dir.exists():
        # Fallback to local dummy data
        print(f"Source directory {source_dir} not found. generating dummy data...")
        source_dir = Path("dummy_data")
        source_dir.mkdir(exist_ok=True)

        # Generate some compressible data
        for i in range(5):
            with open(source_dir / f"text_{i}.txt", "w") as f:
                f.write(
                    ("This is some repeated text to test compression. " * 100 + "\n")
                    * 1000
                )

        # Generate some binary data with patterns
        for i in range(5):
            with open(source_dir / f"binary_{i}.bin", "wb") as f:
                f.write(b"\x00" * 10000 + b"\xff" * 10000)

    benchmark_profiles(str(source_dir))

    # Cleanup dummy data if we created it
    if source_dir.name == "dummy_data":
        import shutil

        shutil.rmtree(source_dir)
