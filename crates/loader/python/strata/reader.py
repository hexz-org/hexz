"""Pythonic reader interface for Strata snapshots.

This module provides the high-level Reader and AsyncReader classes that wrap
the Rust-implemented StrataReader with a more pythonic interface.
"""

from typing import Optional, Any, Dict
from pathlib import Path

from . import _strata_core
from .exceptions import IOError, FormatError
from .typing import PathLike
from .utils import Metadata


class Reader:
    """High-level reader for Strata snapshots with pythonic interface.

    Provides a file-like interface with additional random access capabilities.
    Supports context managers, pickle serialization, and slice notation.

    Example:
        >>> with strata.Reader("dataset.st") as reader:
        ...     data = reader.read(4096)
        ...     # Or random access
        ...     chunk = reader.read_at(offset=1000, size=100)
        ...     # Or slice notation
        ...     chunk = reader[1000:1100]
    """

    def __init__(
        self,
        path: PathLike,
        *,
        cache_size: Optional[str] = None,
        prefetch: bool = True,
        s3_region: Optional[str] = None,
        endpoint_url: Optional[str] = None,
        allow_restricted: bool = False,
    ):
        """Open a Strata snapshot for reading.

        Args:
            path: Path or URL to the snapshot file
            cache_size: Cache size (e.g., "512M", "1G")
            prefetch: Enable prefetching for sequential reads
            s3_region: AWS region for S3 URLs
            endpoint_url: Custom S3 endpoint URL
            allow_restricted: Allow connections to private/internal IPs
        """
        self._path = str(path)
        self._reader = _strata_core.StrataReader(
            self._path,
            s3_region=s3_region,
            endpoint_url=endpoint_url,
            allow_restricted=allow_restricted,
        )
        # TODO: Wire up cache_size and prefetch to Rust config

    def read(self, size: int = -1) -> bytes:
        """Read bytes from current position and advance cursor.

        Args:
            size: Number of bytes to read (-1 for all remaining)

        Returns:
            Bytes read from the snapshot
        """
        return self._reader.read(size)

    def read_at(self, offset: int, size: int) -> bytes:
        """Read bytes at a specific offset without moving cursor.

        Args:
            offset: Byte offset to read from
            size: Number of bytes to read

        Returns:
            Bytes read from the snapshot
        """
        return self._reader.read_at(offset, size)

    def read_range(self, start: int, end: int) -> bytes:
        """Read byte range [start, end).

        Args:
            start: Starting byte offset (inclusive)
            end: Ending byte offset (exclusive)

        Returns:
            Bytes in the specified range
        """
        return self._reader.read_at(start, end - start)

    def seek(self, offset: int, whence: int = 0) -> int:
        """Seek to a position in the file.

        Args:
            offset: Offset to seek to
            whence: 0 (absolute), 1 (relative), 2 (from end)

        Returns:
            New absolute position
        """
        return self._reader.seek(offset, whence)

    def tell(self) -> int:
        """Get current position in the file.

        Returns:
            Current byte offset
        """
        return self._reader.tell()

    @property
    def size(self) -> int:
        """Total size of the snapshot in bytes."""
        return self._reader.size()

    @property
    def metadata(self) -> Metadata:
        """File metadata (version, compression, etc.)."""
        return Metadata(self._reader.metadata())

    def iter_chunks(self, chunk_size: int = 1024 * 1024):
        """Iterate over the snapshot in fixed-size chunks.

        Args:
            chunk_size: Size of each chunk in bytes (default 1MB)

        Yields:
            Bytes chunks from the snapshot
        """
        offset = 0
        total = self.size
        while offset < total:
            size = min(chunk_size, total - offset)
            chunk = self.read_at(offset, size)
            yield chunk
            offset += size

    def close(self) -> None:
        """Close the snapshot and release resources."""
        self._reader.close()

    def __enter__(self):
        """Context manager entry."""
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        """Context manager exit."""
        self.close()

    def __getitem__(self, key) -> bytes:
        """Support slice notation: reader[100:200].

        Args:
            key: Slice object

        Returns:
            Bytes in the specified range

        Raises:
            TypeError: If key is not a slice
        """
        if isinstance(key, slice):
            start = key.start or 0
            stop = key.stop or self.size
            return self.read_at(start, stop - start)
        raise TypeError("indices must be slices")

    def __getstate__(self) -> Dict[str, Any]:
        """Support for pickle serialization."""
        return self._reader.__getstate__()

    def __setstate__(self, state: Dict[str, Any]) -> None:
        """Support for pickle deserialization."""
        self._reader.__setstate__(state)

    def __repr__(self) -> str:
        return f"Reader({self._path!r})"


class AsyncReader:
    """Async reader for Strata snapshots.

    Provides the same interface as Reader but with async/await support.

    Example:
        >>> async with strata.AsyncReader("dataset.st") as reader:
        ...     data = await reader.read(4096)
        ...     async for chunk in reader:
        ...         process(chunk)
    """

    def __init__(
        self,
        path: PathLike,
        *,
        cache_size: Optional[str] = None,
        s3_region: Optional[str] = None,
        endpoint_url: Optional[str] = None,
        allow_restricted: bool = False,
    ):
        """Open a Strata snapshot for async reading.

        Args:
            path: Path or URL to the snapshot file
            cache_size: Cache size (e.g., "512M", "1G")
            s3_region: AWS region for S3 URLs
            endpoint_url: Custom S3 endpoint URL
            allow_restricted: Allow connections to private/internal IPs
        """
        self._path = str(path)
        self._reader = _strata_core.AsyncStrataReader(
            self._path,
            s3_region=s3_region,
            endpoint_url=endpoint_url,
            allow_restricted=allow_restricted,
        )
        # TODO: Wire up cache_size to Rust config
        self._chunk_size = 1024 * 1024  # 1MB default

    async def read(self, size: int = -1) -> bytes:
        """Async read bytes from current position.

        Args:
            size: Number of bytes to read (-1 for all)

        Returns:
            Bytes read from the snapshot
        """
        return await self._reader.read(size)

    async def read_at(self, offset: int, size: int) -> bytes:
        """Async read bytes at a specific offset.

        Args:
            offset: Byte offset to read from
            size: Number of bytes to read

        Returns:
            Bytes read from the snapshot
        """
        return await self._reader.read_at(offset, size)

    async def iter_chunks(self, chunk_size: int = 1024 * 1024):
        """Async iterate over the snapshot in fixed-size chunks.

        Args:
            chunk_size: Size of each chunk in bytes (default 1MB)

        Yields:
            Bytes chunks from the snapshot
        """
        offset = 0
        # For now, we'll need to get size from somewhere
        # This requires AsyncStrataReader to expose a size method
        # TODO: Add size() method to AsyncStrataReader in Rust
        raise NotImplementedError("Async chunk iteration requires size() method")

    async def close(self) -> None:
        """Async close the snapshot and release resources."""
        await self._reader.close()

    async def __aenter__(self):
        """Async context manager entry."""
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb):
        """Async context manager exit."""
        await self.close()

    def __aiter__(self):
        """Async iteration support."""
        return self

    async def __anext__(self) -> bytes:
        """Async iteration: yield chunks of data.

        Yields:
            Chunks of data from the snapshot
        """
        if not hasattr(self, "_iter_offset"):
            self._iter_offset = 0

        # Get total size (we need to cache this to avoid async issues)
        if not hasattr(self, "_total_size"):
            # Assume AsyncStrataReader has a size() method similar to StrataReader
            # For now, we'll need to implement this properly
            raise NotImplementedError(
                "Async iteration requires size() method on AsyncStrataReader"
            )

        if self._iter_offset >= self._total_size:
            raise StopAsyncIteration

        size = min(self._chunk_size, self._total_size - self._iter_offset)
        chunk = await self.read_at(self._iter_offset, size)
        self._iter_offset += size
        return chunk

    def __repr__(self) -> str:
        return f"AsyncReader({self._path!r})"
