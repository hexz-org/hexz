"""Extended tests for strata.reader module (coverage for uncovered paths)."""

import pytest
import strata
from strata.reader import _parse_cache_size


class TestParseCacheSize:
    """Test _parse_cache_size helper function."""

    def test_bytes_only(self):
        assert _parse_cache_size("1024") == 1024

    def test_kilobytes(self):
        assert _parse_cache_size("512K") == 512 * 1024

    def test_kilobytes_with_b(self):
        assert _parse_cache_size("512KB") == 512 * 1024

    def test_megabytes(self):
        assert _parse_cache_size("256M") == 256 * 1024 * 1024

    def test_megabytes_with_b(self):
        assert _parse_cache_size("256MB") == 256 * 1024 * 1024

    def test_gigabytes(self):
        assert _parse_cache_size("1G") == 1024 * 1024 * 1024

    def test_gigabytes_with_b(self):
        assert _parse_cache_size("2GB") == 2 * 1024 * 1024 * 1024

    def test_terabytes(self):
        assert _parse_cache_size("1T") == 1024**4

    def test_case_insensitive(self):
        assert _parse_cache_size("512m") == 512 * 1024 * 1024

    def test_with_whitespace(self):
        assert _parse_cache_size("  512M  ") == 512 * 1024 * 1024

    def test_float_value(self):
        assert _parse_cache_size("1.5G") == int(1.5 * 1024 * 1024 * 1024)

    def test_invalid_format(self):
        with pytest.raises(ValueError, match="Invalid cache size"):
            _parse_cache_size("abc")

    def test_empty_string(self):
        with pytest.raises(ValueError, match="Invalid cache size"):
            _parse_cache_size("")

    def test_mib_format(self):
        # "MIB" should be normalized to "M" after stripping I and B
        assert _parse_cache_size("512MIB") == 512 * 1024 * 1024

    def test_gib_format(self):
        assert _parse_cache_size("1GIB") == 1024 * 1024 * 1024


class TestReaderSliceNotation:
    """Test Reader __getitem__ slice access."""

    def test_integer_index_raises_type_error(self, base_snap_path):
        reader = strata.Reader(base_snap_path)
        with pytest.raises(TypeError, match="indices must be slices"):
            _ = reader[0]
        reader.close()

    def test_slice_with_start_stop(self, base_snap_path):
        reader = strata.Reader(base_snap_path)
        data = reader[0:100]
        assert len(data) == 100
        reader.close()

    def test_slice_with_none_start(self, base_snap_path):
        reader = strata.Reader(base_snap_path)
        data = reader[:100]
        assert len(data) == 100
        reader.close()


class TestReaderReadRange:
    """Test Reader.read_range method."""

    def test_read_range_basic(self, base_snap_path):
        reader = strata.Reader(base_snap_path)
        data = reader.read_range(0, 100)
        assert len(data) == 100
        reader.close()

    def test_read_range_middle(self, base_snap_path):
        reader = strata.Reader(base_snap_path)
        data = reader.read_range(1000, 2000)
        assert len(data) == 1000
        reader.close()


class TestReaderReadinto:
    """Test Reader.readinto method."""

    def test_readinto_basic(self, base_snap_path):
        reader = strata.Reader(base_snap_path)
        buf = bytearray(100)
        n = reader.readinto(buf)
        assert n == 100
        reader.close()


class TestReaderContextManager:
    """Test context manager behavior."""

    def test_context_manager(self, base_snap_path):
        with strata.Reader(base_snap_path) as r:
            data = r.read(100)
            assert len(data) == 100

    def test_repr(self, base_snap_path):
        reader = strata.Reader(base_snap_path)
        r = repr(reader)
        assert "Reader(" in r
        reader.close()


class TestReaderMetadata:
    """Test Reader.metadata property."""

    def test_metadata_returns_metadata(self, base_snap_path):
        reader = strata.Reader(base_snap_path)
        meta = reader.metadata
        assert hasattr(meta, "version")
        reader.close()


class TestReaderIterChunks:
    """Test Reader.iter_chunks iterator."""

    def test_iter_chunks_basic(self, base_snap_path):
        reader = strata.Reader(base_snap_path)
        total = 0
        for chunk in reader.iter_chunks(chunk_size=64 * 1024):
            total += len(chunk)
        assert total == reader.size
        reader.close()

    def test_iter_chunks_small(self, base_snap_path):
        reader = strata.Reader(base_snap_path)
        chunks = list(reader.iter_chunks(chunk_size=4096))
        assert len(chunks) > 0
        reader.close()


class TestReaderGetSetState:
    """Test pickle serialization support."""

    def test_getstate(self, base_snap_path):
        reader = strata.Reader(base_snap_path)
        reader.seek(100)
        state = reader.__getstate__()
        assert state["path"] == base_snap_path
        assert state["position"] == 100
        reader.close()

    def test_setstate(self, base_snap_path):
        reader = strata.Reader(base_snap_path)
        state = {"path": base_snap_path, "position": 50}
        reader.__setstate__(state)
        assert reader.tell() == 50
        reader.close()

    def test_setstate_no_position(self, base_snap_path):
        reader = strata.Reader(base_snap_path)
        state = {"path": base_snap_path}
        reader.__setstate__(state)
        assert reader.tell() == 0
        reader.close()


class TestReaderCacheSize:
    """Test Reader with cache_size parameter."""

    def test_reader_with_cache_size(self, base_snap_path):
        reader = strata.Reader(base_snap_path, cache_size="512M")
        data = reader.read(100)
        assert len(data) == 100
        reader.close()

    def test_reader_no_prefetch(self, base_snap_path):
        reader = strata.Reader(base_snap_path, prefetch=False)
        data = reader.read(100)
        assert len(data) == 100
        reader.close()


class TestReaderSeekTell:
    """Test seek and tell."""

    def test_seek_absolute(self, base_snap_path):
        reader = strata.Reader(base_snap_path)
        pos = reader.seek(500)
        assert pos == 500
        assert reader.tell() == 500
        reader.close()

    def test_seek_relative(self, base_snap_path):
        reader = strata.Reader(base_snap_path)
        reader.seek(100)
        pos = reader.seek(50, 1)  # relative
        assert pos == 150
        reader.close()

    def test_seek_from_end(self, base_snap_path):
        reader = strata.Reader(base_snap_path)
        pos = reader.seek(-100, 2)  # from end
        assert pos == reader.size - 100
        reader.close()
