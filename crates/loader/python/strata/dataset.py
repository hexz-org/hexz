"""ML dataset integration for PyTorch and TensorFlow.

This module provides high-performance dataset classes backed by Strata snapshots,
with features like smart caching, prefetching, and shuffling.
"""

from typing import Optional, Callable, Dict, Any, List, Tuple, Literal
from concurrent.futures import ThreadPoolExecutor, Future
from threading import Lock
from pathlib import Path

try:
    import torch  # noqa: F401
    from torch.utils.data import Dataset as TorchDataset

    HAS_TORCH = True
except ImportError:
    TorchDataset = object
    HAS_TORCH = False

from .reader import Reader
from .typing import PathLike
from .exceptions import ValidationError


from collections import OrderedDict
import struct


class LRUCache:
    """Least-Recently-Used cache for dataset items.

    Implements LRU eviction policy with configurable max size in megabytes.
    Uses OrderedDict for O(1) access and eviction.
    """

    def __init__(self, max_size_mb: int):
        """Create LRU cache.

        Args:
            max_size_mb: Maximum cache size in megabytes
        """
        self.max_size_bytes = max_size_mb * 1024 * 1024
        self.current_size = 0
        self.cache: OrderedDict[int, bytes] = OrderedDict()
        self.hits = 0
        self.misses = 0

    def get(self, key: int) -> Optional[bytes]:
        """Get item from cache.

        Args:
            key: Item index

        Returns:
            Cached bytes or None if not in cache
        """
        if key in self.cache:
            # Move to end (most recently used)
            self.cache.move_to_end(key)
            self.hits += 1
            return self.cache[key]
        else:
            self.misses += 1
            return None

    def put(self, key: int, value: bytes) -> None:
        """Add item to cache with LRU eviction if needed.

        Args:
            key: Item index
            value: Bytes to cache
        """
        value_size = len(value)

        # If key already in cache, remove old size
        if key in self.cache:
            old_value = self.cache.pop(key)
            self.current_size -= len(old_value)

        # Add to cache
        self.cache[key] = value
        self.current_size += value_size

        # Evict least recently used items until under size limit
        while self.current_size > self.max_size_bytes and self.cache:
            # Evict least recently used (first in OrderedDict)
            _, evicted_value = self.cache.popitem(last=False)
            self.current_size -= len(evicted_value)

    def stats(self) -> Dict[str, Any]:
        """Return cache statistics.

        Returns:
            Dict with hit_rate, hits, misses, size_mb, items
        """
        total = self.hits + self.misses
        hit_rate = self.hits / total if total > 0 else 0.0

        return {
            "enabled": True,
            "hit_rate": hit_rate,
            "hits": self.hits,
            "misses": self.misses,
            "size_mb": self.current_size / (1024 * 1024),
            "items": len(self.cache),
        }

    def clear(self) -> None:
        """Clear the cache."""
        self.cache.clear()
        self.current_size = 0


class Prefetcher:
    """Background prefetcher for upcoming items.

    Uses a thread pool to prefetch items that are likely to be accessed soon.
    """

    def __init__(
        self,
        reader: Reader,
        prefetch_factor: int,
        num_workers: int,
        item_size: Optional[int] = None,
        index: Optional[List[Tuple[int, int]]] = None,
    ):
        """Create prefetcher.

        Args:
            reader: Reader to prefetch from
            prefetch_factor: Number of items ahead to prefetch
            num_workers: Number of worker threads
            item_size: Fixed item size in bytes (if not using index)
            index: Optional index for variable-length items
        """
        self.reader = reader
        self.prefetch_factor = prefetch_factor
        self.num_workers = num_workers
        self.item_size = item_size
        self.index = index
        self.active = True

        # Thread pool for background loading
        self.executor = ThreadPoolExecutor(max_workers=num_workers)

        # Store prefetched items by index
        self.prefetched: Dict[int, bytes] = {}

        # Track in-flight prefetch jobs to avoid duplicates
        self.pending: Dict[int, Future] = {}

        # Lock for thread-safe access to prefetched and pending
        self.lock = Lock()

    def hint(self, index: int) -> None:
        """Hint that index will be accessed soon.

        Args:
            index: Item index that will be accessed
        """
        if not self.active:
            return

        with self.lock:
            # Skip if already prefetched or being prefetched
            if index in self.prefetched or index in self.pending:
                return

            # Submit prefetch job
            future = self.executor.submit(self._prefetch_worker, index)
            self.pending[index] = future

    def get(self, index: int) -> Optional[bytes]:
        """Get prefetched item if available.

        Args:
            index: Item index

        Returns:
            Prefetched bytes or None if not available
        """
        with self.lock:
            # Check if already prefetched
            if index in self.prefetched:
                data = self.prefetched.pop(index)
                return data

            # Check if prefetch is in progress
            if index in self.pending:
                future = self.pending.pop(index)
                # Wait for it to complete (non-blocking check first)
                if future.done():
                    try:
                        data = future.result()
                        return data
                    except Exception:
                        # Prefetch failed, return None
                        return None

            return None

    def _prefetch_worker(self, index: int) -> bytes:
        """Worker function to prefetch an item.

        Args:
            index: Item index to prefetch

        Returns:
            Prefetched bytes
        """
        # Calculate offset and size
        if self.index:
            offset, size = self.index[index]
        else:
            if self.item_size is None:
                raise ValueError("Either item_size or index must be provided")
            offset = index * self.item_size
            size = self.item_size

        # Read data
        data = self.reader.read_at(offset, size)

        # Store in prefetched dict
        with self.lock:
            self.prefetched[index] = data
            # Remove from pending
            if index in self.pending:
                del self.pending[index]

        return data

    def shutdown(self) -> None:
        """Shutdown prefetcher and worker threads."""
        self.active = False
        with self.lock:
            # Cancel pending jobs
            for future in self.pending.values():
                future.cancel()
            self.pending.clear()
            self.prefetched.clear()

        # Shutdown executor
        self.executor.shutdown(wait=True)


class Dataset(TorchDataset):
    """High-performance PyTorch dataset backed by Strata.

    Features:
    - Smart LRU caching with configurable size
    - Background prefetching for next batches
    - Shuffling with reproducible seeds
    - Variable-length item support via index file
    - DDP-compatible epoch shuffling
    - Transform composition
    - Low-overhead copy option for high performance
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
            prefetch_factor: Number of items ahead to prefetch (0 to disable)
            num_workers: Number of prefetch worker threads
            zero_copy: Use direct read into buffer (faster but read-only).
                      Note: Currently involves one internal copy; full zero-copy
                      is planned for v0.3.
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
            raise ImportError(
                "PyTorch is required for Dataset. Install with: pip install torch"
            )

        if item_size is None and index_file is None:
            raise ValidationError("Either item_size or index_file must be provided")

        self._prefetcher: Optional[Prefetcher] = None

        # Open reader
        self._reader = Reader(
            path,
            s3_region=s3_region,
            endpoint_url=endpoint_url,
        )

        # Store configuration
        self._item_size = item_size
        self._output_format = output_format
        self._transform = transform
        self._target_transform = target_transform
        self._zero_copy = zero_copy
        self._shuffle = shuffle
        self._seed = seed
        self._epoch = 0

        # Load index if provided
        self._index: Optional[List[Tuple[int, int]]] = None
        if index_file:
            self._index = self._load_index(index_file)

        # Setup shuffling
        self._indices: Optional[List[int]] = None
        if shuffle:
            self._indices = self._create_indices()

        # Setup caching
        self._cache: Optional[LRUCache] = None
        if cache_size_mb > 0:
            self._cache = LRUCache(cache_size_mb)

        # Setup prefetching
        self._prefetcher: Optional[Prefetcher] = None
        if prefetch_factor > 0 and num_workers > 0:
            self._prefetcher = Prefetcher(
                self._reader,
                prefetch_factor,
                num_workers,
                item_size=self._item_size,
                index=self._index,
            )

    def _load_index(self, path: PathLike) -> List[Tuple[int, int]]:
        """Load index file with (offset, size) tuples.

        The index file is a simple binary format:
        [uint64 offset][uint64 size]
        [uint64 offset][uint64 size]
        ...

        Args:
            path: Path to .idx file

        Returns:
            List of (offset, size) tuples for each item
        """
        index_path = Path(path)
        if not index_path.exists():
            raise FileNotFoundError(f"Index file not found: {path}")

        items = []
        item_struct = struct.Struct("<QQ")  # Little-endian, 2 uint64s

        with open(index_path, "rb") as f:
            while True:
                chunk = f.read(item_struct.size)
                if len(chunk) < item_struct.size:
                    break
                offset, size = item_struct.unpack(chunk)
                items.append((offset, size))

        return items

    def _create_indices(self) -> List[int]:
        """Create shuffled indices list.

        Returns:
            Shuffled list of indices
        """
        import random

        rng = random.Random(self._seed + self._epoch if self._seed else None)
        indices = list(range(len(self)))
        rng.shuffle(indices)
        return indices

    def __len__(self) -> int:
        """Return dataset length."""
        if self._index:
            return len(self._index)
        return self._reader.size // self._item_size

    def __getitem__(self, idx: int):
        """Get item at index.

        Args:
            idx: Item index

        Returns:
            Tensor, NumPy array, or bytes depending on output_format
        """
        # Map through shuffle indices
        if self._indices is not None:
            idx = self._indices[idx]

        # Check cache first
        if self._cache:
            cached = self._cache.get(idx)
            if cached is not None:
                return self._decode_item(cached)

        # Check prefetcher
        if self._prefetcher:
            prefetched = self._prefetcher.get(idx)
            if prefetched is not None:
                if self._cache:
                    self._cache.put(idx, prefetched)
                return self._decode_item(prefetched)

        # Get offset and size
        if self._index:
            offset, size = self._index[idx]
        else:
            offset = idx * self._item_size
            size = self._item_size

        # Hint prefetcher about next items
        if self._prefetcher:
            for i in range(1, self._prefetcher.prefetch_factor + 1):
                if idx + i < len(self):
                    self._prefetcher.hint(idx + i)

        # Read data
        data = self._reader.read_at(offset, size)

        # Cache it
        if self._cache:
            self._cache.put(idx, data)

        # Decode and return
        return self._decode_item(data)

    def _decode_item(self, data: bytes):
        """Decode bytes to requested format.

        Args:
            data: Raw bytes from snapshot

        Returns:
            Decoded item in requested format
        """
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

        This should be called at the start of each epoch when using
        DistributedDataParallel to ensure different shuffling per epoch.

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
            self._indices = self._create_indices()

    def cache_stats(self) -> Dict[str, Any]:
        """Get cache statistics.

        Returns:
            Dict with hit_rate, hits, misses, size_mb, items
        """
        if self._cache:
            return self._cache.stats()
        return {"enabled": False}

    def __repr__(self) -> str:
        cache_mb = self._cache.current_size / (1024 * 1024) if self._cache else 0
        return (
            f"Dataset({len(self)} items, "
            f"cache={cache_mb:.1f}MB, "
            f"shuffle={self._shuffle})"
        )

    def __del__(self):
        """Cleanup resources."""
        if self._prefetcher:
            self._prefetcher.shutdown()
        if hasattr(self, "_reader"):
            self._reader.close()


class TFDataset:
    """TensorFlow tf.data.Dataset wrapper for Strata.

    .. warning::
        This class is not yet implemented. Initializing it will raise
        `NotImplementedError`.

    Provides integration with TensorFlow's data loading pipeline.
    """

    def __init__(self, path: PathLike, **kwargs):
        """Create TensorFlow dataset.

        Args:
            path: Path to .st file
            **kwargs: Same as Dataset (except output_format)

        Raises:
            NotImplementedError: This class is not yet functional.
        """
        # TODO: Implement TensorFlow dataset
        # - Store path and kwargs
        # - Initialize when as_dataset() is called
        raise NotImplementedError("TensorFlow dataset not yet implemented")

    def as_dataset(self):
        """Convert to tf.data.Dataset.

        Returns:
            tf.data.Dataset instance
        """
        # TODO: Implement tf.data.Dataset conversion
        # - Import tensorflow
        # - Create generator function
        # - Return tf.data.Dataset.from_generator(...)
        try:
            # import tensorflow as tf
            pass
        except ImportError:
            raise ImportError(
                "TensorFlow is required for TFDataset. "
                "Install with: pip install tensorflow"
            )

        raise NotImplementedError("TensorFlow dataset conversion not yet implemented")


__all__ = ["Dataset", "TFDataset", "LRUCache", "Prefetcher"]
