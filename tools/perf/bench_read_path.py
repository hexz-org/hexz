#!/usr/bin/env python3
"""Benchmark the hexz read path to measure decompression buffer allocation overhead.

Creates a synthetic dataset, packs it, then benchmarks:
  1. Sequential read  — stream the entire file in fixed-size chunks
  2. Random read      — read random offsets (simulates ML shuffled access)
  3. Multi-threaded   — concurrent sequential reads from multiple threads

Configure via environment variables:
    HEXZ_BENCH_SIZE_MB     Total data size in MB   (default: 256)
    HEXZ_BENCH_CHUNK_KB    Read chunk size in KB    (default: 64)
    HEXZ_BENCH_ITERS       Iterations per benchmark (default: 3)
    HEXZ_BENCH_THREADS     Thread count for MT test (default: 4)
    HEXZ_BENCH_COMPRESSION Compression algorithm    (default: lz4)
"""

import os
import time
import random
import tempfile
import threading
import statistics

import numpy as np

# ── Configuration ─────────────────────────────────────────────────────────────
SIZE_MB = int(os.environ.get("HEXZ_BENCH_SIZE_MB", 256))
CHUNK_KB = int(os.environ.get("HEXZ_BENCH_CHUNK_KB", 64))
ITERS = int(os.environ.get("HEXZ_BENCH_ITERS", 3))
THREADS = int(os.environ.get("HEXZ_BENCH_THREADS", 4))
COMPRESSION = os.environ.get("HEXZ_BENCH_COMPRESSION", "lz4")

DATA_SIZE = SIZE_MB * 1024 * 1024
CHUNK_SIZE = CHUNK_KB * 1024


def generate_data(workdir: str) -> str:
    """Generate synthetic data and pack into a hexz snapshot."""
    import hexz

    raw_path = os.path.join(workdir, "raw.bin")
    snap_path = os.path.join(workdir, "bench.hxz")

    print(f"  Generating {SIZE_MB} MB of synthetic data...")
    rng = np.random.default_rng(42)
    # Semi-compressible data: repeated patterns with noise
    pattern = rng.integers(0, 256, size=(1024,), dtype=np.uint8)
    data = np.tile(pattern, DATA_SIZE // 1024)
    # Add 10% noise for realistic compression ratios
    noise_mask = rng.random(len(data)) < 0.1
    data[noise_mask] = rng.integers(0, 256, size=noise_mask.sum(), dtype=np.uint8)

    with open(raw_path, "wb") as f:
        f.write(data.tobytes())

    print(f"  Packing with {COMPRESSION}...")
    t0 = time.monotonic()
    with hexz.Writer(snap_path, compression=COMPRESSION) as w:
        w.add(raw_path)
    dt = time.monotonic() - t0
    snap_size = os.path.getsize(snap_path)
    ratio = DATA_SIZE / snap_size
    print(
        f"  Packed in {dt:.2f}s  ({snap_size / 1024 / 1024:.1f} MB, {ratio:.1f}x ratio)"
    )
    return snap_path


def bench_sequential(snap_path: str) -> list[float]:
    """Benchmark sequential reads through the entire file."""
    import hexz

    throughputs = []
    for i in range(ITERS):
        with hexz.Reader(snap_path, prefetch=True) as reader:
            total = reader.size
            buf = bytearray(CHUNK_SIZE)
            read_bytes = 0
            t0 = time.monotonic()
            offset = 0
            while offset < total:
                to_read = min(CHUNK_SIZE, total - offset)
                n = reader.read(buffer=memoryview(buf)[:to_read], offset=offset)
                read_bytes += n
                offset += n
                if n == 0:
                    break
            dt = time.monotonic() - t0
        tp = read_bytes / dt / 1024 / 1024
        throughputs.append(tp)
        print(
            f"    iter {i + 1}: {tp:.0f} MB/s ({read_bytes / 1024 / 1024:.0f} MB in {dt:.3f}s)"
        )
    return throughputs


def bench_random(snap_path: str) -> list[float]:
    """Benchmark random-access reads (no cache benefit)."""
    import hexz

    num_reads = DATA_SIZE // CHUNK_SIZE
    rng = random.Random(42)

    throughputs = []
    for i in range(ITERS):
        with hexz.Reader(snap_path, prefetch=False, cache_size="16M") as reader:
            total = reader.size
            offsets = [
                rng.randint(0, max(0, total - CHUNK_SIZE)) for _ in range(num_reads)
            ]
            buf = bytearray(CHUNK_SIZE)
            read_bytes = 0
            t0 = time.monotonic()
            for off in offsets:
                n = reader.read(buffer=buf, offset=off)
                read_bytes += n
            dt = time.monotonic() - t0
        tp = read_bytes / dt / 1024 / 1024
        throughputs.append(tp)
        print(f"    iter {i + 1}: {tp:.0f} MB/s ({num_reads} reads in {dt:.3f}s)")
    return throughputs


def bench_multithread(snap_path: str) -> list[float]:
    """Benchmark concurrent sequential reads from multiple threads."""
    import hexz

    chunk_per_thread = DATA_SIZE // THREADS
    throughputs = []

    for i in range(ITERS):
        barrier = threading.Barrier(THREADS + 1)
        results = [0.0] * THREADS

        def worker(tid: int):
            start_off = tid * chunk_per_thread
            end_off = start_off + chunk_per_thread
            buf = bytearray(CHUNK_SIZE)
            with hexz.Reader(snap_path, prefetch=True) as reader:
                barrier.wait()  # sync start
                offset = start_off
                read_bytes = 0
                t0 = time.monotonic()
                while offset < end_off:
                    to_read = min(CHUNK_SIZE, end_off - offset)
                    n = reader.read(buffer=memoryview(buf)[:to_read], offset=offset)
                    read_bytes += n
                    offset += n
                    if n == 0:
                        break
                dt = time.monotonic() - t0
            results[tid] = read_bytes / dt / 1024 / 1024

        threads = []
        for tid in range(THREADS):
            t = threading.Thread(target=worker, args=(tid,))
            t.start()
            threads.append(t)

        barrier.wait()  # let all threads go
        t0 = time.monotonic()
        for t in threads:
            t.join()
        wall_time = time.monotonic() - t0

        total_tp = sum(results)
        throughputs.append(total_tp)
        print(
            f"    iter {i + 1}: {total_tp:.0f} MB/s aggregate "
            f"({', '.join(f'{r:.0f}' for r in results)} per-thread) "
            f"wall={wall_time:.3f}s"
        )
    return throughputs


def bench_large_read(snap_path: str) -> list[float]:
    """Benchmark large reads (1MB) that span many blocks and trigger parallel decompression."""
    import hexz

    large_chunk = 1024 * 1024  # 1MB = 16 blocks @ 64KB
    throughputs = []
    for i in range(ITERS):
        with hexz.Reader(snap_path, prefetch=False) as reader:
            total = reader.size
            buf = bytearray(large_chunk)
            read_bytes = 0
            t0 = time.monotonic()
            offset = 0
            while offset < total:
                to_read = min(large_chunk, total - offset)
                n = reader.read(buffer=memoryview(buf)[:to_read], offset=offset)
                read_bytes += n
                offset += n
                if n == 0:
                    break
            dt = time.monotonic() - t0
        tp = read_bytes / dt / 1024 / 1024
        throughputs.append(tp)
        print(
            f"    iter {i + 1}: {tp:.0f} MB/s ({read_bytes / 1024 / 1024:.0f} MB in {dt:.3f}s)"
        )
    return throughputs


def report(name: str, values: list[float]):
    """Print summary statistics."""
    med = statistics.median(values)
    mn = min(values)
    mx = max(values)
    print(f"  {name:20s}  median={med:7.0f} MB/s  min={mn:7.0f}  max={mx:7.0f}")


def main():
    print("hexz read-path benchmark")
    print(
        f"  data={SIZE_MB}MB  chunk={CHUNK_KB}KB  iters={ITERS}  "
        f"threads={THREADS}  compression={COMPRESSION}"
    )
    print()

    with tempfile.TemporaryDirectory(prefix="hexz_bench_") as workdir:
        snap_path = generate_data(workdir)
        print()

        print("Sequential read (prefetch=True):")
        seq = bench_sequential(snap_path)
        print()

        print(f"Random read ({DATA_SIZE // CHUNK_SIZE} reads, prefetch=False):")
        rand_ = bench_random(snap_path)
        print()

        print("Large read (1MB chunks, parallel decompress):")
        large = bench_large_read(snap_path)
        print()

        print(f"Multi-threaded sequential ({THREADS} threads):")
        mt = bench_multithread(snap_path)
        print()

        print("─" * 60)
        print("Summary:")
        report("Sequential", seq)
        report("Random", rand_)
        report("Large read (1MB)", large)
        report(f"MT ({THREADS} threads)", mt)


if __name__ == "__main__":
    main()
