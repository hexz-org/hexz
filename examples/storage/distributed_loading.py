"""Example: Distributed Data Loading with Multiprocessing.

Standard Python file handles cannot be sent to worker processes. Hexz Readers
are designed to be picklable, allowing a single reader to be shared across
a multiprocessing pool for high-performance parallel data loading.
"""

import multiprocessing as mp
import os
import numpy as np
import hexz
from pathlib import Path

_DATA_DIR = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))), ".data", "storage"
)


def worker_task(reader, start_offset, size):
    """Function run in a separate process."""
    # The reader was pickled, sent here, and transparently re-opened
    data = reader.read(size, offset=start_offset)
    # Perform some 'computation'
    return np.frombuffer(data, dtype=np.uint8).mean()


def run_example():
    os.makedirs(_DATA_DIR, exist_ok=True)

    # 1. Create a 40MB dataset
    path = os.path.join(_DATA_DIR, "multiprocess_data.hxz")
    print(f"Creating test dataset: {path}")
    data = np.random.bytes(40 * 1024 * 1024)
    with hexz.Writer(path) as writer:
        writer.add(data)

    # 2. Open a single reader in the main process
    # We'll share this same object with all workers
    reader = hexz.Reader(path)

    # 3. Use a multiprocessing Pool
    num_workers = 4
    chunk_size = 10 * 1024 * 1024  # 10MB per worker

    print(f"Dispatching {num_workers} workers to read different parts of the file...")

    tasks = [(reader, i * chunk_size, chunk_size) for i in range(num_workers)]

    with mp.Pool(processes=num_workers) as pool:
        results = pool.starmap(worker_task, tasks)

    print(f"Results from workers: {results}")
    print("✓ Successfully shared a single Reader across multiple processes.")

    # Clean up
    reader.close()
    Path(path).unlink()


if __name__ == "__main__":
    # Multiprocessing requires the entry point protection on many platforms
    run_example()
