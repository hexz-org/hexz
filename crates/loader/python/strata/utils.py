"""Utility functions for Strata.

This module provides convenience functions for inspecting, analyzing,
and manipulating Strata snapshots.
"""

from typing import Any, Dict, Optional
from pathlib import Path

from . import _strata_core
from .typing import PathLike

# Format version constants
# These are populated from Rust at module load time
FORMAT_VERSION = _strata_core.get_format_version()
MIN_SUPPORTED_VERSION = _strata_core.get_min_supported_version()
MAX_SUPPORTED_VERSION = _strata_core.get_max_supported_version()


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
    """

    def __init__(self, data: Dict[str, Any]):
        self._data = data

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


def merge_overlay(
    base_path: PathLike,
    overlay_path: PathLike,
    output_path: PathLike,
    *,
    thin: bool = False,
    block_size: int = 65536,
    compression: str = "lz4",
) -> None:
    """Merge a copy-on-write overlay with a base snapshot into a new snapshot.

    Args:
        base_path: Path to the base .st snapshot
        overlay_path: Path to the overlay data file
        output_path: Path for the output .st snapshot
        thin: If True, create a thin snapshot that references the base
        block_size: Block size for the output (default 64K)
        compression: Compression algorithm (default lz4)

    Example:
        >>> strata.merge_overlay("base.st", "overlay.bin", "merged.st")
        >>> # Thin snapshot (references base for unmodified blocks)
        >>> strata.merge_overlay("base.st", "overlay.bin", "thin.st", thin=True)
    """
    builder = _strata_core.StrataBuilder(
        str(output_path),
        block_size=block_size,
        compression=compression,
        compression_level=None,
    )
    builder.merge_overlay(str(base_path), str(overlay_path), thin)
    builder.finalize()


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
    """
    raw_meta = _strata_core.inspect(str(path))
    return Metadata(raw_meta)


class AnalysisReport:
    """Deduplication analysis report.

    Provides property access to analysis results.
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


def analyze(path: PathLike) -> AnalysisReport:
    """Analyze a file for deduplication potential.

    Args:
        path: Path to file to analyze

    Returns:
        AnalysisReport with dedup statistics

    Example:
        >>> report = strata.analyze("data.img")
        >>> print(f"Dedup ratio: {report.dedup_ratio:.2f}x")
        >>> print(f"Savings: {report.savings_percent:.1f}%")
    """
    raw_report = _strata_core.analyze(str(path))
    return AnalysisReport(raw_report)


def diff(path1: PathLike, path2: PathLike) -> Dict[str, Any]:
    """Compare two snapshots and show differences.

    Args:
        path1: Path to first snapshot
        path2: Path to second snapshot

    Returns:
        Dictionary containing diff information

    Example:
        >>> diff_info = strata.diff("base.st", "updated.st")
        >>> print(f"Changed blocks: {diff_info['changed_blocks']}")
    """
    return _strata_core.diff(str(path1), str(path2))


def verify(
    path: PathLike,
    *,
    checksum: bool = True,
    structure: bool = True,
    public_key: Optional[str] = None,
    signature: Optional[str] = None,
) -> bool:
    """Verify snapshot integrity.

    Args:
        path: Path to snapshot to verify
        checksum: Verify block checksums
        structure: Verify file structure
        public_key: Public key for signature verification
        signature: Path to signature file

    Returns:
        True if all checks pass

    Example:
        >>> valid = strata.verify("snapshot.st", public_key="...")
        >>> if not valid:
        ...     print("Snapshot verification failed!")
    """
    if public_key and signature:
        return _strata_core.verify_image(str(path), public_key, signature)
    elif public_key:
        return _strata_core.verify_image(str(path), public_key)
    else:
        # Checksum and/or structure verification (no signature)
        try:
            if structure:
                inspect(path)  # validates header and index can be read
            if checksum:
                # Read through entire snapshot so every block is verified on read
                import strata as _strata

                with _strata.open(path) as reader:
                    for _ in reader.iter_chunks(chunk_size=256 * 1024):
                        pass
        except Exception:
            return False
        return True


def info(path: PathLike) -> None:
    """Print human-readable snapshot information.

    Args:
        path: Path to snapshot

    Example:
        >>> strata.info("snapshot.st")
        Strata Snapshot: snapshot.st
          Version: 1
          Size: 1,234,567 bytes
          Compression: lz4
          Encrypted: Yes
    """
    meta = inspect(path)
    print(f"Strata Snapshot: {path}")
    print(f"  Version: {meta.version}")
    print(f"  Compression: {meta.compression}")
    if meta.has_disk:
        print(f"  Disk size: {meta.disk_size:,} bytes")
    if meta.has_memory:
        print(f"  Memory size: {meta.memory_size:,} bytes")
    if meta.size_compressed > 0:
        print(f"  Compressed: {meta.size_compressed:,} bytes")
    print(f"  Block size: {meta.block_size:,} bytes")
    print(f"  Blocks: {meta.num_blocks:,}")
    print(f"  Encrypted: {'Yes' if meta.encrypted else 'No'}")
    print(f"  Signed: {'Yes' if meta.signed else 'No'}")
