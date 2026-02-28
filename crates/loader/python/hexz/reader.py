"""Pythonic reader interface for Hexz snapshots.

This module provides the high-level Reader and AsyncReader classes that wrap
the Rust-implemented Reader with a more pythonic interface.
"""

from typing import TYPE_CHECKING, Optional, Any, Dict, Union, Iterator

from . import hexz_loader
from .typing import PathLike
from .utils import Metadata
from ._internal import _parse_cache_size

if TYPE_CHECKING:
    from .utils import AnalysisReport


class Reader:
    """High-level reader for Hexz snapshots with pythonic interface.

    Provides a file-like interface with additional random access capabilities.
    Supports context managers, pickle serialization, and slice notation.

    Example:
        >>> with hexz.Reader("dataset.hxz") as reader:
        ...     data = reader.read(4096)
        ...     # Zero-copy into buffer
        ...     buf = bytearray(4096)
        ...     n = reader.read(buffer=buf)
        ...     chunk = reader.read(100, offset=1000)  # random access
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
        """Open a Hexz snapshot for reading.

        Args:
            path: Path or URL to the snapshot file
            cache_size: Cache size (e.g., "512M", "1G", "2GB"). If None, uses default.
            prefetch: Enable prefetching for sequential reads (default: True)
            s3_region: AWS region for S3 URLs
            endpoint_url: Custom S3 endpoint URL
            allow_restricted: Allow connections to private/internal IPs
        """
        self._path = str(path)

        # Parse cache size if provided
        cache_capacity_bytes = None
        if cache_size is not None:
            cache_capacity_bytes = _parse_cache_size(cache_size)

        # Default to 4 blocks of prefetch when enabled
        prefetch_count = 4 if prefetch else 0

        self._reader = hexz_loader.Reader(
            self._path,
            s3_region=s3_region,
            endpoint_url=endpoint_url,
            allow_restricted=allow_restricted,
            prefetch_count=prefetch_count,
            cache_capacity_bytes=cache_capacity_bytes,
        )

    def read(
        self,
        size: int = -1,
        *,
        offset: Optional[int] = None,
        buffer: Optional[Union[bytearray, memoryview]] = None,
    ) -> Union[bytes, int]:
        """Read bytes or fill a buffer. Single method for stream and random access.

        From current position (default) or at a specific offset. With a buffer,
        fills it and returns the number of bytes read.

        Args:
            size: Number of bytes to read (-1 for all remaining). Ignored when
                buffer is provided (then up to len(buffer) bytes are read).
            offset: If given, read from this byte offset without moving the cursor.
                If None (default), read from current position and advance the cursor.
            buffer: If provided, fill this writable buffer and return bytes read (int).
                Use when reusing one buffer in a loop; combine with offset= for random access.

        Returns:
            If buffer is None: bytes. If buffer is provided: int (bytes read).

        Example:
            >>> data = reader.read(4096)
            >>> chunk = reader.read(100, offset=1000)
            >>> n = reader.read(buffer=buf)
            >>> n = reader.read(buffer=buf, offset=0)
        """
        if buffer is not None:
            return self._read_into(buffer, offset=offset)
        rust_size = None if size == -1 else size
        return self._reader.read(rust_size, offset)

    def _read_into(
        self,
        buffer: Union[bytearray, memoryview],
        *,
        offset: Optional[int] = None,
    ) -> int:
        """Read into a writable buffer (private). Used by read() when buffer= is given."""
        if offset is not None:
            return self._reader._read_at_into(offset, buffer)
        return self._reader.readinto(buffer)

    def readinto(self, buffer: Union[bytearray, memoryview]) -> int:
        """Read from current position into buffer. File-like API; returns bytes read."""
        return self.read(buffer=buffer)

    def read_range(self, start: int, end: int) -> bytes:
        """Read byte range [start, end).

        Args:
            start: Starting byte offset (inclusive)
            end: Ending byte offset (exclusive)

        Returns:
            Bytes in the specified range
        """
        return self.read(end - start, offset=start)

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
        meta_dict = self._reader.metadata()
        meta_dict["path"] = self._path
        return Metadata(meta_dict)

    def analyze(self) -> "AnalysisReport":
        """Analyze snapshot for deduplication statistics.

        Returns:
            AnalysisReport with dedup ratio and savings information

        Example:
            >>> with hexz.open("snapshot.hxz") as reader:
            ...     report = reader.analyze()
            ...     print(f"Dedup savings: {report.savings_percent:.1f}%")
        """
        from .utils import AnalysisReport
        import os

        # Use the Rust analyze function on the path
        from . import hexz_loader

        raw_report = hexz_loader.analyze(self._path)
        # Add total_bytes from file size
        file_size = os.path.getsize(self._path)
        raw_report["total_bytes"] = float(file_size)
        return AnalysisReport(raw_report)

    def iter_chunks(self, chunk_size: int = 1024 * 1024) -> Iterator[memoryview]:
        """Iterate over the snapshot in fixed-size chunks using a single reused buffer.

        Uses :meth:`read` with a reused buffer so each chunk does not allocate.

        Args:
            chunk_size: Size of each chunk in bytes (default 1MB).

        Yields:
            :class:`memoryview` of each chunk. **Valid only until the next iteration**
            or until the iterator is advanced; use ``bytes(chunk)`` if you need to
            keep the data (e.g. for ``b"".join(iter_chunks())`` a copy is made).
        """
        buf = bytearray(chunk_size)
        offset = 0
        total = self.size
        while offset < total:
            to_read = min(chunk_size, total - offset)
            n = self.read(buffer=memoryview(buf)[:to_read], offset=offset)
            if n == 0:
                break
            yield memoryview(buf)[:n]
            offset += n

    def close(self) -> None:
        """Close the snapshot and release resources."""
        self._reader.close()

    def __enter__(self):
        """Context manager entry."""
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        """Context manager exit."""
        self.close()

    def __getitem__(self, key: Union[int, slice]) -> bytes:
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
            return self.read(stop - start, offset=start)
        raise TypeError("indices must be slices")

    def __getstate__(self) -> Dict[str, Any]:
        """Support for pickle serialization (path + position)."""
        return {"path": self._path, "position": self.tell()}

    def __setstate__(self, state: Dict[str, Any]) -> None:
        """Support for pickle deserialization."""
        self._path = state["path"]
        self._reader = hexz_loader.Reader(self._path)
        self._reader.seek(state.get("position", 0), 0)

    def __len__(self) -> int:
        """Total size of the snapshot in bytes."""
        return self.size

    def __repr__(self) -> str:
        return f"Reader({self._path!r})"


class AsyncReader:
    """Async reader for Hexz snapshots.

    Use as an async context manager; the snapshot is opened when you enter the context.

    Example:
        >>> async with hexz.AsyncReader("dataset.hxz") as reader:
        ...     data = await reader.read(4096)
        ...     chunk = await reader.read(100, offset=0)
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
        """Create an async reader (opens on entering the context).

        Args:
            path: Path or URL to the snapshot file
            cache_size: Cache size (e.g., "512M", "1G", "2GB"). If None, uses default.
            prefetch: Enable prefetching for sequential reads (default: True)
            s3_region: AWS region for S3 URLs
            endpoint_url: Custom S3 endpoint URL
            allow_restricted: Allow connections to private/internal IPs
        """
        self._path = str(path)
        self._cache_capacity_bytes = (
            _parse_cache_size(cache_size) if cache_size else None
        )
        self._prefetch_count = 4 if prefetch else 0
        self._s3_region = s3_region
        self._endpoint_url = endpoint_url
        self._allow_restricted = allow_restricted
        self._reader: Optional[Any] = None

    async def __aenter__(self) -> "AsyncReader":
        """Open the snapshot; use as async with hexz.AsyncReader(path) as reader."""
        self._reader = await hexz_loader.AsyncReader.create(
            self._path,
            s3_region=self._s3_region,
            endpoint_url=self._endpoint_url,
            allow_restricted=self._allow_restricted,
            prefetch_count=self._prefetch_count,
            cache_capacity_bytes=self._cache_capacity_bytes,
        )
        return self

    async def __aexit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> None:
        """Exit the context (no-op; Rust reader has no explicit close)."""
        pass

    def _ensure_open(self) -> None:
        if self._reader is None:
            raise RuntimeError(
                "AsyncReader must be used as async with hexz.AsyncReader(path) as reader"
            )

    def size(self) -> int:
        """Size of the primary stream in bytes."""
        self._ensure_open()
        return self._reader.size()

    async def read(
        self, size: Optional[int] = None, *, offset: Optional[int] = None
    ) -> bytes:
        """Read bytes. From current position (default) or at a specific offset.

        Args:
            size: Number of bytes to read (None for all remaining).
            offset: If given, read from this byte offset without moving the cursor.
                If None (default), read from current position and advance the cursor.

        Returns:
            Bytes read from the snapshot
        """
        self._ensure_open()
        return await self._reader.read(size, offset)

    async def seek(self, offset: int, whence: int = 0) -> int:
        """Seek to a position. Returns new position."""
        self._ensure_open()
        return await self._reader.seek(offset, whence)

    def tell(self) -> int:
        """Current read position."""
        self._ensure_open()
        return self._reader.tell()

    def __repr__(self) -> str:
        return f"AsyncReader({self._path!r})"


__all__ = ["Reader", "AsyncReader"]


def __dir__():
    return __all__
