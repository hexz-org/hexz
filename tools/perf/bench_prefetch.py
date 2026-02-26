#!/usr/bin/env python3
"""Benchmark: Prefetch thread::spawn overhead.

Creates a snapshot, then reads it sequentially with prefetching enabled.
On local SSD, std::thread::spawn (~1-2ms overhead) dominates over actual I/O.
Replacing it with rayon::spawn (submits to existing thread pool) should show
measurable improvement for sequential reads with prefetching.

Run BEFORE the fix to get baseline, then AFTER to compare:
    python bench_prefetch.py
"""

import os
import tempfile
import time
import statistics

import hexz


def create_test_snapshot(path: str, size_mb: int = 64) -> int:
    """Create a snapshot with enough data to trigger many prefetches."""
    data_path = path + ".raw"
    # Create compressible data (repeated pattern with some variation)
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


def bench_sequential_read(
    snap_path: str, total_size: int, read_size: int = 65536, prefetch: bool = True
) -> float:
    """Sequential read through entire file, returns throughput in MB/s."""
    with hexz.open(snap_path, prefetch=prefetch) as reader:
        start = time.perf_counter()
        offset = 0
        while offset < total_size:
            reader.read(min(read_size, total_size - offset))
            offset += read_size
        elapsed = time.perf_counter() - start
    return (total_size / (1024 * 1024)) / elapsed


def main():
    print("=" * 60)
    print("Benchmark: Prefetch Thread Spawn Overhead")
    print("=" * 60)

    with tempfile.TemporaryDirectory(prefix="hexz_bench_prefetch_") as tmp:
        snap_path = os.path.join(tmp, "bench.hxz")

        print("\nCreating 64MB test snapshot...")
        total_size = create_test_snapshot(snap_path, size_mb=64)
        print(f"  Snapshot created ({total_size / 1024 / 1024:.0f} MB logical)")

        # Warmup
        print("\nWarming up...")
        bench_sequential_read(snap_path, total_size, prefetch=True)
        bench_sequential_read(snap_path, total_size, prefetch=False)

        n_runs = 10
        print(f"\nRunning {n_runs} iterations each...\n")

        # Benchmark: no prefetch (control)
        no_pf = []
        for _ in range(n_runs):
            no_pf.append(bench_sequential_read(snap_path, total_size, prefetch=False))

        # Benchmark: with prefetch (default: 4 blocks)
        with_pf = []
        for _ in range(n_runs):
            with_pf.append(bench_sequential_read(snap_path, total_size, prefetch=True))

        # Benchmark: smaller reads (more prefetch triggers)
        small_no_pf = []
        for _ in range(n_runs):
            small_no_pf.append(
                bench_sequential_read(
                    snap_path, total_size, read_size=4096, prefetch=False
                )
            )

        small_with_pf = []
        for _ in range(n_runs):
            small_with_pf.append(
                bench_sequential_read(
                    snap_path, total_size, read_size=4096, prefetch=True
                )
            )

        print(
            f"{'Config':<40} {'Median MB/s':>12} {'Stdev':>8} {'Min':>10} {'Max':>10}"
        )
        print("-" * 80)
        for label, results in [
            ("64K reads, no prefetch", no_pf),
            ("64K reads, prefetch=4", with_pf),
            ("4K reads, no prefetch", small_no_pf),
            ("4K reads, prefetch=4", small_with_pf),
        ]:
            med = statistics.median(results)
            sd = statistics.stdev(results) if len(results) > 1 else 0
            print(
                f"{label:<40} {med:>12.1f} {sd:>8.1f} {min(results):>10.1f} {max(results):>10.1f}"
            )

        print()
        med_pf = statistics.median(with_pf)
        med_no = statistics.median(no_pf)
        if med_pf > med_no:
            pct = (med_pf / med_no - 1) * 100
            print(f"64K: Prefetch is {pct:.1f}% faster than no prefetch")
        else:
            pct = (1 - med_pf / med_no) * 100
            print(
                f"64K: Prefetch is {pct:.1f}% SLOWER than no prefetch (thread spawn overhead!)"
            )

        med_pf_s = statistics.median(small_with_pf)
        med_no_s = statistics.median(small_no_pf)
        if med_pf_s > med_no_s:
            pct = (med_pf_s / med_no_s - 1) * 100
            print(f"4K:  Prefetch is {pct:.1f}% faster than no prefetch")
        else:
            pct = (1 - med_pf_s / med_no_s) * 100
            print(
                f"4K:  Prefetch is {pct:.1f}% SLOWER than no prefetch (thread spawn overhead!)"
            )

    print("\nDone.")


if __name__ == "__main__":
    main()
