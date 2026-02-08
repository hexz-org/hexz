"""Strata: High-performance snapshot storage for machine learning.

Strata is a Python library for reading and creating compressed snapshots
with random access support. It's optimized for:
- Machine learning dataset streaming (PyTorch/TensorFlow integration)
- Virtual machine disk and memory snapshots
- Large binary file storage with deduplication

Key Features:
- **Random access**: Read any byte range without decompressing entire file
- **Compression**: LZ4 (fast) or Zstandard (high ratio)
- **Streaming**: Read from local files, HTTP, or S3
- **Zero-copy**: NumPy arrays without data copies
- **Async support**: Asynchronous I/O for high-throughput workloads

Quick Start:
    >>> import strata
    >>> import numpy as np
    >>>
    >>> # Open a snapshot
    >>> reader = strata.open("dataset.st")
    >>>
    >>> # Read bytes
    >>> data = reader.read_at(offset=0, length=4096)
    >>>
    >>> # Read NumPy array
    >>> arr = strata.read_array(reader, offset=0, shape=(100, 784), dtype='float32')
    >>>
    >>> # Create a snapshot
    >>> strata.pack(
    ...     disk="disk.img",
    ...     output="snapshot.st",
    ...     compression="lz4"
    ... )

See documentation for advanced usage: https://github.com/strata-storage/strata
"""

from ._strata_core import (
    StrataReader,
    AsyncStrataReader,
    StrataBuilder,
    pack,
    inspect,
    analyze,
    diff,
    sign_image,
    verify_image,
    snapshot_vm,
)

from .io import open, read_array
from .builder import SnapshotBuilder
from .mount import Mount as mount

__all__ = [
    "StrataReader",
    "AsyncStrataReader",
    "StrataBuilder",
    "SnapshotBuilder",
    "pack",
    "inspect",
    "analyze",
    "diff",
    "sign_image",
    "verify_image",
    "snapshot_vm",
    "open",
    "read_array",
    "mount",
]
