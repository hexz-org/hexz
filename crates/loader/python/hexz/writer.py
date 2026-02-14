"""Pythonic writer interface for creating Hexz snapshots.

This module provides the high-level Writer class that wraps the Rust-implemented
Builder with a more pythonic interface.
"""

from typing import Optional, Any, Dict, Literal
from pathlib import Path
import tempfile
import warnings
import json

from . import hexz_loader
from .exceptions import ValidationError
from .typing import PathLike, PackingMode

# Compression level mappings for different modes
COMPRESSION_LEVELS = {
    "fast": {"lz4": None, "zstd": 1},  # Fast compression
    "balanced": {"lz4": None, "zstd": 3},  # Balanced (default)
    "tight": {"lz4": None, "zstd": 9},  # Maximum compression
}


class Writer:
    """High-level writer for creating Hexz snapshots with pythonic interface.

    Provides a fluent API for building snapshots with automatic finalization
    via context managers.

    Example:
        >>> with hexz.Writer("output.hxz", compression="zstd") as writer:
        ...     writer.add("disk.img")
        ...     writer.add_metadata({"created": "2026-02-09"})
        ... # Automatically finalized on exit
    """

    def __init__(
        self,
        path: PathLike,
        *,
        compression: Literal["lz4", "zstd"] = "lz4",
        mode: Optional[PackingMode] = None,
        packing: Optional[PackingMode] = None,
        block_size: int = 64 * 1024,
        dedup: bool = True,
        cdc: bool = False,
        encrypt: bool = False,
        password: Optional[str] = None,
    ):
        """Create a new snapshot writer.

        Args:
            path: Path to output .st file
            compression: Compression algorithm ("lz4" or "zstd")
            mode: Packing mode alias for 'packing' (for backward compatibility)
            packing: Packing mode - "fast", "balanced", or "tight"
                  Controls compression level and speed tradeoff
            block_size: Block size in bytes
            dedup: Enable deduplication
            cdc: Enable content-defined chunking (requires dedup=True)
            encrypt: Enable encryption (Note: Use hexz.pack() for encryption support)
            password: Encryption password (required if encrypt=True)

        Example:
            >>> # Fast compression (good for temporary files)
            >>> with hexz.Writer("temp.hxz", packing="fast") as w:
            ...     w.add("data.img")
            >>>
            >>> # Tight compression (good for archival)
            >>> with hexz.Writer("archive.hxz", packing="tight", compression="zstd") as w:
            ...     w.add("data.img")
        """
        self._path = str(path)
        self._compression = compression

        # Determine packing mode (prefer 'packing' over 'mode')
        resolved_mode = packing or mode or "balanced"
        self._mode = resolved_mode
        self._dedup = dedup
        self._cdc = cdc
        self._encrypt = encrypt
        self._password = password
        self._metadata = {}

        # Map packing mode to compression level
        if resolved_mode not in COMPRESSION_LEVELS:
            raise ValidationError(
                f"Invalid packing mode: {resolved_mode}. "
                f"Choose from: {list(COMPRESSION_LEVELS.keys())}"
            )

        compression_level = COMPRESSION_LEVELS[resolved_mode].get(compression)

        # Create underlying builder
        self._builder = hexz_loader.Builder(
            self._path,
            block_size=block_size,
            compression=compression,
            compression_level=compression_level,
            dedup=dedup,
            cdc=cdc,
        )

        # Note: Writer currently supports basic linear writing.
        # For encryption support, use hexz.pack()
        # which utilizes the more advanced PackConfig pipeline.

        if encrypt:
            warnings.warn(
                "Encryption in Writer is not yet implemented. "
                "Use hexz.pack() for encryption support.",
                UserWarning,
            )

        self._finalized = False
        self._bytes_written = 0

    def add(self, source: Any, *, kind: Optional[str] = None) -> "Writer":
        """Add any source to the snapshot (fluent API).

        Dispatches to specific methods based on source type:
        - str/Path: add_file()
        - bytes: add_bytes()
        - numpy array: add_array()

        Args:
            source: Source to add (file path, bytes, array, etc.)
            kind: Optional hint about source type ("disk", "memory", etc.)

        Returns:
            Self for method chaining
        """
        if isinstance(source, (str, Path)):
            self.add_file(str(source), kind=kind)
        elif isinstance(source, bytes):
            self.add_bytes(source)
        else:
            # Try numpy array
            if hasattr(source, "__array_interface__"):
                self.add_array(source)
            else:
                raise ValidationError(f"Cannot add source of type {type(source)}")

        return self

    def add_file(
        self, path: PathLike, *, kind: Optional[str] = None, **kwargs: Any
    ) -> "Writer":
        """Add a file to the snapshot.

        Args:
            path: Path to file
            kind: Optional kind hint ("disk", "memory", etc.)
            **kwargs: Extra arguments (e.g., name) - currently ignored

        Returns:
            Self for method chaining
        """
        path_str = str(path)

        # Dispatch based on kind
        if kind == "disk" or kind is None:
            self._builder.add_disk_file(path_str)
        elif kind == "memory":
            self._builder.add_memory_file(path_str)
        else:
            raise ValidationError(f"Unknown kind: {kind}")

        return self

    def add_bytes(self, data: bytes, **kwargs: Any) -> "Writer":
        """Add raw bytes to the snapshot.

        Args:
            data: Bytes to add
            **kwargs: Extra arguments (e.g., name) - currently ignored

        Returns:
            Self for method chaining

        .. warning::
            **Performance Note:** This method currently writes the bytes to a
            temporary file on disk before adding them to the snapshot. This
            incurs significant I/O overhead. For large datasets, prefer using
            `add_file()` with existing files.
        """
        # Write to temporary file and add it
        # This is not ideal but works with current Rust API
        with tempfile.NamedTemporaryFile(delete=False) as f:
            temp_path = f.name
            f.write(data)

        try:
            self.add_file(temp_path)
        finally:
            # Clean up temp file
            try:
                import os

                os.unlink(temp_path)
            except Exception:
                pass

        return self

    def add_array(
        self,
        array: Any,
        *,
        offset: Optional[int] = None,
        name: Optional[str] = None,
        **kwargs: Any,
    ) -> "Writer":
        """Add a NumPy array to the snapshot.

        Args:
            array: NumPy array to add
            offset: Optional byte offset (currently ignored)
            name: Optional name for the array (currently ignored)
            **kwargs: Extra arguments - currently ignored

        Returns:
            Self for method chaining

        Note:
            Current implementation converts array to bytes.
            Named arrays and metadata require Rust support (TODO).
        """
        # Check if numpy is available
        try:
            import numpy as np
        except ImportError:
            raise ImportError("NumPy is required for add_array()")

        # Ensure contiguous array
        if not array.flags["C_CONTIGUOUS"]:
            warnings.warn(
                "Array is not C-contiguous, making a copy.",
                UserWarning,
            )
            array = np.ascontiguousarray(array)

        # Convert to bytes and add
        data_bytes = array.tobytes()
        return self.add_bytes(data_bytes)

    def add_metadata(self, metadata: Dict[str, Any]) -> "Writer":
        """Add custom metadata to the snapshot.

        Args:
            metadata: Dictionary of metadata

        Returns:
            Self for method chaining
        """
        # Store metadata in-memory
        if not hasattr(self, "_metadata"):
            self._metadata = {}

        self._metadata.update(metadata)
        return self

    def write(self, data: bytes, *, offset: Optional[int] = None) -> int:
        """Write bytes at a specific offset.

        Args:
            data: Bytes to write
            offset: Optional byte offset (currently ignored)

        Returns:
            Number of bytes written

        Note:
            Current implementation appends data sequentially.
            Offset parameter is accepted for API compatibility but not yet used.
            Requires Rust support for random-access writing.
        """
        if offset is not None:
            warnings.warn(
                "Random-access writing not yet supported. "
                "Data will be appended sequentially.",
                UserWarning,
            )

        self.add_bytes(data)
        return len(data)

    @property
    def bytes_written(self) -> int:
        """Total bytes written so far."""
        return self._builder.get_bytes_written()

    def tell(self) -> int:
        """Get current write position."""
        return self.bytes_written

    def merge_overlay(
        self,
        *,
        base: PathLike,
        overlay: PathLike,
        thin: bool = False,
    ) -> "Writer":
        """Merge a copy-on-write overlay with a base snapshot.

        Args:
            base: Path to the base .st snapshot
            overlay: Path to the overlay data file
            thin: If True, create a thin snapshot that references the base

        Returns:
            Self for method chaining

        Example:
            >>> with hexz.Writer("merged.hxz") as writer:
            ...     writer.merge_overlay(base="base.hxz", overlay="overlay.img")
            ...
            >>> # Thin snapshot (references base for unmodified blocks)
            >>> with hexz.Writer("thin.hxz") as writer:
            ...     writer.merge_overlay(base="base.hxz", overlay="overlay.img", thin=True)
        """
        self._builder.merge_overlay(str(base), str(overlay), thin)
        return self

    def finalize(self) -> None:
        """Finalize the snapshot and write all metadata.

        This must be called to complete snapshot creation. It:
        - Writes the master index
        - Updates the header
        - Flushes all buffers
        """
        if not self._finalized:
            if self._metadata:
                try:
                    json_bytes = json.dumps(self._metadata).encode("utf-8")
                    self._builder.set_metadata(json_bytes)
                except (TypeError, ValueError) as e:
                    warnings.warn(
                        f"Failed to serialize metadata to JSON: {e}. "
                        "Metadata will not be persisted.",
                        UserWarning,
                    )

            self._builder.finalize()
            self._finalized = True

    def __enter__(self):
        """Context manager entry."""
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        """Context manager exit with automatic finalization."""
        if exc_type is None and not self._finalized:
            self.finalize()

    def __repr__(self) -> str:
        return f"Writer({self._path!r}, compression={self._compression!r})"
