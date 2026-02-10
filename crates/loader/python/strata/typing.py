"""Type aliases and protocols for Strata.

This module defines common types used throughout the Strata API.
"""

from typing import Union, Tuple, Protocol, Any, Dict, Literal
from pathlib import Path
import os

# Path-like types
PathLike = Union[str, os.PathLike, Path]

# Array shape type
Shape = Tuple[int, ...]

# Compression types
CompressionAlgorithm = Literal["lz4", "zstd", "none"]

# Build profiles - preset configurations for specific use cases
BuildProfile = Literal["ml", "eda", "embedded", "generic", "archival"]

# Packing modes - control compression speed vs ratio
PackingMode = Literal["fast", "balanced", "tight"]

# Deduplication algorithms
DeduplicationMode = Literal[
    "dcam",  # DCAM sampling - fast approximate dedup
    "full",  # Full sweep - accurate but slower
    "none",  # No deduplication
]


class ReadableBuffer(Protocol):
    """Protocol for objects that support the buffer protocol for reading."""

    def __buffer__(self, flags: int) -> memoryview: ...


class WritableBuffer(Protocol):
    """Protocol for objects that support the buffer protocol for writing."""

    def __buffer__(self, flags: int) -> memoryview: ...


class ProgressCallback(Protocol):
    """Protocol for progress callback functions."""

    def __call__(self, current: int, total: int) -> None:
        """Called with (current_bytes, total_bytes) during operations.

        Args:
            current: Number of bytes processed so far
            total: Total number of bytes to process
        """
        ...


# Metadata dictionary type
MetadataDict = Dict[str, Any]
