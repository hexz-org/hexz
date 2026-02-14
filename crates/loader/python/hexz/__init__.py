"""Hexz: High-performance snapshot storage for machine learning.

Hexz is a Python library for reading and creating compressed snapshots
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
    >>> import hexz
    >>>
    >>> # Build a snapshot with smart defaults
    >>> meta = hexz.build("data/", "dataset.hxz", profile="ml")
    >>>
    >>> # Read with modern API
    >>> with hexz.open("dataset.hxz") as reader:
    ...     data = reader[0:4096]  # Slice notation!
    ...     meta = reader.metadata  # Property access!
    ...     print(meta)  # Human-readable info
    >>>
    >>> # ML integration
    >>> dataset = hexz.Dataset("dataset.hxz", item_size=1024)
    >>> loader = torch.utils.data.DataLoader(dataset, batch_size=32)
    >>>
    >>> # Cryptographic signing
    >>> from hexz import crypto
    >>> crypto.keygen("key.priv", "key.pub")
    >>> crypto.sign("dataset.hxz", "key.priv")
    >>> crypto.verify("dataset.hxz", "key.pub")

See documentation for advanced usage: https://github.com/hexz-storage/hexz
"""

from typing import Any, Union

# Arrays (always available)
from .array import ArrayView, read_array, write_array

# Exceptions (always available)
from .exceptions import (
    CacheError,
    CompressionError,
    EncryptionError,
    Error,
    FormatError,
    IOError,
    MountError,
    NetworkError,
    ValidationError,
    VersionError,
)

# Build helpers (always available)
from .profiles import PROFILES, build

# Core I/O (always available)
from .reader import AsyncReader, Reader

# Types (always available)
from .typing import PathLike

# Inspection & Utilities (always available)
from .utils import (
    FORMAT_VERSION,
    MAX_SUPPORTED_VERSION,
    MIN_SUPPORTED_VERSION,
    AnalysisReport,
    Metadata,
    inspect,
    verify,
)
from .writer import Writer

# Optional: ML Integration (requires torch/tensorflow)
try:
    from .dataset import Dataset

    _HAS_DATASET = True
except ImportError:
    _HAS_DATASET = False
    Dataset = None  # type: ignore

try:
    from .dataset import TFDataset

    _HAS_TFDATASET = True
except ImportError:
    _HAS_TFDATASET = False
    TFDataset = None  # type: ignore

# Optional: Cryptographic signing (requires signing feature)
try:
    from . import crypto

    _HAS_CRYPTO = True
except ImportError:
    _HAS_CRYPTO = False
    crypto = None  # type: ignore

# Optional: FUSE mounting (requires FUSE and platform support)
try:
    from .mount import mount

    _HAS_MOUNT = True
except ImportError:
    _HAS_MOUNT = False
    mount = None  # type: ignore


def open(path: PathLike, *, mode: str = "r", **options: Any) -> Union[Reader, Writer]:
    """Open a Hexz snapshot for reading or writing.

    Args:
        path: Path to .hxz file. Supports local paths, HTTP/HTTPS URLs, and S3 URIs.
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
        >>> with hexz.open("data.hxz") as reader:
        ...     data = reader.read(4096)
        ...     chunk = reader.read(100, offset=0)  # random access
        ...
        >>> # Read with custom cache and prefetch disabled
        >>> with hexz.open("data.hxz", cache_size="2G", prefetch=False) as reader:
        ...     data = reader.read(4096)
        ...
        >>> # Write a new snapshot
        >>> with hexz.open("out.hxz", mode="w", packing="tight") as writer:
        ...     writer.add("input.img")
    """
    if "r" in mode:
        return Reader(path, **options)
    elif "w" in mode:
        return Writer(path, **options)
    else:
        raise ValueError(f"Invalid mode: {mode}")


__version__ = "0.1.0"


def version() -> str:
    """Return the version of the Hexz library."""
    return __version__


# Build __all__ dynamically based on available features
__all__ = [
    # === Core I/O (5) ===
    "open",
    "version",
    "Reader",
    "AsyncReader",
    "Writer",
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
    # === Types (3) ===
    "AnalysisReport",
    "Metadata",
    "PathLike",
    # === Version Constants (3) ===
    "FORMAT_VERSION",
    "MIN_SUPPORTED_VERSION",
    "MAX_SUPPORTED_VERSION",
    # === Exceptions (10) ===
    "Error",
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

# Add optional features if available
if _HAS_DATASET:
    __all__.append("Dataset")

if _HAS_TFDATASET:
    __all__.append("TFDataset")

if _HAS_CRYPTO:
    __all__.append("crypto")

if _HAS_MOUNT:
    __all__.append("mount")
