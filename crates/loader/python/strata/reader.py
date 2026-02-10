"""Pythonic reader interface for Strata snapshots.

This module provides the high-level Reader and AsyncReader classes that wrap
the Rust-implemented StrataReader with a more pythonic interface.
"""

from typing import Optional, Any, Dict, Union
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
        ...     # Zero-copy into buffer
        ...     buf = bytearray(4096)
        ...     n = reader.read(buffer=buf)
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

    def read(
        self,
        size: int = -1,
        *,
        buffer: Optional[Union[bytearray, memoryview]] = None,
    ) -> Union[bytes, int]:
        """Read from current position and advance cursor.

        With no buffer, returns bytes (may allocate). With a buffer, fills it
        (zero-copy) and returns the number of bytes read.

        Args:
            size: Number of bytes to read (-1 for all remaining). Ignored when
                buffer is provided (then up to len(buffer) bytes are read).
            buffer: If provided, a writable buffer (e.g. bytearray) to fill.
                Uses the zero-copy backend; returns number of bytes read (int).

        Returns:
            If buffer is None: bytes read. If buffer is provided: int (bytes read).

        Example:
            >>> data = reader.read(4096)
            >>> n = reader.read(buffer=bytearray(4096))
        """
        if buffer is not None:
            return self._reader.readinto(buffer)
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

        Uses a single buffer and read(buffer=...) for zero-copy reads.

        Args:
            chunk_size: Size of each chunk in bytes (default 1MB)

        Yields:
            Bytes chunks from the snapshot
        """
        buf = bytearray(chunk_size)
        offset = 0
        total = self.size
        while offset < total:
            to_read = min(chunk_size, total - offset)
            self.seek(offset)
            # Slice of memoryview so read(buffer=...) writes into buf (bytearray[:] is a copy)
            n = self.read(buffer=memoryview(buf)[:to_read])
            if n == 0:
                break
            yield bytes(buf[:n])
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
        """Support for pickle serialization (path + position)."""
        return {"path": self._path, "position": self.tell()}

    def __setstate__(self, state: Dict[str, Any]) -> None:
        """Support for pickle deserialization."""
        self._path = state["path"]
        self._reader = _strata_core.StrataReader(self._path)
        self._reader.seek(state.get("position", 0), 0)

    def __repr__(self) -> str:
        return f"Reader({self._path!r})"


class AsyncReader:
    """Async reader for Strata snapshots.

    Use as an async context manager; the snapshot is opened when you enter the context.

    Example:
        >>> async with strata.AsyncReader("dataset.st") as reader:
        ...     data = await reader.read(4096)
        ...     chunk = await reader.read_at(0, 100)
    """

    def __init__(
        self,
        path: PathLike,
        *,
        s3_region: Optional[str] = None,
        endpoint_url: Optional[str] = None,
        allow_restricted: bool = False,
    ):
        """Create an async reader (opens on entering the context).

        Args:
            path: Path or URL to the snapshot file
            s3_region: AWS region for S3 URLs
            endpoint_url: Custom S3 endpoint URL
            allow_restricted: Allow connections to private/internal IPs
        """
        self._path = str(path)
        self._s3_region = s3_region
        self._endpoint_url = endpoint_url
        self._allow_restricted = allow_restricted
        self._reader: Optional[Any] = None

    async def __aenter__(self) -> "AsyncReader":
        """Open the snapshot; use as async with strata.AsyncReader(path) as reader."""
        self._reader = await _strata_core.AsyncStrataReader.create(
            self._path,
            s3_region=self._s3_region,
            endpoint_url=self._endpoint_url,
            allow_restricted=self._allow_restricted,
        )
        return self

    async def __aexit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> None:
        """Exit the context (no-op; Rust reader has no explicit close)."""
        pass

    def _ensure_open(self) -> None:
        if self._reader is None:
            raise RuntimeError(
                "AsyncReader must be used as async with strata.AsyncReader(path) as reader"
            )

    def size(self) -> int:
        """Size of the disk stream in bytes."""
        self._ensure_open()
        return self._reader.size()

    async def read(self, size: Optional[int] = None) -> bytes:
        """Read bytes from current position.

        Args:
            size: Number of bytes to read (None for all remaining)

        Returns:
            Bytes read from the snapshot
        """
        self._ensure_open()
        return await self._reader.read(size)

    async def read_at(self, offset: int, length: int) -> bytes:
        """Read bytes at a specific offset.

        Args:
            offset: Byte offset to read from
            length: Number of bytes to read

        Returns:
            Bytes read from the snapshot
        """
        self._ensure_open()
        return await self._reader.read_at(offset, length)

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
