"""Extended tests for strata.exceptions module."""

import pytest
from strata.exceptions import (
    StrataError,
    IOError as StrataIOError,
    NetworkError,
    MountError,
    CompressionError,
    ValidationError,
    FormatError,
    EncryptionError,
    SignatureError,
    CacheError,
    VersionError,
)


class TestExceptionHierarchy:
    """Test the exception inheritance chain."""

    def test_strata_error_is_exception(self):
        assert issubclass(StrataError, Exception)

    def test_io_error_inherits_strata_error(self):
        assert issubclass(StrataIOError, StrataError)

    def test_io_error_inherits_os_error(self):
        assert issubclass(StrataIOError, OSError)

    def test_network_error_inherits_io_error(self):
        assert issubclass(NetworkError, StrataIOError)

    def test_mount_error_inherits_strata_error(self):
        assert issubclass(MountError, StrataError)

    def test_compression_error_inherits_strata_error(self):
        assert issubclass(CompressionError, StrataError)

    def test_validation_error_inherits_strata_error(self):
        assert issubclass(ValidationError, StrataError)

    def test_format_error_inherits_strata_error(self):
        assert issubclass(FormatError, StrataError)

    def test_encryption_error_inherits_strata_error(self):
        assert issubclass(EncryptionError, StrataError)

    def test_signature_error_inherits_strata_error(self):
        assert issubclass(SignatureError, StrataError)

    def test_cache_error_inherits_strata_error(self):
        assert issubclass(CacheError, StrataError)

    def test_version_error_inherits_format_error(self):
        assert issubclass(VersionError, FormatError)

    def test_version_error_inherits_strata_error(self):
        assert issubclass(VersionError, StrataError)


class TestExceptionInstantiation:
    """Test that all exceptions can be raised and caught."""

    def test_strata_error_message(self):
        err = StrataError("test error")
        assert str(err) == "test error"

    def test_io_error_message(self):
        err = StrataIOError("disk full")
        assert str(err) == "disk full"

    def test_network_error_message(self):
        err = NetworkError("connection refused")
        assert str(err) == "connection refused"

    def test_mount_error_message(self):
        err = MountError("fuse not found")
        assert str(err) == "fuse not found"

    def test_compression_error_message(self):
        err = CompressionError("invalid compressed data")
        assert str(err) == "invalid compressed data"

    def test_validation_error_message(self):
        err = ValidationError("block_size must be positive")
        assert str(err) == "block_size must be positive"

    def test_format_error_message(self):
        err = FormatError("wrong magic bytes")
        assert str(err) == "wrong magic bytes"

    def test_encryption_error_message(self):
        err = EncryptionError("wrong password")
        assert str(err) == "wrong password"

    def test_signature_error_message(self):
        err = SignatureError("invalid signature")
        assert str(err) == "invalid signature"

    def test_cache_error_message(self):
        err = CacheError("cache full")
        assert str(err) == "cache full"


class TestVersionError:
    """Test VersionError with its extra attributes."""

    def test_basic_version_error(self):
        err = VersionError("version mismatch")
        assert str(err) == "version mismatch"
        assert err.file_version is None
        assert err.supported_version is None

    def test_version_error_with_versions(self):
        err = VersionError("too old", file_version=1, supported_version=3)
        assert err.file_version == 1
        assert err.supported_version == 3
        assert "too old" in str(err)

    def test_version_error_with_only_file_version(self):
        err = VersionError("unknown version", file_version=99)
        assert err.file_version == 99
        assert err.supported_version is None


class TestExceptionCatching:
    """Test catching exceptions at various levels of the hierarchy."""

    def test_catch_network_as_io(self):
        with pytest.raises(StrataIOError):
            raise NetworkError("timeout")

    def test_catch_network_as_strata(self):
        with pytest.raises(StrataError):
            raise NetworkError("timeout")

    def test_catch_io_as_os_error(self):
        with pytest.raises(OSError):
            raise StrataIOError("disk error")

    def test_catch_version_as_format(self):
        with pytest.raises(FormatError):
            raise VersionError("too new", file_version=99)

    def test_catch_version_as_strata(self):
        with pytest.raises(StrataError):
            raise VersionError("too new")

    def test_catch_all_strata_errors(self):
        exceptions = [
            StrataError("base"),
            StrataIOError("io"),
            NetworkError("net"),
            MountError("mount"),
            CompressionError("comp"),
            ValidationError("valid"),
            FormatError("format"),
            EncryptionError("encrypt"),
            SignatureError("sig"),
            CacheError("cache"),
            VersionError("ver"),
        ]
        for exc in exceptions:
            with pytest.raises(StrataError):
                raise exc
