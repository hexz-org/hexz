"""Extended tests for strata __init__.py module."""

import pytest
import strata


class TestOpenFunction:
    """Test strata.open() with various modes."""

    def test_open_invalid_mode(self):
        with pytest.raises(ValueError, match="Invalid mode"):
            strata.open("dummy.st", mode="x")

    def test_open_invalid_mode_append(self):
        with pytest.raises(ValueError, match="Invalid mode"):
            strata.open("dummy.st", mode="a")


class TestVersion:
    """Test version-related functionality."""

    def test_version_returns_string(self):
        v = strata.version()
        assert isinstance(v, str)

    def test_version_not_empty(self):
        v = strata.version()
        assert len(v) > 0

    def test_dunder_version(self):
        assert hasattr(strata, "__version__")
        assert isinstance(strata.__version__, str)

    def test_version_matches_dunder(self):
        assert strata.version() == strata.__version__


class TestAllExports:
    """Test that __all__ exports are accessible."""

    def test_all_defined(self):
        assert hasattr(strata, "__all__")
        assert len(strata.__all__) > 0

    def test_all_exports_accessible(self):
        for name in strata.__all__:
            assert hasattr(strata, name), f"Missing export: {name}"

    def test_reader_class_available(self):
        assert hasattr(strata, "Reader")

    def test_writer_class_available(self):
        assert hasattr(strata, "Writer")

    def test_async_reader_available(self):
        assert hasattr(strata, "AsyncReader")

    def test_dataset_available(self):
        assert hasattr(strata, "Dataset")

    def test_crypto_submodule(self):
        assert hasattr(strata, "crypto")
        assert hasattr(strata.crypto, "keygen")
        assert hasattr(strata.crypto, "sign")
        assert hasattr(strata.crypto, "verify")

    def test_build_available(self):
        assert hasattr(strata, "build")
        assert callable(strata.build)

    def test_profiles_available(self):
        assert hasattr(strata, "PROFILES")
        assert isinstance(strata.PROFILES, dict)

    def test_inspect_available(self):
        assert hasattr(strata, "inspect")
        assert callable(strata.inspect)


class TestExceptionExports:
    """Test that all exceptions are properly exported."""

    def test_strata_error(self):
        assert issubclass(strata.StrataError, Exception)

    def test_io_error(self):
        assert issubclass(strata.IOError, strata.StrataError)

    def test_network_error(self):
        assert issubclass(strata.NetworkError, strata.IOError)

    def test_format_error(self):
        assert issubclass(strata.FormatError, strata.StrataError)

    def test_validation_error(self):
        assert issubclass(strata.ValidationError, strata.StrataError)

    def test_compression_error(self):
        assert issubclass(strata.CompressionError, strata.StrataError)

    def test_encryption_error(self):
        assert issubclass(strata.EncryptionError, strata.StrataError)

    def test_mount_error(self):
        assert issubclass(strata.MountError, strata.StrataError)

    def test_cache_error(self):
        assert issubclass(strata.CacheError, strata.StrataError)

    def test_version_error(self):
        assert issubclass(strata.VersionError, strata.FormatError)


class TestVersionConstants:
    """Test version constants."""

    def test_format_version(self):
        assert isinstance(strata.FORMAT_VERSION, int)
        assert strata.FORMAT_VERSION > 0

    def test_min_supported_version(self):
        assert isinstance(strata.MIN_SUPPORTED_VERSION, int)
        assert strata.MIN_SUPPORTED_VERSION > 0

    def test_max_supported_version(self):
        assert isinstance(strata.MAX_SUPPORTED_VERSION, int)
        assert strata.MAX_SUPPORTED_VERSION >= strata.MIN_SUPPORTED_VERSION

    def test_format_version_in_range(self):
        assert (
            strata.MIN_SUPPORTED_VERSION
            <= strata.FORMAT_VERSION
            <= strata.MAX_SUPPORTED_VERSION
        )


class TestPathLike:
    """Test PathLike type export."""

    def test_pathlike_available(self):
        assert hasattr(strata, "PathLike")
