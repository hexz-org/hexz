# Clean Python API Blueprint

## Philosophy

This is the **clean, modern Python API** for Strata - no legacy cruft, no backward compatibility shims. Every class, function, and parameter is intentional and follows Python best practices.

## Core Principles

1. **Pythonic First:** Properties not methods, context managers, slice notation, iterators
2. **Zero Ceremony:** Minimal boilerplate, sensible defaults
3. **Type-Safe:** Full type hints, Literal types for enums
4. **Performance:** Zero-copy where possible, smart caching, async support
5. **Semantic:** User-facing names describe intent, not implementation

---

## Complete Module Structure

```
strata/
├── __init__.py          # Public API exports
├── _core.pyi            # Type stubs for Rust bindings
├── typing.py            # Type aliases and protocols
├── exceptions.py        # Exception hierarchy
├── reader.py            # Reader classes (sync/async)
├── writer.py            # Writer class
├── array.py             # NumPy integration
├── dataset.py           # PyTorch/TensorFlow datasets
├── mount.py             # Filesystem mounting
├── utils.py             # Utility functions
└── profiles.py          # Build profiles and presets
```

**Removed Files:**
- ~~`builder.py`~~ - Legacy SnapshotBuilder wrapper (delete)
- ~~`io.py`~~ - Legacy open() function (delete)
- ~~`torch.py`~~ - Old PyTorch integration (replace with dataset.py)

---

## 1. Public API (`__init__.py`)

```python
"""Strata - High-performance snapshot storage for ML and systems.

Modern Pythonic API for creating, reading, and managing Strata snapshots.
"""

# Core I/O
from .reader import Reader, AsyncReader
from .writer import Writer
from .array import read_array, write_array, ArrayView

# ML Integration
from .dataset import Dataset, TFDataset

# Utilities
from .utils import inspect, analyze, diff, verify, info
from .mount import mount, unmount, MountPoint

# Build helpers
from .profiles import build, PROFILES

# Types
from .typing import (
    PathLike,
    Shape,
    PackingMode,
    BuildProfile,
    DeduplicationMode,
)

# Exceptions
from .exceptions import (
    StrataError,
    IOError,
    NetworkError,
    FormatError,
    ValidationError,
    CompressionError,
    EncryptionError,
    MountError,
    CacheError,
    VersionError,
)

# Data classes
from .utils import Metadata, AnalysisReport


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


__version__ = "0.2.0"

__all__ = [
    # I/O
    "open",
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
    # Types
    "Metadata",
    "AnalysisReport",
    "PathLike",
    "Shape",
    "PackingMode",
    "BuildProfile",
    "DeduplicationMode",
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
```

---

## 2. Type System (`typing.py`)

```python
"""Type aliases and protocols for Strata."""

from typing import Union, Tuple, Protocol, Literal
from pathlib import Path
import os

# Path types
PathLike = Union[str, os.PathLike, Path]

# Array types
Shape = Tuple[int, ...]

# Packing modes control compression speed vs ratio
PackingMode = Literal["fast", "balanced", "tight"]

# Build profiles are preset configurations for specific use cases
BuildProfile = Literal["ml", "eda", "embedded", "generic", "archival"]

# Deduplication algorithms
DeduplicationMode = Literal[
    "dcam",      # DCAM sampling - fast approximate dedup
    "full",      # Full sweep - accurate but slower
    "none",      # No deduplication
]

# Compression algorithms
CompressionAlgorithm = Literal["lz4", "zstd", "none"]


class ReadableBuffer(Protocol):
    """Protocol for readable buffer objects."""
    def __buffer__(self, flags: int) -> memoryview: ...


class WritableBuffer(Protocol):
    """Protocol for writable buffer objects."""
    def __buffer__(self, flags: int) -> memoryview: ...


class ProgressCallback(Protocol):
    """Protocol for progress callbacks."""
    def __call__(self, current: int, total: int, stage: str) -> None:
        """Called during operations.

        Args:
            current: Bytes processed so far
            total: Total bytes to process
            stage: Description of current stage
        """
        ...
```

---

## 3. Packing Modes & Deduplication (`writer.py` excerpt)

```python
"""Writer configuration with semantic modes."""

from typing import Literal, Optional
from .typing import PackingMode, DeduplicationMode, CompressionAlgorithm

# Packing mode configurations
# Each mode defines compression algorithm, level, and dedup strategy
PACKING_MODES = {
    "fast": {
        "compression": "lz4",
        "compression_level": None,  # LZ4 has no levels
        "dedup_mode": "dcam",       # Fast approximate dedup
        "dedup_sample_rate": 0.1,   # Sample 10% of blocks
        "block_size": 64 * 1024,    # 64KB blocks
    },
    "balanced": {
        "compression": "lz4",
        "compression_level": None,
        "dedup_mode": "dcam",
        "dedup_sample_rate": 0.3,   # Sample 30% of blocks
        "block_size": 64 * 1024,
    },
    "tight": {
        "compression": "zstd",
        "compression_level": 9,     # Maximum compression
        "dedup_mode": "full",       # Full dedup sweep
        "dedup_sample_rate": 1.0,   # Check all blocks
        "block_size": 128 * 1024,   # Larger blocks for better compression
    },
}


class Writer:
    """High-level writer for creating Strata snapshots.

    The Writer provides a clean, fluent API for building snapshots with
    automatic finalization and semantic configuration options.
    """

    def __init__(
        self,
        path: PathLike,
        *,
        # Semantic configuration
        packing: PackingMode = "balanced",

        # Fine-grained overrides (optional)
        compression: Optional[CompressionAlgorithm] = None,
        compression_level: Optional[int] = None,
        dedup_mode: Optional[DeduplicationMode] = None,
        block_size: Optional[int] = None,

        # Security
        encrypt: bool = False,
        password: Optional[str] = None,

        # Metadata
        metadata: Optional[Dict[str, Any]] = None,

        # Performance
        progress: Optional[ProgressCallback] = None,
    ):
        """Create a new snapshot writer.

        Args:
            path: Output .st file path
            packing: Packing mode controlling compression/dedup tradeoff
                     - "fast": Quick writes, moderate compression (DCAM 10%)
                     - "balanced": Good balance (DCAM 30%, default)
                     - "tight": Maximum compression, full dedup (100%)
            compression: Override compression algorithm
            compression_level: Override compression level
            dedup_mode: Override deduplication algorithm
                       - "dcam": Fast approximate (DCAM sampling)
                       - "full": Accurate full sweep
                       - "none": Disable deduplication
            block_size: Override block size
            encrypt: Enable AES-256-GCM encryption
            password: Encryption password (required if encrypt=True)
            metadata: Optional metadata dict to store
            progress: Optional progress callback

        Example:
            >>> # Simple case - use preset
            >>> with strata.Writer("out.st", packing="tight") as w:
            ...     w.add("large_file.img")
            ...
            >>> # Advanced - override specific settings
            >>> with strata.Writer(
            ...     "out.st",
            ...     packing="balanced",
            ...     dedup_mode="full",  # Override to full dedup
            ...     encrypt=True,
            ...     password="secret",
            ... ) as w:
            ...     w.add("sensitive.img")
        """
        # Get base configuration from packing mode
        config = PACKING_MODES[packing].copy()

        # Apply overrides
        if compression is not None:
            config["compression"] = compression
        if compression_level is not None:
            config["compression_level"] = compression_level
        if dedup_mode is not None:
            config["dedup_mode"] = dedup_mode
        if block_size is not None:
            config["block_size"] = block_size

        # TODO: Create Rust StrataBuilder with full config
        # - Pass dedup_mode and sample_rate
        # - Pass encryption parameters
        # - Set up progress callback

    def add(self, source: Any, *, name: Optional[str] = None) -> "Writer":
        """Add data to snapshot (auto-detects type)."""
        # TODO: Implement

    def add_file(self, path: PathLike, *, name: Optional[str] = None) -> "Writer":
        """Add file to snapshot."""
        # TODO: Implement

    def add_bytes(self, data: bytes, *, name: Optional[str] = None) -> "Writer":
        """Add raw bytes to snapshot."""
        # TODO: Implement (needs Rust support)

    def add_array(
        self,
        array: np.ndarray,
        *,
        name: str,
        metadata: Optional[Dict[str, Any]] = None,
    ) -> "Writer":
        """Add NumPy array with optional metadata."""
        # TODO: Implement named array storage

    def add_dataset(
        self,
        dataset: Any,  # torch.utils.data.Dataset or similar
        *,
        name: str,
        item_shape: Optional[Shape] = None,
    ) -> "Writer":
        """Add entire dataset efficiently."""
        # TODO: Implement batch writing

    def finalize(self) -> Metadata:
        """Finalize snapshot and return metadata."""
        # TODO: Implement
```

---

## 4. Build Profiles (`profiles.py`)

```python
"""Pre-configured build profiles for common use cases."""

from typing import Dict, Any, Literal
from .typing import BuildProfile, PathLike
from .writer import Writer

# Build profiles map to specific Writer configurations
PROFILES: Dict[BuildProfile, Dict[str, Any]] = {
    # Machine Learning: Fast writes, good compression, optimized for sequential access
    "ml": {
        "packing": "fast",
        "block_size": 128 * 1024,      # Large blocks for sequential reads
        "dedup_mode": "dcam",           # Fast approximate dedup
        "compression": "lz4",           # Fast compression/decompression
    },

    # Exploratory Data Analysis: Balanced, good for notebooks and experiments
    "eda": {
        "packing": "balanced",
        "block_size": 64 * 1024,
        "dedup_mode": "dcam",
        "compression": "lz4",
    },

    # Embedded: Maximum compression for resource-constrained environments
    "embedded": {
        "packing": "tight",
        "block_size": 32 * 1024,        # Smaller blocks
        "dedup_mode": "full",           # Full dedup
        "compression": "zstd",
        "compression_level": 9,
    },

    # Generic: Balanced defaults for general use
    "generic": {
        "packing": "balanced",
        "block_size": 64 * 1024,
        "dedup_mode": "dcam",
        "compression": "lz4",
    },

    # Archival: Maximum compression and dedup for long-term storage
    "archival": {
        "packing": "tight",
        "block_size": 256 * 1024,       # Very large blocks
        "dedup_mode": "full",
        "compression": "zstd",
        "compression_level": 19,        # Ultra compression
    },
}


def build(
    source: PathLike,
    output: PathLike,
    *,
    profile: BuildProfile = "generic",
    **overrides: Any,
) -> Metadata:
    """Build a snapshot using a preset profile.

    This is a convenience function that combines Writer configuration
    and common build patterns.

    Args:
        source: Source file, directory, or data
        output: Output .st file path
        profile: Build profile to use
        **overrides: Override any profile settings

    Returns:
        Metadata object with snapshot information

    Example:
        >>> # ML dataset with defaults
        >>> meta = strata.build("imagenet/", "imagenet.st", profile="ml")
        >>> print(f"Compressed to {meta.size_compressed / 1e9:.1f} GB")
        ...
        >>> # Archival with encryption
        >>> meta = strata.build(
        ...     "backup/",
        ...     "backup.st",
        ...     profile="archival",
        ...     encrypt=True,
        ...     password="secret",
        ... )
    """
    # Get profile configuration
    config = PROFILES[profile].copy()

    # Apply overrides
    config.update(overrides)

    # Build snapshot
    with Writer(output, **config) as writer:
        writer.add(source)

    # Return metadata
    from .utils import inspect
    return inspect(output)
```

---

## 5. Dataset Module (`dataset.py`)

```python
"""ML dataset integration for PyTorch and TensorFlow."""

from typing import Optional, Callable, Literal
import warnings

try:
    import torch
    from torch.utils.data import Dataset as TorchDataset
    HAS_TORCH = True
except ImportError:
    TorchDataset = object
    HAS_TORCH = False

from .reader import Reader
from .typing import PathLike, Shape


class LRUCache:
    """Least-Recently-Used cache for dataset items."""

    def __init__(self, max_size_mb: int):
        """Create LRU cache.

        Args:
            max_size_mb: Maximum cache size in megabytes
        """
        # TODO: Implement LRU eviction

    def get(self, key: int) -> Optional[bytes]:
        """Get item from cache."""
        # TODO: Implement

    def put(self, key: int, value: bytes) -> None:
        """Add item to cache."""
        # TODO: Implement

    def stats(self) -> Dict[str, Any]:
        """Return cache statistics."""
        # TODO: Implement


class Prefetcher:
    """Background prefetcher for upcoming items."""

    def __init__(
        self,
        reader: Reader,
        prefetch_factor: int,
        num_workers: int,
    ):
        """Create prefetcher.

        Args:
            reader: Reader to prefetch from
            prefetch_factor: Number of batches to prefetch
            num_workers: Number of worker threads
        """
        # TODO: Implement thread pool
        # TODO: Implement prefetch queue

    def hint(self, index: int) -> None:
        """Hint that index will be accessed soon."""
        # TODO: Implement

    def shutdown(self) -> None:
        """Shutdown prefetcher."""
        # TODO: Implement


class Dataset(TorchDataset):
    """High-performance PyTorch dataset backed by Strata.

    Features:
    - Smart LRU caching with configurable size
    - Background prefetching for next batches
    - Shuffling with reproducible seeds
    - Variable-length item support via index file
    - DDP-compatible epoch shuffling
    - Transform composition
    - Zero-copy option for maximum performance
    """

    def __init__(
        self,
        path: PathLike,
        *,
        # Data format
        item_size: Optional[int] = None,
        index_file: Optional[PathLike] = None,
        output_format: Literal["bytes", "numpy", "tensor"] = "tensor",

        # Transforms
        transform: Optional[Callable] = None,
        target_transform: Optional[Callable] = None,

        # Performance
        cache_size_mb: int = 512,
        prefetch_factor: int = 2,
        num_workers: int = 0,
        zero_copy: bool = False,

        # Shuffling
        shuffle: bool = False,
        seed: Optional[int] = None,

        # S3 options
        s3_region: Optional[str] = None,
        endpoint_url: Optional[str] = None,
    ):
        """Create a Strata-backed dataset.

        Args:
            path: Path to .st file (local or s3://)
            item_size: Fixed item size in bytes (required if no index)
            index_file: Path to index file for variable-length items
            output_format: Output format - "bytes", "numpy", or "tensor"
            transform: Optional transform for features
            target_transform: Optional transform for labels
            cache_size_mb: LRU cache size in MB (0 to disable)
            prefetch_factor: Number of batches to prefetch (0 to disable)
            num_workers: Number of prefetch worker threads
            zero_copy: Use zero-copy views (faster but read-only)
            shuffle: Enable shuffling
            seed: Random seed for shuffling
            s3_region: AWS region for S3 files
            endpoint_url: Custom S3 endpoint

        Example:
            >>> # Simple fixed-size items
            >>> dataset = strata.Dataset(
            ...     "imagenet.st",
            ...     item_size=150528,  # 224*224*3
            ...     transform=transforms.ToTensor(),
            ... )
            ...
            >>> # Variable-length with index
            >>> dataset = strata.Dataset(
            ...     "text.st",
            ...     index_file="text.idx",
            ...     output_format="bytes",
            ... )
            ...
            >>> # S3 with caching
            >>> dataset = strata.Dataset(
            ...     "s3://bucket/data.st",
            ...     item_size=1024,
            ...     cache_size_mb=2048,
            ...     s3_region="us-west-2",
            ... )
        """
        if not HAS_TORCH:
            raise ImportError("PyTorch is required for Dataset")

        if item_size is None and index_file is None:
            raise ValidationError(
                "Either item_size or index_file must be provided"
            )

        # Open reader
        self._reader = Reader(
            path,
            s3_region=s3_region,
            endpoint_url=endpoint_url,
        )

        # Load index if provided
        self._index = self._load_index(index_file) if index_file else None

        # Configure
        self._item_size = item_size
        self._output_format = output_format
        self._transform = transform
        self._target_transform = target_transform
        self._zero_copy = zero_copy

        # Setup shuffling
        self._shuffle = shuffle
        self._seed = seed
        self._indices = self._create_indices() if shuffle else None
        self._epoch = 0

        # Setup caching
        self._cache = LRUCache(cache_size_mb) if cache_size_mb > 0 else None

        # Setup prefetching
        self._prefetcher = (
            Prefetcher(self._reader, prefetch_factor, num_workers)
            if prefetch_factor > 0
            else None
        )

    def _load_index(self, path: PathLike) -> List[Tuple[int, int]]:
        """Load index file with (offset, size) tuples."""
        # TODO: Implement index loading

    def _create_indices(self) -> List[int]:
        """Create shuffled indices list."""
        # TODO: Implement shuffle with seed

    def __len__(self) -> int:
        """Return dataset length."""
        if self._index:
            return len(self._index)
        return self._reader.size // self._item_size

    def __getitem__(self, idx: int):
        """Get item at index.

        Returns:
            Tensor, NumPy array, or bytes depending on output_format
        """
        # Map through shuffle indices
        if self._indices is not None:
            idx = self._indices[idx]

        # Check cache
        if self._cache:
            cached = self._cache.get(idx)
            if cached is not None:
                return self._decode_item(cached)

        # Get offset and size
        if self._index:
            offset, size = self._index[idx]
        else:
            offset = idx * self._item_size
            size = self._item_size

        # Hint prefetcher
        if self._prefetcher:
            self._prefetcher.hint(idx + 1)

        # Read data
        data = self._reader.read_at(offset, size)

        # Cache it
        if self._cache:
            self._cache.put(idx, data)

        # Decode and transform
        return self._decode_item(data)

    def _decode_item(self, data: bytes):
        """Decode bytes to requested format."""
        if self._output_format == "bytes":
            item = data
        elif self._output_format == "numpy":
            import numpy as np
            item = np.frombuffer(data, dtype=np.uint8)
            if not self._zero_copy:
                item = item.copy()
        elif self._output_format == "tensor":
            import torch
            item = torch.frombuffer(bytearray(data), dtype=torch.uint8)
        else:
            raise ValueError(f"Invalid output_format: {self._output_format}")

        # Apply transform
        if self._transform:
            item = self._transform(item)

        return item

    def set_epoch(self, epoch: int) -> None:
        """Update epoch for DDP shuffling.

        Args:
            epoch: Current epoch number

        Example:
            >>> for epoch in range(10):
            ...     dataset.set_epoch(epoch)
            ...     for batch in dataloader:
            ...         train(batch)
        """
        self._epoch = epoch
        if self._shuffle:
            # Re-shuffle with epoch-specific seed
            import random
            rng = random.Random(self._seed + epoch)
            self._indices = list(range(len(self)))
            rng.shuffle(self._indices)

    def cache_stats(self) -> Dict[str, Any]:
        """Get cache statistics.

        Returns:
            Dict with hit_rate, hits, misses, size_mb
        """
        if self._cache:
            return self._cache.stats()
        return {"enabled": False}

    def __repr__(self) -> str:
        return (
            f"Dataset({len(self)} items, "
            f"cache={self._cache_size_mb}MB, "
            f"shuffle={self._shuffle})"
        )


class TFDataset:
    """TensorFlow tf.data.Dataset wrapper for Strata."""

    def __init__(self, path: PathLike, **kwargs):
        """Create TensorFlow dataset.

        Args:
            path: Path to .st file
            **kwargs: Same as Dataset
        """
        # TODO: Implement TF dataset

    def as_dataset(self):
        """Convert to tf.data.Dataset."""
        import tensorflow as tf

        # TODO: Implement generator

        return tf.data.Dataset.from_generator(...)
```

---

## 6. Complete File Structure

Here's the complete file structure showing what exists, what's stubbed, and what's removed:

```
strata/
├── __init__.py              ✅ Complete - clean public API, no legacy
├── _core.pyi                ✅ Complete - type stubs for Rust
├── typing.py                ✅ Complete - all type aliases
├── exceptions.py            ✅ Complete - full hierarchy
│
├── reader.py                ✅ Complete - Reader + AsyncReader
│   ├── Reader               ✅ Done - file-like + random access
│   ├── AsyncReader          ⚠️  Needs: size() method, iter_chunks()
│   └── [NO LEGACY CODE]
│
├── writer.py                🔨 In Progress
│   ├── Writer.__init__      ✅ Done - packing modes, dedup modes
│   ├── Writer.add()         TODO - auto-detect source type
│   ├── Writer.add_file()    ✅ Done - delegates to Rust
│   ├── Writer.add_bytes()   ⚠️  Works but inefficient (temp file)
│   ├── Writer.add_array()   ⚠️  Works but inefficient (temp file)
│   ├── Writer.add_metadata  ⚠️  In-memory only, not persisted
│   ├── Writer.add_dataset() TODO - batch write datasets
│   ├── Writer.finalize()    ✅ Done
│   └── PACKING_MODES        ✅ Done - fast/balanced/tight configs
│
├── array.py                 ✅ Complete
│   ├── read_array()         ✅ Done - zero-copy option
│   ├── write_array()        ✅ Done
│   └── ArrayView            ✅ Done - memmap-like lazy loading
│
├── dataset.py               TODO - High Priority
│   ├── Dataset              TODO - PyTorch dataset class
│   ├── TFDataset            TODO - TensorFlow wrapper
│   ├── LRUCache             TODO - Smart caching
│   └── Prefetcher           TODO - Background prefetching
│
├── utils.py                 ✅ Complete
│   ├── inspect()            ✅ Done - returns Metadata object
│   ├── analyze()            ✅ Done - returns AnalysisReport
│   ├── diff()               ✅ Done
│   ├── verify()             ⚠️  Signature verification only
│   ├── info()               ✅ Done - pretty printing
│   ├── Metadata             ✅ Done - property access
│   └── AnalysisReport       ✅ Done - dedup stats
│
├── mount.py                 🔨 Needs Refactor
│   ├── mount()              ✅ Works but not pythonic
│   ├── unmount()            ✅ Works
│   └── MountPoint           TODO - Refactor to proper class
│
└── profiles.py              TODO - High Priority
    ├── PROFILES             TODO - ml/eda/embedded/generic/archival
    └── build()              TODO - Convenience function

REMOVED (Delete These):
├── ❌ builder.py            DELETE - Legacy SnapshotBuilder
├── ❌ io.py                 DELETE - Legacy open()
└── ❌ torch.py              DELETE - Old PyTorch integration
```

---

## Summary

### What's Clean Now
✅ Reader - fully pythonic, no legacy
✅ Array - complete NumPy integration
✅ Utils - clean utility functions
✅ Exceptions - proper hierarchy
✅ Types - comprehensive type system

### What Needs Rust Changes
🔧 Writer.add_bytes() - needs direct byte writing in Rust
🔧 Writer.add_metadata() - needs metadata block support in Rust
🔧 Encryption - needs AES-GCM implementation in Rust
🔧 Dedup modes - needs DCAM/full sweep toggle in Rust

### What Needs Python Implementation
📝 Dataset - LRU cache, prefetching, shuffling (pure Python)
📝 Profiles - build() function and presets (pure Python)
📝 Mount - refactor to cleaner API (pure Python)

### Priority Order
1. **Dataset module** - Makes Strata usable for ML (8-10 hours)
2. **Profiles module** - Great UX improvement (2-3 hours)
3. **Rust metadata** - Enables rich inspection (4-6 hours)
4. **Rust dedup modes** - Performance tuning (3-4 hours)
5. **Mount refactor** - Polish (4-6 hours)

This is a **clean slate** - no backward compatibility, all intentional design choices.
