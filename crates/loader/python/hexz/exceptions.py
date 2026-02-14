"""Custom exception hierarchy for Hexz.

This module defines all exceptions that can be raised by the Hexz library,
providing clear error types for different failure modes.
"""

from typing import Optional


class Error(Exception):
    """Base exception for all Hexz-related errors.

    All custom exceptions in the Hexz library inherit from this base class,
    making it easy to catch all Hexz-specific errors with a single except clause.
    """

    pass


class IOError(Error, OSError):
    """I/O operation failed.

    Raised when reading from or writing to storage fails. This includes:
    - File system errors
    - Network errors (HTTP, S3)
    - Device errors

    Inherits from both Error and OSError for compatibility with
    standard Python error handling.
    """

    pass


class NetworkError(IOError):
    """Network-related I/O error.

    Raised for failures in remote storage backends:
    - HTTP connection failures
    - S3 authentication errors
    - Timeout errors
    - DNS resolution failures
    """

    pass


class MountError(Error):
    """Filesystem mounting/unmounting error.

    Raised when FUSE or NBD mounting operations fail:
    - Binary not found
    - Mount point unavailable
    - Permission denied
    - Device busy
    """

    pass


class CompressionError(Error):
    """Compression or decompression error.

    Raised when compression operations fail:
    - Unsupported compression algorithm
    - Corrupted compressed data
    - Compression level out of range
    """

    pass


class ValidationError(Error):
    """Data validation error.

    Raised when data fails validation checks:
    - Invalid parameters
    - Type mismatches
    - Range violations
    - Constraint failures
    """

    pass


class FormatError(Error):
    """Invalid file format.

    Raised when a file doesn't match the expected Hexz format:
    - Wrong magic bytes
    - Unsupported version
    - Corrupted header
    - Invalid index structure
    """

    pass


class EncryptionError(Error):
    """Encryption or decryption error.

    Raised when cryptographic operations fail:
    - Wrong password
    - Invalid key
    - Corrupted encrypted data
    - Missing encryption parameters
    """

    pass


class SignatureError(Error):
    """Signature verification error.

    Raised when signature operations fail:
    - Invalid signature
    - Wrong public key
    - Signature verification failed
    - Missing signature
    """

    pass


class CacheError(Error):
    """Cache operation error.

    Raised when cache operations fail:
    - Cache full
    - Eviction failed
    - Invalid cache configuration
    """

    pass


class VersionError(FormatError):
    """Incompatible file version.

    Raised when a file's version is not supported:
    - Version too old
    - Version too new
    - Migration required
    """

    def __init__(
        self,
        message: str,
        file_version: Optional[int] = None,
        supported_version: Optional[int] = None,
    ):
        super().__init__(message)
        self.file_version = file_version
        self.supported_version = supported_version
