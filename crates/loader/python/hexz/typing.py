"""Type aliases and protocols for Hexz.

This module defines common types used throughout the Hexz API.
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


# Metadata dictionary type
MetadataDict = Dict[str, Any]

__all__ = [
    "PathLike",
    "Shape",
    "CompressionAlgorithm",
    "BuildProfile",
    "PackingMode",
    "DeduplicationMode",
    "ReadableBuffer",
    "WritableBuffer",
    "MetadataDict",
]


def __dir__():
    return __all__
