"""Hexz: High-performance snapshot storage for machine learning.

Hexz is a Python library for reading and creating compressed snapshots
with random access support. It's optimized for:

- ML model checkpointing and weight deduplication
- Large binary file storage with random access
- Virtual machine disk and memory snapshots

Key Features:

- Random access: Read any byte range (e.g. specific tensors) without decompressing entire file
- Deduplication: CDC-based dedup across versions (perfect for model fine-tuning)
- Streaming: Read from local files, HTTP, or S3
- Zero-copy: Direct loading into NumPy/PyTorch via buffer protocol
- Async support: Asynchronous I/O for high-throughput workloads

Quick Start:
    >>> import hexz
    >>>
    >>> # Read a specific tensor from a checkpoint
    >>> with hexz.open("model.hxz") as reader:
    ...     # Read 1GB weight at offset 4096 directly into a buffer
    ...     weights = reader.read(1024*1024*1024, offset=4096)
    ...     meta = reader.metadata
    ...     print(f"Primary size: {meta.primary_size}")
    >>>
    >>> # Build a new snapshot (checkpoint)
    >>> with hexz.Writer("new_ckpt.hxz", compression="zstd") as writer:
    ...     writer.add("weights.bin")
    ...     writer.add_metadata({"framework": "pytorch", "epoch": 10})

See documentation for advanced usage: https://github.com/hexz-storage/hexz
"""

from typing import Any, Union

# Checkpoint (always available — torch is lazy-imported inside function bodies)
from . import checkpoint

# Arrays (always available)
from .array import ArrayView, read_array, write_array

# Conversion utilities (always available)
from .convert import convert

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


__version__ = "0.7.0"


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
    # === Checkpoint (1) ===
    "checkpoint",
    # === Conversion (1) ===
    "convert",
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
if _HAS_CRYPTO:
    __all__.append("crypto")

if _HAS_MOUNT:
    __all__.append("mount")
