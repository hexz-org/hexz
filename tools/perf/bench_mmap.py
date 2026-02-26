#!/usr/bin/env python3
"""Benchmark: MmapBackend vs FileBackend for local reads.

Measures sequential and random read throughput through the Python API.
The storage backend is selected internally — this script measures the
end-to-end effect of switching from FileBackend (pread + BytesMut alloc)
to MmapBackend (zero-copy Bytes::slice).

Run BEFORE wiring MmapBackend to get baseline, then AFTER to compare:
    python tools/perf/bench_mmap.py
"""

import os
import tempfile
import time
import statistics
import random

import hexz


def create_test_snapshot(path: str, size_mb: int = 128) -> int:
    """Create a snapshot with enough data for meaningful benchmarking."""
    data_path = path + ".raw"
    chunk = bytes(range(256)) * 256  # 64KB chunk, moderately compressible
    total = size_mb * 1024 * 1024
    with open(data_path, "wb") as f:
        written = 0
        while written < total:
            f.write(chunk)
            written += len(chunk)

    with hexz.open(path, mode="w", compression="lz4") as w:
        w.add(data_path)

    os.unlink(data_path)
    return total


def bench_sequential(snap_path: str, total_size: int, read_size: int = 65536) -> float:
    """Sequential read, returns MB/s."""
    with hexz.open(snap_path, prefetch=False) as reader:
        start = time.perf_counter()
        offset = 0
        while offset < total_size:
            reader.read(min(read_size, total_size - offset))
            offset += read_size
        elapsed = time.perf_counter() - start
    return (total_size / (1024 * 1024)) / elapsed


def bench_random(
    snap_path: str, total_size: int, read_size: int = 4096, n_reads: int = 10000
) -> float:
    """Random read, returns reads/sec."""
    max_offset = total_size - read_size
    offsets = [random.randint(0, max_offset) for _ in range(n_reads)]

    with hexz.open(snap_path, prefetch=False) as reader:
        start = time.perf_counter()
        for off in offsets:
            reader.read(read_size, offset=off)
        elapsed = time.perf_counter() - start
    return n_reads / elapsed


def main():
    print("=" * 65)
    print("Benchmark: Local Backend Read Performance (FileBackend vs Mmap)")
    print("=" * 65)

    with tempfile.TemporaryDirectory(prefix="hexz_bench_mmap_") as tmp:
        snap_path = os.path.join(tmp, "bench.hxz")

        print("\nCreating 128MB test snapshot...")
        total_size = create_test_snapshot(snap_path, size_mb=128)
        print(f"  Snapshot created ({total_size / 1024 / 1024:.0f} MB logical)")

        # Warmup
        print("\nWarming up...")
        bench_sequential(snap_path, total_size)
        bench_random(snap_path, total_size)

        n_runs = 8
        print(f"\nRunning {n_runs} iterations each...\n")

        # Sequential benchmarks
        seq_64k = [
            bench_sequential(snap_path, total_size, 65536) for _ in range(n_runs)
        ]
        seq_4k = [bench_sequential(snap_path, total_size, 4096) for _ in range(n_runs)]
        seq_1m = [
            bench_sequential(snap_path, total_size, 1048576) for _ in range(n_runs)
        ]

        # Random benchmarks
        rand_4k = [bench_random(snap_path, total_size, 4096) for _ in range(n_runs)]
        rand_64k = [bench_random(snap_path, total_size, 65536) for _ in range(n_runs)]

        print(f"{'Config':<40} {'Median':>12} {'Stdev':>10} {'Unit':<10}")
        print("-" * 75)
        for label, results, unit in [
            ("Sequential 4K reads", seq_4k, "MB/s"),
            ("Sequential 64K reads", seq_64k, "MB/s"),
            ("Sequential 1M reads", seq_1m, "MB/s"),
            ("Random 4K reads", rand_4k, "reads/s"),
            ("Random 64K reads", rand_64k, "reads/s"),
        ]:
            med = statistics.median(results)
            sd = statistics.stdev(results) if len(results) > 1 else 0
            print(f"{label:<40} {med:>12.1f} {sd:>10.1f} {unit:<10}")

    print("\nDone.")


if __name__ == "__main__":
    main()
