"""Utility functions for Strata.

This module provides convenience functions for inspecting, analyzing,
and manipulating Strata snapshots.
"""

from typing import Any, Dict, Optional

from . import strata_loader
from .typing import PathLike

# Format version constants
# These are populated from Rust at module load time
FORMAT_VERSION = strata_loader.get_format_version()
MIN_SUPPORTED_VERSION = strata_loader.get_min_supported_version()
MAX_SUPPORTED_VERSION = strata_loader.get_max_supported_version()


class Metadata:
    """Structured metadata from a Strata snapshot.

    Provides property access to metadata fields with IDE autocomplete support.

    Example:
        >>> meta = strata.inspect("snapshot.st")
        >>> meta.version
        1
        >>> meta.compression
        'lz4'
        >>> meta.disk_size
        1048576
        >>> meta.compression_ratio
        2.5
        >>> print(meta)  # Human-readable output
        >>> meta.print()  # Same as print(meta)
    """

    def __init__(self, data: Dict[str, Any]):
        self._data = data
        self._path = data.get("path", None)

    @property
    def version(self) -> int:
        """Format version number."""
        return self._data.get("version", 0)

    @property
    def compression(self) -> str:
        """Compression algorithm used."""
        return self._data.get("compression", "unknown")

    @property
    def disk_size(self) -> int:
        """Size of disk data in bytes."""
        return self._data.get("disk_size", 0)

    @property
    def memory_size(self) -> int:
        """Size of memory data in bytes."""
        return self._data.get("memory_size", 0)

    @property
    def size_compressed(self) -> int:
        """Total compressed size in bytes."""
        return self._data.get("file_size", 0)

    @property
    def block_size(self) -> int:
        """Block size used during creation."""
        return self._data.get("block_size", 0)

    @property
    def num_blocks(self) -> int:
        """Number of blocks in the snapshot."""
        return self._data.get("num_blocks", 0)

    @property
    def encrypted(self) -> bool:
        """Whether the snapshot is encrypted."""
        return self._data.get("encrypted", False)

    @property
    def signed(self) -> bool:
        """Whether the snapshot is signed."""
        return self._data.get("signed", False)

    @property
    def has_disk(self) -> bool:
        """Whether snapshot contains disk data."""
        return self.disk_size > 0

    @property
    def has_memory(self) -> bool:
        """Whether snapshot contains memory data."""
        return self.memory_size > 0

    @property
    def is_compatible(self) -> bool:
        """Whether this snapshot can be read by the current version."""
        return self._data.get("is_compatible", False)

    @property
    def compatibility_status(self) -> str:
        """Version compatibility status: 'full', 'degraded', or 'incompatible'."""
        return self._data.get("compatibility_status", "unknown")

    @property
    def compatibility_message(self) -> str:
        """Human-readable description of version compatibility."""
        return self._data.get("compatibility_message", "Unknown compatibility")

    @property
    def compression_ratio(self) -> float:
        """Compression ratio (uncompressed / compressed)."""
        return self._data.get("ratio", 0.0)

    def __getitem__(self, key: str) -> Any:
        """Dict-like access for any metadata key (e.g. parent_path for thin snapshots)."""
        return self._data[key]

    def __repr__(self) -> str:
        return f"Metadata(version={self.version}, compression={self.compression!r})"

    def __str__(self) -> str:
        """Human-readable snapshot information."""
        lines = []
        if self._path:
            lines.append(f"Strata Snapshot: {self._path}")
        else:
            lines.append("Strata Snapshot")
        lines.append(f"  Version: {self.version}")
        lines.append(f"  Compression: {self.compression}")
        if self.has_disk:
            lines.append(f"  Disk size: {self.disk_size:,} bytes")
        if self.has_memory:
            lines.append(f"  Memory size: {self.memory_size:,} bytes")
        if self.size_compressed > 0:
            lines.append(f"  Compressed: {self.size_compressed:,} bytes")
            if self.compression_ratio > 0:
                lines.append(f"  Ratio: {self.compression_ratio:.2f}x")
        lines.append(f"  Block size: {self.block_size:,} bytes")
        lines.append(f"  Blocks: {self.num_blocks:,}")
        lines.append(f"  Encrypted: {'Yes' if self.encrypted else 'No'}")
        lines.append(f"  Signed: {'Yes' if self.signed else 'No'}")
        return "\n".join(lines)

    def print(self) -> None:
        """Print human-readable snapshot information to stdout.

        Example:
            >>> meta = strata.inspect("snapshot.st")
            >>> meta.print()
            Strata Snapshot: snapshot.st
              Version: 1
              Compression: lz4
              ...
        """
        print(str(self))

    @staticmethod
    def diff(path1: PathLike, path2: PathLike) -> Dict[str, Any]:
        """Compare two snapshots and show differences.

        Args:
            path1: Path to first snapshot (base)
            path2: Path to second snapshot or overlay file

        Returns:
            Dictionary containing diff information

        Example:
            >>> diff_info = strata.Metadata.diff("base.st", "updated.st")
            >>> print(f"Changed blocks: {diff_info['changed_blocks']}")

        Note:
            If path2 is an overlay file (.bin), analyzes overlay metadata.
            If both are snapshots, returns basic comparison.
        """
        # Check if path2 is an overlay file or snapshot
        path2_str = str(path2)
        if path2_str.endswith(".bin"):
            # This is an overlay file, use the Rust diff function
            return strata_loader.diff(path2_str)
        else:
            # Both are snapshots, do basic metadata comparison
            meta1 = inspect(path1)
            meta2 = inspect(path2)
            return {
                "size_diff": meta2.disk_size - meta1.disk_size,
                "compression_same": meta1.compression == meta2.compression,
                "version_same": meta1.version == meta2.version,
            }


# merge_overlay has been moved to Writer.merge_overlay()
# Old top-level function removed in v0.1.0-beta
# Re-added for backward compatibility


def merge_overlay(
    base: PathLike, overlay: PathLike, output: PathLike, thin: bool = False
) -> None:
    """Merge an overlay file into a new snapshot.

    Args:
        base: Path to base snapshot
        overlay: Path to overlay file (.bin)
        output: Path to output snapshot
        thin: If True, create a thin snapshot that references the base

    Example:
        >>> strata.merge_overlay("base.st", "overlay.bin", "merged.st")
        >>> # Or thin snapshot:
        >>> strata.merge_overlay("base.st", "overlay.bin", "thin.st", thin=True)
    """
    from .writer import Writer

    with Writer(output) as writer:
        writer.merge_overlay(base=base, overlay=overlay, thin=thin)


def inspect(path: PathLike) -> Metadata:
    """Inspect a Strata snapshot and return structured metadata.

    Args:
        path: Path to .st file

    Returns:
        Metadata object with snapshot information

    Example:
        >>> meta = strata.inspect("snapshot.st")
        >>> print(f"Version: {meta.version}")
        >>> print(f"Compression: {meta.compression}")
        >>> print(f"Size: {meta.disk_size:,} bytes")
        >>> print(meta)  # Human-readable output
        >>> meta.print()  # Same as above
    """
    raw_meta = strata_loader.inspect(str(path))
    raw_meta["path"] = str(path)  # Store path for display
    return Metadata(raw_meta)


class AnalysisReport:
    """Deduplication analysis report.

    Provides property access to analysis results.
    Obtained via :meth:`Reader.analyze`.
    """

    def __init__(self, data: Dict[str, Any]):
        self._data = data

    @property
    def unique_bytes(self) -> int:
        """Number of unique bytes after deduplication."""
        return self._data.get("unique_bytes", 0)

    @property
    def total_bytes(self) -> int:
        """Total bytes analyzed."""
        return self._data.get("total_bytes", 0)

    @property
    def predicted_ratio(self) -> float:
        """Predicted deduplication ratio."""
        return self._data.get("predicted_ratio", 1.0)

    @property
    def dedup_ratio(self) -> float:
        """Deduplication ratio (< 1.0 means savings)."""
        ratio = self.unique_bytes / self.total_bytes if self.total_bytes > 0 else 1.0
        return ratio

    @property
    def savings_percent(self) -> float:
        """Percentage of space saved by deduplication."""
        return (1.0 - self.dedup_ratio) * 100.0

    def __repr__(self) -> str:
        return f"AnalysisReport(dedup_ratio={self.dedup_ratio:.2f}, savings={self.savings_percent:.1f}%)"


# analyze() has been moved to Reader.analyze()
# diff() has been moved to Metadata.diff()
# Old top-level functions removed in v0.1.0-beta
# Re-added analyze() for backward compatibility


def analyze(path: PathLike) -> AnalysisReport:
    """Analyze a file for deduplication potential.

    Args:
        path: Path to file to analyze

    Returns:
        AnalysisReport with deduplication statistics

    Example:
        >>> report = strata.analyze("dataset.tar")
        >>> print(f"Predicted ratio: {report.predicted_ratio:.2f}x")
        >>> print(f"Savings: {report.savings_percent:.1f}%")
    """
    from . import strata_loader
    import os

    raw_report = strata_loader.analyze(str(path))
    # Add total_bytes from file size
    file_size = os.path.getsize(str(path))
    raw_report["total_bytes"] = float(file_size)
    return AnalysisReport(raw_report)


def verify(
    path: PathLike,
    *,
    checksum: bool = True,
    structure: bool = True,
    public_key: Optional[PathLike] = None,
) -> bool:
    """Verify snapshot integrity and optionally signature.

    Performs structural validation, checksum verification, and optional
    cryptographic signature verification.

    Args:
        path: Path to snapshot to verify
        checksum: Verify block checksums by reading entire file
        structure: Verify file structure (header and index)
        public_key: Optional path to public key for signature verification

    Returns:
        True if all checks pass, False otherwise

    Example:
        >>> # Basic integrity check
        >>> valid = strata.verify("snapshot.st")
        ...
        >>> # With signature verification
        >>> valid = strata.verify("snapshot.st", public_key="key.pub")
        >>> if not valid:
        ...     print("Snapshot verification failed!")
    """
    # Signature verification if public key provided
    if public_key:
        from . import crypto

        if not crypto.verify(path, public_key):
            return False

    # Checksum and/or structure verification
    try:
        if structure:
            inspect(path)  # validates header and index can be read
        if checksum:
            # Read through entire snapshot so every block is verified on read
            from .reader import Reader

            with Reader(path) as reader:
                for _ in reader.iter_chunks(chunk_size=256 * 1024):
                    pass
    except Exception:
        return False

    return True


# info() has been moved to Metadata.print() or str(Metadata)
# Old top-level function removed in v0.1.0-beta
