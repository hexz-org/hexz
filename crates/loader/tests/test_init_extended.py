"""Extended tests for hexz __init__.py module."""

import pytest
import hexz


class TestOpenFunction:
    """Test hexz.open() with various modes."""

    def test_open_invalid_mode(self):
        with pytest.raises(ValueError, match="Invalid mode"):
            hexz.open("dummy.hxz", mode="x")

    def test_open_invalid_mode_append(self):
        with pytest.raises(ValueError, match="Invalid mode"):
            hexz.open("dummy.hxz", mode="a")


class TestVersion:
    """Test version-related functionality."""

    def test_version_returns_string(self):
        v = hexz.version()
        assert isinstance(v, str)

    def test_version_not_empty(self):
        v = hexz.version()
        assert len(v) > 0

    def test_dunder_version(self):
        assert hasattr(hexz, "__version__")
        assert isinstance(hexz.__version__, str)

    def test_version_matches_dunder(self):
        assert hexz.version() == hexz.__version__


class TestAllExports:
    """Test that __all__ exports are accessible."""

    def test_all_defined(self):
        assert hasattr(hexz, "__all__")
        assert len(hexz.__all__) > 0

    def test_all_exports_accessible(self):
        for name in hexz.__all__:
            assert hasattr(hexz, name), f"Missing export: {name}"

    def test_reader_class_available(self):
        assert hasattr(hexz, "Reader")

    def test_writer_class_available(self):
        assert hasattr(hexz, "Writer")

    def test_async_reader_available(self):
        assert hasattr(hexz, "AsyncReader")

    def test_dataset_available(self):
        assert hasattr(hexz, "Dataset")

    def test_crypto_submodule(self):
        assert hasattr(hexz, "crypto")
        assert hasattr(hexz.crypto, "keygen")
        assert hasattr(hexz.crypto, "sign")
        assert hasattr(hexz.crypto, "verify")

    def test_build_available(self):
        assert hasattr(hexz, "build")
        assert callable(hexz.build)

    def test_profiles_available(self):
        assert hasattr(hexz, "PROFILES")
        assert isinstance(hexz.PROFILES, dict)

    def test_inspect_available(self):
        assert hasattr(hexz, "inspect")
        assert callable(hexz.inspect)


class TestExceptionExports:
    """Test that all exceptions are properly exported."""

    def test_hexz_error(self):
        assert issubclass(hexz.Error, Exception)

    def test_io_error(self):
        assert issubclass(hexz.IOError, hexz.Error)

    def test_network_error(self):
        assert issubclass(hexz.NetworkError, hexz.IOError)

    def test_format_error(self):
        assert issubclass(hexz.FormatError, hexz.Error)

    def test_validation_error(self):
        assert issubclass(hexz.ValidationError, hexz.Error)

    def test_compression_error(self):
        assert issubclass(hexz.CompressionError, hexz.Error)

    def test_encryption_error(self):
        assert issubclass(hexz.EncryptionError, hexz.Error)

    def test_mount_error(self):
        assert issubclass(hexz.MountError, hexz.Error)

    def test_cache_error(self):
        assert issubclass(hexz.CacheError, hexz.Error)

    def test_version_error(self):
        assert issubclass(hexz.VersionError, hexz.FormatError)


class TestVersionConstants:
    """Test version constants."""

    def test_format_version(self):
        assert isinstance(hexz.FORMAT_VERSION, int)
        assert hexz.FORMAT_VERSION > 0

    def test_min_supported_version(self):
        assert isinstance(hexz.MIN_SUPPORTED_VERSION, int)
        assert hexz.MIN_SUPPORTED_VERSION > 0

    def test_max_supported_version(self):
        assert isinstance(hexz.MAX_SUPPORTED_VERSION, int)
        assert hexz.MAX_SUPPORTED_VERSION >= hexz.MIN_SUPPORTED_VERSION

    def test_format_version_in_range(self):
        assert (
            hexz.MIN_SUPPORTED_VERSION
            <= hexz.FORMAT_VERSION
            <= hexz.MAX_SUPPORTED_VERSION
        )


class TestPathLike:
    """Test PathLike type export."""

    def test_pathlike_available(self):
        assert hasattr(hexz, "PathLike")
