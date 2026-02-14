"""Type stubs for Rust-implemented core functionality.

This file provides type hints for the _hexz_core extension module
implemented in Rust using PyO3.
"""

from typing import Any, Dict, Optional

class Reader:
    """Low-level reader for Hexz snapshots (Rust implementation)."""

    def __init__(
        self,
        path: str,
        s3_region: Optional[str] = None,
        endpoint_url: Optional[str] = None,
        allow_restricted: bool = False,
    ) -> None:
        """Open a Hexz snapshot for reading.

        Args:
            path: Path or URL to the snapshot file
            s3_region: AWS region for S3 URLs
            endpoint_url: Custom S3 endpoint URL
            allow_restricted: Allow connections to private/internal IPs
        """
        ...

    def read_at(self, offset: int, length: int) -> bytes:
        """Read bytes at a specific offset without moving cursor.

        Args:
            offset: Byte offset to read from
            length: Number of bytes to read

        Returns:
            Bytes read from the snapshot
        """
        ...

    def _read_at_into(self, offset: int, buffer: bytearray) -> int:
        """Read at offset into a writable buffer. Returns bytes read. Buffer need not be zeroed.
        On OSError, buffer contents are undefined and must not be read."""
        ...

    def seek(self, offset: int, whence: int = 0) -> int:
        """Seek to a position in the file.

        Args:
            offset: Offset to seek to
            whence: 0 (absolute), 1 (relative), 2 (from end)

        Returns:
            New absolute position
        """
        ...

    def tell(self) -> int:
        """Get current position in the file.

        Returns:
            Current byte offset
        """
        ...

    def read(self, size: int = -1) -> bytes:
        """Read bytes from current position and advance cursor.

        Args:
            size: Number of bytes to read (-1 for all)

        Returns:
            Bytes read from the snapshot
        """
        ...

    def readinto(self, buffer: bytearray) -> int:
        """Read into a pre-allocated buffer.

        Args:
            buffer: Buffer to read into

        Returns:
            Number of bytes read
        """
        ...

    def size(self) -> int:
        """Get total size of the snapshot.

        Returns:
            Total size in bytes
        """
        ...

    def metadata(self) -> Dict[str, Any]:
        """Get snapshot metadata.

        Returns:
            Dictionary containing metadata (version, compression, sizes, etc.)
        """
        ...

    def close(self) -> None:
        """Close the snapshot and release resources."""
        ...

    def __getstate__(self) -> Dict[str, Any]:
        """Support for pickle serialization."""
        ...

    def __setstate__(self, state: Dict[str, Any]) -> None:
        """Support for pickle deserialization."""
        ...

class AsyncReader:
    """Async reader for Hexz snapshots (Rust implementation)."""

    def __init__(
        self,
        path: str,
        s3_region: Optional[str] = None,
        endpoint_url: Optional[str] = None,
        allow_restricted: bool = False,
    ) -> None:
        """Open a Hexz snapshot for async reading.

        Args:
            path: Path or URL to the snapshot file
            s3_region: AWS region for S3 URLs
            endpoint_url: Custom S3 endpoint URL
            allow_restricted: Allow connections to private/internal IPs
        """
        ...

    async def read_at(self, offset: int, length: int) -> bytes:
        """Async read bytes at a specific offset.

        Args:
            offset: Byte offset to read from
            length: Number of bytes to read

        Returns:
            Bytes read from the snapshot
        """
        ...

    async def read(self, size: int = -1) -> bytes:
        """Async read bytes from current position.

        Args:
            size: Number of bytes to read (-1 for all)

        Returns:
            Bytes read from the snapshot
        """
        ...

    async def close(self) -> None:
        """Async close the snapshot and release resources."""
        ...

class Builder:
    """Low-level builder for creating Hexz snapshots (Rust implementation)."""

    def __init__(
        self,
        output_path: str,
        block_size: int = 65536,
        compression: str = "lz4",
        compression_level: Optional[int] = None,
    ) -> None:
        """Create a new snapshot builder.

        Args:
            output_path: Path to output .hxz file
            block_size: Block size in bytes
            compression: Compression algorithm ("lz4" or "zstd")
            compression_level: Compression level (algorithm-specific, optional)
        """
        ...

    def add_disk_file(self, path: str) -> None:
        """Add a disk image file to the snapshot.

        Args:
            path: Path to disk image file
        """
        ...

    def add_memory_file(self, path: str) -> None:
        """Add a memory dump file to the snapshot.

        Args:
            path: Path to memory dump file
        """
        ...

    def merge_overlay(
        self, base_path: str, overlay_path: str, thin: bool = False
    ) -> None:
        """Merge an overlay file with a base snapshot.

        Args:
            base_path: Path to base snapshot
            overlay_path: Path to overlay (COW) file
            thin: Create thin snapshot with parent reference
        """
        ...

    def finalize(self) -> None:
        """Finalize the snapshot and write all metadata."""
        ...

def pack(
    disk: str,
    output: str,
    compression: str = "lz4",
    encrypt: bool = False,
    password: Optional[str] = None,
    dedup: bool = True,
    min_chunk: int = 16384,
    avg_chunk: int = 65536,
    max_chunk: int = 131072,
) -> None:
    """Pack a disk image into a Hexz snapshot.

    Args:
        disk: Path to input disk image
        output: Path to output .hxz file
        compression: Compression algorithm ("lz4" or "zstd")
        encrypt: Enable encryption
        password: Encryption password
        dedup: Enable deduplication
        min_chunk: Minimum CDC chunk size
        avg_chunk: Average CDC chunk size
        max_chunk: Maximum CDC chunk size
    """
    ...

def inspect(path: str) -> Dict[str, Any]:
    """Inspect a Hexz snapshot and return metadata.

    Args:
        path: Path to .hxz file

    Returns:
        Dictionary containing metadata (version, compression, sizes, etc.)
    """
    ...

def analyze(path: str) -> Dict[str, Any]:
    """Analyze a file for deduplication potential.

    Args:
        path: Path to file to analyze

    Returns:
        Dictionary containing dedup statistics
    """
    ...

def diff(path1: str, path2: str) -> Dict[str, Any]:
    """Compare two snapshots and show differences.

    Args:
        path1: Path to first snapshot
        path2: Path to second snapshot

    Returns:
        Dictionary containing diff information
    """
    ...

def sign_image(path: str, private_key: str, output: Optional[str] = None) -> None:
    """Sign a snapshot with Ed25519 private key.

    Args:
        path: Path to snapshot to sign
        private_key: Ed25519 private key
        output: Path to signature file (optional)
    """
    ...

def verify_image(path: str, public_key: str, signature: Optional[str] = None) -> bool:
    """Verify a snapshot signature with Ed25519 public key.

    Args:
        path: Path to snapshot to verify
        public_key: Ed25519 public key
        signature: Path to signature file (optional)

    Returns:
        True if signature is valid
    """
    ...

def snapshot_vm(qmp_socket: str, output: str) -> None:
    """Take a live snapshot of a running VM via QMP.

    Args:
        qmp_socket: Path to QMP socket
        output: Path to output snapshot file
    """
    ...
