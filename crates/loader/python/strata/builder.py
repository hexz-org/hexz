"""Snapshot builder API for creating Strata archives programmatically.

This module provides a high-level Python interface for building Strata
snapshots incrementally, supporting:
- Disk and memory stream addition
- Overlay merging (thin snapshots)
- Context manager pattern for automatic finalization
"""

from ._strata_core import StrataBuilder as _StrataBuilder


class SnapshotBuilder:
    """Context manager for creating new Strata snapshots incrementally.

    This class wraps the Rust StrataBuilder to provide a Pythonic interface
    for snapshot creation. It automatically handles finalization when used
    as a context manager.

    Args:
        output_path: Path to the output .st file
        block_size: Block size in bytes (default: 64KB)
        compression: Compression algorithm - "lz4" or "zstd" (default: "lz4")

    Example:
        >>> with SnapshotBuilder("output.st") as builder:
        ...     builder.add_disk("disk.img")
        ...     builder.add_memory("memory.dump")

        >>> # Or without context manager:
        >>> builder = SnapshotBuilder("output.st", compression="zstd")
        >>> builder.add_disk("disk.img")
        >>> builder.finalize()
    """

    def __init__(
        self, output_path: str, block_size: int = 65536, compression: str = "lz4"
    ):
        self.builder = _StrataBuilder(output_path, block_size, compression)

    def add_disk(self, path: str):
        """Add a disk image file to the snapshot.

        Args:
            path: Path to the disk image file
        """
        self.builder.add_disk_file(path)

    def add_memory(self, path: str):
        """Add a memory dump file to the snapshot.

        Args:
            path: Path to the memory dump file
        """
        self.builder.add_memory_file(path)

    def merge_overlay(self, base_path: str, overlay_path: str, thin: bool = False):
        """Merge an overlay file with a base snapshot.

        Args:
            base_path: Path to the base snapshot
            overlay_path: Path to the overlay (COW) file
            thin: If True, create a thin snapshot with parent reference
        """
        self.builder.merge_overlay(base_path, overlay_path, thin)

    def finalize(self):
        """Finalize the snapshot and write all metadata.

        This method must be called to complete snapshot creation. It:
        - Writes the master index
        - Updates the header
        - Flushes all buffers

        Note:
            Automatically called when using context manager pattern.
        """
        self.builder.finalize()

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        if exc_type is None:
            self.finalize()
