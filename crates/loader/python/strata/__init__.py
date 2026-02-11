"""Strata: High-performance snapshot storage for machine learning.

Strata is a Python library for reading and creating compressed snapshots
with random access support. It's optimized for:

- Machine learning dataset streaming (PyTorch/TensorFlow integration)
- Virtual machine disk and memory snapshots
- Large binary file storage with deduplication

Key Features:

- Random access: Read any byte range without decompressing entire file
- Compression: LZ4 (fast) or Zstandard (high ratio)
- Streaming: Read from local files, HTTP, or S3
- Zero-copy: NumPy arrays without data copies
- Async support: Asynchronous I/O for high-throughput workloads

Quick Start:
    >>> import strata
    >>>
    >>> # Build a snapshot with smart defaults
    >>> meta = strata.build("data/", "dataset.st", profile="ml")
    >>>
    >>> # Read with modern API
    >>> with strata.open("dataset.st") as reader:
    ...     data = reader[0:4096]  # Slice notation!
    ...     meta = reader.metadata  # Property access!
    ...     print(meta)  # Human-readable info
    >>>
    >>> # ML integration
    >>> dataset = strata.Dataset("dataset.st", item_size=1024)
    >>> loader = torch.utils.data.DataLoader(dataset, batch_size=32)
    >>>
    >>> # Cryptographic signing
    >>> from strata import crypto
    >>> crypto.keygen("key.priv", "key.pub")
    >>> crypto.sign("dataset.st", "key.priv")
    >>> crypto.verify("dataset.st", "key.pub")

See documentation for advanced usage: https://github.com/strata-storage/strata
"""

from typing import Union, Any

# Import submodules for qualified access
from . import crypto  # strata.crypto.keygen(), etc.

# Core I/O
from .reader import AsyncReader, Reader
from .writer import Writer

# ML Integration
from .dataset import Dataset, TFDataset

# Arrays
from .array import ArrayView, read_array, write_array

# Build helpers
from .profiles import PROFILES, build

# Inspection & Utilities
from .utils import (
    FORMAT_VERSION,
    MAX_SUPPORTED_VERSION,
    MIN_SUPPORTED_VERSION,
    AnalysisReport,
    Metadata,
    inspect,
    verify,
)

# Mount
from .mount import mount

# Types (commonly used only)
from .typing import PathLike

# Exceptions
from .exceptions import (
    CacheError,
    CompressionError,
    EncryptionError,
    FormatError,
    IOError,
    MountError,
    NetworkError,
    StrataError,
    ValidationError,
    VersionError,
)


def open(path: PathLike, *, mode: str = "r", **options: Any) -> Union[Reader, Writer]:
    """Open a Strata snapshot for reading or writing.

    Args:
        path: Path to .st file. Supports local paths, HTTP/HTTPS URLs, and S3 URIs.
        mode: 'r' for reading, 'w' for writing
        **options: Additional options for Reader or Writer

    Keyword Arguments (Read Mode):
        cache_size (str): Block cache size (e.g., "512M", "1G", "2GB"). Default: ~4MB
        prefetch (bool): Enable background prefetching for sequential reads. Default: True
        s3_region (str): AWS region for S3 URLs
        endpoint_url (str): Custom S3 endpoint URL (for MinIO, Ceph, etc.)
        allow_restricted (bool): Allow connections to private/internal IPs. Default: False

    Keyword Arguments (Write Mode):
        compression (str): Compression algorithm ('lz4' or 'zstd')
        block_size (int): Block size in bytes
        packing (str): Packing strategy ('fast', 'tight', etc.)

    Returns:
        Reader or Writer instance

    Example:
        >>> # Read with default settings (cache_size=default, prefetch=True)
        >>> with strata.open("data.st") as reader:
        ...     data = reader.read(4096)
        ...     chunk = reader.read(100, offset=0)  # random access
        ...
        >>> # Read with custom cache and prefetch disabled
        >>> with strata.open("data.st", cache_size="2G", prefetch=False) as reader:
        ...     data = reader.read(4096)
        ...
        >>> # Write a new snapshot
        >>> with strata.open("out.st", mode="w", packing="tight") as writer:
        ...     writer.add("input.img")
    """
    if "r" in mode:
        return Reader(path, **options)
    elif "w" in mode:
        return Writer(path, **options)
    else:
        raise ValueError(f"Invalid mode: {mode}")


__version__ = "0.1.0-alpha"


def version() -> str:
    """Return the version of the Strata library."""
    return __version__


__all__ = [
    # === Core I/O (5) ===
    "open",
    "version",
    "Reader",
    "AsyncReader",
    "Writer",
    # === ML Integration (2) ===
    "Dataset",
    "TFDataset",
    # === Arrays (3) ===
    "read_array",
    "write_array",
    "ArrayView",
    # === Build (2) ===
    "build",
    "PROFILES",
    # === Inspection (1) ===
    "inspect",
    # === Utilities (1) ===
    "verify",
    # === Mount (1) ===
    "mount",
    # === Submodules (1) ===
    "crypto",  # strata.crypto.keygen/sign/verify
    # === Types (3) ===
    "AnalysisReport",
    "Metadata",
    "PathLike",
    # === Version Constants (3) ===
    "FORMAT_VERSION",
    "MIN_SUPPORTED_VERSION",
    "MAX_SUPPORTED_VERSION",
    # === Exceptions (10) ===
    "StrataError",
    "IOError",
    "NetworkError",
    "FormatError",
    "ValidationError",
    "CompressionError",
    "EncryptionError",
    "MountError",
    "CacheError",
    "VersionError",
]
