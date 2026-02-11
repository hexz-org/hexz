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
    >>>
    >>> # ML integration
    >>> dataset = strata.Dataset("dataset.st", item_size=1024)
    >>> loader = torch.utils.data.DataLoader(dataset, batch_size=32)

See documentation for advanced usage: https://github.com/strata-storage/strata
"""

from typing import Union

# Import Rust-implemented core functions
# Note: keygen, sign_image, verify_image are Rust-builtins; __doc__ is read-only on
# some Python versions. See docs/source/api/signing.rst for full documentation.
from ._strata_core import keygen, pack, sign_image, snapshot_vm, verify_image
from .array import ArrayView, read_array, write_array

# ML Integration
from .dataset import Dataset, TFDataset

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
from .mount import MountPoint, mount, unmount

# Build helpers
from .profiles import PROFILES, build

# Core I/O
from .reader import AsyncReader, Reader

# Types
from .typing import (
    BuildProfile,
    CompressionAlgorithm,
    DeduplicationMode,
    PackingMode,
    PathLike,
    Shape,
)

# Utilities
from .utils import (
    FORMAT_VERSION,
    MAX_SUPPORTED_VERSION,
    MIN_SUPPORTED_VERSION,
    AnalysisReport,
    Metadata,
    analyze,
    diff,
    info,
    inspect,
    merge_overlay,
    verify,
)
from .writer import Writer


def open(path: PathLike, *, mode: str = "r", **options) -> Union[Reader, Writer]:
    """Open a Strata snapshot for reading or writing.

    Args:
        path: Path to .st file
        mode: 'r' for reading, 'w' for writing
        **options: Additional options for Reader or Writer

    Returns:
        Reader or Writer instance

    Example:
        >>> with strata.open("data.st") as reader:
        ...     data = reader.read(4096)
        ...     chunk = reader.read(100, offset=0)  # random access
        ...
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
    # I/O
    "open",
    "version",
    "Reader",
    "AsyncReader",
    "Writer",
    # Arrays
    "read_array",
    "write_array",
    "ArrayView",
    # ML
    "Dataset",
    "TFDataset",
    # Utilities
    "inspect",
    "analyze",
    "diff",
    "verify",
    "info",
    "mount",
    "unmount",
    "MountPoint",
    # Build
    "build",
    "PROFILES",
    "pack",
    "merge_overlay",
    # VM / signing
    "keygen",
    "sign_image",
    "verify_image",
    "snapshot_vm",
    # Types
    "Metadata",
    "AnalysisReport",
    "PathLike",
    "Shape",
    "PackingMode",
    "BuildProfile",
    "DeduplicationMode",
    "CompressionAlgorithm",
    # Version constants
    "FORMAT_VERSION",
    "MIN_SUPPORTED_VERSION",
    "MAX_SUPPORTED_VERSION",
    # Exceptions
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
